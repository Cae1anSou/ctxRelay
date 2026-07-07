use ctxrelay_backend::Dest;
use std::fmt;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct DestError(pub String);

impl fmt::Display for DestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DestError {}

pub type Result<T> = std::result::Result<T, DestError>;

/// 把 `/` 换成 `-` 的简单 slug 规则——只对不含冒号/空格/盘符的普通 Unix 路径成立
/// (架构文档 §5 明确警告过更复杂的编码规则不应该被反向工程)。这里只把它当"这个
/// 项目是不是已经在 Claude Code 里开过"的探测手段,不是唯一真相来源。
fn candidate_slug(canonical_project_dir: &Path) -> String {
    canonical_project_dir
        .display()
        .to_string()
        .replace('/', "-")
}

/// 解析 Claude Code backend 的 `Dest`。
///
/// 1. 先看 `<projects_root>/<slug>` 是否已经存在(意味着这个项目之前真的在 Claude
///    Code 里打开过)——存在就直接用,零成本、零 API 调用。
/// 2. 不存在时,`allow_bootstrap=false`(默认)直接报错,把决定权交还给用户。
///    `allow_bootstrap=true` 时,起一个一次性的真实 `claude -p` 调用(只为了让它
///    顺带建出这个目录,调用本身花费少量真实 API 额度),再丢弃它产生的那份
///    一次性会话文件,只留下发现到的目录。
///
/// 这个函数硬编码只认识 Claude Code 的目录规则,不是一个"给所有 backend 通用"的
/// 抽象——目前只有一个 backend,提前抽象一个还没有第二个实现的 trait 是过度设计;
/// 等 `be-codex` 落地时,Codex 的 Dest 发现逻辑大概率长得完全不一样,到时候再看
/// 要不要抽公共接口。
pub fn resolve_claude_code_dest(
    project_dir: &Path,
    projects_root: &Path,
    cli_version: &str,
    allow_bootstrap: bool,
) -> Result<Dest> {
    let canonical = project_dir.canonicalize().map_err(|e| {
        DestError(format!(
            "project dir {} does not exist: {e}",
            project_dir.display()
        ))
    })?;

    let slug = candidate_slug(&canonical);
    let session_dir = projects_root.join(&slug);

    if !session_dir.is_dir() {
        if !allow_bootstrap {
            return Err(DestError(format!(
                "{} 从未在 Claude Code 里打开过(找不到 {});请先在该目录手动跑一次 \
                 `claude`,或者带上 --bootstrap 让 ctxrelay 代为一次性初始化(会花费少量 \
                 真实 API 额度)。",
                project_dir.display(),
                session_dir.display()
            )));
        }
        bootstrap_project_dir(&canonical, &session_dir)?;
    }

    let git_branch = detect_git_branch(&canonical);

    Ok(Dest {
        session_dir,
        cwd: canonical,
        git_branch,
        cli_version: cli_version.to_string(),
    })
}

fn bootstrap_project_dir(canonical_project_dir: &Path, expected_session_dir: &Path) -> Result<()> {
    let throwaway_id = uuid::Uuid::new_v4().to_string();
    let status = Command::new("claude")
        .arg("--session-id")
        .arg(&throwaway_id)
        .arg("-p")
        .arg("ok")
        .current_dir(canonical_project_dir)
        .status()
        .map_err(|e| DestError(format!("failed to run claude CLI for bootstrap: {e}")))?;

    if !status.success() {
        return Err(DestError(format!(
            "bootstrap claude invocation exited with status {status}"
        )));
    }

    if !expected_session_dir.is_dir() {
        return Err(DestError(format!(
            "bootstrap ran but {} still doesn't exist; claude CLI's directory naming may have \
             changed (see architecture.md §5 on not reverse-engineering the slug rule)",
            expected_session_dir.display()
        )));
    }

    // 丢弃这次一次性会话本身的文件——我们只要目录,不要这条"ok"废话记录混进真正
    // 要写的会话历史里。
    let throwaway_file = expected_session_dir.join(format!("{throwaway_id}.jsonl"));
    let _ = std::fs::remove_file(throwaway_file);

    Ok(())
}

fn detect_git_branch(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}
