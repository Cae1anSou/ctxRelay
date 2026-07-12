use be_claude_code::commit::commit;
use be_claude_code::legalize::legalize;
use be_claude_code::lower::lower;
use ctxrelay_backend::{document_digest, Dest, TargetSpec};
use ctxrelay_ir::{
    Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId,
};
use semver::Version;
use std::path::PathBuf;

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

/// 离线串联 legalize → lower → commit 三段式,不碰真实 `claude` CLI,不花钱,随
/// `cargo test --workspace` 正常跑。验证的是"接线对不对"这件事:
/// - `legalize` 产出的 `LoweringReport` 真的原样出现在最终 `Manifest.report` 里
///   (不是巧合地被 `Default` 值糊弄过去——这份文档确实有 1 条 Reasoning 被内联、
///   1 条 ForeignAction 被内联,如果穿线逻辑被改坏,这两个计数会变成 0)。
/// - `Manifest.ir_digest` 等于对**原始**(legalize 之前)`Document` 算出的摘要,
///   不是对 legalize 之后的版本算的(两者内容不同,算出来的哈希必然不同,足以
///   区分接线是否正确)。
#[test]
fn legalize_lower_commit_thread_report_and_ir_digest_correctly() {
    let doc = sample_document();
    let ir_digest = document_digest(&doc);

    let (legalized, report) = legalize(&doc).expect("legalize should succeed");
    assert_eq!(report.inlined_reasoning, 1);
    assert_eq!(report.inlined_foreign_actions, 1);

    let lowered = lower(&legalized).expect("lower should succeed");
    let session_id = lowered.session_id.clone();

    let scratch = std::env::temp_dir().join(format!("ctxrelay-pipeline-test-{session_id}"));
    let _ = std::fs::remove_dir_all(&scratch);

    let dest = Dest {
        session_dir: scratch.clone(),
        cwd: PathBuf::from("/tmp/some-project"),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };

    let manifest = commit(
        lowered,
        &dest,
        TargetSpec {
            tool: "claude-code".to_string(),
            version_range: ">=2.1.0".to_string(),
        },
        report.clone(),
        ir_digest.clone(),
    )
    .expect("commit should succeed");

    assert_eq!(manifest.report, report);
    assert_eq!(manifest.ir_digest, ir_digest);

    // 反证:对 legalize 之后的 Document 求摘要,结果应该跟原始摘要不同——
    // 这确认了 ir_digest 真的是对原始 Document 算的,不是凑巧撞上的。
    let legalized_digest = document_digest(&legalized);
    assert_ne!(
        manifest.ir_digest, legalized_digest,
        "ir_digest should reference the original Document, not the legalized one"
    );

    std::fs::remove_dir_all(&scratch).ok();
}
