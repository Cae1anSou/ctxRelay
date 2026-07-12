use ctxrelay_backend::{document_digest, Manifest};
use ctxrelay_frontend::SourceRef;
use ctxrelay_ir::Document;
use std::fmt;
use std::path::PathBuf;

use crate::dest::resolve_claude_code_dest;
use crate::registry::Registry;

#[derive(Debug)]
pub struct CoreError(pub String);

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CoreError {}

pub type Result<T> = std::result::Result<T, CoreError>;

/// 只 parse 出 IR,不碰任何 backend——对应 CLI 的 `ctxrelay ir` 子命令。
pub fn run_ir(registry: &Registry, source: SourceRef) -> Result<Document> {
    let acquire = registry
        .find_acquire(&source)
        .ok_or_else(|| CoreError("no registered frontend accepts this source".to_string()))?;
    let raw = acquire
        .acquire(source)
        .map_err(|e| CoreError(e.to_string()))?;
    let parse = registry.find_parse(acquire.id()).ok_or_else(|| {
        CoreError(format!(
            "no Parse registered for frontend id {:?}",
            acquire.id()
        ))
    })?;
    parse.parse(raw).map_err(|e| CoreError(e.to_string()))
}

pub struct ImportOptions {
    pub backend_name: String,
    pub project_dir: PathBuf,
    pub dry_run: bool,
    pub allow_bootstrap: bool,
    pub claude_projects_root: PathBuf,
    pub cli_version: String,
}

/// 完整 import 管线:Acquire → Parse → legalize → lower → (dry-run 提前返回 / commit)。
/// 对应 CLI 的 `ctxrelay import` 子命令。
///
/// Dest 解析目前硬编码走 `resolve_claude_code_dest`,只对 `claude-code` 这一个
/// backend 有意义——等第二个 backend(比如 `be-codex`)落地、它的目录发现逻辑
/// 大概率完全不同的时候,再决定要不要抽一个通用接口,现在只有一个实现,抽象没有
/// 意义。
pub fn run_import(registry: &Registry, source: SourceRef, opts: ImportOptions) -> Result<Manifest> {
    let doc = run_ir(registry, source)?;
    commit_document(registry, doc, opts)
}

/// 跳过 Acquire,直接从已经到手的字节(比如浏览器扩展 POST 过来的内容)走
/// Parse → legalize → lower → commit。对应 CLI 的 `ctxrelay listen` 子命令收到一次
/// 抓取之后要做的事。
///
/// `frontend_id` 用来在 Registry 里找对应的 Parse(比如 `"fe-claude-live"`)——这里
/// 不经过 `find_acquire`,因为压根没有一个 `SourceRef` 描述"浏览器刚刚 POST 给我
/// 的这段字节",跳过 Acquire 直接查 Parse 是唯一说得通的路径。
pub fn run_import_from_bytes(
    registry: &Registry,
    raw: Vec<u8>,
    frontend_id: &str,
    opts: ImportOptions,
) -> Result<Manifest> {
    let parse = registry.find_parse(frontend_id).ok_or_else(|| {
        CoreError(format!(
            "no Parse registered for frontend id {frontend_id:?}"
        ))
    })?;
    let doc = parse.parse(raw).map_err(|e| CoreError(e.to_string()))?;
    commit_document(registry, doc, opts)
}

fn commit_document(registry: &Registry, doc: Document, opts: ImportOptions) -> Result<Manifest> {
    let ir_digest = document_digest(&doc);

    let backend = registry.find_backend(&opts.backend_name).ok_or_else(|| {
        CoreError(format!(
            "no backend registered with name {:?}",
            opts.backend_name
        ))
    })?;

    let (legalized, report) = backend
        .legalize(&doc)
        .map_err(|e| CoreError(e.to_string()))?;
    let lowered = backend
        .lower(&legalized)
        .map_err(|e| CoreError(e.to_string()))?;

    if opts.dry_run {
        return Ok(Manifest {
            ir_digest,
            target: backend.target(),
            writes: vec![],
            created_session_ids: vec![lowered.session_id],
            report,
            cwd: opts.project_dir,
        });
    }

    let dest = resolve_claude_code_dest(
        &opts.project_dir,
        &opts.claude_projects_root,
        &opts.cli_version,
        opts.allow_bootstrap,
    )
    .map_err(|e| CoreError(e.to_string()))?;

    backend
        .commit(lowered, &dest, report, ir_digest)
        .map_err(|e| CoreError(e.to_string()))
}
