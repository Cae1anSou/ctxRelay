//! ctxRelay 核心管线:薄 driver + frontend/backend 注册表(架构文档 §7)。
//!
//! 只做编排,不重新实现任何 `Acquire`/`Parse`/`Backend` 的逻辑——那些都在各自的
//! crate 里,`ctxrelay-core` 只负责把它们接起来。

pub mod registry;
pub mod dest;

pub use registry::Registry;
pub use dest::resolve_claude_code_dest;
