use be_claude_code::legalize::legalize;
use be_claude_code::lower::lower;
use ctxrelay_ir::{
    Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId,
};
use proptest::prelude::*;
use semver::Version;

/// 补上 `ctxrelay-ir/tests/roundtrip.rs` 那条测试改名之后留下的空白:那条测试只验证
/// `Document` 自身的 serde 往返,从未真正跑过任何 backend 的 `lower()`,`lower()` 的
/// 正确性此前只靠几条手写样例(`tests/lower.rs`/`tests/legalize.rs`)覆盖,不是 fuzz
/// 级别的保障。
///
/// 这条测试跑真实的 `legalize → lower` 管线(不是重新实现一份平行逻辑去自证),对
/// 任意生成的 `Document` 断言:每个 block 的人类可读内容,最终都能在对应 turn 产出
/// 的 JSONL `message.content[].text` 里找到。这仍然不是架构文档 §9 承诺的完整
/// "lower → parse 往返"(那需要一个"claude-code 原生格式 → IR"的反向 parser,目前
/// 不存在,`lower` 是单向的)——这里验证的是单向 content-effect 保真:内容没有在
/// `lower` 这一跳丢失或被截断,不代表可以从输出反推回原始 IR。
fn arb_text() -> impl Strategy<Value = String> {
    "[\\PC]{0,40}"
}

fn arb_artifact() -> impl Strategy<Value = Artifact> {
    ("[a-z/]{3,15}", arb_text()).prop_map(|(media, content)| Artifact { media, content })
}

fn arb_block() -> impl Strategy<Value = Block> {
    prop_oneof![
        arb_text().prop_map(|content| Block::Text { content }),
        (proptest::option::of("[a-z]{1,8}"), arb_text())
            .prop_map(|(language, content)| Block::Code { language, content }),
        (
            "[a-z_]{3,15}",
            proptest::option::of(arb_text()),
            proptest::option::of(arb_artifact()),
        )
            .prop_map(|(kind, summary, artifact)| {
                // reasoning/verifiable_signature 恒 false:这条测试关心的是内容保真,
                // `verifiable_signature: true` 那条"应该报错而不是 panic"的行为已经由
                // `tests/legalize.rs::rejects_verifiable_signature_with_error_instead_of_panicking`
                // 单独覆盖,混进这里只会让 fuzz 的大部分样本都在测同一件事。
                Block::foreign_action(kind, summary, artifact, false, false)
            }),
        arb_text().prop_map(|content| Block::Reasoning {
            content,
            caps: BlockCaps {
                reasoning: true,
                verifiable_signature: false,
                replayable: false,
            },
        }),
    ]
}

fn arb_role() -> impl Strategy<Value = Role> {
    prop_oneof![Just(Role::User), Just(Role::Assistant)]
}

fn arb_turn() -> impl Strategy<Value = Turn> {
    // turn id 只用于派生 lower() 里的消息 UUID,不影响这条测试关心的东西(内容是否
    // 保真)——不需要跨 turn 唯一,随机字符串就够,不必像 arb_document 那样按位置
    // 生成序号。
    (
        "[a-zA-Z0-9]{1,10}",
        arb_role(),
        proptest::collection::vec(arb_block(), 1..4),
    )
        .prop_map(|(id, role, blocks)| Turn {
            id: TurnId(id),
            role,
            origin: Origin {
                vendor: "anthropic".to_string(),
                model: Some("claude-sonnet-5".to_string()),
                surface: "claude.ai".to_string(),
            },
            blocks,
            timestamp: None,
        })
}

fn arb_document() -> impl Strategy<Value = Document> {
    proptest::collection::vec(arb_turn(), 1..5).prop_map(|turns| Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns,
    })
}

proptest! {
    #[test]
    fn lower_preserves_every_block_content(doc in arb_document()) {
        let (legalized, _report) = legalize(&doc).expect("legalize should succeed");
        let lowered = lower(&legalized).expect("lower should succeed");

        // legalized.turns[0] 是 legalize 合成的 preamble,legalized.turns[i+1] 对应
        // doc.turns[i];lower 按顺序 1:1 把 turn 转成 JSONL 行,顺序不变。
        prop_assert_eq!(lowered.lines.len(), doc.turns.len() + 1);

        for (i, turn) in doc.turns.iter().enumerate() {
            let line = &lowered.lines[i + 1];
            let content = line["message"]["content"]
                .as_array()
                .expect("message.content must be an array");
            let turn_text: String = content
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");

            for block in &turn.blocks {
                match block {
                    Block::Text { content } => {
                        prop_assert!(turn_text.contains(content.as_str()))
                    }
                    Block::Code { content, .. } => {
                        prop_assert!(turn_text.contains(content.as_str()))
                    }
                    Block::Reasoning { content, .. } => {
                        prop_assert!(turn_text.contains(content.as_str()))
                    }
                    Block::ForeignAction {
                        kind,
                        summary,
                        artifact,
                        ..
                    } => {
                        prop_assert!(turn_text.contains(kind.as_str()));
                        if let Some(summary) = summary {
                            prop_assert!(turn_text.contains(summary.as_str()));
                        }
                        if let Some(artifact) = artifact {
                            prop_assert!(turn_text.contains(artifact.content.as_str()));
                        }
                    }
                }
            }
        }
    }
}
