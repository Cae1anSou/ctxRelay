use ctxrelay_frontend::{Acquire, FrontendError, RawBytes, Result, SourceRef};

/// claude.ai 分享快照的 Acquire 实现。
///
/// V1 范围:只支持 `SourceRef::File`(人工从浏览器另存为)。`SourceRef::Url`
/// 的自动抓取需要浏览器扩展 + 本地桥(架构文档 §12 步骤 6),尚未实现——
/// 这里明确报错而不是假装支持,避免调用方以为传个分享链接就能work。
pub struct ClaudeShareAcquire;

impl Acquire for ClaudeShareAcquire {
    fn id(&self) -> &'static str {
        "fe-claude-share"
    }

    fn accepts(&self, input: &SourceRef) -> bool {
        matches!(input, SourceRef::File(_))
    }

    fn acquire(&self, input: SourceRef) -> Result<RawBytes> {
        match input {
            SourceRef::File(path) => std::fs::read(&path)
                .map_err(|e| FrontendError(format!("failed to read {}: {e}", path.display()))),
            SourceRef::Url(url) => Err(FrontendError(format!(
                "fe-claude-share V1 只支持人工另存为文件(SourceRef::File);\
                 URL 自动抓取见架构文档 §12 步骤 6,尚未实现。收到的 URL: {url}"
            ))),
        }
    }
}
