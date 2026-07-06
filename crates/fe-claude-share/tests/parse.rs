use ctxrelay_ir::{Block, Role, TurnId};
use ctxrelay_frontend::Parse;
use fe_claude_share::ClaudeShareParse;
use semver::Version;

#[test]
fn parses_real_claude_share_snapshot() {
    let raw = std::fs::read("tests/fixtures/sample_snapshot.json").expect("fixture must exist");

    let doc = ClaudeShareParse.parse(raw).expect("should parse real snapshot");

    assert_eq!(doc.ir_version, Version::new(0, 1, 0));
    assert_eq!(doc.source.vendor, "anthropic");
    assert_eq!(doc.source.surface, "claude.ai");
    assert_eq!(doc.source.exported_at, None);

    assert_eq!(doc.turns.len(), 4);

    assert_eq!(doc.turns[0].id, TurnId("019f2a73-bc00-7057-a549-98a974fc8677".to_string()));
    assert_eq!(doc.turns[0].role, Role::User);
    assert_eq!(doc.turns[0].origin.vendor, "anthropic");
    assert_eq!(doc.turns[0].origin.surface, "claude.ai");
    assert_eq!(doc.turns[0].origin.model, None);
    assert_eq!(doc.turns[0].blocks.len(), 1);
    match &doc.turns[0].blocks[0] {
        Block::Text { content } => assert!(content.starts_with("我想做做一个科研copilot")),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(doc.turns[1].role, Role::Assistant);
    assert_eq!(doc.turns[2].role, Role::User);
    assert_eq!(doc.turns[3].role, Role::Assistant);

    match &doc.turns[3].blocks[0] {
        Block::Text { content } => assert!(content.starts_with("先把那个\"MCP 套 MCP\"的顾虑拆掉")),
        other => panic!("expected Text block, got {other:?}"),
    }
}
