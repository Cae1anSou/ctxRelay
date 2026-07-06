use ctxrelay_backend::{LoweringReport, Manifest, TargetSpec, WriteRecord};
use ctxrelay_core::{run_undo, UndoAction};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

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
