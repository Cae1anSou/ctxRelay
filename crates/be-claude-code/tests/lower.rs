use be_claude_code::lower::lower;
use ctxrelay_ir::{Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;

fn legalized_document() -> Document {
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
                    content: "暗号是紫色的长颈鹿".to_string(),
                }],
                timestamp: None,
            },
            Turn {
                id: TurnId("t2".to_string()),
                role: Role::Assistant,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: Some("claude-sonnet-5".to_string()),
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![Block::Text {
                    content: "记住了。".to_string(),
                }],
                timestamp: None,
            },
        ],
    }
}

#[test]
fn lowers_turns_into_chained_jsonl_records() {
    let doc = legalized_document();
    let lowered = lower(&doc).expect("lower should succeed");

    assert_eq!(lowered.lines.len(), 2);

    let first = &lowered.lines[0];
    assert_eq!(first["type"], "user");
    assert_eq!(first["parentUuid"], serde_json::Value::Null);
    assert_eq!(first["message"]["role"], "user");
    assert_eq!(first["message"]["content"][0]["type"], "text");
    assert_eq!(first["message"]["content"][0]["text"], "暗号是紫色的长颈鹿");

    let second = &lowered.lines[1];
    assert_eq!(second["type"], "assistant");
    assert_eq!(second["parentUuid"], first["uuid"]);
    assert_eq!(second["message"]["role"], "assistant");
    assert_eq!(second["message"]["content"][0]["text"], "记住了。");
}

#[test]
fn lower_is_deterministic() {
    let doc = legalized_document();
    let a = lower(&doc).expect("lower should succeed");
    let b = lower(&doc).expect("lower should succeed");

    assert_eq!(a.session_id, b.session_id);
    assert_eq!(a.lines, b.lines);
}

/// `lower` 的调用约定是"只接受 legalize 之后的 Document"(Reasoning/ForeignAction
/// 都应该已经被内联成 Text)。这个约定不是编译期强制的,曾经违反它会直接 panic
/// 崩掉进程;现在改成走 `Result::Err`,由调用方（`ctxrelay-core::commit_document`)
/// 正常传播,不会在 `ctxrelay listen` 这类"响应必须被发出"的路径上裸崩。
#[test]
fn errors_instead_of_panicking_on_un_legalized_block() {
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
                model: None,
                surface: "claude.ai".to_string(),
            },
            blocks: vec![Block::Reasoning {
                content: "没有经过 legalize 的思考".to_string(),
                caps: BlockCaps {
                    reasoning: true,
                    verifiable_signature: false,
                    replayable: false,
                },
            }],
            timestamp: None,
        }],
    };

    let result = lower(&doc);
    assert!(result.is_err());
}
