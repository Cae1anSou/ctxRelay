//! ctxRelay 中立 IR:所有 Web 源与所有 CLI 目标共同的最小语义内核。
//! 契约:只承诺 content-effect(对话内容/代码/推理链)的保真,
//! 对 action-effect 只承诺"标记存在 + 携带产物",绝不承诺可回放。

mod block;
mod document;

pub use block::{Artifact, Block, BlockCaps};
pub use document::{Document, Origin, Role, SourceProvenance, Turn, TurnId};
