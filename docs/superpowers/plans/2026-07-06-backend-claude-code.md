# ctxrelay-backend trait 骨架 + be-claude-code Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 `docs/architecture.md` §5/§8 定义 `ctxrelay-backend` 的 `Backend` trait 骨架(`TargetSpec`/`CapPolicy`/`LoweringReport`/`LoweredSession`/`Dest`/`Manifest`),并实现第一个 backend `be-claude-code`:把中立 IR `Document` 通过 `legalize → lower → commit` 三段式,变成 Claude Code 能 `--resume` 加载的 `~/.claude/projects/<slug>/*.jsonl` 会话文件。

**Architecture:** `ctxrelay-backend` crate 只放 trait 定义,依赖 `ctxrelay-ir`,不认识任何具体 backend。`be-claude-code` crate 依赖 `ctxrelay-ir` + `ctxrelay-backend`,内部拆成三个职责单一的模块:`legalize.rs`(丢弃/内联不合法的 IR 构造)、`lower.rs`(纯函数,IR → JSONL 记录的内存表示,session_id/turn_uuid 全部确定性派生,不依赖环境/随机数)、`commit.rs`(唯一写盘处,把环境信息——cwd/gitBranch/version——盖到每条记录上再落盘,产出 `Manifest`)。字段映射基于对 Claude Code 真实会话 JSONL 格式的实测(见下一节),不是猜测。

**Tech Stack:** Rust (edition 2021),serde_json(构造 JSONL 记录),uuid(v5 feature,确定性派生 turn/session UUID),sha2(Manifest 的写入内容摘要),time(格式化时间戳为 RFC3339)。

---

## 已实测确认的 Claude Code 会话格式

在写这份计划前,直接用本机已安装的真实 `claude` CLI(版本 2.1.201)做了三次实测,不是从文档或猜测得出:

**实测 1:目录 slug 编码规则**——在 `/Users/caoxinzhuo/code/ctxRelay/tmp/resume-test` 里跑 `claude --session-id <uuid> -p "hi"`,观察到它在 `~/.claude/projects/` 下新建的目录是 `-Users-caoxinzhuo-code-ctxRelay-tmp-resume-test`——对不含冒号/空格/盘符的普通 Unix 路径,就是简单的把 `/` 替换成 `-`。**这条规则只在这次实测覆盖的路径形状下成立**,架构文档 §5 已经警告过更复杂的路径(冒号/空格/盘符)编码规则不应该被反向工程——所以 `be-claude-code` 的 `commit()` 不会自己算 slug,这仍然是调用方(未来的 core/cli)通过 `Dest::session_dir` 传入的信息。

**实测 2:最小必需字段集合**——手写了一份不含 `usage`/`diagnostics`/hook 相关字段的精简 JSONL(只有 `parentUuid`/`isSidechain`/`type`/`message`/`uuid`/`timestamp`/`userType`/`cwd`/`sessionId`/`version`/`gitBranch`),放进正确目录后跑 `claude --resume <uuid> -p "我之前告诉你的暗号是什么?"`,真实模型正确答出了埋在 JSONL 里的暗号"紫色的长颈鹿在下棋"。这证明这份精简字段集合就足够让 `--resume` 正常工作,不需要 `usage`/`diagnostics`/`requestId`/`promptId` 这些遥测字段。

**实测 3:`message.content` 的数组形式**——第二次实测把 user turn 的 `content` 从纯字符串换成 `[{"type":"text","text":"..."}]` 数组形式,同样能被正确 resume(模型正确答出第二个暗号"蓝色的企鹅在读诗")。这确认 user/assistant 两种角色可以用统一的"content 是一个 block 数组"的数据结构,不需要为 user 角色特殊处理成纯字符串。

**实测中未确认、明确标注为限制的点**:
- 本机安装版本没有观察到任何 `sessions-index.json` 文件参与 resume 流程,`commit()` 因此不写这个文件——但这是"这个版本、这次实测"的观察,不同版本可能不同(这正是 `TargetSpec.version_range` 存在的意义:版本漂移时应该开一个新 backend,而不是在这里加 `if version < X`)。
- Reasoning(thinking)block 的真实 signature 字节:IR 当前的 `BlockCaps.verifiable_signature` 只是一个 bool,没有字段能装下真实的 thinking signature——所以这次 `legalize` 对所有 Reasoning 一律丢弃,不区分 `verifiable_signature` 是否为 true。这是当前 IR 的已知限制,不是这次实现的疏漏,已经在下面的代码注释里如实写明。

---

## File Structure

