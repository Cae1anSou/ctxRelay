# ctxrelay-frontend trait 骨架 + fe-claude-share(V1)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 `docs/architecture.md` §4 定义 `ctxrelay-frontend` 的 `Acquire`/`Parse` trait 骨架,并实现第一个 frontend `fe-claude-share`(V1):Acquire 只支持人工另存为的 `SourceRef::File`(URL 自动抓取留给 §12 步骤 6 的浏览器扩展方案),Parse 把 claude.ai 分享快照(`chat_snapshots`)JSON 解析为中立 IR `Document`。

**Architecture:** `ctxrelay-frontend` crate 只放 trait 定义(`SourceRef`/`Acquire`/`Parse`/错误类型),依赖 `ctxrelay-ir`,不依赖任何具体 frontend。`fe-claude-share` crate 依赖 `ctxrelay-ir` + `ctxrelay-frontend`,内部拆成 `parse.rs`(私有 raw JSON 结构体 + `Parse` 实现)和 `acquire.rs`(`Acquire` 实现,V1 只读文件)。Parse 的字段映射基于用户提供的一份真实 claude.ai 分享快照样例(已确认结构),遇到未识别的 content block 类型一律归一成 `Block::foreign_action`,不臆测未见过的结构(例如 thinking/tool_use 目前样例中没有出现,不假装支持)。

**Tech Stack:** Rust (edition 2021),serde + serde_json(解析 JSON),semver(构造 `ir_version`),time(带 `parsing` feature,解析 RFC3339 时间戳)。

---

## 已确认的真实数据结构(来自用户提供的样例,`tmp/example.json`)

```json
{
  "uuid": "5492c7eb-...",
  "conversation_uuid": "fca79960-...",
  "created_at": "2026-07-06T02:13:13.024816Z",
  "updated_at": "2026-07-06T02:13:13.024816Z",
  "snapshot_name": "...",
  "created_by": "...",
  "creator": { "uuid": "...", "full_name": "..." },
  "project_uuid": null,
  "chat_messages": [
    {
      "uuid": "019f2a73-bc00-7057-a549-98a974fc8677",
      "text": "",
      "content": [
        {
          "start_timestamp": "2026-07-04T00:07:43.189641Z",
          "stop_timestamp": "2026-07-04T00:07:43.189641Z",
          "flags": null,
          "type": "text",
          "text": "...",
          "citations": []
        }
      ],
      "sender": "human",
      "index": 0,
      "created_at": "2026-07-04T00:07:43.189767Z",
      "updated_at": "2026-07-04T00:07:43.189767Z",
      "input_mode": "text",
      "truncated": false,
      "stop_reason": null,
      "compaction_summary": null,
      "attachments": [],
      "files": [],
      "parent_message_uuid": "00000000-0000-4000-8000-000000000000",
      "image_count": 0,
      "file_count": 0
    }
  ],
  "up_to_date": true,
  "is_public": true
}
```

关键观察(直接决定下面的字段映射,不是猜测):
- `chat_messages` 已经是按 `index` 线性递增、通过 `parent_message_uuid` 首尾相接的单链——说明分享快照**已经只保留了被选中的分支**,不需要自己重建树,只需按 `index` 排序即可还原顺序。
- `sender` 只观察到 `"human"`/`"assistant"` 两种取值。
- `content` 是数组,每项都有 `type` 字段(样例中只出现过 `"text"`)和(当 `type=="text"` 时)对应的 `text` 字段。**没有观察到 thinking/tool_use/artifact 类型的 content block**——所以 Parse 对未识别的 `type` 一律归一成 `Block::foreign_action`,不假装认识一个没见过的结构。
- 顶层和消息级都有 `created_at`/`updated_at`,但没有任何字段记录"这份 JSON 是什么时候被另存为到本地的"——所以 `SourceProvenance.exported_at` 在 V1 里如实填 `None`,不用 `conversation` 的 `updated_at` 冒充"导出时间"(那是两个不同的时间点,冒充是编造),也不用当前系统时间(那会让 Parse 不再是纯函数,违反 §4 的契约)。
- 消息级没有 model 字段,所以 `Origin.model` 填 `None`。

---

## File Structure

