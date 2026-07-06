//! ctxRelay frontend 契约(架构文档 §4)。
//!
//! `Acquire` 是唯一允许有副作用的一跳(文件/网络 I/O),只管把 bytes 弄到手,
//! 不理解内容语义。`Parse` 是纯函数,把厂商专有字节 lower 进中立 IR。
//! 两者都只声明"我产出/接受什么能力",不关心任何具体 backend 的存在。

use std::fmt;
use std::path::PathBuf;

use ctxrelay_ir::Document;

/// 待 Acquire 的输入引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRef {
    /// 例如分享链接。
    Url(String),
    /// 例如账号导出的 JSON,或人工另存为的快照文件。
    File(PathBuf),
}

/// Acquire 拿到的未解析原始字节。
pub type RawBytes = Vec<u8>;

/// Acquire/Parse 共用的错误类型。V1 只需要区分"这是什么问题",不需要精细分类。
#[derive(Debug)]
pub struct FrontendError(pub String);

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for FrontendError {}

pub type Result<T> = std::result::Result<T, FrontendError>;

/// 唯一的副作用一跳:按 `SourceRef` 类型/模式路由,把 bytes 弄到手。
pub trait Acquire {
    fn id(&self) -> &'static str;
    /// 按 `SourceRef` 类型/URL 模式路由,不读取内容语义。
    fn accepts(&self, input: &SourceRef) -> bool;
    fn acquire(&self, input: SourceRef) -> Result<RawBytes>;
}

/// 纯函数:给字节吐 IR,把厂商专有结构 lower 进中立 `Document`。
pub trait Parse {
    fn id(&self) -> &'static str;
    fn parse(&self, raw: RawBytes) -> Result<Document>;
}
