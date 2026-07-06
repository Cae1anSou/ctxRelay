//! Claude Code backend:把 IR lower 成 `~/.claude/projects/<slug>/*.jsonl` 的会话记录。
//!
//! 目录 slug 的发现/解析不是这个 crate 的职责(架构文档 §5),`commit` 只管把
//! `LoweredSession` 写进调用方已经解析好的 `Dest::session_dir`。

pub mod commit;
pub mod legalize;
pub mod lower;

use ctxrelay_backend::{
    Backend, CapPolicy, Dest, LoweredSession, LoweringReport, Manifest, Result, TargetSpec,
};
use ctxrelay_ir::Document;

pub struct ClaudeCodeBackend;

impl Backend for ClaudeCodeBackend {
    fn target(&self) -> TargetSpec {
        TargetSpec {
            tool: "claude-code".to_string(),
            // 经验证的最低版本:本计划的 --resume 兼容性验证跑在 2.1.201 上;
            // 未标注上界,因为架构文档明确 JSONL schema 会无预兆变化,届时应该
            // 拆出一个新的"claude-code vX backend",而不是在这里加 if。
            version_range: ">=2.1.0".to_string(),
        }
    }

    fn required_caps(&self) -> CapPolicy {
        CapPolicy {
            // IR 目前没有字段能装下真实的 thinking signature 字节,统一按不可信处理
            // (详见 legalize.rs 的注释)。
            accepts_verifiable_reasoning: false,
        }
    }

    fn legalize(&self, doc: &Document) -> (Document, LoweringReport) {
        legalize::legalize(doc)
    }

    fn lower(&self, doc: &Document) -> Result<LoweredSession> {
        lower::lower(doc)
    }

    fn commit(&self, lowered: LoweredSession, dest: &Dest, report: LoweringReport) -> Result<Manifest> {
        commit::commit(lowered, dest, self.target(), report)
    }
}
