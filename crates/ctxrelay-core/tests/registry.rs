use ctxrelay_core::Registry;
use ctxrelay_frontend::SourceRef;
use std::path::PathBuf;

#[test]
fn finds_fe_claude_share_for_file_source() {
    let registry = Registry::with_defaults();
    let source = SourceRef::File(PathBuf::from("does-not-need-to-exist.json"));

    let acquire = registry
        .find_acquire(&source)
        .expect("should find an Acquire for File source");
    assert_eq!(acquire.id(), "fe-claude-share");

    let parse = registry
        .find_parse(acquire.id())
        .expect("should find matching Parse");
    assert_eq!(parse.id(), "fe-claude-share");
}

#[test]
fn does_not_find_acquire_for_url_source() {
    let registry = Registry::with_defaults();
    let source = SourceRef::Url("https://claude.ai/share/xyz".to_string());

    // fe-claude-share V1 只接受 File,不接受 Url(见架构文档 §12 步骤 2)。
    assert!(registry.find_acquire(&source).is_none());
}

#[test]
fn finds_claude_code_backend_by_name() {
    let registry = Registry::with_defaults();
    let backend = registry
        .find_backend("claude-code")
        .expect("should find claude-code backend");
    assert_eq!(backend.target().tool, "claude-code");
}

#[test]
fn does_not_find_unknown_backend() {
    let registry = Registry::with_defaults();
    assert!(registry.find_backend("codex").is_none());
}
