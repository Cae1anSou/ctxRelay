use ctxrelay_core::resolve_claude_code_dest;
use std::path::PathBuf;

#[test]
fn finds_existing_session_dir_without_bootstrapping() {
    let project_dir = std::env::temp_dir().join("ctxrelay-dest-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-dest-test-projects-root");
    let _ = std::fs::remove_dir_all(&projects_root);
    let slug = canonical.display().to_string().replace('/', "-");
    let expected_session_dir = projects_root.join(&slug);
    std::fs::create_dir_all(&expected_session_dir).unwrap();

    let dest = resolve_claude_code_dest(&project_dir, &projects_root, "2.1.201", false)
        .expect("should find existing dir without bootstrap");

    assert_eq!(dest.session_dir, expected_session_dir);
    assert_eq!(dest.cwd, canonical);
    assert_eq!(dest.cli_version, "2.1.201");

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}

#[test]
fn errors_when_missing_and_bootstrap_disallowed() {
    let project_dir = std::env::temp_dir().join("ctxrelay-dest-test-project-2");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-dest-test-projects-root-2");
    let _ = std::fs::remove_dir_all(&projects_root);

    let result = resolve_claude_code_dest(&project_dir, &projects_root, "2.1.201", false);
    assert!(result.is_err());

    std::fs::remove_dir_all(&project_dir).ok();
}

#[test]
fn errors_when_project_dir_does_not_exist() {
    let project_dir = std::env::temp_dir().join("ctxrelay-dest-test-does-not-exist");
    let _ = std::fs::remove_dir_all(&project_dir);
    let projects_root = std::env::temp_dir().join("ctxrelay-dest-test-projects-root-3");

    let result = resolve_claude_code_dest(&project_dir, &projects_root, "2.1.201", false);
    assert!(result.is_err());
}

/// 真实调用 `claude` CLI 走一遍引导流程,花费少量 API 额度,默认不随 `cargo test` 跑:
/// `cargo test -p ctxrelay-core --test dest -- --ignored`
#[test]
#[ignore]
fn bootstraps_session_dir_when_missing_and_allowed() {
    let project_dir = std::env::temp_dir().join("ctxrelay-dest-bootstrap-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let home = std::env::var("HOME").expect("HOME must be set");
    let projects_root = PathBuf::from(home).join(".claude/projects");
    let slug = canonical.display().to_string().replace('/', "-");
    let session_dir = projects_root.join(&slug);
    let _ = std::fs::remove_dir_all(&session_dir);

    let dest = resolve_claude_code_dest(&project_dir, &projects_root, "2.1.201", true)
        .expect("bootstrap should succeed");

    assert_eq!(dest.session_dir, session_dir);
    assert!(session_dir.is_dir());
    // 引导用的那个一次性会话文件(uuid.jsonl)应该已经被清掉——但 session_dir 里
    // 可能还有 Claude Code 自己的其他项目级状态(比如 memory/ 自动记忆目录,这是
    // 正式产品功能,不是引导噪音,不归我们清理)。这里只断言我们知道自己创建的那个
    // 特定文件不在了,不对整个目录的内容做更强的假设。
    let entries: Vec<_> = std::fs::read_dir(&session_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        !entries
            .iter()
            .any(|name| name.to_string_lossy().ends_with(".jsonl")),
        "bootstrap should not leave its throwaway session file behind, found: {entries:?}"
    );

    std::fs::remove_dir_all(&session_dir).ok();
    std::fs::remove_dir_all(&project_dir).ok();
}
