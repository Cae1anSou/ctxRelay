use be_claude_code::commit::commit;
use be_claude_code::legalize::legalize;
use be_claude_code::lower::lower;
use ctxrelay_backend::{Dest, TargetSpec};
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use std::path::PathBuf;
use std::process::Command;

/// 仅用于本测试定位 `claude --resume` 会去哪个目录找会话——不是 be-claude-code 的
/// 公开职责(架构文档 §5 明确 slug 编码规则不应该被反向工程,生产代码里这是
/// core/cli 的事)。这里的简单斜杠替换规则已经过真实 `claude` CLI 手工验证(见本
/// 计划开头"已实测确认的 Claude Code 会话格式"一节),仅对不含冒号/空格/盘符的
/// 普通 Unix 路径成立。
fn slug_for(path: &std::path::Path) -> String {
    path.display().to_string().replace('/', "-")
}

/// 端到端验证:IR → legalize → lower → commit 写出的 JSONL,`claude --resume` 真的
/// 能加载并在下一轮记起里面埋的暗号。这条测试会真实调用 `claude` CLI、花费少量
/// API 额度,默认不随 `cargo test` 跑,需要显式加 `-- --ignored` 才会执行:
///
/// `cargo test -p be-claude-code --test conformance -- --ignored`
#[test]
#[ignore]
fn claude_code_can_resume_a_committed_session() {
    let codeword = "橙色的仙人掌在打字";
    let scratch_project = std::env::temp_dir().join("ctxrelay-conformance-scratch-project");
    let _ = std::fs::remove_dir_all(&scratch_project);
    std::fs::create_dir_all(&scratch_project).expect("create scratch project dir");
    // NOTE: 在 macOS 上 `std::env::temp_dir()` 返回的是 `/var/folders/...`,而这其实是
    // `/private/var/folders/...` 的符号链接。`claude` CLI 内部会把 cwd canonicalize 成
    // 真实路径再计算 project slug,如果这里不 canonicalize,slug 就会算错、
    // `--resume` 会报 "No conversation found"。canonicalize 必须在 create_dir_all 之后
    // 才能生效(路径要先存在)。
    let scratch_project = scratch_project
        .canonicalize()
        .expect("canonicalize scratch project dir");

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
            origin: Origin {
                vendor: "anthropic".to_string(),
                model: None,
                surface: "claude.ai".to_string(),
            },
            blocks: vec![Block::Text {
                content: format!("暗号是:{codeword}"),
            }],
            timestamp: None,
        }],
    };

    let ir_digest = ctxrelay_backend::document_digest(&doc);
    let (legalized, report) = legalize(&doc).expect("legalize should succeed");
    let lowered = lower(&legalized).expect("lower should succeed");
    let session_id = lowered.session_id.clone();

    let home = std::env::var("HOME").expect("HOME must be set");
    let session_dir = PathBuf::from(home)
        .join(".claude/projects")
        .join(slug_for(&scratch_project));
    let _ = std::fs::remove_dir_all(&session_dir);

    let dest = Dest {
        session_dir: session_dir.clone(),
        cwd: scratch_project.clone(),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };

    commit(
        lowered,
        &dest,
        TargetSpec {
            tool: "claude-code".to_string(),
            version_range: ">=2.1.0".to_string(),
        },
        report,
        ir_digest,
    )
    .expect("commit should succeed");

    let output = Command::new("claude")
        .arg("--resume")
        .arg(&session_id)
        .arg("-p")
        .arg("我之前告诉你的暗号是什么?只回复暗号本身")
        .arg("--output-format")
        .arg("json")
        .current_dir(&scratch_project)
        .output()
        .expect("failed to run claude CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected JSON output from claude, got error {e}: {stdout}"));
    let result_text = parsed["result"]
        .as_str()
        .expect("result field should be a string");

    assert!(
        result_text.contains(codeword),
        "expected claude to recall the codeword {codeword:?}, got: {result_text:?}"
    );

    std::fs::remove_dir_all(&session_dir).ok();
    std::fs::remove_dir_all(&scratch_project).ok();
}
