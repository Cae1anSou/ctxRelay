use ctxrelay_backend::LoweringReport;
use ctxrelay_ir::{Block, Document, Origin, Role, Turn, TurnId};

/// 把 IR 合法化成 Claude Code 能接受的形状:
///
/// - `Reasoning` 目前一律没有可信签名(`caps.verifiable_signature` 恒为 false——
///   IR 当前没有字段能装下真实的 thinking signature 字节),无法安全重建一个能
///   通过 Claude API 校验的 thinking block,强行塞一个自造的 signature 只会触发
///   `400 Invalid signature in thinking block`。但这不代表内容要被销毁:跟
///   `ForeignAction` 同样的降级策略,内联成 `Text`(加一个 `[Thinking]` 前缀标出
///   身份),思考内容原样保留,只是失去"真正的 thinking block"这个语义身份。
///   如果未来某个 frontend 真的能提供 `verifiable_signature: true` 的 Reasoning,
///   这里需要新增一个分支把它写成目标原生 thinking block,而不是复用这条内联
///   路径。
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
                Block::Reasoning { content, caps } => {
                    if caps.verifiable_signature {
                        // 目前没有任何 frontend 会产出这个组合(IR 无法承载真实签名字节),
                        // 一旦出现,需要专门实现"写成目标原生 thinking block"而不是静默走
                        // 内联降级这条路,所以先明确拒绝而不是悄悄按不可信处理。
                        panic!(
                            "legalize() received Reasoning with verifiable_signature=true, \
                             but no code path writes real thinking blocks yet"
                        );
                    }
                    blocks.push(Block::Text { content: format!("[Thinking]\n{content}") });
                    report.inlined_reasoning += 1;
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
