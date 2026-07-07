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

/// 回归测试:`current_leaf_message_uuid` 之外的"孤儿"分支(比如用户重新生成过
/// 回答、放弃的旧回复)不应该出现在最终的 `Document` 里——树回溯逻辑必须真的沿
/// `parent_message_uuid` 走,不能只是把 `chat_messages` 数组原样排过去。这里手工
/// 构造一个带分支的最小样例:t1 -> t2(被选中的分支)和 t1 -> t2b(被放弃的分支,
/// 不在 current_leaf 的链上)。
#[test]
fn excludes_abandoned_branches_not_reachable_from_current_leaf() {
    let raw = r#"
    {
      "model": "claude-sonnet-5",
      "current_leaf_message_uuid": "t2",
      "chat_messages": [
        {
          "uuid": "t1",
          "content": [{ "type": "text", "text": "问题" }],
          "sender": "human",
          "created_at": "2026-01-01T00:00:00Z",
          "parent_message_uuid": "00000000-0000-4000-8000-000000000000"
        },
        {
          "uuid": "t2b",
          "content": [{ "type": "text", "text": "被放弃的旧回答" }],
          "sender": "assistant",
          "created_at": "2026-01-01T00:00:01Z",
          "parent_message_uuid": "t1"
        },
        {
          "uuid": "t2",
          "content": [{ "type": "text", "text": "被选中的回答" }],
          "sender": "assistant",
          "created_at": "2026-01-01T00:00:02Z",
          "parent_message_uuid": "t1"
        }
      ]
    }
    "#;

    let doc = ClaudeLiveParse.parse(raw.as_bytes().to_vec()).expect("should parse branched conversation");

    assert_eq!(doc.turns.len(), 2, "abandoned branch t2b must be excluded");
    assert_eq!(doc.turns[0].id, TurnId("t1".to_string()));
    assert_eq!(doc.turns[1].id, TurnId("t2".to_string()));
    match &doc.turns[1].blocks[0] {
        Block::Text { content } => assert_eq!(content, "被选中的回答"),
        other => panic!("expected Text block, got {other:?}"),
    }
}

/// 回归测试:未识别的 content block 类型(比如 tool_use)归一成 ForeignAction 时,
/// 完整原始 JSON 必须原样保留在 artifact 里——和 fe-claude-share 已经验证过的同一
/// 条不变量。
#[test]
fn foreign_action_preserves_raw_content_for_unrecognized_block_type() {
    let raw = r#"
    {
      "model": "claude-sonnet-5",
      "current_leaf_message_uuid": "t1",
      "chat_messages": [
        {
          "uuid": "t1",
          "content": [
            { "type": "text", "text": "before tool call" },
            { "type": "tool_use", "id": "tool_1", "name": "web_search", "input": { "query": "rust hashmap" } }
          ],
          "sender": "assistant",
          "created_at": "2026-01-01T00:00:00Z",
          "parent_message_uuid": "00000000-0000-4000-8000-000000000000"
        }
      ]
    }
    "#;

    let doc = ClaudeLiveParse.parse(raw.as_bytes().to_vec()).expect("should parse conversation with tool_use");

    assert_eq!(doc.turns.len(), 1);
    assert_eq!(doc.turns[0].blocks.len(), 2);

    match &doc.turns[0].blocks[1] {
        Block::ForeignAction { kind, artifact, caps, .. } => {
            assert_eq!(kind, "tool_use");
            assert!(!caps.replayable);
            let artifact = artifact.as_ref().expect("artifact should be present, not a hollow shell");
            assert!(artifact.content.contains("web_search"));
            assert!(artifact.content.contains("rust hashmap"));
        }
        other => panic!("expected ForeignAction block, got {other:?}"),
    }
}
