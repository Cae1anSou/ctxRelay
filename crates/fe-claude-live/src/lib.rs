//! claude.ai 认证态实时对话 frontend。
//!
//! 只实现 `Parse`,不实现 `Acquire`——数据来源是浏览器扩展主动 POST 到本地桥
//! (架构文档 §4/§10.1),不是 ctxrelay 自己发起的拉取,不适配 `Acquire::acquire`
//! "给一个 SourceRef,主动拿到 bytes"的语义。`ctxrelay-core` 的
//! `run_import_from_bytes` 会跳过 `Acquire`,直接把已经到手的字节交给这里的 Parse。

mod parse;

pub use parse::ClaudeLiveParse;
