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
    let (legalized, report) = legalize(&doc).expect("legalize should succeed");

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

/// `caps.verifiable_signature: true` 是 IR 类型系统合法、property test 也会生成的
/// 取值组合(目前没有 frontend 产出它,但 legalize 不能假设"永远不会收到")。这里
/// 曾经是直接 `panic!`——crash 整个 `ctxrelay import` 进程,而不是像其他不支持的
/// 输入一样走 `Result::Err` 由 CLI 顶层正常报错退出。这条测试锁定"不支持的输入
/// 走错误路径,不是裸崩"这个行为。
#[test]
fn rejects_verifiable_signature_with_error_instead_of_panicking() {
    let doc = Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![Turn {
            id: TurnId("t1".to_string()),
            role: Role::Assistant,
            origin: Origin {
                vendor: "anthropic".to_string(),
                model: Some("opus-4".to_string()),
                surface: "claude.ai".to_string(),
            },
            blocks: vec![Block::Reasoning {
                content: "带真实签名的思考".to_string(),
                caps: BlockCaps {
                    reasoning: true,
                    verifiable_signature: true,
                    replayable: false,
                },
            }],
            timestamp: None,
        }],
    };

    let result = legalize(&doc);
    assert!(result.is_err());
}
