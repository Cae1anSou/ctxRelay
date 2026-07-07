use be_claude_code::legalize::legalize;
use ctxrelay_ir::{
    Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId,
};
use semver::Version;

fn sample_document() -> Document {
    Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![
            Turn {
                id: TurnId("t1".to_string()),
                role: Role::User,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: None,
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![Block::Text {
                    content: "你好".to_string(),
                }],
                timestamp: None,
            },
            Turn {
                id: TurnId("t2".to_string()),
                role: Role::Assistant,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: Some("opus-4".to_string()),
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![
                    Block::Reasoning {
                        content: "内部推理过程".to_string(),
                        caps: BlockCaps {
                            reasoning: true,
                            verifiable_signature: false,
                            replayable: false,
                        },
                    },
                    Block::foreign_action(
                        "web_search",
                        Some("搜索了 rust uuid v5".to_string()),
                        Some(Artifact {
                            media: "application/json".to_string(),
                            content: "{\"query\":\"rust uuid v5\"}".to_string(),
                        }),
                        false,
                        false,
                    ),
                    Block::Text {
                        content: "根据搜索结果...".to_string(),
                    },
                ],
                timestamp: None,
            },
        ],
    }
}

#[test]
fn inlines_reasoning_and_foreign_action_as_text() {
    let doc = sample_document();
    let (legalized, report) = legalize(&doc);

    assert_eq!(report.inlined_reasoning, 1);
    assert_eq!(report.inlined_foreign_actions, 1);

    // turns[0] 是合成的 preamble,原始两轮各自往后挪一位
    assert_eq!(legalized.turns.len(), 3);

    match &legalized.turns[0].blocks[0] {
        Block::Text { content } => {
            assert!(content.contains("anthropic") && content.contains("claude.ai"))
        }
        other => panic!("expected preamble Text block, got {other:?}"),
    }

    assert_eq!(legalized.turns[1].id, TurnId("t1".to_string()));
    match &legalized.turns[1].blocks[0] {
        Block::Text { content } => assert_eq!(content, "你好"),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(legalized.turns[2].id, TurnId("t2".to_string()));
    assert_eq!(legalized.turns[2].blocks.len(), 3);
    match &legalized.turns[2].blocks[0] {
        Block::Text { content } => {
            assert!(content.contains("[Thinking]"));
            assert!(content.contains("内部推理过程"));
        }
        other => panic!("expected inlined Reasoning as Text, got {other:?}"),
    }
    match &legalized.turns[2].blocks[1] {
        Block::Text { content } => {
            assert!(content.contains("web_search"));
            assert!(content.contains("rust uuid v5"));
        }
        other => panic!("expected inlined ForeignAction as Text, got {other:?}"),
    }
    match &legalized.turns[2].blocks[2] {
        Block::Text { content } => assert_eq!(content, "根据搜索结果..."),
        other => panic!("expected Text block, got {other:?}"),
    }
}
