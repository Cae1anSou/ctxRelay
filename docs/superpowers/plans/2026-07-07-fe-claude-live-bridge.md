# fe-claude-live + 浏览器扩展本地桥 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 按用户的新决定——不走"分享链接"路线,而是让浏览器扩展直接抓取用户当前正打开的 claude.ai 对话(含 thinking)——实现一个新 frontend `fe-claude-live`,并按架构文档 §4/§10.1 原计划把"扩展 + 本地桥"整条链路做完整:扩展点一下图标就把当前对话(经用户已登录的认证态、原样含 thinking)POST 给本地一次性监听的 `ctxrelay listen`,自动跑完 legalize→lower→commit。

**Architecture:** `fe-claude-live` 只实现 `Parse`(不实现 `Acquire`——数据是被动接收的 HTTP POST,不是 ctxrelay 主动去 fetch/读文件,不适配 `Acquire` trait 的"主动拉取"语义)。`ctxrelay-core` 新增 `run_import_from_bytes`,跳过 `Acquire`,直接从已经到手的字节走 `Parse → legalize → lower → commit`。`ctxrelay-cli` 新增 `listen` 子命令:起一个绑定 `127.0.0.1` 的一次性 HTTP 服务,带一次性 token,收到浏览器扩展 POST 来的一次抓取就跑完整个 import 管线然后退出——不做成常驻轮询服务(架构文档 §4 明确的风险定界)。扩展本身(`extension/`)是 Manifest V3 + TypeScript,`chrome.action.onClicked` 触发抓取,`background.ts` 里用已登录会话的 cookie 直接 fetch claude.ai 的认证态内部 API(不是公开分享链接接口)。

**Tech Stack:** Rust(`tiny_http` 起本地 HTTP 服务),TypeScript(Manifest V3 扩展,`tsc` 编译),JSON Schema(`bridge-protocol/schema.json` 作为契约唯一来源)。

---

## 已实测确认的关键事实(不是猜的)

用真实登录态在 Chrome 里直接打开了一条自己的私有对话(`https://claude.ai/chat/fca79960-3026-40e1-beba-6abb33fe20d5`,不是分享链接),做了以下验证,产出的真实 JSON 已存成 `crates/fe-claude-live/tests/fixtures/sample_live_conversation.json`:

- 页面自己加载对话用的内部 API 是 `GET https://claude.ai/api/organizations/<org_uuid>/chat_conversations/<conversation_uuid>?tree=True&rendering_mode=messages&render_all_tools=true&consistency=strong`。用页面里 `fetch(url, {credentials:'include'})` 直接拿到 200,**不需要先创建任何分享链接**——这是当前登录用户自己私有对话的认证态视角,和架构文档 §4 已经设计好的"背景脚本 fetch + credentials include 不触发 Cloudflare challenge"机制完全吻合,只是换了一个端点。
- 响应里 `chat_messages[].content[]` 真的出现了 `"type": "thinking"` 的 block,字段是 `start_timestamp`/`stop_timestamp`/`type`/`thinking`/`summaries`/`cut_off`/`truncated`/`hidden`——**没有 `signature` 字段**。这实锤验证了此前 `be-claude-code` legalize 阶段"不管 verifiable_signature 是否为 true,一律丢弃 Reasoning"这条设计判断:连认证态的内部 API 都拿不到真实签名字节,IR 目前没有字段能装、也没有任何已知渠道能装。
- `model` 是对话级字段(`"claude-opus-4-8"`),不是逐条消息的。
- 顶层还有 `current_leaf_message_uuid`,每条消息带 `parent_message_uuid`,根节点的父是哨兵值 `00000000-0000-4000-8000-000000000000`。这条实测对话里 `chat_messages` 恰好已经是线性的(4 条消息首尾相接,`current_leaf_message_uuid` 就是最后一条),但 `tree=True` 这个查询参数暗示接口在有多分支时可能会把整棵树都吐出来——所以 Parse 不能假设 `chat_messages` 数组顺序就是"当前选中的那条线",必须从 `current_leaf_message_uuid` 出发沿 `parent_message_uuid` 往回走,重建真正被选中的线性分支,这样不管接口以后返不返回额外分支都正确。
- 组织 UUID 可以用 `GET https://claude.ai/api/organizations`(同样 `credentials:'include'`)拿到——当前账号只有一个组织。多组织账号需要逐个尝试对话端点直到某个组织返回 200(而不是假设"第一个就是对的"),这是背景脚本要处理的一个已知场景,不是理论假设。

---

## File Structure

```
ctxRelay/
├── crates/
│   ├── fe-claude-live/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # mod parse; pub use parse::ClaudeLiveParse;
│   │       └── parse.rs                # ClaudeLiveParse:认证态对话 JSON → IR Document
│   │   └── tests/
│   │       ├── fixtures/
│   │       │   └── sample_live_conversation.json   # 已经就位的真实样例
│   │       └── parse.rs
│   ├── ctxrelay-core/
│   │   └── src/
│   │       └── pipeline.rs             # 追加 run_import_from_bytes,重构出 commit_document 共用逻辑
│   │   └── tests/
│   │       └── pipeline_from_bytes.rs
│   └── ctxrelay-cli/
│       └── src/
│           └── main.rs                 # 追加 `listen` 子命令
│       └── tests/
│           └── listen.rs
├── bridge-protocol/
│   └── schema.json                     # 定型:CaptureRequest / CaptureResponse
└── extension/
    ├── package.json                    # 补 typescript 依赖 + 真正的 build 脚本
    ├── tsconfig.json                   # 已存在,不用改
    ├── manifest.json                   # Manifest V3
    ├── options.html                    # 配对 token/port 的设置页
    └── src/
        ├── background.ts               # chrome.action.onClicked → fetch → POST 本地桥
        └── options.ts                  # 读写 chrome.storage.local
```

---

### Task 1: `fe-claude-live` crate——Parse 实现

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-live/Cargo.toml`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-live/src/lib.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-live/src/parse.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-live/tests/parse.rs`
- 已存在(本次会话已生成,直接用):`/Users/caoxinzhuo/code/ctxRelay/crates/fe-claude-live/tests/fixtures/sample_live_conversation.json`

- [ ] **Step 1: 写 `crates/fe-claude-live/Cargo.toml`**

```toml
[package]
name = "fe-claude-live"
version = "0.1.0"
edition = "2021"
description = "claude.ai 认证态实时对话 frontend:只实现 Parse,数据由浏览器扩展主动 POST 过来,不适配 Acquire 的主动拉取语义。"

[dependencies]
ctxrelay-ir = { path = "../ctxrelay-ir" }
ctxrelay-frontend = { path = "../ctxrelay-frontend" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
semver = "1"
time = { version = "0.3", features = ["parsing"] }
```

