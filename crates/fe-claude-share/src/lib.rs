//! claude.ai 分享快照(chat_snapshots)frontend。
//!
//! V1 范围:Acquire 只支持人工从浏览器另存为的本地文件(`SourceRef::File`)。
//! URL 自动抓取(浏览器扩展 + 本地桥)见架构文档 §12 步骤 6,尚未实现。

// TODO(Task 4): 恢复 `mod acquire;` 与 `pub use acquire::ClaudeShareAcquire;`——
// 本任务(Task 3)只交付 parse.rs,acquire.rs 尚未创建,声明未存在的模块会
// 让整个 crate(含本任务的测试)编译不过,故先注释,留给创建 acquire.rs 的任务恢复。
// mod acquire;
mod parse;

// pub use acquire::ClaudeShareAcquire;
pub use parse::ClaudeShareParse;
