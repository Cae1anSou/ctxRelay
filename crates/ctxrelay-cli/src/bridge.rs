use serde::Deserialize;

/// 对应 `bridge-protocol/schema.json` 的 `CaptureRequest`——字段名/必需性必须和
/// schema 保持一致,这份 schema 才是两侧契约的唯一权威来源,这里只是它在 Rust 里
/// 的一份手写投影。
#[derive(Debug, Deserialize)]
pub struct CaptureRequest {
    pub version: String,
    pub token: String,
    pub conversation_id: String,
    pub org_id: String,
    #[serde(default)]
    pub captured_at: Option<String>,
    pub snapshot: serde_json::Value,
}
