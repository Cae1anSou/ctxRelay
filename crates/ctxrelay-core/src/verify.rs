use ctxrelay_backend::Manifest;
use std::fmt;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct VerifyError(pub String);

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for VerifyError {}

pub type Result<T> = std::result::Result<T, VerifyError>;

/// 读 manifest——这是 `run_verify` 里唯一能离线测试的部分,不涉及任何真实 CLI 调用。
pub fn load_manifest(manifest_path: &Path) -> Result<Manifest> {
    let raw = std::fs::read_to_string(manifest_path)
        .map_err(|e| VerifyError(format!("failed to read manifest {}: {e}", manifest_path.display())))?;
    serde_json::from_str(&raw).map_err(|e| VerifyError(format!("invalid manifest JSON: {e}")))
}

/// 冒烟测试:`claude --resume <session_id>` 能不能正常加载并给出回应。这不是内容
/// 一致性校验(真实用户对话没有"正确答案"可断言),只确认 resume 这条路没坏——
/// 内容一致性验证靠 `be-claude-code` 的 conformance 测试(埋暗号词),那条测试
/// 知道正确答案该是什么。
pub fn run_verify(manifest_path: &Path) -> Result<String> {
    let manifest = load_manifest(manifest_path)?;
    let session_id = manifest
        .created_session_ids
        .first()
        .ok_or_else(|| VerifyError("manifest has no created_session_ids".to_string()))?;

    let output = Command::new("claude")
        .arg("--resume")
        .arg(session_id)
        .arg("-p")
        .arg("用一句话总结我们上一轮聊了什么")
        .arg("--output-format")
        .arg("json")
        .current_dir(&manifest.cwd)
        .output()
        .map_err(|e| VerifyError(format!("failed to run claude CLI: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| VerifyError(format!("expected JSON output from claude, got error {e}: {stdout}")))?;
    parsed["result"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| VerifyError("claude output missing string \"result\" field".to_string()))
}