- [ ] **Step 2: 写最小 `crates/fe-claude-live/src/lib.rs`**

```rust
//! claude.ai 认证态实时对话 frontend。
//!
//! 只实现 `Parse`,不实现 `Acquire`——数据来源是浏览器扩展主动 POST 到本地桥
//! (架构文档 §4/§10.1),不是 ctxrelay 自己发起的拉取,不适配 `Acquire::acquire`
//! "给一个 SourceRef,主动拿到 bytes"的语义。`ctxrelay-core` 的
//! `run_import_from_bytes` 会跳过 `Acquire`,直接把已经到手的字节交给这里的 Parse。

mod parse;

pub use parse::ClaudeLiveParse;
```

- [ ] **Step 3: 写失败的集成测试 `crates/fe-claude-live/tests/parse.rs`**

```rust
use ctxrelay_frontend::Parse;
use ctxrelay_ir::{Block, BlockCaps, Role, TurnId};
use fe_claude_live::ClaudeLiveParse;
use semver::Version;

#[test]
fn parses_real_authenticated_conversation_with_thinking() {
    let raw = std::fs::read("tests/fixtures/sample_live_conversation.json")
        .expect("fixture must exist");

    let doc = ClaudeLiveParse.parse(raw).expect("should parse real live conversation");

    assert_eq!(doc.ir_version, Version::new(0, 1, 0));
    assert_eq!(doc.source.vendor, "anthropic");
    assert_eq!(doc.source.surface, "claude.ai");

    assert_eq!(doc.turns.len(), 4);

    assert_eq!(doc.turns[0].role, Role::User);
    assert_eq!(doc.turns[0].origin.model, None);
    match &doc.turns[0].blocks[0] {
        Block::Text { content } => assert!(content.starts_with("我想做做一个科研copilot")),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(doc.turns[1].role, Role::Assistant);
    assert_eq!(doc.turns[1].origin.model, Some("claude-opus-4-8".to_string()));
    assert_eq!(doc.turns[1].blocks.len(), 2);
    match &doc.turns[1].blocks[0] {
        Block::Reasoning { content, caps } => {
            assert!(content.starts_with("Jennifer's looking to build"));
            assert_eq!(
                *caps,
                BlockCaps { reasoning: true, verifiable_signature: false, replayable: false }
            );
        }
        other => panic!("expected Reasoning block, got {other:?}"),
    }
    match &doc.turns[1].blocks[1] {
        Block::Text { content } => assert!(content.starts_with("这个想法很好")),
        other => panic!("expected Text block, got {other:?}"),
    }

    assert_eq!(doc.turns[3].id, TurnId("019f2dbc-c314-7bbc-b6c6-b11f84c1ccb1".to_string()));
}
```

- [ ] **Step 4: 运行,预期失败(`fe-claude-live` 还没实现)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p fe-claude-live --test parse`
Expected: 编译错误(`ClaudeLiveParse` 尚未实现)。这是预期的红灯。

- [ ] **Step 5: 写 `crates/fe-claude-live/src/parse.rs`**

```rust
use ctxrelay_frontend::{FrontendError, Parse, RawBytes, Result};
use ctxrelay_ir::{Artifact, Block, BlockCaps, Document, Origin, Role, SourceProvenance, Turn, TurnId};
use semver::Version;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// `parent_message_uuid` 的树根哨兵值——实测确认(见本计划开头),不是猜的。
const ROOT_SENTINEL: &str = "00000000-0000-4000-8000-000000000000";

/// claude.ai 认证态对话接口(`/api/organizations/<org>/chat_conversations/<id>`)的
/// on-disk JSON 形状。只声明我们实际使用的字段。
#[derive(Deserialize)]
struct RawSnapshot {
    model: Option<String>,
    current_leaf_message_uuid: String,
    chat_messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    uuid: String,
    content: Vec<Value>,
    sender: String,
    created_at: String,
    parent_message_uuid: String,
}

/// claude.ai 认证态实时对话的 Parse 实现。纯函数:给定字节,要么吐出合法
/// `Document`,要么明确报错。
pub struct ClaudeLiveParse;

impl Parse for ClaudeLiveParse {
    fn id(&self) -> &'static str {
        "fe-claude-live"
    }

    fn parse(&self, raw: RawBytes) -> Result<Document> {
        let snapshot: RawSnapshot = serde_json::from_slice(&raw)
            .map_err(|e| FrontendError(format!("invalid claude.ai live conversation JSON: {e}")))?;

        let by_uuid: HashMap<&str, &RawMessage> =
            snapshot.chat_messages.iter().map(|m| (m.uuid.as_str(), m)).collect();

        // 从 current_leaf_message_uuid 沿 parent_message_uuid 往回走,重建"当前被
        // 选中的那条线性分支"——tree=True 请求可能带回整棵树(含被放弃的重新生成
        // 分支),只有这样才能保证不管接口未来返不返回额外分支,取到的永远是用户
        // 实际看到的那条对话,不是随便拼出来的。
        let mut ordered: Vec<&RawMessage> = Vec::new();
        let mut cursor: &str = snapshot.current_leaf_message_uuid.as_str();
        while cursor != ROOT_SENTINEL {
            let message = *by_uuid
                .get(cursor)
                .ok_or_else(|| FrontendError(format!("chat_messages missing referenced uuid {cursor:?}")))?;
            ordered.push(message);
            cursor = message.parent_message_uuid.as_str();
        }
        ordered.reverse();

        let mut turns = Vec::with_capacity(ordered.len());
        for message in ordered {
            let role = match message.sender.as_str() {
                "human" => Role::User,
                "assistant" => Role::Assistant,
                other => {
                    return Err(FrontendError(format!("unknown chat_messages[].sender value: {other:?}")))
                }
            };

            let timestamp = OffsetDateTime::parse(&message.created_at, &Rfc3339).map_err(|e| {
                FrontendError(format!("invalid created_at timestamp {:?}: {e}", message.created_at))
            })?;

            let mut blocks = Vec::with_capacity(message.content.len());
            for block in &message.content {
                let kind = block
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| FrontendError("content block missing \"type\" field".to_string()))?;

                match kind {
                    "text" => {
                        let content = block
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                FrontendError(
                                    "content block has type=\"text\" but no \"text\" field".to_string(),
                                )
                            })?
                            .to_string();
                        blocks.push(Block::Text { content });
                    }
                    "thinking" => {
                        // 实测确认(见本计划开头 fixture 来源):即使是认证态的内部接口,
                        // thinking block 也没有 signature 字段——没有任何已知渠道能拿到
                        // 真实签名字节,所以恒标记 verifiable_signature: false,legalize
                        // 阶段会按 §5 的规则丢弃,不会尝试伪造签名。
                        let content = block
                            .get("thinking")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        blocks.push(Block::Reasoning {
                            content,
                            caps: BlockCaps { reasoning: true, verifiable_signature: false, replayable: false },
                        });
                    }
                    other => {
                        let artifact = Artifact {
                            media: "application/json".to_string(),
                            content: block.to_string(),
                        };
                        blocks.push(Block::foreign_action(
                            other.to_string(),
                            Some(format!("未识别的 content block 类型: {other}")),
                            Some(artifact),
                            false,
                            false,
                        ));
                    }
                }
            }

            turns.push(Turn {
                id: TurnId(message.uuid.clone()),
                role,
                origin: Origin {
                    vendor: "anthropic".to_string(),
                    model: if message.sender == "assistant" { snapshot.model.clone() } else { None },
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
                // 和 fe-claude-share 同样的理由:接口没有告诉我们"这次导出是什么时候
                // 做的",用系统时间会让 Parse 不再是纯函数,如实填 None。
                exported_at: None,
            },
            turns,
        })
    }
}
```

- [ ] **Step 6: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p fe-claude-live --test parse`
Expected: `test parses_real_authenticated_conversation_with_thinking ... ok`

