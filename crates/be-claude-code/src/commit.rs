use ctxrelay_backend::{BackendError, Dest, LoweredSession, LoweringReport, Manifest, Result, TargetSpec, WriteRecord};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// 唯一的写盘处。把 `lower` 产出的记录逐条盖上环境信息(sessionId/cwd/gitBranch/
/// version/userType)再写成 JSONL 文件,产出记录了写了什么的 `Manifest`。
pub fn commit(
    lowered: LoweredSession,
    dest: &Dest,
    target: TargetSpec,
    report: LoweringReport,
) -> Result<Manifest> {
    std::fs::create_dir_all(&dest.session_dir).map_err(|e| {
        BackendError(format!(
            "failed to create session dir {}: {e}",
            dest.session_dir.display()
        ))
    })?;

    let path = dest.session_dir.join(format!("{}.jsonl", lowered.session_id));

    let mut buffer = String::new();
    for mut line in lowered.lines {
        stamp_environment(&mut line, &lowered.session_id, dest);
        buffer.push_str(&line.to_string());
        buffer.push('\n');
    }

    std::fs::write(&path, &buffer)
        .map_err(|e| BackendError(format!("failed to write {}: {e}", path.display())))?;

    let sha256 = format!("{:x}", Sha256::digest(buffer.as_bytes()));

    Ok(Manifest {
        ir_digest: lowered.ir_digest,
        target,
        writes: vec![WriteRecord { path, sha256 }],
        created_session_ids: vec![lowered.session_id],
        report,
    })
}

fn stamp_environment(line: &mut Value, session_id: &str, dest: &Dest) {
    let obj = line.as_object_mut().expect("lower() always produces JSON objects");
    obj.insert("sessionId".to_string(), Value::String(session_id.to_string()));
    obj.insert("cwd".to_string(), Value::String(dest.cwd.display().to_string()));
    obj.insert("version".to_string(), Value::String(dest.cli_version.clone()));
    obj.insert(
        "gitBranch".to_string(),
        Value::String(dest.git_branch.clone().unwrap_or_default()),
    );
    obj.insert("userType".to_string(), Value::String("external".to_string()));
}
