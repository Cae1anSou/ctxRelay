use be_claude_code::lower::lower;
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
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
                blocks: vec![Block::Text { content: "暗号是紫色的长颈鹿".to_string() }],
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
                blocks: vec![Block::Text { content: "记住了。".to_string() }],
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
