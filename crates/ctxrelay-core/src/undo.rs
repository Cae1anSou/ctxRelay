use ctxrelay_backend::Manifest;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct UndoError(pub String);

impl fmt::Display for UndoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UndoError {}

pub type Result<T> = std::result::Result<T, UndoError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoAction {
    Deleted(PathBuf),
    SkippedModified(PathBuf),
    SkippedMissing(PathBuf),
}

/// 撤销一次 commit:对每条 `WriteRecord` 核对当前文件内容的 sha256 是否还等于
/// commit 时记录的那个——不等就说明文件在 commit 之后被改过(比如用户真的在
/// Claude Code 里继续聊过),这时候拒绝删除,而不是不由分说地清掉用户的真实数据。
pub fn run_undo(manifest_path: &Path) -> Result<Vec<UndoAction>> {
    let raw = std::fs::read_to_string(manifest_path)
        .map_err(|e| UndoError(format!("failed to read manifest {}: {e}", manifest_path.display())))?;
    let manifest: Manifest =
        serde_json::from_str(&raw).map_err(|e| UndoError(format!("invalid manifest JSON: {e}")))?;

    let mut actions = Vec::with_capacity(manifest.writes.len());
    for write in manifest.writes {
        if !write.path.exists() {
            actions.push(UndoAction::SkippedMissing(write.path));
            continue;
        }
        let content = std::fs::read(&write.path)
            .map_err(|e| UndoError(format!("failed to read {}: {e}", write.path.display())))?;
        let actual_sha256 = format!("{:x}", Sha256::digest(&content));
        if actual_sha256 != write.sha256 {
            actions.push(UndoAction::SkippedModified(write.path));
            continue;
        }
        std::fs::remove_file(&write.path)
            .map_err(|e| UndoError(format!("failed to delete {}: {e}", write.path.display())))?;
        actions.push(UndoAction::Deleted(write.path));
    }
    Ok(actions)
}
