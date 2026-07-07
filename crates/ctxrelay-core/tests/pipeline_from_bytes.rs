use ctxrelay_core::{run_import_from_bytes, ImportOptions, Registry};
use std::path::PathBuf;

const FIXTURE: &str = "../fe-claude-live/tests/fixtures/sample_live_conversation.json";

#[test]
fn run_import_from_bytes_commits_a_live_capture() {
    let registry = Registry::with_defaults();
    let raw = std::fs::read(FIXTURE).expect("fixture must exist");

    let project_dir = std::env::temp_dir().join("ctxrelay-live-pipeline-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-live-pipeline-test-projects-root");
    let _ = std::fs::remove_dir_all(&projects_root);
    let slug = canonical.display().to_string().replace('/', "-");
    std::fs::create_dir_all(projects_root.join(&slug)).unwrap();

    let opts = ImportOptions {
        backend_name: "claude-code".to_string(),
        project_dir: project_dir.clone(),
        dry_run: false,
        allow_bootstrap: false,
        claude_projects_root: projects_root.clone(),
        cli_version: "2.1.201".to_string(),
    };

    let manifest =
        run_import_from_bytes(&registry, raw, "fe-claude-live", opts).expect("import should succeed");

    assert_eq!(manifest.writes.len(), 1);
    assert!(manifest.writes[0].path.exists());

    let content = std::fs::read_to_string(&manifest.writes[0].path).unwrap();
    // thinking 应该已经被 legalize 丢弃,不应该出现在最终写盘的 JSONL 里。
    assert!(!content.contains("\"thinking\""));
    // preamble + 4 条真实轮次。
    assert_eq!(content.lines().count(), 5);

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}

#[test]
fn run_import_from_bytes_errors_on_unknown_frontend_id() {
    let registry = Registry::with_defaults();
    let raw = std::fs::read(FIXTURE).expect("fixture must exist");

    let opts = ImportOptions {
        backend_name: "claude-code".to_string(),
        project_dir: PathBuf::from("/tmp/irrelevant"),
        dry_run: true,
        allow_bootstrap: false,
        claude_projects_root: PathBuf::from("/tmp/irrelevant"),
        cli_version: "2.1.201".to_string(),
    };

    let result = run_import_from_bytes(&registry, raw, "fe-not-registered", opts);
    assert!(result.is_err());
}