```
ctxRelay/
├── crates/
│   ├── ctxrelay-ir/                    # 已存在,不改
│   ├── ctxrelay-frontend/              # 已存在,不改
│   ├── fe-claude-share/                # 已存在,不改
│   ├── ctxrelay-backend/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs                  # Backend trait + TargetSpec/CapPolicy/LoweringReport/LoweredSession/Dest/Manifest/WriteRecord/BackendError
│   └── be-claude-code/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs                  # ClaudeCodeBackend + impl Backend,委托给三个子模块
│           ├── legalize.rs             # 丢 Reasoning、内联 ForeignAction、插入 preamble turn
│           ├── lower.rs                # 纯函数:IR → JSONL 记录(内存态),确定性 UUID
│           └── commit.rs               # 唯一写盘处:盖环境信息 + 写文件 + 产出 Manifest
│       └── tests/
│           ├── legalize.rs
│           ├── lower.rs
│           ├── commit.rs
│           └── conformance.rs          # 真实调用 claude --resume,默认 #[ignore],要花真实 API 额度
```

---

### Task 1: `ctxrelay-backend` trait 骨架

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-backend/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-backend/src/lib.rs`

- [ ] **Step 1: 写 `crates/ctxrelay-backend/Cargo.toml`**

```toml
[package]
name = "ctxrelay-backend"
version = "0.1.0"
edition = "2021"
description = "ctxRelay backend 契约:Backend trait 定义 + TargetSpec/CapPolicy/LoweringReport/LoweredSession/Dest/Manifest。只依赖 ctxrelay-ir。"

[dependencies]
ctxrelay-ir = { path = "../ctxrelay-ir" }
serde_json = "1"
```

- [ ] **Step 2: 写 `crates/ctxrelay-backend/src/lib.rs`**

```rust
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
```

- [ ] **Step 3: 编译验证**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build -p ctxrelay-backend`
Expected: `Compiling ctxrelay-backend v0.1.0 (...)` 然后 `Finished`,无 error。

- [ ] **Step 4: 提交**

```bash
git add crates/ctxrelay-backend
git commit -m "feat(backend): define Backend trait contract per architecture §5/§8"
```

---

### Task 2: `be-claude-code` crate 骨架(空壳,先不接 trait)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/lib.rs`

- [ ] **Step 1: 写 `crates/be-claude-code/Cargo.toml`**

```toml
[package]
name = "be-claude-code"
version = "0.1.0"
edition = "2021"
description = "Claude Code backend:把 IR lower 成 ~/.claude/projects/<slug>/*.jsonl 会话记录。"

[dependencies]
ctxrelay-ir = { path = "../ctxrelay-ir" }
ctxrelay-backend = { path = "../ctxrelay-backend" }
serde_json = "1"
uuid = { version = "1", features = ["v5"] }
sha2 = "0.10"
time = { version = "0.3", features = ["formatting", "parsing"] }
```

- [ ] **Step 2: 写最小 `crates/be-claude-code/src/lib.rs`(先不声明任何子模块,保持这一步能独立编译)**