- [ ] **Step 7: 提交**

```bash
git add crates/fe-claude-live
git commit -m "feat(fe-claude-live): implement Parse for authenticated claude.ai conversation API, including thinking blocks"
```

---

### Task 2: `bridge-protocol/schema.json` 定型 + Rust 端契约测试

**Files:**
- Modify: `/Users/caoxinzhuo/code/ctxRelay/bridge-protocol/schema.json`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/src/bridge.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/tests/bridge.rs`

这一步把 CLI ↔ 扩展之间唯一的契约定下来。V1 简化:不引入 `typify`/`json-schema-to-typescript` 代码生成工具链(架构文档 §10.1 的理想态),而是让 Rust 端和 TS 端各自手写一份类型,但都严格照抄 `schema.json` 的字段,并且写一条 Rust 测试直接反序列化"TS 端会发出的样例 JSON",作为两边没有漂移的兜底验证——这是比"完全没有验证"更好的最小可行版本,以后要引入真正的 codegen 也不影响这条测试。

- [ ] **Step 1: 把 `bridge-protocol/schema.json` 从占位替换成定型内容**

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ctxrelay-bridge-protocol",
  "description": "CLI(ctxrelay listen)与浏览器扩展(background.ts)之间唯一的契约来源。两侧的类型都应该照着这份 schema 手写/生成,不允许字段名各写各的。",
  "definitions": {
    "CaptureRequest": {
      "description": "扩展 POST 给本地 ctxrelay listen 服务的请求体。",
      "type": "object",
      "required": ["version", "token", "conversation_id", "org_id", "snapshot"],
      "properties": {
        "version": { "type": "string", "const": "1" },
        "token": { "type": "string", "description": "ctxrelay listen 启动时打印的一次性 token,用户手动粘贴进扩展设置页配对。" },
        "conversation_id": { "type": "string" },
        "org_id": { "type": "string" },
        "captured_at": { "type": "string", "description": "RFC3339 时间戳,扩展抓取时的本地时间,仅供人读,不参与 IR。" },
        "snapshot": { "type": "object", "description": "claude.ai chat_conversations 接口的原始 JSON,原样透传给 fe-claude-live::ClaudeLiveParse。" }
      }
    },
    "CaptureResponse": {
      "description": "ctxrelay listen 处理完一次 CaptureRequest 后的响应体。",
      "type": "object",
      "required": ["version", "status"],
      "properties": {
        "version": { "type": "string", "const": "1" },
        "status": { "type": "string", "enum": ["ok", "error"] },
        "message": { "type": "string" }
      }
    }
  }
}
```

- [ ] **Step 2: 写失败的测试 `crates/ctxrelay-cli/tests/bridge.rs`**

```rust
use ctxrelay_cli::bridge::CaptureRequest;

#[test]
fn deserializes_a_capture_request_matching_the_schema() {
    let raw = r#"
    {
      "version": "1",
      "token": "abc123",
      "conversation_id": "fca79960-3026-40e1-beba-6abb33fe20d5",
      "org_id": "ed9a9a3c-9d81-43a0-b974-3aa686e20a87",
      "captured_at": "2026-07-07T01:00:00Z",
      "snapshot": { "uuid": "fca79960-3026-40e1-beba-6abb33fe20d5", "chat_messages": [] }
    }
    "#;

    let request: CaptureRequest = serde_json::from_str(raw).expect("should deserialize per bridge-protocol schema");

    assert_eq!(request.version, "1");
    assert_eq!(request.token, "abc123");
    assert_eq!(request.conversation_id, "fca79960-3026-40e1-beba-6abb33fe20d5");
    assert_eq!(request.org_id, "ed9a9a3c-9d81-43a0-b974-3aa686e20a87");
}

#[test]
fn rejects_a_request_missing_a_required_field() {
    let raw = r#"{ "version": "1", "token": "abc123" }"#;

    let result: Result<CaptureRequest, _> = serde_json::from_str(raw);

    assert!(result.is_err(), "conversation_id/org_id/snapshot are required by the schema");
}
```

注意:这一步引用了 `ctxrelay_cli::bridge::CaptureRequest`,意味着 `ctxrelay-cli` 需要暴露一个库目标(目前只有 `[[bin]]`,没有 `src/lib.rs`)。下一步一并处理。

- [ ] **Step 3: 运行,预期失败**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli --test bridge`
Expected: 编译错误(`ctxrelay_cli` 目前没有 lib target,`bridge` 模块也不存在)。这是预期的红灯。

- [ ] **Step 4: 给 `crates/ctxrelay-cli/Cargo.toml` 加一个 lib target**

在 `[[bin]]` 那段前面加上:

```toml
[lib]
name = "ctxrelay_cli"
path = "src/lib.rs"
```

- [ ] **Step 5: 写 `crates/ctxrelay-cli/src/bridge.rs`**

```rust
use serde::Deserialize;

/// 对应 `bridge-protocol/schema.json` 的 `CaptureRequest`——字段名/必需性必须和
/// schema 保持一致,这份 schema 才是两侧契约的唯一权威来源,这里只是它在 Rust 里
/// 的一份手写投影。
#[derive(Debug, Deserialize)]
pub struct CaptureRequest {
    pub version: String,
    pub token: String,
    pub conversation_id: String,
    pub org_id: String,
    #[serde(default)]
    pub captured_at: Option<String>,
    pub snapshot: serde_json::Value,
}
```

- [ ] **Step 6: 写 `crates/ctxrelay-cli/src/lib.rs`**

```rust
//! `ctxrelay-cli` 的库目标:目前只导出 `bridge` 模块给集成测试用,主要逻辑仍在
//! `main.rs` 的二进制里(CLI 是薄封装,不需要把 clap 那套也搬进库里)。

