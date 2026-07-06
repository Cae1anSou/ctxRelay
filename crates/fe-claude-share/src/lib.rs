//! claude.ai 分享快照(chat_snapshots)frontend。
//!
//! V1 范围:Acquire 只支持人工从浏览器另存为的本地文件(`SourceRef::File`)。
//! URL 自动抓取(浏览器扩展 + 本地桥)见架构文档 §12 步骤 6,尚未实现。

mod acquire;
mod parse;

pub use acquire::ClaudeShareAcquire;
pub use parse::ClaudeShareParse;
