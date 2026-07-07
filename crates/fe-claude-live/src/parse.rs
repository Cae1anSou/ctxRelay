use ctxrelay_frontend::{FrontendError, Parse, RawBytes, Result};
use ctxrelay_ir::{Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// `parent_message_uuid` 的树根哨兵值——实测确认(真实调用 claude.ai 认证态 API 得到的),不是猜的。
const ROOT_SENTINEL: &str = "00000000-0000-4000-8000-000000000000";

/// claude.ai 认证态对话接口(`/api/organizations/<org>/chat_conversations/<id>`)的
/// on-disk JSON 形状。只声明我们实际使用的字段。
#[derive(Deserialize)]
struct RawSnapshot {
    model: Option<String>,
    current_leaf_message_uuid: String,
    chat_messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    uuid: String,
    content: Vec<Value>,
    sender: String,
    created_at: String,
    parent_message_uuid: String,
}

/// claude.ai 认证态实时对话的 Parse 实现。纯函数:给定字节,要么吐出合法
/// `Document`,要么明确报错。
pub struct ClaudeLiveParse;

impl Parse for ClaudeLiveParse {
    fn id(&self) -> &'static str {
        "fe-claude-live"
    }

    fn parse(&self, raw: RawBytes) -> Result<Document> {
        let snapshot: RawSnapshot = serde_json::from_slice(&raw)
            .map_err(|e| FrontendError(format!("invalid claude.ai live conversation JSON: {e}")))?;

        let by_uuid: HashMap<&str, &RawMessage> =
            snapshot.chat_messages.iter().map(|m| (m.uuid.as_str(), m)).collect();

        // 从 current_leaf_message_uuid 沿 parent_message_uuid 往回走,重建"当前被
        // 选中的那条线性分支"——tree=True 请求可能带回整棵树(含被放弃的重新生成
        // 分支),只有这样才能保证不管接口未来返不返回额外分支,取到的永远是用户
        // 实际看到的那条对话,不是随便拼出来的。
        let mut ordered: Vec<&RawMessage> = Vec::new();
        let mut cursor: &str = snapshot.current_leaf_message_uuid.as_str();
        while cursor != ROOT_SENTINEL {
            let message = *by_uuid
                .get(cursor)
                .ok_or_else(|| FrontendError(format!("chat_messages missing referenced uuid {cursor:?}")))?;
            ordered.push(message);
            cursor = message.parent_message_uuid.as_str();
        }
        ordered.reverse();

        let mut turns = Vec::with_capacity(ordered.len());
        for message in ordered {
            let role = match message.sender.as_str() {
                "human" => Role::User,
                "assistant" => Role::Assistant,
                other => {
                    return Err(FrontendError(format!("unknown chat_messages[].sender value: {other:?}")))
                }
            };

            let timestamp = OffsetDateTime::parse(&message.created_at, &Rfc3339).map_err(|e| {
                FrontendError(format!("invalid created_at timestamp {:?}: {e}", message.created_at))
            })?;

            let mut blocks = Vec::with_capacity(message.content.len());
            for block in &message.content {
                let kind = block
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| FrontendError("content block missing \"type\" field".to_string()))?;

                match kind {
                    "text" => {
                        let content = block
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                FrontendError(
                                    "content block has type=\"text\" but no \"text\" field".to_string(),
                                )
                            })?
                            .to_string();
                        blocks.push(Block::Text { content });
                    }
                    "thinking" => {
                        // 实测确认:即使是认证态的内部接口,thinking block 也没有
                        // signature 字段——没有任何已知渠道能拿到真实签名字节,所以
                        // 恒标记 verifiable_signature: false,legalize 阶段会按 §5 的
                        // 规则丢弃,不会尝试伪造签名。
                        let content = block
                            .get("thinking")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        blocks.push(Block::Reasoning {
                            content,
                            caps: BlockCaps { reasoning: true, verifiable_signature: false, replayable: false },
                        });
                    }
                    other => {
                        let artifact = Artifact {
                            media: "application/json".to_string(),
                            content: block.to_string(),
                        };
                        blocks.push(Block::foreign_action(
                            other.to_string(),
                            Some(format!("未识别的 content block 类型: {other}")),
                            Some(artifact),
                            false,
                            false,
                        ));
                    }
                }
            }

            turns.push(Turn {
                id: TurnId(message.uuid.clone()),
                role,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: if message.sender == "assistant" { snapshot.model.clone() } else { None },
                    surface: "claude.ai".to_string(),
                },
                blocks,
                timestamp: Some(timestamp),
            });
        }

        Ok(Document {
            ir_version: Version::new(0, 1, 0),
            source: SourceProvenance {
                vendor: "anthropic".to_string(),
                surface: "claude.ai".to_string(),
                // 接口没有告诉我们"这次导出是什么时候做的",用系统时间会让 Parse
                // 不再是纯函数,如实填 None。
                exported_at: None,
            },
            turns,
        })
    }
}
