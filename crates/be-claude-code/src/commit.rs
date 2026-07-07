use ctxrelay_backend::{
    BackendError, Dest, LoweredSession, LoweringReport, Manifest, Result, TargetSpec, WriteRecord,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// 唯一的写盘处。把 `lower` 产出的记录逐条盖上环境信息(sessionId/cwd/gitBranch/
/// version/userType)再写成 JSONL 文件,产出记录了写了什么的 `Manifest`。
///
/// 写入前会拒绝覆盖已存在的会话文件:`session_id` 由文档内容确定性派生,如果同一份
/// IR 被 commit 两次(误操作重试、或用户已经在 Claude Code 里真实继续过这个会话),
/// 无条件 `fs::write` 会静默覆盖掉已经存在的、可能包含用户真实后续对话的内容——这
/// 正是架构文档 §1 点名要避免的"静默失败模式"。调用方如果确实要重新导入,应该先
/// 显式删除旧文件(或者未来由 core 提供的 `--force`/`undo` 流程处理),不应该由
/// `commit` 自己悄悄决定。
pub fn commit(
    lowered: LoweredSession,
    dest: &Dest,
    target: TargetSpec,
    report: LoweringReport,
    ir_digest: String,
) -> Result<Manifest> {
    std::fs::create_dir_all(&dest.session_dir).map_err(|e| {
        BackendError(format!(
            "failed to create session dir {}: {e}",
            dest.session_dir.display()
        ))
    })?;

    let path = dest
        .session_dir
        .join(format!("{}.jsonl", lowered.session_id));

    if path.exists() {
        return Err(BackendError(format!(
            "refusing to overwrite existing session file {}: this import may have already \
             been committed once, and the file may contain real conversation continued since \
             then; delete it first if you intend to re-import",
            path.display()
        )));
    }

    let mut buffer = String::new();
    for mut line in lowered.lines {
        stamp_environment(&mut line, &lowered.session_id, dest);
        buffer.push_str(&line.to_string());
        buffer.push('\n');
    }

    // 先写临时文件,再原子 rename 到目标路径——不用直接 `fs::write(&path, ...)`,
    // 因为那不是原子操作:如果写到一半磁盘写满/进程被杀,会在 `path` 留下一份
    // 内容不完整的半成品文件。那样的话,下次重试会撞上前面 `path.exists()` 的
    // 覆盖保护,而那条错误信息说的是"可能已经真实 commit 过",对一份写坏的垃圾
    // 文件来说是误导性的。`rename` 在同一文件系统内是原子的,要么最终看到的是
    // 完整内容,要么 `path` 根本不存在,不会有第三种状态。
    let tmp_path = dest
        .session_dir
        .join(format!("{}.jsonl.tmp", lowered.session_id));
    std::fs::write(&tmp_path, &buffer)
        .map_err(|e| BackendError(format!("failed to write {}: {e}", tmp_path.display())))?;
    std::fs::rename(&tmp_path, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        BackendError(format!(
            "failed to move {} into place at {}: {e}",
            tmp_path.display(),
            path.display()
        ))
    })?;

    let sha256 = format!("{:x}", Sha256::digest(buffer.as_bytes()));

    Ok(Manifest {
        ir_digest,
        target,
        writes: vec![WriteRecord { path, sha256 }],
        created_session_ids: vec![lowered.session_id],
        report,
        cwd: dest.cwd.clone(),
    })
}

fn stamp_environment(line: &mut Value, session_id: &str, dest: &Dest) {
    let obj = line
        .as_object_mut()
        .expect("lower() always produces JSON objects");
    obj.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    obj.insert(
        "cwd".to_string(),
        Value::String(dest.cwd.display().to_string()),
    );
    obj.insert(
        "version".to_string(),
        Value::String(dest.cli_version.clone()),
    );
    obj.insert(
        "gitBranch".to_string(),
        Value::String(dest.git_branch.clone().unwrap_or_default()),
    );
    obj.insert(
        "userType".to_string(),
        Value::String("external".to_string()),
    );
}
