use ctxrelay_backend::LoweredSession;
use ctxrelay_ir::{Block, Document, Role};
use serde_json::{json, Value};
use sha2::Digest;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// 项目私有的固定命名空间,只用来确定性派生 UUID——不是真实的 DNS/URL/OID 命名空间,
/// 只是一个稳定常量,保证同样的输入永远派生出同样的 UUID(这样 `lower` 才能是纯函数)。
const NAMESPACE: Uuid = Uuid::from_bytes([
    0x6a, 0x1e, 0xd6, 0x9b, 0x0c, 0x3a, 0x4b, 0x1d, 0x9e, 0x77, 0x2f, 0x51, 0x8c, 0xaa, 0x03, 0x77,
]);

fn turn_uuid(turn_id: &str) -> Uuid {
    Uuid::new_v5(&NAMESPACE, turn_id.as_bytes())
}

/// session_id 和 ir_digest 都从文档内容本身确定性派生,不引入随机数或系统时间——
/// 这保证同一份 `Document` 无论何时、在哪台机器上 lower,产出完全一致(可缓存、可 diff)。
fn document_digest(doc: &Document) -> Vec<u8> {
    serde_json::to_vec(doc).expect("Document serialization is infallible")
}

/// Claude Code 的 JSONL 记录只有 `"user"`/`"assistant"` 两种 `type`,没有独立的
/// system 角色记录类型(实测确认,见本计划开头);`Role::System` 目前也没有任何
/// frontend 真的产出过。把它并到 `"user"` 是"最接近的可用槽位"这个诚实的近似,
/// 不是精确建模——如果未来真的需要区分,应该重新评估这条映射。
fn role_str(role: Role) -> &'static str {
    match role {
        Role::User | Role::System => "user",
        Role::Assistant => "assistant",
    }
}

fn block_to_text(block: &Block) -> String {
    match block {
        Block::Text { content } => content.clone(),
        Block::Code { language, content } => match language {
            Some(lang) => format!("```{lang}\n{content}\n```"),
            None => format!("```\n{content}\n```"),
        },
        // legalize 已经把 Reasoning 丢弃、ForeignAction 内联成 Text,lower 不应该再见到
        // 它们;如果真的见到了,说明调用方跳过了 legalize,这是编程错误而不是数据问题,
        // 直接 panic 比静默生成一个内容缺失的会话更安全。
        other => panic!("lower() received un-legalized block: {other:?}"),
    }
}

/// 纯函数:把(已合法化的)IR `Document` 转成 Claude Code 的 JSONL 记录(内存态)。
///
/// 不填 `sessionId`/`cwd`/`gitBranch`/`version`/`userType` 这几个反映"写盘时环境"的
/// 字段——那些交给 `commit` 在真正落盘前盖上去,这样 `lower` 才不需要知道任何环境信息,
/// 保持纯。
pub fn lower(doc: &Document) -> ctxrelay_backend::Result<LoweredSession> {
    let digest_bytes = document_digest(doc);
    let session_id = Uuid::new_v5(&NAMESPACE, &digest_bytes).to_string();
    let ir_digest = format!("{:x}", sha2::Sha256::digest(&digest_bytes));

    let mut lines = Vec::with_capacity(doc.turns.len());
    let mut previous_uuid: Option<String> = None;

    for turn in &doc.turns {
        let uuid = turn_uuid(&turn.id.0).to_string();
        let role = role_str(turn.role);

        let content: Vec<Value> = turn
            .blocks
            .iter()
            .map(|b| json!({ "type": "text", "text": block_to_text(b) }))
            .collect();

        let timestamp = turn
            .timestamp
            .map(|t| t.format(&Rfc3339).expect("valid OffsetDateTime always formats"))
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

        let parent_uuid = match &previous_uuid {
            Some(p) => Value::String(p.clone()),
            None => Value::Null,
        };

        let line = if role == "assistant" {
            json!({
                "parentUuid": parent_uuid,
                "isSidechain": false,
                "message": {
                    "model": turn.origin.model.clone().unwrap_or_else(|| "unknown".to_string()),
                    "id": format!("msg_{uuid}"),
                    "type": "message",
                    "role": "assistant",
                    "content": content,
                    "stop_reason": "end_turn",
                    "stop_sequence": Value::Null,
                },
                "type": "assistant",
                "uuid": uuid,
                "timestamp": timestamp,
            })
        } else {
            json!({
                "parentUuid": parent_uuid,
                "isSidechain": false,
                "type": "user",
                "message": { "role": "user", "content": content },
                "uuid": uuid,
                "timestamp": timestamp,
            })
        };

        previous_uuid = Some(uuid);
        lines.push(line);
    }

    Ok(LoweredSession { session_id, ir_digest, lines })
}
