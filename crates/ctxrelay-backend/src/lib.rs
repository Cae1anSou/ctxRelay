//! ctxRelay backend 契约(架构文档 §5、§8)。
//!
//! `legalize`/`lower` 是纯函数:legalize 把本目标不合法的 IR 构造丢弃/转译并报告丢了什么,
//! lower 把合法化后的 IR 转成目标原生的、可缓存可 diff 的数据结构。`commit` 是唯一允许
//! 写盘的一跳,产出记录了写了什么的 `Manifest`,支撑 undo/dry-run。

use std::fmt;
use std::path::PathBuf;

use ctxrelay_ir::Document;
use sha2::Digest;

/// 对**原始**(legalize 之前)IR `Document` 求内容摘要,作为 `Manifest.ir_digest` 的
/// 唯一权威来源。之所以不在 `lower`/`commit` 内部就地计算,是因为那两步只拿得到
/// legalize 之后的 `Document`——legalize 会丢 Reasoning、内联 ForeignAction、插入
/// preamble,对它求哈希对不上任何一份真实落盘的、可 checkin 的原始 IR 文件
/// (架构文档 §3.4/§8:`ir_digest` 要能回答"这次 commit 到底来自哪份 IR")。调用方
/// 必须在调 `legalize` 之前,先对原始 `Document` 调这个函数,把结果一路带到 `commit`。
pub fn document_digest(doc: &Document) -> String {
    let bytes = serde_json::to_vec(doc).expect("Document serialization is infallible");
    format!("{:x}", sha2::Sha256::digest(&bytes))
}

/// 目标 CLI 及其版本范围——"某某 backend"不是一个东西,是"某某 vX.Y backend"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub tool: String,
    pub version_range: String,
}

/// backend 声明自己接受/拒绝哪些 caps。V1 只有一个有意义的判据:是否接受
/// `verifiable_signature: true` 的 Reasoning 并原样保留成目标原生的 thinking 结构。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapPolicy {
    pub accepts_verifiable_reasoning: bool,
}

/// legalize 阶段的报告:丢了什么、转译了什么,呈现给用户做"对理解的可逆性"。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoweringReport {
    pub dropped_reasoning: usize,
    pub inlined_foreign_actions: usize,
    pub notes: Vec<String>,
}

/// lower 的纯数据产出:目标原生序列,写盘前可缓存、可 diff、可 dry-run。
///
/// 不含 `ir_digest`——那是对**原始**(legalize 之前)`Document` 的摘要,`lower` 拿到的
/// 已经是 legalize 之后的版本,没资格代表它。见 `document_digest` 的文档注释。
#[derive(Debug, Clone, PartialEq)]
pub struct LoweredSession {
    pub session_id: String,
    pub lines: Vec<serde_json::Value>,
}

/// commit 的目标位置与写盘时需要盖上的环境信息。
///
/// `session_dir` 由调用方(core/cli)负责发现/解析——backend 不猜目录 slug 编码规则
/// (架构文档 §5:"不要逆向目录 slug 编码规则……这是逆向出来的、会变")。
#[derive(Debug, Clone)]
pub struct Dest {
    pub session_dir: PathBuf,
    pub cwd: PathBuf,
    pub git_branch: Option<String>,
    pub cli_version: String,
}

/// 一次写盘记录:写了哪个文件、内容的 sha256。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRecord {
    pub path: PathBuf,
    pub sha256: String,
}

/// commit 的产出:记录写了什么,支撑 `ctxrelay undo`。
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub ir_digest: String,
    pub target: TargetSpec,
    pub writes: Vec<WriteRecord>,
    pub created_session_ids: Vec<String>,
    pub report: LoweringReport,
}

/// Backend 契约共用的错误类型。
#[derive(Debug)]
pub struct BackendError(pub String);

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BackendError {}

pub type Result<T> = std::result::Result<T, BackendError>;

/// 一个 CLI 目标的 lowering 契约。
///
/// 与架构文档 §5 的字面签名有两处差异,都是同一个根因:legalize 之后的 `Document`
/// 丢失了一些只有调用方(持有原始 `Document` 的那一方)才知道的信息,`commit` 需要
/// 这些信息才能填出一份诚实的 `Manifest`,所以多接收两个参数:
/// - `report`:`legalize` 产出的 `LoweringReport`,`lower(doc) -> LoweredSession` 拿到
///   的已经是合法化后的 `Document`,天然不知道刚才丢了什么。
/// - `ir_digest`:对**原始** `Document`(调 `legalize` 之前)的内容摘要,用
///   `document_digest` 计算。`lower`/`commit` 都只见得到合法化后的版本,没资格代表
///   原始 IR 的身份。
///
/// 调用方必须在调用 `legalize` 之前就对原始 `Document` 算好 `ir_digest`,并在拿到
/// `legalize` 的 `report` 后,把两者一并带到 `commit`。
pub trait Backend {
    fn target(&self) -> TargetSpec;
    fn required_caps(&self) -> CapPolicy;
    fn legalize(&self, doc: &Document) -> (Document, LoweringReport);
    fn lower(&self, doc: &Document) -> Result<LoweredSession>;
    fn commit(
        &self,
        lowered: LoweredSession,
        dest: &Dest,
        report: LoweringReport,
        ir_digest: String,
    ) -> Result<Manifest>;
}
