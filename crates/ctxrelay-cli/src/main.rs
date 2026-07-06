use clap::{Parser, Subcommand};
use ctxrelay_core::{run_import, run_ir, run_undo, run_verify, ImportOptions, Registry};
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

#[allow(clippy::too_many_arguments)]
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
