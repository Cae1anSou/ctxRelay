# ctxRelay 地基阶段(仓库脚手架 + ctxrelay-ir)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按 `docs/architecture.md` §12 步骤 0-1,搭好单仓库骨架(Cargo workspace + extension 占位 + bridge-protocol 占位),并把 `ctxrelay-ir` crate 的类型系统、on-disk JSON schema、版本号、round-trip property test 钉死——这是整个项目的地基,后续所有 frontend/backend 都依赖它但它不依赖任何东西。

**Architecture:** 严格按 `docs/architecture.md` §3.1 的数据模型实现 `Document/Turn/Block/BlockCaps` 等类型,derive `serde::Serialize/Deserialize` 作为 on-disk JSON 表示,`ir_version` 用 `semver::Version`。用 `proptest` 写一个 round-trip property test(任意构造的 `Document` 经 JSON 序列化再反序列化后与原值相等),验证 §9 提到的"IR 层 property test"的最基础版本。

**Tech Stack:** Rust (edition 2021), Cargo workspace, serde + serde_json, semver(serde feature), time(serde feature), proptest(dev-dependency)。

---

## File Structure

```
ctxRelay/
├── Cargo.toml                      # workspace 根
├── crates/
│   └── ctxrelay-ir/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs               # 模块声明 + re-export
│           ├── block.rs             # Block, BlockCaps, Artifact
│           └── document.rs          # Document, Turn, TurnId, Role, Origin, SourceProvenance
│       └── tests/
│           └── roundtrip.rs         # proptest round-trip property test
├── extension/
│   ├── package.json                # 占位,独立 TS 工具链
│   └── tsconfig.json
├── bridge-protocol/
│   └── schema.json                 # 占位
└── docs/architecture.md            # 已存在,不改
```

---

### Task 0: 初始化 git 仓库与 workspace 骨架

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/.gitignore`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/package.json`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/tsconfig.json`
- Create: `/Users/caoxinzhuo/code/ctxRelay/bridge-protocol/schema.json`

- [ ] **Step 1: git init**

Run: `git init` (在 `/Users/caoxinzhuo/code/ctxRelay` 下)
Expected: `Initialized empty Git repository in .../ctxRelay/.git/`

- [ ] **Step 2: 写 `.gitignore`**

```gitignore
/target
extension/node_modules
extension/dist
.DS_Store
```

- [ ] **Step 3: 写根 `Cargo.toml`(workspace,此时 `crates/` 还没有成员,先留空占位数组不行——Cargo 要求 members 路径存在才 resolve;所以这一步先只声明 workspace,下一个 Task 创建 `ctxrelay-ir` 后再验证)**

```toml
[workspace]
resolver = "2"
members = ["crates/*"]
```

- [ ] **Step 4: 写 `extension/package.json` 占位**

```json
{
  "name": "ctxrelay-extension",
  "version": "0.0.1",
  "private": true,
  "description": "ctxRelay 浏览器扩展占位包,后续实现 background service worker",
  "scripts": {
    "build": "echo 'TODO: implement extension build' && exit 0"
  }
}
```

- [ ] **Step 5: 写 `extension/tsconfig.json` 占位**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ES2022",
    "moduleResolution": "Bundler",
    "strict": true,
    "outDir": "dist"
  },
  "include": ["src"]
}
```

- [ ] **Step 6: 写 `bridge-protocol/schema.json` 占位(空 schema,后续 §10.1 阶段再定型)**

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ctxrelay-bridge-protocol",
  "description": "PLACEHOLDER: CLI <-> extension job/response schema, 定型见架构文档 §10.1",
  "type": "object"
}
```

- [ ] **Step 7: 提交**

```bash
git add Cargo.toml .gitignore extension bridge-protocol docs README.md
git commit -m "chore: scaffold ctxRelay monorepo (workspace + extension/bridge-protocol placeholders)"
```

---

### Task 1: `ctxrelay-ir` crate 骨架 + 依赖

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-ir/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-ir/src/lib.rs`

- [ ] **Step 1: 写 `crates/ctxrelay-ir/Cargo.toml`**

```toml
[package]
name = "ctxrelay-ir"
version = "0.1.0"
edition = "2021"
description = "ctxRelay 中立 IR:类型 + schema + serde + 版本号。出度为 0,不依赖 workspace 内任何其他 crate。"

[dependencies]
serde = { version = "1", features = ["derive"] }
semver = { version = "1", features = ["serde"] }
time = { version = "0.3", features = ["serde", "formatting", "parsing"] }

[dev-dependencies]
serde_json = "1"
proptest = "1"
```

- [ ] **Step 2: 写最小 `src/lib.rs`(先让 workspace 能 resolve,类型在 Task 2/3 补全)**

```rust
//! ctxRelay 中立 IR:所有 Web 源与所有 CLI 目标共同的最小语义内核。
//! 契约:只承诺 content-effect(对话内容/代码/推理链)的保真,
//! 对 action-effect 只承诺"标记存在 + 携带产物",绝不承诺可回放。

mod block;
mod document;

pub use block::{Artifact, Block, BlockCaps};
pub use document::{Document, Origin, Role, SourceProvenance, Turn, TurnId};
```

