use be_claude_code::ClaudeCodeBackend;
use ctxrelay_backend::Backend;
use ctxrelay_frontend::{Acquire, Parse, SourceRef};
use fe_claude_live::ClaudeLiveParse;
use fe_claude_share::{ClaudeShareAcquire, ClaudeShareParse};

/// 薄注册表(架构文档 §7:"core 是一个薄 driver + frontend/backend 注册表")。
///
/// `Acquire` 和 `Parse` 按 `id()` 配对——同一个 frontend(比如 `fe-claude-share`)
/// 的 `Acquire`/`Parse` 实现共用同一个 id 字符串,这是它们"属于同一个 frontend"
/// 的唯一联系,注册表不做任何更聪明的匹配。
pub struct Registry {
    acquirers: Vec<Box<dyn Acquire>>,
    parsers: Vec<Box<dyn Parse>>,
    backends: Vec<Box<dyn Backend>>,
}

impl Registry {
    /// V1 只有一个 frontend(`fe-claude-share`)和一个 backend(`be-claude-code`)。
    /// 加第二个的时候,这里加一行就够,不需要动 `Registry` 本身的结构——这正是
    /// §10 说的"v0:编译期注册表 + trait object,足够"。
    pub fn with_defaults() -> Self {
        Registry {
            acquirers: vec![Box::new(ClaudeShareAcquire)],
            // fe-claude-live 只注册 Parse,不注册 Acquire——它没有实现 Acquire trait
            // (数据来源是浏览器扩展主动 POST,不是 ctxrelay 主动拉取,见
            // fe-claude-live/src/lib.rs 顶部的文档注释)。
            parsers: vec![Box::new(ClaudeShareParse), Box::new(ClaudeLiveParse)],
            backends: vec![Box::new(ClaudeCodeBackend)],
        }
    }

    pub fn find_acquire(&self, source: &SourceRef) -> Option<&dyn Acquire> {
        self.acquirers
            .iter()
            .find(|a| a.accepts(source))
            .map(|b| b.as_ref())
    }

    pub fn find_parse(&self, frontend_id: &str) -> Option<&dyn Parse> {
        self.parsers
            .iter()
            .find(|p| p.id() == frontend_id)
            .map(|b| b.as_ref())
    }

    pub fn find_backend(&self, name: &str) -> Option<&dyn Backend> {
        self.backends
            .iter()
            .find(|b| b.target().tool == name)
            .map(|b| b.as_ref())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
