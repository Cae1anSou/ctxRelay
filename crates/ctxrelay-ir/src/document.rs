use crate::block::Block;
use semver::Version;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// 对话轮次的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    System,
}

/// 轮次的来源描述——仅描述性,绝不驱动 IR 内部分支(§6 试金石)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub vendor: String,
    pub model: Option<String>,
    pub surface: String,
}

/// 整份文档的来源描述:来自哪次导出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub vendor: String,
    pub surface: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub exported_at: Option<OffsetDateTime>,
}

/// doc 内稳定的轮次标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub String);

/// 一次对话轮次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub id: TurnId,
    pub role: Role,
    pub origin: Origin,
    pub blocks: Vec<Block>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub timestamp: Option<OffsetDateTime>,
}

/// IR 的顶层容器。`ir_version` 是 frontend/backend 独立演进的 ABI 版本号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub ir_version: Version,
    pub source: SourceProvenance,
    pub turns: Vec<Turn>,
}
