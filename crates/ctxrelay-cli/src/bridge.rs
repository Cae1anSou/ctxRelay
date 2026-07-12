use serde::{Deserialize, Serialize};

/// 对应 `bridge-protocol/schema.json` 的 `CaptureRequest`——字段名/必需性必须和
/// schema 保持一致,这份 schema 才是两侧契约的唯一权威来源,这里只是它在 Rust 里
/// 的一份手写投影。
///
/// `frontend_id` 是 Rust 侧 `Registry::find_parse` 用的路由键,必须等于某个已注册
/// frontend crate 的 `Parse::id()`(例如 `"fe-claude-live"`)——桥本身不认识任何
/// 具体应用,只按这个字符串转发,具体应用是谁由发请求的插件决定。
#[derive(Debug, Deserialize)]
pub struct CaptureRequest {
    pub version: String,
    pub token: String,
    pub frontend_id: String,
    #[serde(default)]
    pub captured_at: Option<String>,
    pub snapshot: serde_json::Value,
}

/// 对应 `bridge-protocol/schema.json` 的 `CaptureResponse`——同样是手写投影。
/// 在此之前 `ctxrelay listen` 是用 `format!` 手拼这个响应体的字符串,`message`
/// 字段没有做 JSON 转义,一旦错误信息里含引号/换行会产出非法 JSON,扩展侧解析
/// 响应体会直接失败。改成走 `Serialize` 之后由 `serde_json` 负责转义。
#[derive(Debug, Serialize)]
pub struct CaptureResponse {
    pub version: &'static str,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl CaptureResponse {
    pub fn ok() -> Self {
        Self {
            version: "1",
            status: "ok",
            message: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            version: "1",
            status: "error",
            message: Some(message.into()),
        }
    }

    /// 序列化失败在语法上不可能发生(所有字段都是 String/&str),但既然签名要返回
    /// 字符串给 `tiny_http::Response::from_string`,这里给一个绝不会被触发的兜底,
    /// 好过 `.unwrap()` 在一个用户看不到 stderr 的路径上直接崩掉本该负责回应的进程。
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| r#"{"version":"1","status":"error"}"#.to_string())
    }
}
