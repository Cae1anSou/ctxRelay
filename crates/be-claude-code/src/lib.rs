//! Claude Code backend:把 IR lower 成 `~/.claude/projects/<slug>/*.jsonl` 的会话记录。
//!
//! 目录 slug 的发现/解析不是这个 crate 的职责(架构文档 §5),`commit` 只管把
//! `LoweredSession` 写进调用方已经解析好的 `Dest::session_dir`。
//!
//! 三个子模块(legalize/lower/commit)在后续任务里逐个加入,`ClaudeCodeBackend` 的
//! `impl Backend` 要等三个子模块都存在后才接线(见 Task 5)。

pub mod legalize;
pub mod lower;
