//! ctxRelay 核心管线:薄 driver + frontend/backend 注册表(架构文档 §7)。
//!
//! 只做编排,不重新实现任何 `Acquire`/`Parse`/`Backend` 的逻辑——那些都在各自的
//! crate 里,`ctxrelay-core` 只负责把它们接起来。

pub mod dest;
pub mod pipeline;
pub mod registry;
pub mod undo;
pub mod verify;

pub use dest::resolve_claude_code_dest;
pub use pipeline::{run_import, run_import_from_bytes, run_ir, CoreError, ImportOptions, Result};
pub use registry::Registry;
pub use undo::{run_undo, UndoAction};
pub use verify::run_verify;