```
ctxRelay/
├── crates/
│   ├── ctxrelay-ir/                    # 已存在,不改
│   ├── ctxrelay-frontend/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs                  # SourceRef, Acquire, Parse, FrontendError, RawBytes, Result
│   └── fe-claude-share/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                  # 模块声明 + re-export ClaudeShareAcquire/ClaudeShareParse
│           ├── parse.rs                # 私有 raw JSON 结构体 + ClaudeShareParse + Parse 实现
│           └── acquire.rs              # ClaudeShareAcquire + Acquire 实现(V1 只支持 File)
│       └── tests/
│           ├── fixtures/
│           │   └── sample_snapshot.json    # 用户提供的真实样例,原样存放
│           ├── parse.rs                # Parse 集成测试
│           └── acquire.rs              # Acquire 集成测试
```

---

### Task 1: `ctxrelay-frontend` trait 骨架

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-frontend/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-frontend/src/lib.rs`

- [ ] **Step 1: 写 `crates/ctxrelay-frontend/Cargo.toml`**

```toml
[package]
name = "ctxrelay-frontend"
version = "0.1.0"
edition = "2021"
description = "ctxRelay frontend 契约:Acquire/Parse trait 定义。只依赖 ctxrelay-ir,不认识任何具体 frontend。"

[dependencies]
ctxrelay-ir = { path = "../ctxrelay-ir" }
```

- [ ] **Step 2: 写 `crates/ctxrelay-frontend/src/lib.rs`**

```rust
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
```

- [ ] **Step 3: 编译验证**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build -p ctxrelay-frontend`
Expected: `Compiling ctxrelay-frontend v0.1.0 (...)` 然后 `Finished`,无 error。

- [ ] **Step 4: 提交**

```bash
git add crates/ctxrelay-frontend
git commit -m "feat(frontend): define Acquire/Parse trait contract per architecture §4"
```

---

### Task 2: `fe-claude-share` crate 骨架

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/src/lib.rs`

- [ ] **Step 1: 写 `crates/fe-claude-share/Cargo.toml`**

```toml
[package]
name = "fe-claude-share"
version = "0.1.0"
edition = "2021"
description = "claude.ai 分享快照(chat_snapshots)frontend。V1 仅支持人工另存为文件的 Acquire;URL 自动抓取见架构文档 §12 步骤 6。"

