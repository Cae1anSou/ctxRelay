use std::path::PathBuf;
use std::process::Command;

fn ctxrelay_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ctxrelay"))
}

#[test]
fn ir_subcommand_writes_ir_json_from_real_fixture() {
    let output_path = std::env::temp_dir().join("ctxrelay-cli-test-output.ir.json");
    let _ = std::fs::remove_file(&output_path);

    let status = Command::new(ctxrelay_bin())
        .arg("ir")
        .arg("../fe-claude-share/tests/fixtures/sample_snapshot.json")
        .arg("--output")
        .arg(&output_path)
        .status()
        .expect("failed to run ctxrelay ir");

    assert!(status.success());
    assert!(output_path.exists());

    let content = std::fs::read_to_string(&output_path).expect("output file should exist");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");
    assert_eq!(
        parsed["turns"]
            .as_array()
            .expect("turns should be an array")
            .len(),
        4
    );

    std::fs::remove_file(&output_path).ok();
}

/// 端到端:真实调用 `ctxrelay import` 落一个会话,再用 `ctxrelay verify` 冒烟验证。
/// 手动预先建好 session_dir 来跳过 `--bootstrap`(避免这条测试还要再花一次 bootstrap
/// 的 API 额度),但 `import`(写 JSONL)本身和 `verify`(真实 `claude --resume`)都
/// 是真刀真枪跑的。会花费少量真实 API 额度,默认不随 `cargo test` 跑:
/// `cargo test -p ctxrelay-cli --test cli -- --ignored`
#[test]
#[ignore]
fn import_then_verify_round_trip_through_the_real_binary() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-e2e-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).expect("create scratch project dir");
    let project_dir = project_dir
        .canonicalize()
        .expect("canonicalize scratch project dir");

    let home = std::env::var("HOME").expect("HOME must be set");
    let slug = project_dir.display().to_string().replace('/', "-");
    let session_dir = PathBuf::from(&home).join(".claude/projects").join(&slug);
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("pre-create session dir to skip bootstrap");

    let manifest_path = project_dir.join("manifest.json");

    let import_status = Command::new(ctxrelay_bin())
        .arg("import")
        .arg("../fe-claude-share/tests/fixtures/sample_snapshot.json")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--manifest-out")
        .arg(&manifest_path)
        .status()
        .expect("failed to run ctxrelay import");

    assert!(import_status.success());
    assert!(manifest_path.exists());

    let verify_status = Command::new(ctxrelay_bin())
        .arg("verify")
        .arg(&manifest_path)
        .status()
        .expect("failed to run ctxrelay verify");

    assert!(verify_status.success());

    std::fs::remove_dir_all(&session_dir).ok();
    std::fs::remove_dir_all(&project_dir).ok();
}
