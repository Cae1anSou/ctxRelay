use serde::{Deserialize, Serialize};

/// 中立能力描述符——解耦的关键:backend 只据此决策,永不问"来自哪个源"。
/// frontend 在产出每个 block 时如实填写;backend 的 legalize 只读这个结构判断取舍。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCaps {
    pub reasoning: bool,
    pub verifiable_signature: bool,
    /// ForeignAction 恒为 false:IR 层不提供可被误用成"回放"的结构。
    pub replayable: bool,
}

/// 外部效应的人类可读产物(例如 artifact/web_search/code_interpreter 的渲染结果)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub media: String,
    pub content: String,
}

/// 一次对话轮次里的内容单元。
///
/// 厂商专有工具(artifact / web_search / code_interpreter / grounding …)在 IR 里
/// 不各自建模,全部归一成 `ForeignAction`:一次外部效应 + 一份人类可读产物,
/// 不承诺可回放。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Block {
    Text {
        content: String,
    },
    Code {
        language: Option<String>,
        content: String,
    },
    ForeignAction {
        /// 不透明标签,IR 不解释其语义(例如 "artifact" / "web_search")。
        kind: String,
        summary: Option<String>,
        artifact: Option<Artifact>,
        caps: BlockCaps,
    },
    Reasoning {
        content: String,
        caps: BlockCaps,
    },
}