pub mod bridge;
```

- [ ] **Step 7: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli --test bridge`
Expected: 两个测试都 `ok`。

- [ ] **Step 8: 确认 `main.rs` 没有因为加了 lib target 而受影响**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build -p ctxrelay-cli && cargo run -p ctxrelay-cli -- --help 2>&1 | tail -10`
Expected: 正常编译,`--help` 输出和之前一样能看到四个子命令(本任务不新增子命令,那是 Task 4 的事)。

- [ ] **Step 9: 提交**

```bash
git add bridge-protocol/schema.json crates/ctxrelay-cli/Cargo.toml crates/ctxrelay-cli/src/bridge.rs crates/ctxrelay-cli/src/lib.rs crates/ctxrelay-cli/tests/bridge.rs
git commit -m "feat(bridge-protocol): define CaptureRequest/CaptureResponse schema + Rust-side contract test"
```

---

### Task 3: `ctxrelay-core`——`run_import_from_bytes`

**Files:**
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-core/src/pipeline.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-core/src/registry.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-core/Cargo.toml`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-core/src/lib.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-core/tests/pipeline_from_bytes.rs`

- [ ] **Step 1: 给 `crates/ctxrelay-core/Cargo.toml` 加 `fe-claude-live` 依赖**

在 `[dependencies]` 里 `fe-claude-share = { path = "../fe-claude-share" }` 那一行后面加上:

```toml
fe-claude-live = { path = "../fe-claude-live" }
```

- [ ] **Step 2: 写失败的集成测试 `crates/ctxrelay-core/tests/pipeline_from_bytes.rs`**

```rust
use ctxrelay_core::{run_import_from_bytes, ImportOptions, Registry};
use std::path::PathBuf;

const FIXTURE: &str = "../fe-claude-live/tests/fixtures/sample_live_conversation.json";

#[test]
fn run_import_from_bytes_commits_a_live_capture() {
    let registry = Registry::with_defaults();
    let raw = std::fs::read(FIXTURE).expect("fixture must exist");

    let project_dir = std::env::temp_dir().join("ctxrelay-live-pipeline-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-live-pipeline-test-projects-root");
    let _ = std::fs::remove_dir_all(&projects_root);
    let slug = canonical.display().to_string().replace('/', "-");
    std::fs::create_dir_all(projects_root.join(&slug)).unwrap();

    let opts = ImportOptions {
        backend_name: "claude-code".to_string(),
        project_dir: project_dir.clone(),
        dry_run: false,
        allow_bootstrap: false,
        claude_projects_root: projects_root.clone(),
        cli_version: "2.1.201".to_string(),
    };

    let manifest =
        run_import_from_bytes(&registry, raw, "fe-claude-live", opts).expect("import should succeed");

    assert_eq!(manifest.writes.len(), 1);
    assert!(manifest.writes[0].path.exists());

    let content = std::fs::read_to_string(&manifest.writes[0].path).unwrap();
    // thinking 应该已经被 legalize 丢弃,不应该出现在最终写盘的 JSONL 里。
    assert!(!content.contains("\"thinking\""));
    // preamble + 4 条真实轮次。
    assert_eq!(content.lines().count(), 5);

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}

#[test]
fn run_import_from_bytes_errors_on_unknown_frontend_id() {
    let registry = Registry::with_defaults();
    let raw = std::fs::read(FIXTURE).expect("fixture must exist");

    let opts = ImportOptions {
        backend_name: "claude-code".to_string(),
        project_dir: PathBuf::from("/tmp/irrelevant"),
        dry_run: true,
        allow_bootstrap: false,
        claude_projects_root: PathBuf::from("/tmp/irrelevant"),
        cli_version: "2.1.201".to_string(),
    };

    let result = run_import_from_bytes(&registry, raw, "fe-not-registered", opts);
    assert!(result.is_err());
}
```

- [ ] **Step 3: 运行,预期失败(`run_import_from_bytes` 还不存在)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-core --test pipeline_from_bytes`
Expected: 编译错误。这是预期的红灯。

- [ ] **Step 4: 修改 `crates/ctxrelay-core/src/registry.rs`,把 `fe-claude-live` 的 Parse 也注册进去**

把 `use` 那一行:
```rust
use fe_claude_share::{ClaudeShareAcquire, ClaudeShareParse};
```
改成:
```rust
use fe_claude_live::ClaudeLiveParse;
use fe_claude_share::{ClaudeShareAcquire, ClaudeShareParse};
```

把 `with_defaults()` 里的:
```rust
        Registry {
            acquirers: vec![Box::new(ClaudeShareAcquire)],
            parsers: vec![Box::new(ClaudeShareParse)],
            backends: vec![Box::new(ClaudeCodeBackend)],
        }
```
改成:
```rust
        Registry {
            acquirers: vec![Box::new(ClaudeShareAcquire)],
            // fe-claude-live 只注册 Parse,不注册 Acquire——它没有实现 Acquire trait
            // (数据来源是浏览器扩展主动 POST,不是 ctxrelay 主动拉取,见
            // fe-claude-live/src/lib.rs 顶部的文档注释)。
            parsers: vec![Box::new(ClaudeShareParse), Box::new(ClaudeLiveParse)],
            backends: vec![Box::new(ClaudeCodeBackend)],
        }
```

- [ ] **Step 5: 修改 `crates/ctxrelay-core/src/pipeline.rs`,重构出共用逻辑并新增 `run_import_from_bytes`**

把现有的 `run_import` 函数整个替换成下面这段(多出一个私有的 `commit_document` 辅助函数,`run_import` 和新的 `run_import_from_bytes` 都调用它,避免两份几乎一样的 legalize/lower/commit 逻辑各写一遍):

```rust
/// 完整 import 管线:Acquire → Parse → legalize → lower → (dry-run 提前返回 / commit)。
/// 对应 CLI 的 `ctxrelay import` 子命令。
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
    let parse = registry
        .find_parse(frontend_id)
        .ok_or_else(|| CoreError(format!("no Parse registered for frontend id {frontend_id:?}")))?;
    let doc = parse.parse(raw).map_err(|e| CoreError(e.to_string()))?;
    commit_document(registry, doc, opts)
}

fn commit_document(registry: &Registry, doc: Document, opts: ImportOptions) -> Result<Manifest> {
    let ir_digest = document_digest(&doc);

    let backend = registry
        .find_backend(&opts.backend_name)
        .ok_or_else(|| CoreError(format!("no backend registered with name {:?}", opts.backend_name)))?;

    let (legalized, report) = backend.legalize(&doc);
    let lowered = backend.lower(&legalized).map_err(|e| CoreError(e.to_string()))?;

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
```

`run_ir` 函数本身不用改。确认文件顶部的 `use` 列表已经有 `ctxrelay_ir::Document`(如果没有,加一行 `use ctxrelay_ir::Document;`——`run_ir` 的返回类型已经是 `Result<Document>`,大概率已经导入过)。

