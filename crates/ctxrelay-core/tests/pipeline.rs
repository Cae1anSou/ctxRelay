use ctxrelay_core::{run_import, run_ir, ImportOptions, Registry};
use ctxrelay_frontend::SourceRef;
use std::path::PathBuf;

const FIXTURE: &str = "../fe-claude-share/tests/fixtures/sample_snapshot.json";

#[test]
fn run_ir_parses_real_fixture_into_document() {
    let registry = Registry::with_defaults();
    let source = SourceRef::File(PathBuf::from(FIXTURE));

    let doc = run_ir(&registry, source).expect("should parse real fixture");

    assert_eq!(doc.turns.len(), 4);
}

#[test]
fn run_import_dry_run_produces_manifest_without_writing_anything() {
    let registry = Registry::with_defaults();
    let source = SourceRef::File(PathBuf::from(FIXTURE));

    let opts = ImportOptions {
        backend_name: "claude-code".to_string(),
        project_dir: PathBuf::from("/tmp/some-project"),
        dry_run: true,
        allow_bootstrap: false,
        claude_projects_root: PathBuf::from("/tmp/irrelevant-for-dry-run"),
        cli_version: "2.1.201".to_string(),
    };

    let manifest = run_import(&registry, source, opts).expect("dry run should succeed");

    assert!(manifest.writes.is_empty());
    assert_eq!(manifest.created_session_ids.len(), 1);
    assert!(manifest.report.notes.iter().any(|n| n.contains("preamble")));
}

#[test]
fn run_import_commits_when_session_dir_already_exists() {
    let registry = Registry::with_defaults();
    let source = SourceRef::File(PathBuf::from(FIXTURE));

    let project_dir = std::env::temp_dir().join("ctxrelay-pipeline-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-pipeline-test-projects-root");
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

    let manifest = run_import(&registry, source, opts).expect("import should succeed");

    assert_eq!(manifest.writes.len(), 1);
    assert!(manifest.writes[0].path.exists());
    assert_eq!(manifest.cwd, canonical);

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}