- [ ] **Step 3: 验证 workspace 能识别新成员(此时 `block.rs`/`document.rs` 还不存在,预期编译失败于"file not found",这是正常的中间态,不是本 Task 的失败信号)**

Run: `cargo metadata --no-deps --format-version=1 -q | grep -o '"name":"ctxrelay-ir"'`
Expected: `"name":"ctxrelay-ir"` (证明 workspace member 解析成功;此命令只读 manifest,不编译源码,所以 `block.rs`/`document.rs` 缺失不影响它)

---

### Task 2: `Block` / `BlockCaps` / `Artifact` 类型(§3.1)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-ir/src/block.rs`

本 Task 只写代码,不单独编译验证——`lib.rs` 里 `mod document;` 此时还找不到文件,任何编译尝试都会失败于无关错误。整个 crate 的编译验证放在 Task 3 Step 2,那时 `document.rs` 也补齐了。

- [ ] **Step 1: 写 `src/block.rs`**

```rust
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
```

---

### Task 3: `Document` / `Turn` / `Origin` / `Role` / `SourceProvenance` 类型(§3.1)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-ir/src/document.rs`

- [ ] **Step 1: 写 `src/document.rs`**

```rust
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
```

- [ ] **Step 2: 编译整个 crate**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build -p ctxrelay-ir`
Expected: `Compiling ctxrelay-ir v0.1.0 (...)` 然后 `Finished` ,无 error。若报 `time::serde::rfc3339::option` 找不到,检查 `Cargo.toml` 里 `time` 的 `features` 是否含 `"parsing"`(rfc3339 序列化需要它)。

- [ ] **Step 3: 提交**

```bash
git add crates/ctxrelay-ir
git commit -m "feat(ir): define Document/Turn/Block/BlockCaps types per architecture §3.1"
```

---

### Task 4: on-disk JSON 示例固定测试(先验证 schema 与文档示例一致)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-ir/tests/fixture.rs`

- [ ] **Step 1: 写失败的固定测试——直接用架构文档 §3.4 给出的示例 JSON 片段(简化为单轮,因为示例的 assistant 轮里同时有 Reasoning/Text/ForeignAction,这里先验证反序列化能整体成功且字段值正确)**

```rust
use ctxrelay_ir::{Block, Document, Role};

#[test]
fn parses_architecture_doc_example() {
    let raw = r#"
    {
      "ir_version": "0.1.0",
      "source": { "vendor": "anthropic", "surface": "claude.ai", "exported_at": null },
      "turns": [
        {
          "id": "t1",
          "role": "User",
          "origin": { "vendor": "anthropic", "model": null, "surface": "claude.ai" },
          "blocks": [ { "type": "Text", "content": "我们把这个 IR 迁移工具设计一下" } ],
          "timestamp": null
        }
      ]
    }
    "#;

    let doc: Document = serde_json::from_str(raw).expect("should parse");
    assert_eq!(doc.turns.len(), 1);
    assert_eq!(doc.turns[0].role, Role::User);
    match &doc.turns[0].blocks[0] {
        Block::Text { content } => assert_eq!(content, "我们把这个 IR 迁移工具设计一下"),
        other => panic!("expected Text block, got {other:?}"),
    }
}
```

- [ ] **Step 2: 运行,预期失败(此时 `Document`/`Block` 尚未 `pub` 导出到位或字段名不匹配)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-ir --test fixture`
Expected: 如果 Task 3 已正确实现,这条测试应该**直接通过**(因为类型已经和文档示例字段对齐)。若报 `unknown field` 或 `missing field`,对照 Task 2/3 的字段名修正 `block.rs`/`document.rs`,不要改这条测试——测试内容锚定的是架构文档 §3.4 的示例,测试是权威。

- [ ] **Step 3: 确认通过**

Run: `cargo test -p ctxrelay-ir --test fixture`
Expected: `test parses_architecture_doc_example ... ok`

- [ ] **Step 4: 提交**

```bash
git add crates/ctxrelay-ir/tests/fixture.rs
git commit -m "test(ir): pin Document parsing to architecture.md §3.4 example JSON"
```

---

### Task 5: round-trip property test(§9 IR 层 property test 的最基础版本)

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-ir/tests/roundtrip.rs`

- [ ] **Step 1: 写 proptest strategy + round-trip property test**

