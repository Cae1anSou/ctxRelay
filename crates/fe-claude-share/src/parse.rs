use ctxrelay_frontend::{FrontendError, Parse, RawBytes, Result};
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// claude.ai 分享快照(chat_snapshots)的 on-disk JSON 形状。
/// 只声明我们实际使用的字段——serde 默认忽略未声明的字段,不需要窜改成
/// `#[serde(deny_unknown_fields)]`,因为快照里还有大量我们不关心的元数据
/// (snapshot_name / creator / is_public / attachments 等)。
#[derive(Deserialize)]
struct RawSnapshot {
    chat_messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    uuid: String,
    content: Vec<RawContentBlock>,
    sender: String,
    index: u64,
    created_at: String,
}

#[derive(Deserialize)]
struct RawContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

/// claude.ai 分享快照的 Parse 实现。纯函数:给定字节,要么吐出合法 `Document`,
/// 要么明确报错,绝不静默吞掉解析不出来的东西。
pub struct ClaudeShareParse;

impl Parse for ClaudeShareParse {
    fn id(&self) -> &'static str {
        "fe-claude-share"
    }

    fn parse(&self, raw: RawBytes) -> Result<Document> {
        let snapshot: RawSnapshot = serde_json::from_slice(&raw)
            .map_err(|e| FrontendError(format!("invalid claude.ai chat_snapshot JSON: {e}")))?;

        let mut messages = snapshot.chat_messages;
        messages.sort_by_key(|m| m.index);

        let mut turns = Vec::with_capacity(messages.len());
        for message in messages {
            let role = match message.sender.as_str() {
                "human" => Role::User,
                "assistant" => Role::Assistant,
                other => {
                    return Err(FrontendError(format!(
                        "unknown chat_messages[].sender value: {other:?}"
                    )))
                }
            };

            let timestamp = OffsetDateTime::parse(&message.created_at, &Rfc3339).map_err(|e| {
                FrontendError(format!(
                    "invalid created_at timestamp {:?}: {e}",
                    message.created_at
                ))
            })?;

            let mut blocks = Vec::with_capacity(message.content.len());
            for block in message.content {
                match block.kind.as_str() {
                    "text" => {
                        let content = block.text.ok_or_else(|| {
                            FrontendError(
                                "content block has type=\"text\" but no \"text\" field".to_string(),
                            )
                        })?;
                        blocks.push(Block::Text { content });
                    }
                    other => {
                        // 未识别的 content block 类型(例如未来遇到 thinking/tool_use/artifact):
                        // 归一成 ForeignAction,不假装认识一个当前样例里没见过的结构。
                        blocks.push(Block::foreign_action(
                            other.to_string(),
                            None,
                            None,
                            false,
                            false,
                        ));
                    }
                }
            }

            turns.push(Turn {
                id: TurnId(message.uuid),
                role,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: None,
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
                // 快照 JSON 里没有任何字段记录"这份文件是什么时候被另存为的",
                // 用会话的 updated_at 冒充会误导语义,用当前系统时间又会让 Parse
                // 不再是纯函数(违反架构文档 §4 的契约),所以如实填 None。
                exported_at: None,
            },
            turns,
        })
    }
}
