use clap::{Parser, Subcommand};
use ctxrelay_core::{
    run_import, run_import_from_bytes, run_ir, run_undo, run_verify, ImportOptions, Registry,
};
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
        /// 目标 backend 名字,目前只有 "claude-code" 这一个
        #[arg(long)]
        to: String,
        /// 要写入哪个本地项目目录(决定 ~/.claude/projects/ 下对应哪个会话目录)
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        dry_run: bool,
        /// 目标项目从未在 Claude Code 里打开过时,花一点真实 API 额度让 ctxrelay
        /// 代为一次性初始化(见 `resolve_claude_code_dest` 的文档注释)
        #[arg(long)]
        bootstrap: bool,
        /// manifest 输出路径,默认写到 <project>/.ctxrelay/manifests/<session-id>.manifest.json
        #[arg(long)]
        manifest_out: Option<PathBuf>,
    },
    /// 撤销一次 commit
    Undo { manifest: PathBuf },
    /// 冒烟测试:resume 一次 commit 出来的会话
    Verify { manifest: PathBuf },
    /// 起一个一次性本地服务,等浏览器扩展 POST 一次抓取,跑完整个 import 管线后退出
    Listen {
        /// 目标 backend 名字,目前只有 "claude-code" 这一个
        #[arg(long)]
        to: String,
        /// 要写入哪个本地项目目录(决定 ~/.claude/projects/ 下对应哪个会话目录)
        #[arg(long)]
        project: PathBuf,
        /// 本地服务监听的端口,要和浏览器扩展设置页里填的端口一致
        #[arg(long, default_value_t = 47651)]
        port: u16,
        /// manifest 输出路径,默认写到 <project>/.ctxrelay/manifests/<session-id>.manifest.json
        #[arg(long)]
        manifest_out: Option<PathBuf>,
        /// 目标项目从未在 Claude Code 里打开过时,花一点真实 API 额度让 ctxrelay
        /// 代为一次性初始化(见 `resolve_claude_code_dest` 的文档注释)
        #[arg(long)]
        bootstrap: bool,
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
        Command::Import {
            input,
            to,
            project,
            dry_run,
            bootstrap,
            manifest_out,
        } => run_import_command(
            &registry,
            input,
            to,
            project,
            dry_run,
            bootstrap,
            manifest_out,
        ),
        Command::Undo { manifest } => run_undo_command(manifest),
        Command::Verify { manifest } => run_verify_command(manifest),
        Command::Listen {
            to,
            project,
            port,
            manifest_out,
            bootstrap,
            claude_projects_root,
        } => run_listen_command(
            to,
            project,
            port,
            manifest_out,
            bootstrap,
            claude_projects_root,
        ),
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
        let session_id = manifest
            .created_session_ids
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        project
            .join(".ctxrelay")
            .join("manifests")
            .join(format!("{session_id}.manifest.json"))
    });
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, json).map_err(|e| e.to_string())?;

    if dry_run {
        println!(
            "dry-run: would write session {:?}, manifest saved to {}",
            manifest.created_session_ids,
            manifest_path.display()
        );
    } else {
        println!(
            "committed session {:?}, manifest saved to {}",
            manifest.created_session_ids,
            manifest_path.display()
        );
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
    let home =
        std::env::var("HOME").map_err(|_| "HOME environment variable not set".to_string())?;
    Ok(PathBuf::from(home).join(".claude/projects"))
}

