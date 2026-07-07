use clap::{Parser, Subcommand};
use ctxrelay_core::{run_import, run_import_from_bytes, run_ir, run_undo, run_verify, ImportOptions, Registry};
use ctxrelay_frontend::SourceRef;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "ctxrelay")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 只 parse 出 IR,不 commit
    Ir {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// 完整导入:Acquire → Parse → legalize → lower → commit
    Import {
        input: PathBuf,
        #[arg(long)]
        to: String,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        bootstrap: bool,
        #[arg(long)]
        manifest_out: Option<PathBuf>,
    },
    /// 撤销一次 commit
    Undo { manifest: PathBuf },
    /// 冒烟测试:resume 一次 commit 出来的会话
    Verify { manifest: PathBuf },
    /// 起一个一次性本地服务,等浏览器扩展 POST 一次抓取,跑完整个 import 管线后退出
    Listen {
        #[arg(long)]
        to: String,
        #[arg(long)]
        project: PathBuf,
        #[arg(long, default_value_t = 47651)]
        port: u16,
        #[arg(long)]
        manifest_out: Option<PathBuf>,
        /// 仅测试用:覆盖 `~/.claude/projects` 的默认位置
        #[arg(long)]
        claude_projects_root: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let registry = Registry::with_defaults();

    let result = match cli.command {
        Command::Ir { input, output } => run_ir_command(&registry, input, output),
        Command::Import { input, to, project, dry_run, bootstrap, manifest_out } => {
            run_import_command(&registry, input, to, project, dry_run, bootstrap, manifest_out)
        }
        Command::Undo { manifest } => run_undo_command(manifest),
        Command::Verify { manifest } => run_verify_command(manifest),
        Command::Listen { to, project, port, manifest_out, claude_projects_root } => {
            run_listen_command(to, project, port, manifest_out, claude_projects_root)
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run_ir_command(registry: &Registry, input: PathBuf, output: PathBuf) -> Result<(), String> {
    let doc = run_ir(registry, SourceRef::File(input)).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&output, json).map_err(|e| e.to_string())?;
    println!("wrote IR to {}", output.display());
    Ok(())
}

fn run_import_command(
    registry: &Registry,
    input: PathBuf,
    to: String,
    project: PathBuf,
    dry_run: bool,
    bootstrap: bool,
    manifest_out: Option<PathBuf>,
) -> Result<(), String> {
    let claude_projects_root = claude_projects_root().map_err(|e| e.to_string())?;
    let cli_version = detect_claude_version().unwrap_or_else(|| "unknown".to_string());

    let opts = ImportOptions {
        backend_name: to,
        project_dir: project.clone(),
        dry_run,
        allow_bootstrap: bootstrap,
        claude_projects_root,
        cli_version,
    };

    let manifest = run_import(registry, SourceRef::File(input), opts).map_err(|e| e.to_string())?;

    let manifest_path = manifest_out.unwrap_or_else(|| {
        let session_id = manifest.created_session_ids.first().cloned().unwrap_or_else(|| "unknown".to_string());
        project.join(".ctxrelay").join("manifests").join(format!("{session_id}.manifest.json"))
    });
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, json).map_err(|e| e.to_string())?;

    if dry_run {
        println!("dry-run: would write session {:?}, manifest saved to {}", manifest.created_session_ids, manifest_path.display());
    } else {
        println!("committed session {:?}, manifest saved to {}", manifest.created_session_ids, manifest_path.display());
    }
    Ok(())
}

fn run_undo_command(manifest_path: PathBuf) -> Result<(), String> {
    let actions = run_undo(&manifest_path).map_err(|e| e.to_string())?;
    for action in actions {
        println!("{action:?}");
    }
    Ok(())
}

fn run_verify_command(manifest_path: PathBuf) -> Result<(), String> {
    let summary = run_verify(&manifest_path).map_err(|e| e.to_string())?;
    println!("{summary}");
    Ok(())
}

fn claude_projects_root() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME environment variable not set".to_string())?;
    Ok(PathBuf::from(home).join(".claude/projects"))
}

fn detect_claude_version() -> Option<String> {
    let output = std::process::Command::new("claude").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 一次性本地 HTTP 服务:起服务、打印 token、等浏览器扩展 POST 一次抓取、校验
/// token、跑完整个 import 管线、写 manifest、响应、退出。刻意不做成常驻服务——
/// 只处理一个请求就退出,避免引入一个长期占用端口、需要额外生命周期管理的后台
/// 进程(这个项目的架构原则是尽量少养常驻轮询服务)。
fn run_listen_command(
    to: String,
    project: PathBuf,
    port: u16,
    manifest_out: Option<PathBuf>,
    claude_projects_root_override: Option<PathBuf>,
) -> Result<(), String> {
    let token = uuid::Uuid::new_v4().to_string();
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| format!("failed to bind 127.0.0.1:{port}: {e}"))?;

    println!("token: {token}");
    println!(
        "ctxrelay listen 正在监听 http://127.0.0.1:{port} ,在 claude.ai 页面打开扩展设置,\
         把上面这个 token 粘贴进去,然后点一下工具栏图标导入当前对话。"
    );

    let mut request = server
        .recv()
        .map_err(|e| format!("failed to receive request: {e}"))?;

    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("failed to read request body: {e}"))?;

    let header_token = request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("X-CtxRelay-Token"))
        .map(|h| h.value.as_str().to_string());

    if header_token.as_deref() != Some(token.as_str()) {
        let response = tiny_http::Response::from_string(
            r#"{"version":"1","status":"error","message":"invalid token"}"#,
        )
        .with_status_code(401);
        let _ = request.respond(response);
        return Err("received request with invalid token".to_string());
    }

    let capture: ctxrelay_cli::bridge::CaptureRequest =
        serde_json::from_str(&body).map_err(|e| format!("invalid CaptureRequest JSON: {e}"))?;

    if capture.token != token {
        let response = tiny_http::Response::from_string(
            r#"{"version":"1","status":"error","message":"invalid token"}"#,
        )
        .with_status_code(401);
        let _ = request.respond(response);
        return Err("CaptureRequest body token does not match".to_string());
    }

    let claude_projects_root = match claude_projects_root_override {
        Some(root) => root,
        None => claude_projects_root().map_err(|e| e.to_string())?,
    };
    let cli_version = detect_claude_version().unwrap_or_else(|| "unknown".to_string());

    let registry = Registry::with_defaults();
    let opts = ImportOptions {
        backend_name: to,
        project_dir: project.clone(),
        dry_run: false,
        allow_bootstrap: false,
        claude_projects_root,
        cli_version,
    };

    let raw = serde_json::to_vec(&capture.snapshot).map_err(|e| e.to_string())?;
    let result = run_import_from_bytes(&registry, raw, "fe-claude-live", opts);

    let (status_code, response_body, outcome) = match &result {
        Ok(manifest) => {
            let manifest_path = manifest_out.unwrap_or_else(|| {
                let session_id =
                    manifest.created_session_ids.first().cloned().unwrap_or_else(|| "unknown".to_string());
                project.join(".ctxrelay").join("manifests").join(format!("{session_id}.manifest.json"))
            });
            if let Some(parent) = manifest_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let json = serde_json::to_string_pretty(manifest).unwrap_or_default();
            let _ = std::fs::write(&manifest_path, json);
            (
                200,
                r#"{"version":"1","status":"ok"}"#.to_string(),
                Ok(format!(
                    "committed session {:?}, manifest saved to {}",
                    manifest.created_session_ids,
                    manifest_path.display()
                )),
            )
        }
        Err(e) => (
            200,
            format!(r#"{{"version":"1","status":"error","message":{:?}}}"#, e.to_string()),
            Err(e.to_string()),
        ),
    };

    let response = tiny_http::Response::from_string(response_body).with_status_code(status_code);
    let _ = request.respond(response);

    match outcome {
        Ok(message) => {
            println!("{message}");
            Ok(())
        }
        Err(message) => Err(message),
    }
}
