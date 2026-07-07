use ctxrelay_frontend::Parse;
use ctxrelay_ir::{Block, BlockCaps, Role, TurnId};
use fe_claude_live::ClaudeLiveParse;
use semver::Version;

#[test]
fn parses_real_authenticated_conversation_with_thinking() {
    let raw = std::fs::read("tests/fixtures/sample_live_conversation.json")
        .expect("fixture must exist");

    let doc = ClaudeLiveParse.parse(raw).expect("should parse real live conversation");

    assert_eq!(doc.ir_version, Version::new(0, 1, 0));
    assert_eq!(doc.source.vendor, "anthropic");
    assert_eq!(doc.source.surface, "claude.ai");

    assert_eq!(doc.turns.len(), 4);

    assert_eq!(doc.turns[0].role, Role::User);
    assert_eq!(doc.turns[0].origin.model, None);
    match &doc.turns[0].blocks[0] {
        Block::Text { content } => assert!(content.starts_with("我想做做一个科研copilot")),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(doc.turns[1].role, Role::Assistant);
    assert_eq!(doc.turns[1].origin.model, Some("claude-opus-4-8".to_string()));
    assert_eq!(doc.turns[1].blocks.len(), 2);
    match &doc.turns[1].blocks[0] {
        Block::Reasoning { content, caps } => {
            assert!(content.starts_with("Jennifer's looking to build"));
            assert_eq!(
                *caps,
                BlockCaps { reasoning: true, verifiable_signature: false, replayable: false }
            );
        }
        other => panic!("expected Reasoning block, got {other:?}"),
    }
    match &doc.turns[1].blocks[1] {
        Block::Text { content } => assert!(content.starts_with("这个想法很好")),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(doc.turns[3].id, TurnId("019f2dbc-c314-7bbc-b6c6-b11f84c1ccb1".to_string()));
}