fn detect_claude_version() -> Option<String> {
    let output = std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 常量时间字符串比较,防的是本机时序攻击(用响应时间差猜 token 字节)——本地
/// 单用户场景下这个威胁模型的实际价值很低,但 token 是这条本地服务唯一的准入
/// 门槛,能顺手做对就不该图省事用短路 `==`。长度不等时直接返回 false 之前先做
/// 一次固定成本的比较,避免因为长度提前分支而泄漏长度信息。
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let len_matches = a.len() == b.len();
    let max_len = a.len().max(b.len());
    let mut diff: u8 = if len_matches { 0 } else { 1 };
    for i in 0..max_len {
        let byte_a = a.get(i).copied().unwrap_or(0);
        let byte_b = b.get(i).copied().unwrap_or(0);
        diff |= byte_a ^ byte_b;
    }
    diff == 0
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
    bootstrap: bool,
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
        .find(|h| {
            h.field
                .as_str()
                .as_str()
                .eq_ignore_ascii_case("X-CtxRelay-Token")
        })
        .map(|h| h.value.as_str().to_string());

    if !header_token
        .as_deref()
        .is_some_and(|h| constant_time_eq(h, &token))
    {
        let response = tiny_http::Response::from_string(
            ctxrelay_cli::bridge::CaptureResponse::error("invalid token").to_json(),
        )
        .with_status_code(401);
        let _ = request.respond(response);
        return Err("received request with invalid token".to_string());
    }

    // 请求体不合 schema(比如 TS/Rust 两侧字段手写投影哪天漂移了)是一个真实存在
    // 的、预期内会发生的失败模式,不是"不可能到达"的编程错误——之前这里用 `?`
    // 直接把错误甩给 `main()`,响应从未发出,扩展侧的 `fetch` 会因为连接被复位而
    // 抛异常,badge 显示成 `N/L`("没连上本地服务"),这个反馈跟真实原因(连上了、
    // 解析崩了)完全对不上,会把用户导向错误的排查方向(去查端口/token,而不是
    // 去查两侧协议是否漂移)。显式捕获、回一个 400,让扩展至少能读到 `status`。
    let capture: ctxrelay_cli::bridge::CaptureRequest = match serde_json::from_str(&body) {
        Ok(capture) => capture,
        Err(e) => {
            let message = format!("invalid CaptureRequest JSON: {e}");
            let response = tiny_http::Response::from_string(
                ctxrelay_cli::bridge::CaptureResponse::error(&message).to_json(),
            )
            .with_status_code(400);
            let _ = request.respond(response);
            return Err(message);
        }
    };

    if !constant_time_eq(&capture.token, &token) {
        let response = tiny_http::Response::from_string(
            ctxrelay_cli::bridge::CaptureResponse::error("invalid token").to_json(),
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
        allow_bootstrap: bootstrap,
        claude_projects_root,
        cli_version,
    };

    let raw = serde_json::to_vec(&capture.snapshot).map_err(|e| e.to_string())?;
    let result = run_import_from_bytes(&registry, raw, &capture.frontend_id, opts);

    let (status_code, response_body, outcome) = match &result {
        Ok(manifest) => {
            let manifest_path = manifest_out.unwrap_or_else(|| {
                let session_id = manifest
                    .created_session_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                project
                    .join(".ctxrelay")
                    .join("manifests")
                    .join(format!("{session_id}.manifest.json"))
            });
            if let Some(parent) = manifest_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let json = serde_json::to_string_pretty(manifest).unwrap_or_default();
            let _ = std::fs::write(&manifest_path, json);
            (
                200,
                ctxrelay_cli::bridge::CaptureResponse::ok().to_json(),
                Ok(format!(
                    "committed session {:?}, manifest saved to {}",
                    manifest.created_session_ids,
                    manifest_path.display()
                )),
            )
        }
        // 之前这里硬编码 200——HTTP 状态码是唯一一个扩展侧不用解析 body 就能看到
        // 的信号,`background.ts` 也确实只看 `res.ok`。回 200 意味着不管管线是不是
        // 真的崩了(比如项目没 bootstrap、backend 报错),扩展工具栏一律显示 `OK`,
        // 用户会以为导入成功,实际上什么都没写进去——这正是架构文档 §1 明确要杜绝
        // 的"静默失败",而且是两端各自的测试都不会报出来的那种。真实失败必须回一个
        // 非 2xx 状态码。
        Err(e) => (
            500,
            ctxrelay_cli::bridge::CaptureResponse::error(e.to_string()).to_json(),
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