```rust
use ctxrelay_ir::{Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use proptest::prelude::*;
use semver::Version;

fn arb_caps() -> impl Strategy<Value = BlockCaps> {
    (any::<bool>(), any::<bool>(), any::<bool>()).prop_map(|(reasoning, verifiable_signature, replayable)| {
        BlockCaps { reasoning, verifiable_signature, replayable }
    })
}

fn arb_artifact() -> impl Strategy<Value = Artifact> {
    ("[a-z/]{3,15}", "[\\PC]{0,40}").prop_map(|(media, content)| Artifact { media, content })
}

fn arb_block() -> impl Strategy<Value = Block> {
    prop_oneof![
        "[\\PC]{0,60}".prop_map(|content| Block::Text { content }),
        (proptest::option::of("[a-z]{1,10}"), "[\\PC]{0,60}")
            .prop_map(|(language, content)| Block::Code { language, content }),
        ("[a-z_]{3,15}", proptest::option::of("[\\PC]{0,40}"), proptest::option::of(arb_artifact()), arb_caps())
            .prop_map(|(kind, summary, artifact, caps)| Block::ForeignAction { kind, summary, artifact, caps }),
        ("[\\PC]{0,60}", arb_caps()).prop_map(|(content, caps)| Block::Reasoning { content, caps }),
    ]
}

fn arb_role() -> impl Strategy<Value = Role> {
    prop_oneof![Just(Role::User), Just(Role::Assistant), Just(Role::System)]
}

fn arb_origin() -> impl Strategy<Value = Origin> {
    ("[a-z]{3,10}", proptest::option::of("[a-z0-9.-]{3,15}"), "[a-z.]{3,15}")
        .prop_map(|(vendor, model, surface)| Origin { vendor, model, surface })
}

fn arb_turn() -> impl Strategy<Value = Turn> {
    ("[a-zA-Z0-9]{1,10}", arb_role(), arb_origin(), proptest::collection::vec(arb_block(), 0..4)).prop_map(
        |(id, role, origin, blocks)| Turn {
            id: TurnId(id),
            role,
            origin,
            blocks,
            timestamp: None,
        },
    )
}

fn arb_document() -> impl Strategy<Value = Document> {
    ("[a-z]{3,10}", "[a-z.]{3,15}", proptest::collection::vec(arb_turn(), 0..5)).prop_map(
        |(vendor, surface, turns)| Document {
            ir_version: Version::new(0, 1, 0),
            source: SourceProvenance { vendor, surface, exported_at: None },
            turns,
        },
    )
}

proptest! {
    #[test]
    fn roundtrip_preserves_content_effect(doc in arb_document()) {
        let json = serde_json::to_string(&doc).expect("serialize");
        let parsed: Document = serde_json::from_str(&json).expect("deserialize");
        prop_assert_eq!(doc, parsed);
    }
}
```

- [ ] **Step 2: 运行(第一次预期直接通过,因为 `Document` 派生了 `PartialEq` 且 serde round-trip 对纯数据结构应天然成立;如果失败,大概率是 `time` 的 RFC3339 序列化在 `None` 分支上有问题,或某个 `Option<String>` 字段在 proptest 生成的字符串里包含了 JSON 转义边界字符——先看报错的具体 diff 再定位)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-ir --test roundtrip`
Expected: `test roundtrip_preserves_content_effect ... ok` (proptest 默认跑 256 个随机 case)

- [ ] **Step 3: 提交**

```bash
git add crates/ctxrelay-ir/tests/roundtrip.rs
git commit -m "test(ir): add proptest round-trip property test for Document"
```

---

### Task 6: 收尾验证——两套工具链互不干扰(§12 步骤 0 的验收标准)

**Files:** 无新文件,只验证。

- [ ] **Step 1: 确认 `cargo build` 不受 `extension/` 影响**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build --workspace 2>&1 | tail -5`
Expected: 正常 `Finished`,输出中不出现任何 `extension/` 相关路径或错误。

- [ ] **Step 2: 确认 `cargo test --workspace` 全绿**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test --workspace 2>&1 | tail -20`
Expected: `test result: ok.` 出现至少 3 次(fixture / roundtrip / 以及 crate 自身的 doc-test 如果有)。

- [ ] **Step 3: 确认 npm 侧不需要接触 `crates/`(仅静态检查,不强制真的跑 npm install,因为 package.json 目前只是占位)**

Run: `cat extension/package.json | grep -q '"name"' && echo OK`
Expected: `OK`

- [ ] **Step 4: 最终提交(如果前面步骤有任何未提交的修正)**

```bash
git status --short
```
若有改动:
```bash
git add -A
git commit -m "chore: verify workspace/extension toolchain isolation"
```

---

## 完成后的状态

- `crates/ctxrelay-ir` 是一个可独立编译、可独立测试、出度为 0 的地基 crate。
- 两个测试文件锚定了两层保证:`fixture.rs` 锚定"能解析架构文档里写死的示例",`roundtrip.rs` 锚定"任意合法 Document 经 JSON 往返内容不丢"。
- `extension/`、`bridge-protocol/` 只有占位,留给 §12 步骤 6 再实现。
- 下一步(不在本计划范围内):Task 2 of 架构文档 §12——`fe-claude-share` frontend,那时需要先决定 `crates/ctxrelay-frontend` 的 trait 骨架(`Acquire`/`Parse`),会开启新的一份 plan。
