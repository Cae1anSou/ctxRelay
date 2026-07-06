use ctxrelay_backend::{LoweringReport, Manifest, TargetSpec};
use ctxrelay_core::verify::load_manifest;
use std::path::PathBuf;

#[test]
fn load_manifest_reads_back_what_was_written() {
    let scratch = std::env::temp_dir().join("ctxrelay-verify-load-test");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();

    let manifest = Manifest {
        ir_digest: "deadbeef".to_string(),
        target: TargetSpec { tool: "claude-code".to_string(), version_range: ">=2.1.0".to_string() },
        writes: vec![],
        created_session_ids: vec!["session-1".to_string()],
        report: LoweringReport::default(),
        cwd: PathBuf::from("/tmp/some-project"),
    };
    let manifest_path = scratch.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let loaded = load_manifest(&manifest_path).expect("should load manifest");

    assert_eq!(loaded, manifest);

    std::fs::remove_dir_all(&scratch).ok();
}

#[test]
fn load_manifest_errors_on_missing_file() {
    let missing = std::env::temp_dir().join("ctxrelay-verify-does-not-exist.json");
    let _ = std::fs::remove_file(&missing);

    let result = load_manifest(&missing);

    assert!(result.is_err());
}

/// 端到端验证 `run_verify` 真的能跑通 `claude --resume`。复用 be-claude-code 的
/// legalize→lower→commit 直接构造一个真实 manifest(跳过 core 自己的 Dest 引导,
/// 手动指定一个已存在的 session_dir,避免这条测试还要再花一次 bootstrap 的 API
/// 额度)。会真实调用 `claude` CLI,花费少量 API 额度,默认不随 `cargo test` 跑:
/// `cargo test -p ctxrelay-core --test verify -- --ignored`
#[test]
#[ignore]
fn run_verify_gets_a_real_response_from_claude() {
    use be_claude_code::{commit::commit, legalize::legalize, lower::lower, ClaudeCodeBackend};
    use ctxrelay_backend::{document_digest, Backend, Dest};
    use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
    use semver::Version;
    use std::path::PathBuf;

    let scratch_project = std::env::temp_dir().join("ctxrelay-core-verify-scratch-project");
    let _ = std::fs::remove_dir_all(&scratch_project);
    std::fs::create_dir_all(&scratch_project).expect("create scratch project dir");
    let scratch_project = scratch_project.canonicalize().expect("canonicalize scratch project dir");

    let doc = Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![Turn {
            id: TurnId("t1".to_string()),
            role: Role::User,
            origin: Origin { vendor: "anthropic".to_string(), model: None, surface: "claude.ai".to_string() },
            blocks: vec![Block::Text { content: "我们在聊 ctxRelay 项目的设计".to_string() }],
            timestamp: None,
        }],
    };

    let ir_digest = document_digest(&doc);
    let backend = ClaudeCodeBackend;
    let (legalized, report) = legalize(&doc);
    let lowered = lower(&legalized).expect("lower should succeed");

    let home = std::env::var("HOME").expect("HOME must be set");
    let slug = scratch_project.display().to_string().replace('/', "-");
    let session_dir = PathBuf::from(home).join(".claude/projects").join(&slug);
    let _ = std::fs::remove_dir_all(&session_dir);

    let dest = Dest {
        session_dir: session_dir.clone(),
        cwd: scratch_project.clone(),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };

    let manifest = commit(lowered, &dest, backend.target(), report, ir_digest).expect("commit should succeed");

    let manifest_path = scratch_project.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let summary = ctxrelay_core::run_verify(&manifest_path).expect("run_verify should succeed");
    assert!(!summary.trim().is_empty(), "expected a non-empty summary from claude");

    std::fs::remove_dir_all(&session_dir).ok();
    std::fs::remove_dir_all(&scratch_project).ok();
}
