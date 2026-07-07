use ctxrelay_ir::{
    Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId,
};
use proptest::prelude::*;
use semver::Version;

fn arb_caps() -> impl Strategy<Value = BlockCaps> {
    (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
        |(reasoning, verifiable_signature, replayable)| BlockCaps {
            reasoning,
            verifiable_signature,
            replayable,
        },
    )
}

fn arb_artifact() -> impl Strategy<Value = Artifact> {
    ("[a-z/]{3,15}", "[\\PC]{0,40}").prop_map(|(media, content)| Artifact { media, content })
}

fn arb_block() -> impl Strategy<Value = Block> {
    prop_oneof![
        "[\\PC]{0,60}".prop_map(|content| Block::Text { content }),
        (proptest::option::of("[a-z]{1,10}"), "[\\PC]{0,60}")
            .prop_map(|(language, content)| Block::Code { language, content }),
        (
            "[a-z_]{3,15}",
            proptest::option::of("[\\PC]{0,40}"),
            proptest::option::of(arb_artifact()),
            any::<bool>(),
            any::<bool>(),
        )
            .prop_map(
                |(kind, summary, artifact, reasoning, verifiable_signature)| {
                    Block::foreign_action(kind, summary, artifact, reasoning, verifiable_signature)
                }
            ),
        ("[\\PC]{0,60}", arb_caps()).prop_map(|(content, caps)| Block::Reasoning { content, caps }),
    ]
}

fn arb_role() -> impl Strategy<Value = Role> {
    prop_oneof![Just(Role::User), Just(Role::Assistant), Just(Role::System)]
}

fn arb_origin() -> impl Strategy<Value = Origin> {
    (
        "[a-z]{3,10}",
        proptest::option::of("[a-z0-9.-]{3,15}"),
        "[a-z.]{3,15}",
    )
        .prop_map(|(vendor, model, surface)| Origin {
            vendor,
            model,
            surface,
        })
}

fn arb_turn() -> impl Strategy<Value = Turn> {
    (
        "[a-zA-Z0-9]{1,10}",
        arb_role(),
        arb_origin(),
        proptest::collection::vec(arb_block(), 0..4),
    )
        .prop_map(|(id, role, origin, blocks)| Turn {
            id: TurnId(id),
            role,
            origin,
            blocks,
            timestamp: None,
        })
}

fn arb_document() -> impl Strategy<Value = Document> {
    (
        "[a-z]{3,10}",
        "[a-z.]{3,15}",
        proptest::collection::vec(arb_turn(), 0..5),
    )
        .prop_map(|(vendor, surface, turns)| Document {
            ir_version: Version::new(0, 1, 0),
            source: SourceProvenance {
                vendor,
                surface,
                exported_at: None,
            },
            turns,
        })
}

proptest! {
    #[test]
    fn roundtrip_preserves_content_effect(doc in arb_document()) {
        let json = serde_json::to_string(&doc).expect("serialize");
        let parsed: Document = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(doc, parsed);
    }

    /// 架构文档 §3.2 契约:ForeignAction 恒不可回放。
    /// `Block::foreign_action` 不接受 `replayable` 参数,这条测试确认无论
    /// `reasoning`/`verifiable_signature` 如何组合,构造出的 caps.replayable 恒为 false。
    #[test]
    fn foreign_action_is_never_replayable(
        kind in "[a-z_]{3,15}",
        reasoning in any::<bool>(),
        verifiable_signature in any::<bool>(),
    ) {
        let block = Block::foreign_action(kind, None, None, reasoning, verifiable_signature);
        match block {
            Block::ForeignAction { caps, .. } => prop_assert!(!caps.replayable),
            other => panic!("expected ForeignAction, got {other:?}"),
        }
    }
}