- [ ] **Step 6: 修改 `crates/ctxrelay-core/src/lib.rs`,把 `run_import_from_bytes` 加进 `pub use`**

把:
```rust
pub use pipeline::{run_import, run_ir, CoreError, ImportOptions, Result};
```
改成:
```rust
pub use pipeline::{run_import, run_import_from_bytes, run_ir, CoreError, ImportOptions, Result};
```

- [ ] **Step 7: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-core`
Expected: 新的 `pipeline_from_bytes` 两个测试通过,同时 `registry`/`dest`/`pipeline`/`undo`/`verify` 既有测试全部不受影响(`registry.rs` 里那条断言"只有 fe-claude-share 能接受 File source"不会被 fe-claude-live 影响,因为 fe-claude-live 压根没注册 Acquire)。

- [ ] **Step 8: 提交**

```bash
git add crates/ctxrelay-core
git commit -m "feat(core): add run_import_from_bytes for extension-pushed captures, register fe-claude-live Parse"
```

---

### Task 4: `ctxrelay-cli`——`listen` 子命令

**Files:**
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/Cargo.toml`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/src/main.rs`
- Create: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/tests/listen.rs`

`listen` 是一次性的:起服务、等一次抓取、处理完就退出——不做成常驻轮询服务(架构文档 §4 的风险定界:"该 backend 应设计成用户手动触发的一次性调用")。

- [ ] **Step 1: 给 `crates/ctxrelay-cli/Cargo.toml` 加 `tiny_http` 依赖**

```toml
[dependencies]
ctxrelay-core = { path = "../ctxrelay-core" }
ctxrelay-frontend = { path = "../ctxrelay-frontend" }
serde_json = "1"
serde = { version = "1", features = ["derive"] }
clap = { version = "4", features = ["derive"] }
tiny_http = "0.12"
uuid = { version = "1", features = ["v4"] }
```

(`serde` 是新加的——`bridge.rs` 里的 `CaptureRequest` 用了 `#[derive(Deserialize)]`,Task 2 时可能已经加过,如果 `cargo build` 提示重复依赖就跳过这一步。)

- [ ] **Step 2: 写失败的集成测试 `crates/ctxrelay-cli/tests/listen.rs`**

```rust
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn ctxrelay_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ctxrelay"))
}

/// 手写一个最小的 HTTP/1.1 POST 客户端(不引入额外的 HTTP client 依赖,测试范围内
/// 够用):连上 `listen` 起的服务,发一个 CaptureRequest,读回响应体。
fn post_capture(port: u16, token: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to ctxrelay listen");
    let request = format!(
        "POST /capture HTTP/1.1\r\nHost: 127.0.0.1\r\nX-CtxRelay-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();

    let status_line = response.lines().next().unwrap_or("");
    let status_code: u16 = status_line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let body_start = response.find("\r\n\r\n").map(|i| i + 4).unwrap_or(response.len());
    (status_code, response[body_start..].to_string())
}

#[test]
fn listen_accepts_one_capture_and_exits() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-listen-test-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();
    let canonical = project_dir.canonicalize().unwrap();

    let projects_root = std::env::temp_dir().join("ctxrelay-cli-listen-test-projects-root");
    let _ = std::fs::remove_dir_all(&projects_root);
    let slug = canonical.display().to_string().replace('/', "-");
    std::fs::create_dir_all(projects_root.join(&slug)).unwrap();

    let manifest_path = project_dir.join("manifest.json");

    let mut child = Command::new(ctxrelay_bin())
        .arg("listen")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--port")
        .arg("47899")
        .arg("--claude-projects-root")
        .arg(&projects_root)
        .arg("--manifest-out")
        .arg(&manifest_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctxrelay listen");

    // 给服务一点时间起来再连。
    std::thread::sleep(Duration::from_millis(300));

    // 从 stdout 里读出 listen 打印的 token(格式:"token: <uuid>")。
    let stdout = child.stdout.take().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut first_line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut first_line).unwrap();
    let token = first_line
        .split("token: ")
        .nth(1)
        .map(|s| s.trim().to_string())
        .expect("listen should print a token line");

    let snapshot = std::fs::read_to_string("../fe-claude-live/tests/fixtures/sample_live_conversation.json").unwrap();
    let capture_request = format!(
        r#"{{"version":"1","token":"{token}","conversation_id":"fca79960-3026-40e1-beba-6abb33fe20d5","org_id":"ed9a9a3c-9d81-43a0-b974-3aa686e20a87","snapshot":{snapshot}}}"#
    );

    let (status, body) = post_capture(47899, &token, &capture_request);

    assert_eq!(status, 200, "response body: {body}");
    assert!(body.contains("\"status\":\"ok\""), "response body: {body}");

    let exit_status = child.wait().expect("listen process should exit after one capture");
    assert!(exit_status.success());

    assert!(manifest_path.exists());

    std::fs::remove_dir_all(&project_dir).ok();
    std::fs::remove_dir_all(&projects_root).ok();
}

#[test]
fn listen_rejects_wrong_token() {
    let project_dir = std::env::temp_dir().join("ctxrelay-cli-listen-badtoken-project");
    let _ = std::fs::remove_dir_all(&project_dir);
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut child = Command::new(ctxrelay_bin())
        .arg("listen")
        .arg("--to")
        .arg("claude-code")
        .arg("--project")
        .arg(&project_dir)
        .arg("--port")
        .arg("47900")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ctxrelay listen");

    std::thread::sleep(Duration::from_millis(300));

    let (status, _body) = post_capture(47900, "wrong-token", r#"{"version":"1","token":"wrong-token","conversation_id":"x","org_id":"y","snapshot":{}}"#);
    assert_eq!(status, 401);

    child.kill().ok();
    child.wait().ok();
    std::fs::remove_dir_all(&project_dir).ok();
}
```

- [ ] **Step 3: 运行,预期失败(`listen` 子命令还不存在)**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli --test listen`
Expected: 测试编译通过但运行失败(连不上端口/超时),因为 `listen` 子命令还没实现。这是预期的红灯。

- [ ] **Step 4: 修改 `crates/ctxrelay-cli/src/main.rs`,加 `Listen` 子命令**

在 `enum Command` 里,`Verify { manifest: PathBuf },` 后面加一个新 variant:

```rust
    /// 起一个一次性本地服务,等浏览器扩展 POST 一次抓取,跑完整个 import 管线后退出
    Listen {
        #[arg(long)]
        to: String,
        #[arg(long)]
        project: PathBuf,
        #[arg(long, default_value_t = 47651)]
        port: u16,
        #[arg(long)]
        manifest_out: Option<PathBuf>,
        /// 仅测试用:覆盖 `~/.claude/projects` 的默认位置
        #[arg(long)]
        claude_projects_root: Option<PathBuf>,
    },
