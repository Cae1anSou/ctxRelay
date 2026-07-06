use ctxrelay_ir::{Block, Document, Role};

#[test]
fn parses_architecture_doc_example() {
    let raw = r#"
    {
      "ir_version": "0.1.0",
      "source": { "vendor": "anthropic", "surface": "claude.ai", "exported_at": null },
      "turns": [
        {
          "id": "t1",
          "role": "User",
          "origin": { "vendor": "anthropic", "model": null, "surface": "claude.ai" },
          "blocks": [ { "type": "Text", "content": "我们把这个 IR 迁移工具设计一下" } ],
          "timestamp": null
        }
      ]
    }
    "#;

    let doc: Document = serde_json::from_str(raw).expect("should parse");
    assert_eq!(doc.turns.len(), 1);
    assert_eq!(doc.turns[0].role, Role::User);
    match &doc.turns[0].blocks[0] {
        Block::Text { content } => assert_eq!(content, "我们把这个 IR 迁移工具设计一下"),
        other => panic!("expected Text block, got {other:?}"),
    }
}