[dependencies]
ctxrelay-ir = { path = "../ctxrelay-ir" }
ctxrelay-frontend = { path = "../ctxrelay-frontend" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
semver = "1"
time = { version = "0.3", features = ["parsing"] }
```

- [ ] **Step 2: 写最小 `crates/fe-claude-share/src/lib.rs`(模块声明先行,内容在 Task 3/4 补全)**

```rust
//! claude.ai 分享快照(chat_snapshots)frontend。
//!
//! V1 范围:Acquire 只支持人工从浏览器另存为的本地文件(`SourceRef::File`)。
//! URL 自动抓取(浏览器扩展 + 本地桥)见架构文档 §12 步骤 6,尚未实现。

mod acquire;
mod parse;

pub use acquire::ClaudeShareAcquire;
pub use parse::ClaudeShareParse;
```

- [ ] **Step 3: 编译检查(此时 `acquire.rs`/`parse.rs` 还不存在,这是预期的中间态,不是本任务要修的问题)**

跳过编译验证——Task 3/4 会把 `parse.rs`/`acquire.rs` 补上后再统一验证整个 crate 编译。

- [ ] **Step 4: 提交**

```bash
git add crates/fe-claude-share/Cargo.toml crates/fe-claude-share/src/lib.rs
git commit -m "chore(fe-claude-share): scaffold crate manifest and module declarations"
```

---

### Task 3: Parse 实现(claude.ai 分享快照 JSON → IR `Document`)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/tests/fixtures/sample_snapshot.json`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/tests/parse.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/src/parse.rs`

- [ ] **Step 1: 把用户提供的真实样例复制到 fixture 路径,原样存放,不脱敏、不裁剪**

把 `/Users/caoxinzhuo/code/ctxRelay/tmp/example.json` 的完整内容复制到 `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/tests/fixtures/sample_snapshot.json`(逐字节相同,包括其中的中文对话内容)。

Run: `mkdir -p /Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/tests/fixtures && cp /Users/caoxinzhuo/code/ctxRelay/tmp/example.json /Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/tests/fixtures/sample_snapshot.json`

- [ ] **Step 2: 写失败的集成测试 `crates/fe-claude-share/tests/parse.rs`**

```rust
use ctxrelay_ir::{Block, Role, TurnId};
use ctxrelay_frontend::Parse;
use fe_claude_share::ClaudeShareParse;
use semver::Version;

#[test]
fn parses_real_claude_share_snapshot() {
    let raw = std::fs::read("tests/fixtures/sample_snapshot.json").expect("fixture must exist");

    let doc = ClaudeShareParse.parse(raw).expect("should parse real snapshot");

    assert_eq!(doc.ir_version, Version::new(0, 1, 0));
    assert_eq!(doc.source.vendor, "anthropic");
    assert_eq!(doc.source.surface, "claude.ai");
    assert_eq!(doc.source.exported_at, None);

    assert_eq!(doc.turns.len(), 4);

    assert_eq!(doc.turns[0].id, TurnId("019f2a73-bc00-7057-a549-98a974fc8677".to_string()));
    assert_eq!(doc.turns[0].role, Role::User);
    assert_eq!(doc.turns[0].origin.vendor, "anthropic");
    assert_eq!(doc.turns[0].origin.surface, "claude.ai");
    assert_eq!(doc.turns[0].origin.model, None);
    assert_eq!(doc.turns[0].blocks.len(), 1);
    match &doc.turns[0].blocks[0] {
        Block::Text { content } => assert!(content.starts_with("我想做做一个科研copilot")),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(doc.turns[1].role, Role::Assistant);
    assert_eq!(doc.turns[2].role, Role::User);
    assert_eq!(doc.turns[3].role, Role::Assistant);

    match &doc.turns[3].blocks[0] {
        Block::Text { content } => assert!(content.starts_with("先把那个\"MCP 套 MCP\"的顾虑拆掉")),
        other => panic!("expected Text block, got {other:?}"),
    }
}
```

- [ ] **Step 3: 运行,预期失败(`fe_claude_share`/`ClaudeShareParse` 还没实现 Parse 逻辑)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p fe-claude-share --test parse`
Expected: 编译错误或运行时错误(`ClaudeShareParse` 类型还不存在具体实现细节,或 `parse` 方法未定义具体行为)。这是预期的红灯,不是 bug。

- [ ] **Step 4: 实现 `crates/fe-claude-share/src/parse.rs`**

```rust
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
```

- [ ] **Step 5: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p fe-claude-share --test parse`
Expected: `test parses_real_claude_share_snapshot ... ok`

- [ ] **Step 6: 提交**

```bash
git add crates/fe-claude-share/src/parse.rs crates/fe-claude-share/tests/parse.rs crates/fe-claude-share/tests/fixtures/sample_snapshot.json
git commit -m "feat(fe-claude-share): implement Parse for claude.ai chat_snapshots JSON"
```

---

### Task 4: Acquire 实现(V1 只支持人工另存为的文件)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/tests/acquire.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-share/src/acquire.rs`

- [ ] **Step 1: 写失败的集成测试 `crates/fe-claude-share/tests/acquire.rs`**

```rust
use ctxrelay_frontend::{Acquire, SourceRef};
use fe_claude_share::ClaudeShareAcquire;
use std::path::PathBuf;

#[test]
fn accepts_file_but_not_url() {
    let acquire = ClaudeShareAcquire;
    assert!(acquire.accepts(&SourceRef::File(PathBuf::from("tests/fixtures/sample_snapshot.json"))));
    assert!(!acquire.accepts(&SourceRef::Url("https://claude.ai/share/xyz".to_string())));
}

#[test]
fn reads_bytes_from_file() {
    let acquire = ClaudeShareAcquire;
    let path = PathBuf::from("tests/fixtures/sample_snapshot.json");
    let expected = std::fs::read(&path).expect("fixture must exist");

    let raw = acquire.acquire(SourceRef::File(path)).expect("should read fixture file");

    assert_eq!(raw, expected);
}

#[test]
fn url_acquire_returns_error_not_implemented() {
    let acquire = ClaudeShareAcquire;
    let result = acquire.acquire(SourceRef::Url("https://claude.ai/share/xyz".to_string()));
    assert!(result.is_err());
}
```

- [ ] **Step 2: 运行,预期失败(`ClaudeShareAcquire` 还没有具体实现)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p fe-claude-share --test acquire`
Expected: 编译错误(`ClaudeShareAcquire` 尚未定义具体行为)。这是预期的红灯。

- [ ] **Step 3: 实现 `crates/fe-claude-share/src/acquire.rs`**

```rust
use ctxrelay_frontend::{Acquire, FrontendError, RawBytes, Result, SourceRef};

/// claude.ai 分享快照的 Acquire 实现。
///
/// V1 范围:只支持 `SourceRef::File`(人工从浏览器另存为)。`SourceRef::Url`
/// 的自动抓取需要浏览器扩展 + 本地桥(架构文档 §12 步骤 6),尚未实现——
/// 这里明确报错而不是假装支持,避免调用方以为传个分享链接就能work。
pub struct ClaudeShareAcquire;

impl Acquire for ClaudeShareAcquire {
    fn id(&self) -> &'static str {
        "fe-claude-share"
    }

    fn accepts(&self, input: &SourceRef) -> bool {
        matches!(input, SourceRef::File(_))
    }

    fn acquire(&self, input: SourceRef) -> Result<RawBytes> {
        match input {
            SourceRef::File(path) => std::fs::read(&path)
                .map_err(|e| FrontendError(format!("failed to read {}: {e}", path.display()))),
            SourceRef::Url(url) => Err(FrontendError(format!(
                "fe-claude-share V1 只支持人工另存为文件(SourceRef::File);\
                 URL 自动抓取见架构文档 §12 步骤 6,尚未实现。收到的 URL: {url}"
            ))),
        }
    }
}
```

- [ ] **Step 4: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p fe-claude-share --test acquire`
Expected: 3 个测试全部 `ok`。

- [ ] **Step 5: 提交**

```bash
git add crates/fe-claude-share/src/acquire.rs crates/fe-claude-share/tests/acquire.rs
git commit -m "feat(fe-claude-share): implement Acquire for manual save-as file (SourceRef::File only)"
```

---

### Task 5: 收尾验证

**Files:** 无新文件,只验证。

- [ ] **Step 1: 整个 workspace 编译**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build --workspace 2>&1 | tail -10`
Expected: `Finished`,无 error。

- [ ] **Step 2: 整个 workspace 测试全绿**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test --workspace 2>&1 | tail -40`
Expected: `ctxrelay-ir` 的既有测试(fixture / roundtrip / foreign_action_is_never_replayable)+ `fe-claude-share` 的新测试(`parses_real_claude_share_snapshot` / `accepts_file_but_not_url` / `reads_bytes_from_file` / `url_acquire_returns_error_not_implemented`)全部 `ok`。

- [ ] **Step 3: 确认依赖图符合 §10 约束——`fe-claude-share` 只依赖 `ir`/`frontend`,不依赖任何 backend(目前还没有 backend crate,这一步验证的是"没有意外引入的依赖")**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo tree -p fe-claude-share --depth 1`
Expected: 输出里只包含 `ctxrelay-ir`、`ctxrelay-frontend`、`serde`、`serde_json`、`semver`、`time` 这几个直接依赖,没有其他 workspace 内 crate。

- [ ] **Step 4: 最终提交(如果前面步骤有任何未提交的修正)**

```bash
git status --short
```
若有改动:
```bash
git add -A
git commit -m "chore: verify frontend trait + fe-claude-share workspace state"
```

---

## 完成后的状态

- `ctxrelay-frontend` 是纯 trait 定义 crate,只依赖 `ctxrelay-ir`。
- `fe-claude-share` 是第一个可用的 frontend:Acquire 支持人工另存为文件,Parse 能把真实 claude.ai 分享快照解析成 IR,并有基于真实数据的固定测试兜底。
- 已知、公开记录的限制(不是隐藏的坑):URL 自动抓取未实现;分享快照是否含 thinking 类型的 content block 尚未被真实样例验证过,遇到会被归一成 `ForeignAction` 而不是报错或崩溃。
- 下一步(不在本计划范围内):架构文档 §12 步骤 3——`be-claude-code` backend,把 IR lower 成 Claude Code 的 JSONL 会话格式,那时候会开一份新计划。