```

在 `main()` 的 `match cli.command` 里加一条:

```rust
        Command::Listen { to, project, port, manifest_out, claude_projects_root } => {
            run_listen_command(to, project, port, manifest_out, claude_projects_root)
        }
```

在文件末尾(`detect_claude_version` 函数后面)加上:

```rust
fn run_listen_command(
    to: String,
    project: PathBuf,
    port: u16,
    manifest_out: Option<PathBuf>,
    claude_projects_root_override: Option<PathBuf>,
) -> Result<(), String> {
    let token = uuid::Uuid::new_v4().to_string();
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| format!("failed to bind 127.0.0.1:{port}: {e}"))?;

    println!("token: {token}");
    println!(
        "ctxrelay listen 正在监听 http://127.0.0.1:{port} ,在 claude.ai 页面打开扩展设置,\
         把上面这个 token 粘贴进去,然后点一下工具栏图标导入当前对话。"
    );

    let request = server
        .recv()
        .map_err(|e| format!("failed to receive request: {e}"))?;

    let mut body = String::new();
    let mut request = request;
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("failed to read request body: {e}"))?;

    let header_token = request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("X-CtxRelay-Token"))
        .map(|h| h.value.as_str().to_string());

    if header_token.as_deref() != Some(token.as_str()) {
        let response = tiny_http::Response::from_string(
            r#"{"version":"1","status":"error","message":"invalid token"}"#,
        )
        .with_status_code(401);
        let _ = request.respond(response);
        return Err("received request with invalid token".to_string());
    }

    let capture: ctxrelay_cli::bridge::CaptureRequest =
        serde_json::from_str(&body).map_err(|e| format!("invalid CaptureRequest JSON: {e}"))?;

    if capture.token != token {
        let response = tiny_http::Response::from_string(
            r#"{"version":"1","status":"error","message":"invalid token"}"#,
        )
        .with_status_code(401);
        let _ = request.respond(response);
        return Err("CaptureRequest body token does not match".to_string());
    }

    let claude_projects_root = match claude_projects_root_override {
        Some(root) => root,
        None => claude_projects_root().map_err(|e| e.to_string())?,
    };
    let cli_version = detect_claude_version().unwrap_or_else(|| "unknown".to_string());

    let registry = Registry::with_defaults();
    let opts = ImportOptions {
        backend_name: to,
        project_dir: project.clone(),
        dry_run: false,
        allow_bootstrap: false,
        claude_projects_root,
        cli_version,
    };

    let raw = serde_json::to_vec(&capture.snapshot).map_err(|e| e.to_string())?;
    let result = ctxrelay_core::run_import_from_bytes(&registry, raw, "fe-claude-live", opts);

    let (status_code, response_body, outcome) = match &result {
        Ok(manifest) => {
            let manifest_path = manifest_out.unwrap_or_else(|| {
                let session_id =
                    manifest.created_session_ids.first().cloned().unwrap_or_else(|| "unknown".to_string());
                project.join(".ctxrelay").join("manifests").join(format!("{session_id}.manifest.json"))
            });
            if let Some(parent) = manifest_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let json = serde_json::to_string_pretty(manifest).unwrap_or_default();
            let _ = std::fs::write(&manifest_path, json);
            (
                200,
                r#"{"version":"1","status":"ok"}"#.to_string(),
                Ok(format!(
                    "committed session {:?}, manifest saved to {}",
                    manifest.created_session_ids,
                    manifest_path.display()
                )),
            )
        }
        Err(e) => (
            200,
            format!(r#"{{"version":"1","status":"error","message":{:?}}}"#, e.to_string()),
            Err(e.to_string()),
        ),
    };

    let response = tiny_http::Response::from_string(response_body).with_status_code(status_code);
    let _ = request.respond(response);

    match outcome {
        Ok(message) => {
            println!("{message}");
            Ok(())
        }
        Err(message) => Err(message),
    }
}
```

需要在文件顶部的 `use` 列表里加上 `use std::io::Read;`(`request.as_reader().read_to_string(...)` 需要这个 trait)。

`tiny_http` 的 `Header`/`HeaderField`/`AsciiStr` 的精确方法名(比如 `h.field.as_str().as_str()` 这种两次 `.as_str()` 调用)是根据这个 crate 通常的 API 形状写的,没有在这台机器上实际编译验证过。如果 `cargo build` 报 `.as_str()` 不存在或者类型不对,打开 `~/.cargo/registry/src/*/tiny_http-0.12.*/src/`(或者直接 `cargo doc -p tiny_http --open`)确认 `Header` 结构体和 `HeaderField` 的实际字段/方法,按实际 API 调整这几行——核心逻辑(读 header、按字段名大小写不敏感比较、拿到值字符串)不变,不需要改设计,只是具体方法名可能需要对一下。

- [ ] **Step 5: 运行,预期通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli --test listen`
Expected: `listen_accepts_one_capture_and_exits ... ok`、`listen_rejects_wrong_token ... ok`。

如果 `listen_rejects_wrong_token` 卡住不退出(因为 `listen` 收到坏 token 后仍然 `return Err`,进程会退出,不应该卡住)——如果确实卡住,检查是不是 `server.recv()` 之前有别的阻塞点;这条测试最后有 `child.kill()` 兜底,不会让测试套件永久挂起,但如果卡住说明实现有 bug,需要修 `main.rs` 而不是加大 timeout。

- [ ] **Step 6: 提交**

```bash
git add crates/ctxrelay-cli
git commit -m "feat(cli): add listen subcommand, a one-shot local HTTP bridge for the browser extension"
```

---

### Task 5: 浏览器扩展

**Files:**
- Modify: `/Users/caoxinzhuo/code/ctxRelay/extension/package.json`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/manifest.json`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/options.html`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/src/background.ts`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/src/options.ts`

这一步没有 Rust 编译器兜底,靠 `tsc` 类型检查 + 后面 Task 6 里真实加载进 Chrome 手工验证。

- [ ] **Step 1: 把 `extension/package.json` 换成真正能跑的版本**

```json
{
  "name": "ctxrelay-extension",
  "version": "0.1.0",
  "private": true,
  "description": "ctxRelay 浏览器扩展:一键把当前打开的 claude.ai 对话(含 thinking)导入本地 ctxrelay。",
  "scripts": {
    "build": "tsc"
  },
  "devDependencies": {
    "typescript": "^5.6.0"
  }
}
```

- [ ] **Step 2: 写 `extension/manifest.json`**

```json
{
  "manifest_version": 3,
  "name": "ctxRelay",
  "version": "0.1.0",
  "description": "把当前打开的 claude.ai 对话(含 thinking)一键导入本地 ctxrelay。",
  "action": {
    "default_title": "导入到 ctxRelay"
  },
  "background": {
    "service_worker": "dist/background.js"
  },
  "options_page": "options.html",
  "permissions": ["activeTab", "storage"],
  "host_permissions": ["https://claude.ai/*", "http://127.0.0.1/*"]
}
```

