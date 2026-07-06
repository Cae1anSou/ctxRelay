//! ctxRelay backend 契约(架构文档 §5、§8)。
//!
//! `legalize`/`lower` 是纯函数:legalize 把本目标不合法的 IR 构造丢弃/转译并报告丢了什么,
//! lower 把合法化后的 IR 转成目标原生的、可缓存可 diff 的数据结构。`commit` 是唯一允许
//! 写盘的一跳,产出记录了写了什么的 `Manifest`,支撑 undo/dry-run。

use std::fmt;
use std::path::PathBuf;

use ctxrelay_ir::Document;

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
#[derive(Debug, Clone)]
pub struct LoweredSession {
    pub session_id: String,
    pub ir_digest: String,
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
#[derive(Debug, Clone)]
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
/// 与架构文档 §5 的字面签名有一处差异:`commit` 这里多接收一个 `report` 参数——
/// `legalize` 产出的 `LoweringReport` 需要一路带到最终的 `Manifest` 里,但
/// `lower(doc) -> LoweredSession` 只接受已合法化的 `Document`,天然拿不到 legalize
/// 阶段丢弃了什么这份信息(被丢弃的东西已经不在 Document 里了)。调用方需要自己
/// 持有 legalize 返回的 report,在调用 commit 时一并传入。
pub trait Backend {
    fn target(&self) -> TargetSpec;
    fn required_caps(&self) -> CapPolicy;
    fn legalize(&self, doc: &Document) -> (Document, LoweringReport);
    fn lower(&self, doc: &Document) -> Result<LoweredSession>;
    fn commit(&self, lowered: LoweredSession, dest: &Dest, report: LoweringReport) -> Result<Manifest>;
}
