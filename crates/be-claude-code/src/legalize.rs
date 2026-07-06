use ctxrelay_backend::LoweringReport;
use ctxrelay_ir::{Block, Document, Origin, Role, Turn, TurnId};

/// 把 IR 合法化成 Claude Code 能接受的形状:
///
/// - `Reasoning` 一律丢弃,不管 `caps.verifiable_signature` 是否为 true——IR 当前
///   没有字段能装下真实的 thinking signature 字节,即使某个 Reasoning 标记为"可
///   验证",也无法安全重建一个能通过 Claude API 校验的 thinking block,强行塞一个
///   自造的 signature 只会触发 `400 Invalid signature in thinking block`。这是
///   当前 IR 的已知限制,不是这里的疏漏。
/// - `ForeignAction` 内联成 `Text`,内容(kind/summary/artifact)一字不丢,只剥掉
///   工具外壳。
/// - 在最前面插入一条 preamble turn,交代这是从 Web 对话迁移的讨论。
pub fn legalize(doc: &Document) -> (Document, LoweringReport) {
    let mut report = LoweringReport::default();
    let mut turns = Vec::with_capacity(doc.turns.len() + 1);

    turns.push(preamble_turn(doc));

    for turn in &doc.turns {
        let mut blocks = Vec::with_capacity(turn.blocks.len());
        for block in &turn.blocks {
            match block {
                Block::Reasoning { .. } => {
                    report.dropped_reasoning += 1;
                }
                Block::ForeignAction { kind, summary, artifact, .. } => {
                    let mut text = format!("[外部操作: {kind}]");
                    if let Some(summary) = summary {
                        text.push('\n');
                        text.push_str(summary);
                    }
                    if let Some(artifact) = artifact {
                        text.push('\n');
                        text.push_str(&artifact.content);
                    }
                    blocks.push(Block::Text { content: text });
                    report.inlined_foreign_actions += 1;
                }
                other => blocks.push(other.clone()),
            }
        }
        turns.push(Turn {
            id: turn.id.clone(),
            role: turn.role,
            origin: turn.origin.clone(),
            blocks,
            timestamp: turn.timestamp,
        });
    }

    report
        .notes
        .push("已在最前插入 preamble turn,说明这是从 Web 对话导入的讨论".to_string());

    let legalized = Document {
        ir_version: doc.ir_version.clone(),
        source: doc.source.clone(),
        turns,
    };

    (legalized, report)
}

fn preamble_turn(doc: &Document) -> Turn {
    let text = format!(
        "以下为从 {} ({}) 导入的讨论,工具调用已内联为文本,从此处继续。",
        doc.source.vendor, doc.source.surface
    );
    Turn {
        id: TurnId("preamble".to_string()),
        role: Role::User,
        origin: Origin {
            vendor: doc.source.vendor.clone(),
            model: None,
            surface: doc.source.surface.clone(),
        },
        blocks: vec![Block::Text { content: text }],
        timestamp: None,
    }
}