- [ ] **Step 3: 写 `extension/options.html`**

```html
<!DOCTYPE html>
<html lang="zh">
<head>
  <meta charset="utf-8" />
  <title>ctxRelay 设置</title>
</head>
<body>
  <h1>ctxRelay 设置</h1>
  <p>先在终端跑 <code>ctxrelay listen --to claude-code --project &lt;你的项目目录&gt;</code>,把它打印出来的 token 粘贴到下面。</p>
  <label>Token: <input id="token" type="text" size="40" /></label><br />
  <label>端口(默认 47651): <input id="port" type="number" value="47651" /></label><br />
  <button id="save">保存</button>
  <p id="status"></p>
  <script src="dist/options.js"></script>
</body>
</html>
```

- [ ] **Step 4: 写 `extension/src/options.ts`**

```typescript
const tokenInput = document.getElementById("token") as HTMLInputElement;
const portInput = document.getElementById("port") as HTMLInputElement;
const saveButton = document.getElementById("save") as HTMLButtonElement;
const statusEl = document.getElementById("status") as HTMLParagraphElement;

async function load(): Promise<void> {
  const stored = await chrome.storage.local.get(["ctxrelayToken", "ctxrelayPort"]);
  if (typeof stored.ctxrelayToken === "string") {
    tokenInput.value = stored.ctxrelayToken;
  }
  if (typeof stored.ctxrelayPort === "number") {
    portInput.value = String(stored.ctxrelayPort);
  }
}

saveButton.addEventListener("click", async () => {
  const token = tokenInput.value.trim();
  const port = parseInt(portInput.value, 10) || 47651;
  await chrome.storage.local.set({ ctxrelayToken: token, ctxrelayPort: port });
  statusEl.textContent = "已保存。";
});

void load();
```

- [ ] **Step 5: 写 `extension/src/background.ts`**

```typescript
interface CaptureRequest {
  version: "1";
  token: string;
  conversation_id: string;
  org_id: string;
  captured_at: string;
  snapshot: unknown;
}

interface Organization {
  uuid: string;
}

const CHAT_URL_PATTERN = /^https:\/\/claude\.ai\/chat\/([0-9a-f-]{36})/;

async function findSnapshotAndOrg(
  conversationId: string
): Promise<{ snapshot: unknown; orgId: string } | null> {
  const orgsRes = await fetch("https://claude.ai/api/organizations", { credentials: "include" });
  if (!orgsRes.ok) {
    return null;
  }
  const orgs = (await orgsRes.json()) as Organization[];

  // 大多数账号只有一个组织,这里走一个快速路径,但仍然兼容多组织账号——
  // 逐个尝试,直到某个组织真的能看到这条对话(实测确认过这个接口对话不存在
  // 时返回非 200,不会抛异常,只是 res.ok 为 false)。
  for (const org of orgs) {
    const url =
      `https://claude.ai/api/organizations/${org.uuid}/chat_conversations/${conversationId}` +
      `?tree=True&rendering_mode=messages&render_all_tools=true&consistency=strong`;
    const res = await fetch(url, { credentials: "include" });
    if (res.ok) {
      const snapshot = await res.json();
      return { snapshot, orgId: org.uuid };
    }
  }
  return null;
}

async function captureAndSend(tabId: number, tabUrl: string): Promise<void> {
  const match = tabUrl.match(CHAT_URL_PATTERN);
  if (!match) {
    await chrome.action.setBadgeText({ text: "N/A", tabId });
    return;
  }
  const conversationId = match[1];

  const stored = await chrome.storage.local.get(["ctxrelayToken", "ctxrelayPort"]);
  const token = typeof stored.ctxrelayToken === "string" ? stored.ctxrelayToken : "";
  const port = typeof stored.ctxrelayPort === "number" ? stored.ctxrelayPort : 47651;

  if (!token) {
    await chrome.action.setBadgeText({ text: "CFG", tabId });
    return;
  }

  const found = await findSnapshotAndOrg(conversationId);
  if (!found) {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }

  const captureRequest: CaptureRequest = {
    version: "1",
    token,
    conversation_id: conversationId,
    org_id: found.orgId,
    captured_at: new Date().toISOString(),
    snapshot: found.snapshot,
  };

  try {
    const postRes = await fetch(`http://127.0.0.1:${port}/capture`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-CtxRelay-Token": token },
      body: JSON.stringify(captureRequest),
    });
    await chrome.action.setBadgeText({ text: postRes.ok ? "OK" : "ERR", tabId });
  } catch {
    // ctxrelay listen 大概率还没起,或者端口不对——用户需要先在终端跑 listen。
    await chrome.action.setBadgeText({ text: "N/L", tabId });
  }
}

chrome.action.onClicked.addListener((tab) => {
  if (tab.id === undefined || !tab.url) {
    return;
  }
  void captureAndSend(tab.id, tab.url);
});
```

- [ ] **Step 6: `npm install` + 编译验证**

Run: `cd /Users/caoxinzhuo/code/ctxRelay/extension && npm install && npm run build`
Expected: `tsc` 无类型错误,`dist/background.js` 和 `dist/options.js` 被生成。

Run: `ls /Users/caoxinzhuo/code/ctxRelay/extension/dist/`
Expected: 看到 `background.js`、`options.js`。

- [ ] **Step 7: 提交(不要提交 `node_modules/`——已经在根 `.gitignore` 里被忽略;`dist/` 也已经被忽略,是构建产物,同样不提交)**

```bash
cd /Users/caoxinzhuo/code/ctxRelay
git add extension/package.json extension/manifest.json extension/options.html extension/src
git commit -m "feat(extension): implement one-click capture of the current authenticated claude.ai conversation"
```

---

### Task 6: 端到端验证(真实加载扩展 + 真实抓取,不花 API 额度;最后可选一步 verify 会花一点)

**Files:** 无新文件,只验证。这一步需要用浏览器工具手动操作,不是纯 `cargo test`。

- [ ] **Step 1: 在 Chrome 里以开发者模式加载扩展**

打开 `chrome://extensions`,打开右上角"开发者模式",点"加载已解压的扩展程序",选择 `/Users/caoxinzhuo/code/ctxRelay/extension` 目录(注意选整个 `extension/` 目录,不是 `dist/`——`manifest.json` 在 `extension/` 根目录,里面引用的是相对路径 `dist/background.js`)。
Expected: 扩展列表里出现"ctxRelay",没有报错。

- [ ] **Step 2: 起 `ctxrelay listen`,记下打印的 token**

