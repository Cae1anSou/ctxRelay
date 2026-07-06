use ctxrelay_backend::{LoweringReport, Manifest, TargetSpec, WriteRecord};
use ctxrelay_core::{run_import, run_undo, ImportOptions, Registry, UndoAction};
use ctxrelay_frontend::SourceRef;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const FIXTURE: &str = "../fe-claude-share/tests/fixtures/sample_snapshot.json";

fn write_file_and_record(path: PathBuf, content: &str) -> WriteRecord {
    std::fs::write(&path, content).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
    WriteRecord { path, sha256 }
}

fn sample_manifest(writes: Vec<WriteRecord>) -> Manifest {
    Manifest {
        ir_digest: "deadbeef".to_string(),
        target: TargetSpec { tool: "claude-code".to_string(), version_range: ">=2.1.0".to_string() },
        writes,
        created_session_ids: vec!["session-1".to_string()],
        report: LoweringReport::default(),
        cwd: PathBuf::from("/tmp/some-project"),
    }
}

#[test]
fn deletes_file_when_content_unchanged() {
    let scratch = std::env::temp_dir().join("ctxrelay-undo-test-unchanged");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();

    let path = scratch.join("session.jsonl");
    let record = write_file_and_record(path.clone(), "hello");
    let manifest = sample_manifest(vec![record]);
    let manifest_path = scratch.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let actions = run_undo(&manifest_path).expect("undo should succeed");

    assert_eq!(actions, vec![UndoAction::Deleted(path.clone())]);
    assert!(!path.exists());

    std::fs::remove_dir_all(&scratch).ok();
}

#[test]
fn skips_file_that_was_modified_since_commit() {
    let scratch = std::env::temp_dir().join("ctxrelay-undo-test-modified");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();

    let path = scratch.join("session.jsonl");
    let record = write_file_and_record(path.clone(), "original content");
    // 模拟用户在 commit 之后真的继续聊过,文件内容变了。
    std::fs::write(&path, "original content PLUS a real continued conversation").unwrap();

    let manifest = sample_manifest(vec![record]);
    let manifest_path = scratch.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let actions = run_undo(&manifest_path).expect("undo should succeed");

    assert_eq!(actions, vec![UndoAction::SkippedModified(path.clone())]);
    assert!(path.exists(), "modified file should NOT be deleted");

    std::fs::remove_dir_all(&scratch).ok();
}

#[test]
fn skips_file_that_is_already_missing() {
    let scratch = std::env::temp_dir().join("ctxrelay-undo-test-missing");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();

    let path = scratch.join("never-existed.jsonl");
    let record = WriteRecord { path: path.clone(), sha256: "irrelevant".to_string() };
    let manifest = sample_manifest(vec![record]);
    let manifest_path = scratch.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let actions = run_undo(&manifest_path).expect("undo should succeed");

    assert_eq!(actions, vec![UndoAction::SkippedMissing(path)]);

    std::fs::remove_dir_all(&scratch).ok();
}

/// 之前几个测试都是拿手工构造的 `Manifest` 喂给 `run_undo`,从未验证过
/// `run_undo` 真的能撤销一次 `run_import` 的真实产出——这条测试补上这个空当:
/// 完整走一遍 `run_import`(离线,预建 session 目录跳过 bootstrap,不碰真实
/// `claude` CLI)拿到一份真正由 commit 产出的 `Manifest`,再用它调 `run_undo`,
/// 断言 `import` 真实写出的文件确实被删掉了。
#[test]
fn undoes_a_manifest_produced_by_a_real_run_import() {
    let registry = Registry::with_defaults();
    let source = SourceRef::File(PathBuf::from(FIXTURE));

    let project_dir = std::env::temp_dir().join("ctxrelay-undo-real-import-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-undo-real-import-test-projects-root");
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
    let written_path = manifest.writes[0].path.clone();
    assert!(written_path.exists(), "import should have really written the session file");

    let manifest_path = project_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();

    let actions = run_undo(&manifest_path).expect("undo should succeed");

    assert_eq!(actions, vec![UndoAction::Deleted(written_path.clone())]);
    assert!(!written_path.exists(), "undo should have deleted the file import really wrote");

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}
