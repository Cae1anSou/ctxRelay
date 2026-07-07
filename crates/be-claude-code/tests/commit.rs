use be_claude_code::commit::commit;
use be_claude_code::lower::lower;
use ctxrelay_backend::{Dest, LoweringReport, TargetSpec};
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use sha2::Digest;
use std::path::PathBuf;

fn legalized_document() -> Document {
    Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![Turn {
            id: TurnId("t1".to_string()),
            role: Role::User,
            origin: Origin {
                vendor: "anthropic".to_string(),
                model: None,
                surface: "claude.ai".to_string(),
            },
            blocks: vec![Block::Text {
                content: "hello".to_string(),
            }],
            timestamp: None,
        }],
    }
}

#[test]
fn writes_jsonl_file_and_manifest() {
    let doc = legalized_document();
    let lowered = lower(&doc).expect("lower should succeed");
    let session_id = lowered.session_id.clone();

    let scratch = std::env::temp_dir().join(format!("ctxrelay-commit-test-{session_id}"));
    let _ = std::fs::remove_dir_all(&scratch);

    let dest = Dest {
        session_dir: scratch.clone(),
        cwd: PathBuf::from("/tmp/some-project"),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };

    let ir_digest = ctxrelay_backend::document_digest(&doc);

    let manifest = commit(
        lowered,
        &dest,
        TargetSpec {
            tool: "claude-code".to_string(),
            version_range: ">=2.1.0".to_string(),
        },
        LoweringReport::default(),
        ir_digest.clone(),
    )
    .expect("commit should succeed");

    assert_eq!(manifest.ir_digest, ir_digest);
    assert_eq!(manifest.created_session_ids, vec![session_id.clone()]);
    assert_eq!(manifest.writes.len(), 1);

    let written_path = scratch.join(format!("{session_id}.jsonl"));
    assert_eq!(manifest.writes[0].path, written_path);

    let content = std::fs::read_to_string(&written_path).expect("file should exist");
    let first: serde_json::Value =
        serde_json::from_str(content.lines().next().expect("first line")).unwrap();
    assert_eq!(first["sessionId"], session_id);
    assert_eq!(first["cwd"], "/tmp/some-project");
    assert_eq!(first["gitBranch"], "main");
    assert_eq!(first["version"], "2.1.201");
    assert_eq!(first["userType"], "external");

    let expected_sha256 = format!("{:x}", sha2::Sha256::digest(content.as_bytes()));
    assert_eq!(manifest.writes[0].sha256, expected_sha256);

    std::fs::remove_dir_all(&scratch).ok();
}

#[test]
fn refuses_to_overwrite_existing_session_file() {
    let doc = legalized_document();
    let session_id = lower(&doc).expect("lower should succeed").session_id;

    let scratch = std::env::temp_dir().join(format!("ctxrelay-commit-overwrite-test-{session_id}"));
    let _ = std::fs::remove_dir_all(&scratch);

    let dest = Dest {
        session_dir: scratch.clone(),
        cwd: PathBuf::from("/tmp/some-project"),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };
    let target = TargetSpec {
        tool: "claude-code".to_string(),
        version_range: ">=2.1.0".to_string(),
    };
    let ir_digest = ctxrelay_backend::document_digest(&doc);

    // 第一次 commit 应该成功。
    commit(
        lower(&doc).expect("lower should succeed"),
        &dest,
        target.clone(),
        LoweringReport::default(),
        ir_digest.clone(),
    )
    .expect("first commit should succeed");

    // 第二次对同一份文档(同样的 session_id,因此同样的文件路径)commit,应该被拒绝,
    // 而不是静默覆盖——这份文件此刻可能已经因为用户在 Claude Code 里真实继续对话
    // 而增长过。
    let second = commit(
        lower(&doc).expect("lower should succeed"),
        &dest,
        target,
        LoweringReport::default(),
        ir_digest,
    );

    assert!(
        second.is_err(),
        "second commit to the same session file should be refused"
    );

    std::fs::remove_dir_all(&scratch).ok();
}