```rust
//! Claude Code backend:把 IR lower 成 `~/.claude/projects/<slug>/*.jsonl` 的会话记录。
//!
//! 目录 slug 的发现/解析不是这个 crate 的职责(架构文档 §5),`commit` 只管把
//! `LoweredSession` 写进调用方已经解析好的 `Dest::session_dir`。
//!
//! 三个子模块(legalize/lower/commit)在后续任务里逐个加入,`ClaudeCodeBackend` 的
//! `impl Backend` 要等三个子模块都存在后才接线(见 Task 5)。
```

- [ ] **Step 3: 编译验证**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build -p be-claude-code`
Expected: `Compiling be-claude-code v0.1.0 (...)` 然后 `Finished`,无 error(此时 crate 里除了一句模块级文档注释什么都没有,应该干净编译)。

- [ ] **Step 4: 提交**

```bash
git add crates/be-claude-code/Cargo.toml crates/be-claude-code/src/lib.rs
git commit -m "chore(be-claude-code): scaffold crate manifest"
```

---

### Task 3: `legalize` 实现(丢 Reasoning、内联 ForeignAction、插入 preamble)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/legalize.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/tests/legalize.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/lib.rs`

- [ ] **Step 1: 写失败的集成测试 `crates/be-claude-code/tests/legalize.rs`**

```rust
use be_claude_code::legalize::legalize;
use ctxrelay_ir::{Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;

fn sample_document() -> Document {
    Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![
            Turn {
                id: TurnId("t1".to_string()),
                role: Role::User,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: None,
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![Block::Text { content: "你好".to_string() }],
                timestamp: None,
            },
            Turn {
                id: TurnId("t2".to_string()),
                role: Role::Assistant,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: Some("opus-4".to_string()),
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![
                    Block::Reasoning {
                        content: "内部推理过程".to_string(),
                        caps: BlockCaps {
                            reasoning: true,
                            verifiable_signature: false,
                            replayable: false,
                        },
                    },
                    Block::foreign_action(
                        "web_search",
                        Some("搜索了 rust uuid v5".to_string()),
                        Some(Artifact {
                            media: "application/json".to_string(),
                            content: "{\"query\":\"rust uuid v5\"}".to_string(),
                        }),
                        false,
                        false,
                    ),
                    Block::Text { content: "根据搜索结果...".to_string() },
                ],
                timestamp: None,
            },
        ],
    }
}

#[test]
fn drops_reasoning_and_inlines_foreign_action() {
    let doc = sample_document();
    let (legalized, report) = legalize(&doc);

    assert_eq!(report.dropped_reasoning, 1);
    assert_eq!(report.inlined_foreign_actions, 1);

    // turns[0] 是合成的 preamble,原始两轮各自往后挪一位
    assert_eq!(legalized.turns.len(), 3);

    match &legalized.turns[0].blocks[0] {
        Block::Text { content } => assert!(content.contains("anthropic") && content.contains("claude.ai")),
        other => panic!("expected preamble Text block, got {other:?}"),
    }

    assert_eq!(legalized.turns[1].id, TurnId("t1".to_string()));
    match &legalized.turns[1].blocks[0] {
        Block::Text { content } => assert_eq!(content, "你好"),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(legalized.turns[2].id, TurnId("t2".to_string()));
    assert_eq!(legalized.turns[2].blocks.len(), 2);
    match &legalized.turns[2].blocks[0] {
        Block::Text { content } => {
            assert!(content.contains("web_search"));
            assert!(content.contains("rust uuid v5"));
        }
        other => panic!("expected inlined ForeignAction as Text, got {other:?}"),
    }
    match &legalized.turns[2].blocks[1] {
        Block::Text { content } => assert_eq!(content, "根据搜索结果..."),
        other => panic!("expected Text block, got {other:?}"),
    }
}
```

- [ ] **Step 2: 运行,预期失败(`legalize` 模块还不存在)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code --test legalize`
Expected: 编译错误(找不到 `be_claude_code::legalize` 模块)。这是预期的红灯。

- [ ] **Step 3: 在 `crates/be-claude-code/src/lib.rs` 里加一行模块声明**

```rust
pub mod legalize;
```

追加到文件末尾即可,顶部的模块级文档注释不用动。

- [ ] **Step 4: 写 `crates/be-claude-code/src/legalize.rs`**

```rust
use ctxrelay_backend::LoweringReport;
use ctxrelay_ir::{Block, Document, Origin, Role, Turn, TurnId};

/// 把 IR 合法化成 Claude Code 能接受的形状:
///
/// - `Reasoning` 一律丢弃,不管 `caps.verifiable_signature` 是否为 true——IR 当前
///   没有字段能装下真实的 thinking signature 字节,即使某个 Reasoning 标记为"可
///   验证",也无法安全重建一个能通过 Claude API 校验的 thinking block,强行塞一个
///   自造的 signature 只会触发 `400 Invalid signature in thinking block`。这是
///   当前 IR 的已知限制,不是这里的疏漏。
/// - `ForeignAction` 内联成 `Text`,内容(kind/summary/artifact)一字不丢,只剥掉
///   工具外壳。
/// - 在最前面插入一条 preamble turn,交代这是从 Web 对话迁移的讨论。
pub fn legalize(doc: &Document) -> (Document, LoweringReport) {
    let mut report = LoweringReport::default();
    let mut turns = Vec::with_capacity(doc.turns.len() + 1);

    turns.push(preamble_turn(doc));

    for turn in &doc.turns {
        let mut blocks = Vec::with_capacity(turn.blocks.len());
        for block in &turn.blocks {
            match block {
                Block::Reasoning { .. } => {
                    report.dropped_reasoning += 1;
                }
                Block::ForeignAction { kind, summary, artifact, .. } => {
                    let mut text = format!("[外部操作: {kind}]");
                    if let Some(summary) = summary {
                        text.push('\n');
                        text.push_str(summary);
                    }
                    if let Some(artifact) = artifact {
                        text.push('\n');
                        text.push_str(&artifact.content);
                    }
                    blocks.push(Block::Text { content: text });
                    report.inlined_foreign_actions += 1;
                }
                other => blocks.push(other.clone()),
            }
        }
        turns.push(Turn {
            id: turn.id.clone(),
            role: turn.role,
            origin: turn.origin.clone(),
            blocks,
            timestamp: turn.timestamp,
        });
    }

    report
        .notes
        .push("已在最前插入 preamble turn,说明这是从 Web 对话导入的讨论".to_string());

    let legalized = Document {
        ir_version: doc.ir_version.clone(),
        source: doc.source.clone(),
        turns,
    };

    (legalized, report)
}

fn preamble_turn(doc: &Document) -> Turn {
    let text = format!(
        "以下为从 {} ({}) 导入的讨论,工具调用已内联为文本,从此处继续。",
        doc.source.vendor, doc.source.surface
    );
    Turn {
        id: TurnId("preamble".to_string()),
        role: Role::User,
        origin: Origin {
            vendor: doc.source.vendor.clone(),
            model: None,
            surface: doc.source.surface.clone(),
        },
        blocks: vec![Block::Text { content: text }],
        timestamp: None,
    }
}
```

- [ ] **Step 5: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code --test legalize`
Expected: `test drops_reasoning_and_inlines_foreign_action ... ok`

- [ ] **Step 6: 提交**

```bash
git add crates/be-claude-code/src/lib.rs crates/be-claude-code/src/legalize.rs crates/be-claude-code/tests/legalize.rs
git commit -m "feat(be-claude-code): implement legalize (drop Reasoning, inline ForeignAction, insert preamble)"
```

---

### Task 4: `lower` 实现(纯函数,IR → JSONL 记录)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/lower.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/tests/lower.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/lib.rs`

- [ ] **Step 1: 写失败的集成测试 `crates/be-claude-code/tests/lower.rs`**

```rust
use be_claude_code::lower::lower;
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;

fn legalized_document() -> Document {
    Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![
            Turn {
                id: TurnId("t1".to_string()),
                role: Role::User,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: None,
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![Block::Text { content: "暗号是紫色的长颈鹿".to_string() }],
                timestamp: None,
            },
            Turn {
                id: TurnId("t2".to_string()),
                role: Role::Assistant,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: Some("claude-sonnet-5".to_string()),
                    surface: "claude.ai".to_string(),
                },
                blocks: vec![Block::Text { content: "记住了。".to_string() }],
                timestamp: None,
            },
        ],
    }
}

#[test]
fn lowers_turns_into_chained_jsonl_records() {
    let doc = legalized_document();
    let lowered = lower(&doc).expect("lower should succeed");

    assert_eq!(lowered.lines.len(), 2);

    let first = &lowered.lines[0];
    assert_eq!(first["type"], "user");
    assert_eq!(first["parentUuid"], serde_json::Value::Null);
    assert_eq!(first["message"]["role"], "user");
    assert_eq!(first["message"]["content"][0]["type"], "text");
    assert_eq!(first["message"]["content"][0]["text"], "暗号是紫色的长颈鹿");

    let second = &lowered.lines[1];
    assert_eq!(second["type"], "assistant");
    assert_eq!(second["parentUuid"], first["uuid"]);
    assert_eq!(second["message"]["role"], "assistant");
    assert_eq!(second["message"]["content"][0]["text"], "记住了。");
}

#[test]
fn lower_is_deterministic() {
    let doc = legalized_document();
    let a = lower(&doc).expect("lower should succeed");
    let b = lower(&doc).expect("lower should succeed");

    assert_eq!(a.session_id, b.session_id);
    assert_eq!(a.ir_digest, b.ir_digest);
    assert_eq!(a.lines, b.lines);
}
```

- [ ] **Step 2: 运行,预期失败(`lower` 模块还不存在)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code --test lower`
Expected: 编译错误(找不到 `be_claude_code::lower` 模块)。这是预期的红灯。

- [ ] **Step 3: 在 `crates/be-claude-code/src/lib.rs` 里追加一行模块声明**

```rust
pub mod lower;
```

- [ ] **Step 4: 写 `crates/be-claude-code/src/lower.rs`**

```rust
use ctxrelay_backend::LoweredSession;
use ctxrelay_ir::{Block, Document, Role};
use serde_json::{json, Value};
use sha2::Digest;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// 项目私有的固定命名空间,只用来确定性派生 UUID——不是真实的 DNS/URL/OID 命名空间,
/// 只是一个稳定常量,保证同样的输入永远派生出同样的 UUID(这样 `lower` 才能是纯函数)。
const NAMESPACE: Uuid = Uuid::from_bytes([
    0x6a, 0x1e, 0xd6, 0x9b, 0x0c, 0x3a, 0x4b, 0x1d, 0x9e, 0x77, 0x2f, 0x51, 0x8c, 0xaa, 0x03, 0x77,
]);

fn turn_uuid(turn_id: &str) -> Uuid {
    Uuid::new_v5(&NAMESPACE, turn_id.as_bytes())
}

/// session_id 和 ir_digest 都从文档内容本身确定性派生,不引入随机数或系统时间——
/// 这保证同一份 `Document` 无论何时、在哪台机器上 lower,产出完全一致(可缓存、可 diff)。
fn document_digest(doc: &Document) -> Vec<u8> {
    serde_json::to_vec(doc).expect("Document serialization is infallible")
}

/// Claude Code 的 JSONL 记录只有 `"user"`/`"assistant"` 两种 `type`,没有独立的
/// system 角色记录类型(实测确认,见本计划开头);`Role::System` 目前也没有任何
/// frontend 真的产出过。把它并到 `"user"` 是"最接近的可用槽位"这个诚实的近似,
/// 不是精确建模——如果未来真的需要区分,应该重新评估这条映射。
fn role_str(role: Role) -> &'static str {
    match role {
        Role::User | Role::System => "user",
        Role::Assistant => "assistant",
    }
}

fn block_to_text(block: &Block) -> String {
    match block {
        Block::Text { content } => content.clone(),
        Block::Code { language, content } => match language {
            Some(lang) => format!("```{lang}\n{content}\n```"),
            None => format!("```\n{content}\n```"),
        },
        // legalize 已经把 Reasoning 丢弃、ForeignAction 内联成 Text,lower 不应该再见到
        // 它们;如果真的见到了,说明调用方跳过了 legalize,这是编程错误而不是数据问题,
        // 直接 panic 比静默生成一个内容缺失的会话更安全。
        other => panic!("lower() received un-legalized block: {other:?}"),
    }
}

/// 纯函数:把(已合法化的)IR `Document` 转成 Claude Code 的 JSONL 记录(内存态)。
///
/// 不填 `sessionId`/`cwd`/`gitBranch`/`version`/`userType` 这几个反映"写盘时环境"的
/// 字段——那些交给 `commit` 在真正落盘前盖上去,这样 `lower` 才不需要知道任何环境信息,
/// 保持纯。
pub fn lower(doc: &Document) -> ctxrelay_backend::Result<LoweredSession> {
    let digest_bytes = document_digest(doc);
    let session_id = Uuid::new_v5(&NAMESPACE, &digest_bytes).to_string();
    let ir_digest = format!("{:x}", sha2::Sha256::digest(&digest_bytes));

    let mut lines = Vec::with_capacity(doc.turns.len());
    let mut previous_uuid: Option<String> = None;

    for turn in &doc.turns {
        let uuid = turn_uuid(&turn.id.0).to_string();
        let role = role_str(turn.role);

        let content: Vec<Value> = turn
            .blocks
            .iter()
            .map(|b| json!({ "type": "text", "text": block_to_text(b) }))
            .collect();

        let timestamp = turn
            .timestamp
            .map(|t| t.format(&Rfc3339).expect("valid OffsetDateTime always formats"))
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

        let parent_uuid = match &previous_uuid {
            Some(p) => Value::String(p.clone()),
            None => Value::Null,
        };

        let line = if role == "assistant" {
            json!({
                "parentUuid": parent_uuid,
                "isSidechain": false,
                "message": {
                    "model": turn.origin.model.clone().unwrap_or_else(|| "unknown".to_string()),
                    "id": format!("msg_{uuid}"),
                    "type": "message",
                    "role": "assistant",
                    "content": content,
                    "stop_reason": "end_turn",
                    "stop_sequence": Value::Null,
                },
                "type": "assistant",
                "uuid": uuid,
                "timestamp": timestamp,
            })
        } else {
            json!({
                "parentUuid": parent_uuid,
                "isSidechain": false,
                "type": "user",
                "message": { "role": "user", "content": content },
                "uuid": uuid,
                "timestamp": timestamp,
            })
        };

        previous_uuid = Some(uuid);
        lines.push(line);
    }

    Ok(LoweredSession { session_id, ir_digest, lines })
}
```

- [ ] **Step 5: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code --test lower`
Expected: 两个测试都 `ok`:`lowers_turns_into_chained_jsonl_records`、`lower_is_deterministic`。

- [ ] **Step 6: 提交**

```bash
git add crates/be-claude-code/src/lib.rs crates/be-claude-code/src/lower.rs crates/be-claude-code/tests/lower.rs
git commit -m "feat(be-claude-code): implement pure lower (deterministic UUIDs, no environment dependency)"
```

---

### Task 5: `commit` 实现 + 接线 `ClaudeCodeBackend`

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/commit.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/tests/commit.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/src/lib.rs`

- [ ] **Step 1: 写失败的集成测试 `crates/be-claude-code/tests/commit.rs`**

```rust
use be_claude_code::commit::commit;
use be_claude_code::lower::lower;
use ctxrelay_backend::{Dest, LoweringReport, TargetSpec};
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use sha2::Digest;
use std::path::PathBuf;

fn legalized_document() -> Document {
    Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![Turn {
            id: TurnId("t1".to_string()),
            role: Role::User,
            origin: Origin {
                vendor: "anthropic".to_string(),
                model: None,
                surface: "claude.ai".to_string(),
            },
            blocks: vec![Block::Text { content: "hello".to_string() }],
            timestamp: None,
        }],
    }
}

#[test]
fn writes_jsonl_file_and_manifest() {
    let doc = legalized_document();
    let lowered = lower(&doc).expect("lower should succeed");
    let session_id = lowered.session_id.clone();

    let scratch = std::env::temp_dir().join(format!("ctxrelay-commit-test-{session_id}"));
    let _ = std::fs::remove_dir_all(&scratch);

    let dest = Dest {
        session_dir: scratch.clone(),
        cwd: PathBuf::from("/tmp/some-project"),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };

    let manifest = commit(
        lowered,
        &dest,
        TargetSpec { tool: "claude-code".to_string(), version_range: ">=2.1.0".to_string() },
        LoweringReport::default(),
    )
    .expect("commit should succeed");

    assert_eq!(manifest.created_session_ids, vec![session_id.clone()]);
    assert_eq!(manifest.writes.len(), 1);

    let written_path = scratch.join(format!("{session_id}.jsonl"));
    assert_eq!(manifest.writes[0].path, written_path);

    let content = std::fs::read_to_string(&written_path).expect("file should exist");
    let first: serde_json::Value =
        serde_json::from_str(content.lines().next().expect("first line")).unwrap();
    assert_eq!(first["sessionId"], session_id);
    assert_eq!(first["cwd"], "/tmp/some-project");
    assert_eq!(first["gitBranch"], "main");
    assert_eq!(first["version"], "2.1.201");
    assert_eq!(first["userType"], "external");

    let expected_sha256 = format!("{:x}", sha2::Sha256::digest(content.as_bytes()));
    assert_eq!(manifest.writes[0].sha256, expected_sha256);

    std::fs::remove_dir_all(&scratch).ok();
}
```

- [ ] **Step 2: 运行,预期失败(`commit` 模块还不存在)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code --test commit`
Expected: 编译错误(找不到 `be_claude_code::commit` 模块)。这是预期的红灯。

- [ ] **Step 3: 在 `crates/be-claude-code/src/lib.rs` 里补全内容(加 `pub mod commit;`,并加上 `ClaudeCodeBackend` + `impl Backend`——现在三个子模块都存在了,可以一次接完整)**

把 `crates/be-claude-code/src/lib.rs` 整个替换成:

```rust
//! Claude Code backend:把 IR lower 成 `~/.claude/projects/<slug>/*.jsonl` 的会话记录。
//!
//! 目录 slug 的发现/解析不是这个 crate 的职责(架构文档 §5),`commit` 只管把
//! `LoweredSession` 写进调用方已经解析好的 `Dest::session_dir`。

pub mod commit;
pub mod legalize;
pub mod lower;

use ctxrelay_backend::{
    Backend, CapPolicy, Dest, LoweredSession, LoweringReport, Manifest, Result, TargetSpec,
};
use ctxrelay_ir::Document;

pub struct ClaudeCodeBackend;

impl Backend for ClaudeCodeBackend {
    fn target(&self) -> TargetSpec {
        TargetSpec {
            tool: "claude-code".to_string(),
            // 经验证的最低版本:本计划的 --resume 兼容性验证跑在 2.1.201 上;
            // 未标注上界,因为架构文档明确 JSONL schema 会无预兆变化,届时应该
            // 拆出一个新的"claude-code vX backend",而不是在这里加 if。
            version_range: ">=2.1.0".to_string(),
        }
    }

    fn required_caps(&self) -> CapPolicy {
        CapPolicy {
            // IR 目前没有字段能装下真实的 thinking signature 字节,统一按不可信处理
            // (详见 legalize.rs 的注释)。
            accepts_verifiable_reasoning: false,
        }
    }

    fn legalize(&self, doc: &Document) -> (Document, LoweringReport) {
        legalize::legalize(doc)
    }

    fn lower(&self, doc: &Document) -> Result<LoweredSession> {
        lower::lower(doc)
    }

    fn commit(&self, lowered: LoweredSession, dest: &Dest, report: LoweringReport) -> Result<Manifest> {
        commit::commit(lowered, dest, self.target(), report)
    }
}
```

- [ ] **Step 4: 写 `crates/be-claude-code/src/commit.rs`**

```rust
use ctxrelay_backend::{BackendError, Dest, LoweredSession, LoweringReport, Manifest, Result, TargetSpec, WriteRecord};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// 唯一的写盘处。把 `lower` 产出的记录逐条盖上环境信息(sessionId/cwd/gitBranch/
/// version/userType)再写成 JSONL 文件,产出记录了写了什么的 `Manifest`。
pub fn commit(
    lowered: LoweredSession,
    dest: &Dest,
    target: TargetSpec,
    report: LoweringReport,
) -> Result<Manifest> {
    std::fs::create_dir_all(&dest.session_dir).map_err(|e| {
        BackendError(format!(
            "failed to create session dir {}: {e}",
            dest.session_dir.display()
        ))
    })?;

    let path = dest.session_dir.join(format!("{}.jsonl", lowered.session_id));

    let mut buffer = String::new();
    for mut line in lowered.lines {
        stamp_environment(&mut line, &lowered.session_id, dest);
        buffer.push_str(&line.to_string());
        buffer.push('\n');
    }

    std::fs::write(&path, &buffer)
        .map_err(|e| BackendError(format!("failed to write {}: {e}", path.display())))?;

    let sha256 = format!("{:x}", Sha256::digest(buffer.as_bytes()));

    Ok(Manifest {
        ir_digest: lowered.ir_digest,
        target,
        writes: vec![WriteRecord { path, sha256 }],
        created_session_ids: vec![lowered.session_id],
        report,
    })
}

fn stamp_environment(line: &mut Value, session_id: &str, dest: &Dest) {
    let obj = line.as_object_mut().expect("lower() always produces JSON objects");
    obj.insert("sessionId".to_string(), Value::String(session_id.to_string()));
    obj.insert("cwd".to_string(), Value::String(dest.cwd.display().to_string()));
    obj.insert("version".to_string(), Value::String(dest.cli_version.clone()));
    obj.insert(
        "gitBranch".to_string(),
        Value::String(dest.git_branch.clone().unwrap_or_default()),
    );
    obj.insert("userType".to_string(), Value::String("external".to_string()));
}
```

- [ ] **Step 5: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code`
Expected: `legalize`/`lower`/`commit` 三个测试文件全部 `ok`(此时 `ClaudeCodeBackend` 也已接线,整个 crate 应该干净编译)。

- [ ] **Step 6: 提交**

```bash
git add crates/be-claude-code/src/lib.rs crates/be-claude-code/src/commit.rs crates/be-claude-code/tests/commit.rs
git commit -m "feat(be-claude-code): implement commit and wire up ClaudeCodeBackend"
```

---

### Task 6: 真实端到端 conformance 测试(默认 `#[ignore]`,会花真实 API 额度)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/be-claude-code/tests/conformance.rs`

- [ ] **Step 1: 写 `crates/be-claude-code/tests/conformance.rs`**

```rust
use be_claude_code::commit::commit;
use be_claude_code::legalize::legalize;
use be_claude_code::lower::lower;
use ctxrelay_backend::{Dest, TargetSpec};
use ctxrelay_ir::{Block, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use std::path::PathBuf;
use std::process::Command;

/// 仅用于本测试定位 `claude --resume` 会去哪个目录找会话——不是 be-claude-code 的
/// 公开职责(架构文档 §5 明确 slug 编码规则不应该被反向工程,生产代码里这是
/// core/cli 的事)。这里的简单斜杠替换规则已经过真实 `claude` CLI 手工验证(见本
/// 计划开头"已实测确认的 Claude Code 会话格式"一节),仅对不含冒号/空格/盘符的
/// 普通 Unix 路径成立。
fn slug_for(path: &std::path::Path) -> String {
    path.display().to_string().replace('/', "-")
}

/// 端到端验证:IR → legalize → lower → commit 写出的 JSONL,`claude --resume` 真的
/// 能加载并在下一轮记起里面埋的暗号。这条测试会真实调用 `claude` CLI、花费少量
/// API 额度,默认不随 `cargo test` 跑,需要显式加 `-- --ignored` 才会执行:
///
/// `cargo test -p be-claude-code --test conformance -- --ignored`
#[test]
#[ignore]
fn claude_code_can_resume_a_committed_session() {
    let codeword = "橙色的仙人掌在打字";
    let scratch_project = std::env::temp_dir().join("ctxrelay-conformance-scratch-project");
    let _ = std::fs::remove_dir_all(&scratch_project);
    std::fs::create_dir_all(&scratch_project).expect("create scratch project dir");

    let doc = Document {
        ir_version: Version::new(0, 1, 0),
        source: SourceProvenance {
            vendor: "anthropic".to_string(),
            surface: "claude.ai".to_string(),
            exported_at: None,
        },
        turns: vec![Turn {
            id: TurnId("t1".to_string()),
            role: Role::User,
            origin: Origin {
                vendor: "anthropic".to_string(),
                model: None,
                surface: "claude.ai".to_string(),
            },
            blocks: vec![Block::Text { content: format!("暗号是:{codeword}") }],
            timestamp: None,
        }],
    };

    let (legalized, report) = legalize(&doc);
    let lowered = lower(&legalized).expect("lower should succeed");
    let session_id = lowered.session_id.clone();

    let home = std::env::var("HOME").expect("HOME must be set");
    let session_dir = PathBuf::from(home)
        .join(".claude/projects")
        .join(slug_for(&scratch_project));
    let _ = std::fs::remove_dir_all(&session_dir);

    let dest = Dest {
        session_dir: session_dir.clone(),
        cwd: scratch_project.clone(),
        git_branch: Some("main".to_string()),
        cli_version: "2.1.201".to_string(),
    };

    commit(
        lowered,
        &dest,
        TargetSpec { tool: "claude-code".to_string(), version_range: ">=2.1.0".to_string() },
        report,
    )
    .expect("commit should succeed");

    let output = Command::new("claude")
        .arg("--resume")
        .arg(&session_id)
        .arg("-p")
        .arg("我之前告诉你的暗号是什么?只回复暗号本身")
        .arg("--output-format")
        .arg("json")
        .current_dir(&scratch_project)
        .output()
        .expect("failed to run claude CLI");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected JSON output from claude, got error {e}: {stdout}"));
    let result_text = parsed["result"].as_str().expect("result field should be a string");

    assert!(
        result_text.contains(codeword),
        "expected claude to recall the codeword {codeword:?}, got: {result_text:?}"
    );

    std::fs::remove_dir_all(&session_dir).ok();
    std::fs::remove_dir_all(&scratch_project).ok();
}
```

- [ ] **Step 2: 正常的 `cargo test` 不应该跑这条测试(确认默认不花钱)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code 2>&1 | tail -15`
Expected: 输出里这条测试显示为 `ignored`,不会被执行。

- [ ] **Step 3: 手动跑一次,确认真的能通过(这一步会花费少量真实 API 额度,执行前请知悉)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p be-claude-code --test conformance -- --ignored`
Expected: `test claude_code_can_resume_a_committed_session ... ok`

- [ ] **Step 4: 提交**

```bash
git add crates/be-claude-code/tests/conformance.rs
git commit -m "test(be-claude-code): add real claude --resume conformance test (ignored by default)"
```

---

### Task 7: 收尾验证

**Files:** 无新文件,只验证。

- [ ] **Step 1: 整个 workspace 编译**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build --workspace 2>&1 | tail -10`
Expected: `Finished`,无 error。

- [ ] **Step 2: 整个 workspace 默认测试全绿(不含 `#[ignore]` 的 conformance 测试)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test --workspace 2>&1 | tail -60`
Expected: `ctxrelay-ir`/`fe-claude-share` 的既有测试 + `be-claude-code` 的 `legalize`/`lower`/`commit` 全部 `ok`,`conformance` 测试显示为 `ignored`。

- [ ] **Step 3: 确认依赖图——`be-claude-code` 只依赖 `ir`/`backend`,不依赖任何 frontend**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo tree -p be-claude-code --depth 1`
Expected: 只包含 `ctxrelay-ir`、`ctxrelay-backend`、`serde_json`、`uuid`、`sha2`、`time`,没有 `fe-claude-share`/`ctxrelay-frontend` 等 frontend 侧 crate。

- [ ] **Step 4: 最终提交(如果前面步骤有任何未提交的修正)**

```bash
git status --short
```
若有改动:
```bash
git add -A
git commit -m "chore: verify be-claude-code workspace state"
```

---

## 完成后的状态

- `ctxrelay-backend` 是纯 trait 定义 crate,只依赖 `ctxrelay-ir`。
- `be-claude-code` 是第一个可用的 backend:`legalize` 丢弃/内联不合法构造并插入 preamble,`lower` 纯函数确定性地把 IR 转成 JSONL 记录,`commit` 唯一写盘处,产出 `Manifest`。
- 已经用真实 `claude` CLI 做过三次手工实测(目录 slug 规则、最小字段集合、content 数组形式),并留了一条默认 `#[ignore]` 的端到端 conformance 测试固化这条验证路径,平时不随 `cargo test` 消耗 API 额度。
- 已知、公开记录的限制:Reasoning 的真实 thinking signature 字节目前无法通过 IR 携带,所以一律丢弃;`sessions-index.json` 在本次实测的版本(2.1.201)里没有观察到参与 resume,更高版本可能不同,届时应该开一个新的 `TargetSpec.version_range`,而不是在现有代码里加版本判断分支。
- 下一步(不在本计划范围内):架构文档 §12 步骤 4——`ctxrelay-core` + `ctxrelay-cli`,把 `import`/`ir`/`undo`/`verify` 串起来,其中需要解决"如何为一个从未在 Claude Code 里打开过的项目发现正确的 `Dest::session_dir`"这个问题(架构文档建议的手法:起一次真实的一次性 session,观察新建了哪个目录)。