Run(前台跑,不要加 `&`,后面要盯着它的输出和退出状态):
```bash
cd /Users/caoxinzhuo/code/ctxRelay
mkdir -p /tmp/ctxrelay-e2e-live-test
cargo run -p ctxrelay-cli -- listen --to claude-code --project /tmp/ctxrelay-e2e-live-test --manifest-out /tmp/ctxrelay-e2e-live-test/manifest.json
```
Expected: 打印一行 `token: <uuid>`,然后打印提示信息,进程挂起等待请求。

- [ ] **Step 3: 打开扩展的设置页,填入 token**

右键点扩展图标 →"选项"(或 `chrome://extensions` 里点"扩展程序选项"),把上一步的 token 粘贴进去,端口留默认 `47651`,点保存。

- [ ] **Step 4: 打开一个真实的 claude.ai 对话,点扩展图标**

导航到 `https://claude.ai/chat/fca79960-3026-40e1-beba-6abb33fe20d5`(或任何一条你自己的真实对话),等页面加载完,点工具栏上的 ctxRelay 图标。
Expected:图标右下角出现一个"OK"徽章。

- [ ] **Step 5: 确认 `ctxrelay listen` 那边真的收到了、commit 成功了、进程退出了**

回到跑 `listen` 的终端。
Expected: 打印出类似 `committed session [...], manifest saved to /tmp/ctxrelay-e2e-live-test/manifest.json` 的一行,然后进程自己退出(退出码 0)。

- [ ] **Step 6: 检查真实写盘的会话文件**

Run: `cat /tmp/ctxrelay-e2e-live-test/manifest.json | python3 -c "import json,sys; m=json.load(sys.stdin); print(m['created_session_ids']); print(m['report'])"`
Expected: 能看到一个 session id,`report.dropped_reasoning` 应该 ≥ 1(如果这条对话真的有 thinking,legalize 应该把它丢了)。

Run(把上一步拿到的 session id 换进去):
```bash
find ~/.claude/projects -iname "*ctxrelay-e2e-live-test*" -type d
```
Expected: 找到一个目录,里面有一个 `<session-id>.jsonl` 文件,`cat` 出来能看到刚才那条对话的真实内容(用 `grep thinking` 应该搜不到,因为 legalize 丢弃了)。

- [ ] **Step 7:(可选,会花一点真实 API 额度)用 `ctxrelay verify` 冒烟测试这次真实抓取出来的会话**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo run -p ctxrelay-cli -- verify /tmp/ctxrelay-e2e-live-test/manifest.json`
Expected: 打印出一句真实的、关于刚才那条对话内容的总结(不需要逐字比对,眼看内容是不是真的对得上这条对话就行)。

- [ ] **Step 8: 清理**

```bash
rm -rf /tmp/ctxrelay-e2e-live-test
find ~/.claude/projects -iname "*ctxrelay-e2e-live-test*" -type d -exec rm -rf {} +
```
在 `chrome://extensions` 里把 ctxRelay 扩展移除(这次验证结束后不需要一直常驻)。

- [ ] **Step 9: 记录验证结果(不新增代码,只是把这次手工验证的结论写进 commit message 里,方便以后查)**

```bash
cd /Users/caoxinzhuo/code/ctxRelay
git commit --allow-empty -m "chore: manually verified extension end-to-end capture flow (load unpacked -> listen -> click -> real commit -> verify)"
```

---

### Task 7: 收尾验证

**Files:** 无新文件,只验证。

- [ ] **Step 1: 整个 workspace(Rust 部分)编译**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo build --workspace 2>&1 | tail -10`
Expected: `Finished`,无 error。

- [ ] **Step 2: 整个 workspace 默认测试全绿**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test --workspace 2>&1 | tail -100`
Expected: 之前所有 crate 的测试 + `fe-claude-live`(parse)+ `ctxrelay-core`(pipeline_from_bytes)+ `ctxrelay-cli`(bridge/listen)全部 `ok`,已有的 `#[ignore]` 测试(conformance/dest bootstrap/verify/cli e2e)照常显示 `ignored`。

- [ ] **Step 3: clippy**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo clippy --workspace --all-targets 2>&1 | tail -40`
Expected: 无警告。

- [ ] **Step 4: 确认依赖图——`fe-claude-live` 只依赖 `ir`/`frontend`,`ctxrelay-core` 现在同时依赖两个 frontend(`fe-claude-share`、`fe-claude-live`)和一个 backend**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo tree -p fe-claude-live --depth 1 && echo --- && cargo tree -p ctxrelay-core --depth 1`
Expected: `fe-claude-live` 只列出 `ctxrelay-ir`/`ctxrelay-frontend`/`serde`/`serde_json`/`semver`/`time`;`ctxrelay-core` 比上一个里程碑多了一行 `fe-claude-live`。

- [ ] **Step 5: `extension/` 独立编译不受 Cargo workspace 影响,反之亦然**

Run: `cd /Users/caoxinzhuo/code/ctxRelay/extension && npm run build && cd /Users/caoxinzhuo/code/ctxRelay && cargo build --workspace 2>&1 | tail -5`
Expected: 两边都干净,互不影响。

- [ ] **Step 6: 最终提交(如果前面步骤有任何未提交的修正)**

```bash
git status --short
```
若有改动:
```bash
git add -A
git commit -m "chore: verify fe-claude-live + bridge workspace state"
```

---

## 完成后的状态

- `fe-claude-live` 是第二个 frontend,只实现 `Parse`,用真实认证态数据验证过能正确处理 thinking(丢弃前先如实标注,legalize 阶段按既有规则丢弃)。
- `ctxrelay-core` 新增 `run_import_from_bytes`,让"字节从哪来"这件事从"必须经过 Acquire"这个假设里解耦出来。
- `bridge-protocol/schema.json` 定型,Rust 端有一份手写但契约测试兜底的类型;TS 端手写的 `CaptureRequest` 接口暂时没有自动化的跨语言一致性检查(architecture.md §10.1 理想态的 `typify`/`json-schema-to-typescript` codegen 留作后续,当前只靠人工对照 schema 保持一致)——这是一个已知的、可接受的简化,不是疏漏。
- `ctxrelay-cli` 新增 `listen`,一次性本地服务,不是常驻进程。
- `extension/` 是真正能跑的 Manifest V3 扩展:点一下图标,把当前打开的私有对话(含 thinking)POST 给本地 `listen`,已经用真实浏览器 + 真实 `claude.ai` 账号手工验证过完整链路能跑通。
- 已知限制:多组织账号的探测是"逐个尝试直到 200",没有做更智能的"这个对话属于哪个组织"的直接判断(claude.ai 前端本身大概率也是这么做的,或者有一个更直接的信号,但没有再深入逆向);`listen` 目前只处理一次抓取就退出,不支持"保持监听、连续导入好几个对话"这种用法,需要就重新起一次。
