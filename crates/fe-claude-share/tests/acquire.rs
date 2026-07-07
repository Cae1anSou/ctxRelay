use ctxrelay_frontend::{Acquire, SourceRef};
use fe_claude_share::ClaudeShareAcquire;
use std::path::PathBuf;

#[test]
fn accepts_file_but_not_url() {
    let acquire = ClaudeShareAcquire;
    assert!(acquire.accepts(&SourceRef::File(PathBuf::from(
        "tests/fixtures/sample_snapshot.json"
    ))));
    assert!(!acquire.accepts(&SourceRef::Url("https://claude.ai/share/xyz".to_string())));
}

#[test]
fn reads_bytes_from_file() {
    let acquire = ClaudeShareAcquire;
    let path = PathBuf::from("tests/fixtures/sample_snapshot.json");
    let expected = std::fs::read(&path).expect("fixture must exist");

    let raw = acquire
        .acquire(SourceRef::File(path))
        .expect("should read fixture file");

    assert_eq!(raw, expected);
}

#[test]
fn url_acquire_returns_error_not_implemented() {
    let acquire = ClaudeShareAcquire;
    let result = acquire.acquire(SourceRef::Url("https://claude.ai/share/xyz".to_string()));
    assert!(result.is_err());
}
