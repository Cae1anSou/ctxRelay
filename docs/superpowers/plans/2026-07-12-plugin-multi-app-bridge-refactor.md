# 插件/Rust 前端职责边界 + 桥接契约泛化 + 插件多源重构 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不新增任何第二个真实源应用(ChatGPT 等)的前提下,把"给一个新 Web 源加支持"这件事的代价从"重写插件"降到"加一个文件 + 注册一行",分三步做到:(1) 把插件与 Rust 前端的职责边界写进架构文档,补上一处已经过时的描述;(2) 泛化 CLI↔插件之间唯一的契约 `bridge-protocol/schema.json`,去掉硬编码的 Claude 专有字段,加一个路由用的 `frontend_id`;(3) 把插件内部从"一个写死处理 claude.ai 的文件"重构成"按站点分发的抓取源注册表",结构上镜像 Rust 侧已经存在的 `Acquire`/`Parse` + `Registry` 模式。

**Architecture:** 现状(已读过全部相关源码确认):Rust 侧的 `ctxrelay-frontend` 早就定义了 `Acquire`(有副作用的拉取)/`Parse`(纯函数,原始字节→IR)两个 trait,`ctxrelay-core::Registry` 按字符串 `id()` 把两者配对、按 `frontend_id` 分发(`crates/ctxrelay-core/src/registry.rs`、`crates/ctxrelay-core/src/pipeline.rs::run_import_from_bytes`)。`fe-claude-live` 只实现 `Parse`——数据来源是浏览器插件主动 POST 来的字节,不是 ctxrelay 主动拉取。这条分发链路本身已经是通用的、可扩展的;唯一的硬编码点是 `crates/ctxrelay-cli/src/main.rs:333` 把 `frontend_id` 写死成字符串字面量 `"fe-claude-live"`,以及 `bridge-protocol/schema.json` 里 `CaptureRequest` 的必需字段 `conversation_id`/`org_id` 是 Claude 专有的、且确认在 Rust 侧反序列化之后再没被读取过(纯粹的死字段)。插件侧(`extension/src/background.ts`)承担的是"认页面 + 调用该应用私有 API 拿认证态数据"这一件事,这本质上是 `Acquire` 语义在浏览器进程里的一个分布式实现,只是跨语言、跨进程,靠 `bridge-protocol/schema.json` 而非 Cargo 依赖图维持契约(架构文档 §10.1 已经点出这一点,但 §10.1 目前描述的还是一个从未落地、后来被"扩展直接抓取当前对话"方案取代的 job-polling 设计,需要一并修正)。据此定的职责边界:**插件只负责"按站点认页面 + 用该应用的方式把原始 payload 弄到手",不负责解释 payload 内容;"把原始 payload 解释成中立 IR"永远留在 Rust 侧、按 `frontend_id` 对应一个 Parse crate**——这条边界不能移动,否则分享链接那条完全不经过插件的路径(`fe-claude-share`)就必须在两个地方各写一份解析逻辑。

**Tech Stack:** Rust(`serde`/`serde_json`,现有 `ctxrelay-cli`/`ctxrelay-core` 测试基建),TypeScript(Manifest V3 浏览器扩展,`tsc` 编译,无打包器——`extension/tsconfig.json` 用 `moduleResolution: "Bundler"`,发出的 JS import 路径与源码里写的一致,不会被重写)。

## Global Constraints

- 注释语言:中文,只解释"为什么",不解释"做什么"(项目现有代码全部遵循此约定,新代码必须一致)。
- `bridge-protocol/schema.json` 是 CLI↔插件之间唯一的契约权威来源(架构文档 §10.1),Rust 和 TS 两侧手写投影必须严格照抄字段名/必需性。
- 不引入任何未经验证的第二个源应用(ChatGPT 等)的具体解析逻辑——本计划只搭好扩展点,不猜测未经实测的 API 形状(参照 `fe-claude-live` 当初的做法:先用真实登录态实测,才敢写 parse 逻辑,见 `docs/superpowers/plans/2026-07-07-fe-claude-live-bridge.md` 开头"已实测确认的关键事实"一节)。
- 插件侧没有测试运行时(`extension/` 下没有任何 test runner),验证手段是 `tsc --noEmit` 类型检查 + 加载 unpacked 扩展手工验证,这是本仓库对 TS 代码一贯的验证方式(见同一份历史计划 Task 5 的说明),不是本计划降低了标准。
- 每个 Task 结束必须保证:`cargo build --workspace` 和 `cargo test --workspace` 全绿(Rust 侧),`extension` 目录下 `npx tsc --noEmit` 无错误(TS 侧)。

---

## Task 1: 更新 `docs/architecture.md`——记录职责边界,修正过时的 bridge-protocol 描述

**Files:**
- Modify: `/Users/caoxinzhuo/code/ctxRelay/docs/architecture.md`(§10.1 替换,新增 §10.2)

**Interfaces:**
- Consumes: 无(纯文档)。
- Produces: 后续 Task 2/3 的实现必须与本 Task 写下的契约描述一致——如果实现阶段发现需要偏离,回来改这份文档,不要让文档和代码分叉(这是本文档自己在 §5 就示范过的规矩:"实现落地时的偏离")。

- [ ] **Step 1: 把 §10.1 里过时的 job/response 描述换成实际落地的 CaptureRequest/CaptureResponse 推送模型 + 泛化后的字段**

`docs/architecture.md` 现在的 §10.1 描述的是一个从未真正实现的设计(`{version, job_id, token, target_url}`,CLI 挂任务、插件轮询取任务)——实际落地的是"用户点一下工具栏图标、插件立刻抓取当前对话、直接 POST 给本地一次性监听的 `ctxrelay listen`"这个更简单的推送模型(见 `docs/superpowers/plans/2026-07-07-fe-claude-live-bridge.md` 开头的 Goal:"按用户的新决定——不走'分享链接'路线"),但 §10.1 从没被回来更新过。用下面这段替换 §10.1 原有的三个要点(保留小标题和整体结构,只换内容):

```markdown
### 10.1 bridge-protocol:跨语言那道边界靠什么维持纪律

`ctxrelay-cli` 和 `extension/background.ts` 是两个不同运行时、不同语言的进程,唯一的接触点是 `ctxrelay listen` 起的那个 `127.0.0.1` 本地一次性端点。这条边界如果只靠"两边约定好格式"心照不宣地维护,就是整个设计里唯一一处失去编译期保障、退化回口头约定的地方——这和 §6 那条"IR 是 frontend/backend 唯一沟通媒介"的原则在精神上是一回事,只是这次没有共享的类型系统能帮你兜底,所以必须显式补一层:

- `bridge-protocol/schema.json` 是这条契约**唯一的权威来源**,定义 `CaptureRequest`(插件 POST 给本地服务的请求体)和 `CaptureResponse`(处理结果)两个形状,带独立于 `ir_version` 的自己的版本号字段——它和 IR 是两条不同的 ABI,不要合并。
- **`CaptureRequest` 只携带三类信息,不掺入任何具体应用的语义**:`token`(配对凭证)、`frontend_id`(路由键,必须等于 Rust 侧某个已注册 `Parse::id()`,例如 `"fe-claude-live"`——`ctxrelay listen` 收到请求后据此在 `Registry` 里查出对应的 `Parse` 实现,不再写死)、`snapshot`(不透明 payload,具体形状完全由 `frontend_id` 对应的 `Parse` 决定,桥本身不解释、也不应该解释其内容)。早期版本的 `CaptureRequest` 还带过 `conversation_id`/`org_id` 两个 Claude 专有字段——这两个字段除了在 Rust 侧反序列化之外从未被下游任何逻辑读取过,是纯粹的死字段,泛化契约时已删除;插件如果需要人类可读的调试标识,应该放进它自己拥有的 `snapshot` 内容里,不属于桥协议本身。
- **两侧的类型都手写投影,严格照抄这份 schema。** V1 不引入 `typify`/`json-schema-to-typescript` 代码生成,靠 `crates/ctxrelay-cli/tests/bridge.rs` 里一条"反序列化插件会发出的样例 JSON"的测试,作为两边没有漂移的最小兜底验证——字段名/必需性任何一次不同步,这条测试会先炸。
- `frontend_id` 这个路由键选在"和 Rust 侧 `Parse::id()` 完全相同的字符串"上,不是巧合:这样插件侧新增一个抓取源时,只需要知道"我对应哪个已经注册好的 Rust frontend",不需要发明一套独立的应用标识体系。见 §10.2。
```

- [ ] **Step 2: 在 §10.1 后面插入新的 §10.2,记录插件侧的职责边界与多源扩展点**

在刚替换完的 §10.1 段落后面(即 §10 "插件化(为 N/M 增长预留)" 这个既有子标题之前)插入:

```markdown
### 10.2 插件侧的职责边界:只做"抓取",不做"解释"

`extension/` 是 §2 narrow waist 图里 frontend 侧的一部分,但它天生没法直接实现 `crates/ctxrelay-frontend` 里的 `Acquire`/`Parse` trait——那是 Rust 类型,插件是另一个语言、另一个进程。它实际扮演的角色是 `Acquire` 语义的一个跨语言实现:**只负责"认出这是哪个应用的页面 + 用该应用私有的、需要认证态的方式把原始数据弄到手",不负责解释数据内容**。解释内容(把厂商专有 JSON 结构 lower 成中立 IR)永远是 Rust 侧对应 `frontend_id` 的 `Parse` 实现的职责,不允许下沉到插件里——原因很直接:`fe-claude-share` 这条路径(账号导出/分享快照)完全不经过插件,如果解析逻辑被分到插件里一份,就必须在 TS 和 Rust 两边各维护一份,或者被迫让所有导入路径都绕道插件,两者都违反 §2 的解耦初衷。

这条边界落到插件内部结构上,是一个"抓取源(`CaptureSource`)注册表"模式,直接镜像 Rust 侧 `ctxrelay-core::Registry` 按 `id()` 匹配的做法:

```
extension/src/
├── sources/
│   ├── types.ts        # CaptureSource 接口:frontendId + matches(url) + capture(url)
│   ├── claude-live.ts   # claude.ai 的实现,frontendId = "fe-claude-live"
│   └── registry.ts      # SOURCES 数组 + resolveSource(url),新增站点只加一行
├── bridge.ts             # 通用的"打包成 CaptureRequest + POST + 设 badge",不认识任何具体应用
├── background.ts         # 入口:按 URL 从 registry 里选出抓取源,拿到 snapshot 后交给 bridge.ts
└── options.ts             # 不变
```

新增一个应用(例如 ChatGPT)的代价被压缩成:在 `sources/` 下新增一个实现 `CaptureSource` 的文件(内容是该应用私有 API 的实测结果,方法上参照 `fe-claude-live` 当初的实测流程,不能凭猜测编写)、在 `registry.ts` 的 `SOURCES` 数组里加一行、在 Rust 侧对称地新增一个 `fe-<app>-live` crate 实现 `Parse` 并注册进 `ctxrelay-core::Registry`。两侧都不需要改 `bridge-protocol/schema.json`,也不需要改 `background.ts`/`bridge.ts`——这正是这条边界要保护的东西。
```

- [ ] **Step 3: 提交**

```bash
git add docs/architecture.md
git commit -m "docs(architecture): record plugin/rust-frontend responsibility split, fix stale bridge-protocol description"
```

---

## Task 2: 泛化 `bridge-protocol` 契约——加 `frontend_id`,去掉死字段 `conversation_id`/`org_id`

**Files:**
- Modify: `/Users/caoxinzhuo/code/ctxRelay/bridge-protocol/schema.json`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/src/bridge.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/tests/bridge.rs`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/src/main.rs`(第 333 行附近)
- Modify: `/Users/caoxinzhuo/code/ctxRelay/crates/ctxrelay-cli/tests/listen.rs`

**Interfaces:**
- Consumes: 无新依赖,复用现有 `ctxrelay_core::run_import_from_bytes(registry, raw, frontend_id, opts)`(签名不变,`crates/ctxrelay-core/src/pipeline.rs`)。
- Produces: `ctxrelay_cli::bridge::CaptureRequest` 新增公开字段 `pub frontend_id: String`,移除 `pub conversation_id: String`/`pub org_id: String`。Task 3(插件)必须在它发出的 JSON 里带 `frontend_id`,不再带 `conversation_id`/`org_id`。

- [ ] **Step 1: 改测试 `crates/ctxrelay-cli/tests/bridge.rs`,预期编译失败(红灯)**

把整个文件替换成:

```rust
use ctxrelay_cli::bridge::CaptureRequest;

#[test]
fn deserializes_a_capture_request_matching_the_schema() {
    let raw = r#"
    {
      "version": "1",
      "token": "abc123",
      "frontend_id": "fe-claude-live",
      "captured_at": "2026-07-07T01:00:00Z",
      "snapshot": { "uuid": "fca79960-3026-40e1-beba-6abb33fe20d5", "chat_messages": [] }
    }
    "#;

    let request: CaptureRequest =
        serde_json::from_str(raw).expect("should deserialize per bridge-protocol schema");

    assert_eq!(request.version, "1");
    assert_eq!(request.token, "abc123");
    assert_eq!(request.frontend_id, "fe-claude-live");
}

#[test]
fn rejects_a_request_missing_a_required_field() {
    let raw = r#"{ "version": "1", "token": "abc123" }"#;

    let result: Result<CaptureRequest, _> = serde_json::from_str(raw);

    assert!(
        result.is_err(),
        "frontend_id/snapshot are required by the schema"
    );
}
```

- [ ] **Step 2: 运行,确认编译失败**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli --test bridge`
Expected: 编译错误——`CaptureRequest` 目前没有 `frontend_id` 字段。这是预期的红灯。

- [ ] **Step 3: 改 `crates/ctxrelay-cli/src/bridge.rs`——`CaptureRequest` 换字段**

把文件里的 `CaptureRequest` 结构体(保留 `CaptureResponse` 及其 impl 不变)从:

```rust
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

改成:

```rust
/// `frontend_id` 是 Rust 侧 `Registry::find_parse` 用的路由键,必须等于某个已注册
/// frontend crate 的 `Parse::id()`(例如 `"fe-claude-live"`)——桥本身不认识任何
/// 具体应用,只按这个字符串转发,具体应用是谁由发请求的插件决定。
#[derive(Debug, Deserialize)]
pub struct CaptureRequest {
    pub version: String,
    pub token: String,
    pub frontend_id: String,
    #[serde(default)]
    pub captured_at: Option<String>,
    pub snapshot: serde_json::Value,
}
```

- [ ] **Step 4: 运行,确认通过**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli --test bridge`
Expected: 两个测试都 `ok`。

- [ ] **Step 5: 改 `bridge-protocol/schema.json`,与 Rust 端保持一致**

把 `CaptureRequest` 定义换成:

```json
    "CaptureRequest": {
      "description": "插件 POST 给本地 ctxrelay listen 服务的请求体。",
      "type": "object",
      "required": ["version", "token", "frontend_id", "snapshot"],
      "properties": {
        "version": { "type": "string", "const": "1" },
        "token": { "type": "string", "description": "ctxrelay listen 启动时打印的一次性 token,用户手动粘贴进插件设置页配对。" },
        "frontend_id": { "type": "string", "description": "路由键,必须等于 Rust 侧某个已注册 frontend crate 的 Parse::id(),例如 \"fe-claude-live\"。ctxrelay listen 据此在 Registry 里查出对应的 Parse 实现,不再写死。" },
        "captured_at": { "type": "string", "description": "RFC3339 时间戳,插件抓取时的本地时间,仅供人读,不参与 IR。" },
        "snapshot": { "type": "object", "description": "不透明的原始 payload,具体形状完全由 frontend_id 对应的 Parse 实现决定,schema 本身不解释其内容。" }
      }
    },
```

- [ ] **Step 6: 改 `crates/ctxrelay-cli/src/main.rs`,去掉硬编码的 `frontend_id`**

第 333 行(`run_listen_command` 函数内)从:

```rust
    let result = run_import_from_bytes(&registry, raw, "fe-claude-live", opts);
```

改成:

```rust
    let result = run_import_from_bytes(&registry, raw, &capture.frontend_id, opts);
```

(`run_import_from_bytes` 对未注册的 `frontend_id` 已经会走 `CoreError` 分支、被这个函数已有的 `Err(e) => (500, CaptureResponse::error(...), ...)` 处理成一个诚实的 500 响应,不需要额外的错误处理——这条路径在 Task 2 开始前就已经是对的,泛化字段来源不改变这一点。)

- [ ] **Step 7: 改 `crates/ctxrelay-cli/tests/listen.rs`,把三处样例 JSON 换成新字段**

文件里有三处内联 JSON 字符串同时用了 `conversation_id`/`org_id`(两处在 `capture_request` 的 `format!` 里,一处在 `listen_rejects_wrong_token` 里)。全部替换成用 `frontend_id`:

把:
```rust
    let capture_request = format!(
        r#"{{"version":"1","token":"{token}","conversation_id":"fca79960-3026-40e1-beba-6abb33fe20d5","org_id":"ed9a9a3c-9d81-43a0-b974-3aa686e20a87","snapshot":{snapshot}}}"#
    );
```
的两处出现,统一改成:
```rust
    let capture_request = format!(
        r#"{{"version":"1","token":"{token}","frontend_id":"fe-claude-live","snapshot":{snapshot}}}"#
    );
```

把:
```rust
        r#"{"version":"1","token":"wrong-token","conversation_id":"x","org_id":"y","snapshot":{}}"#,
```
改成:
```rust
        r#"{"version":"1","token":"wrong-token","frontend_id":"fe-claude-live","snapshot":{}}"#,
```

- [ ] **Step 8: 跑完整的 `ctxrelay-cli` 测试套件,确认没有回归**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && cargo test -p ctxrelay-cli`
Expected: 全部通过,包括 `listen_accepts_one_capture_and_exits`(会真的起服务、发一次抓取、断言 200 + manifest 落盘)和 `listen_rejects_wrong_token`。

- [ ] **Step 9: 跑一遍全 workspace 确认没有别的地方引用了 `conversation_id`/`org_id`**

Run: `cd /Users/caoxinzhuo/code/ctxRelay && grep -rn "conversation_id\|org_id" crates/ bridge-protocol/ && cargo build --workspace`
Expected: `grep` 应该没有匹配(全部已清理);`cargo build --workspace` 编译通过。

- [ ] **Step 10: 提交**

```bash
git add bridge-protocol/schema.json crates/ctxrelay-cli/src/bridge.rs crates/ctxrelay-cli/src/main.rs crates/ctxrelay-cli/tests/bridge.rs crates/ctxrelay-cli/tests/listen.rs
git commit -m "feat(bridge-protocol): generalize CaptureRequest with frontend_id routing key, drop dead Claude-specific fields"
```

---

## Task 3: 插件重构——按站点分发的 `CaptureSource` 注册表

**Files:**
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/src/sources/types.ts`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/src/sources/claude-live.ts`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/src/sources/registry.ts`
- Create: `/Users/caoxinzhuo/code/ctxRelay/extension/src/bridge.ts`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/extension/src/background.ts`
- Modify: `/Users/caoxinzhuo/code/ctxRelay/extension/manifest.json`

**Interfaces:**
- Consumes: Task 2 产出的 `bridge-protocol` 契约——`CaptureRequest` 现在要求 `frontend_id` 字段,不再有 `conversation_id`/`org_id`。
- Produces: `CaptureSource` 接口(`sources/types.ts`)——后续新增站点的唯一契约面。`resolveSource(url): CaptureSource | undefined`(`sources/registry.ts`)。`sendCapture(tabId, frontendId, token, port, snapshot): Promise<void>`(`bridge.ts`)。

- [ ] **Step 1: 写 `extension/src/sources/types.ts`**

```typescript
/**
 * 一个"抓取源"认领一类页面(按 URL 匹配),知道怎么在该应用已登录的认证态下
 * 把当前对话的原始数据弄到手。这一层只做"怎么拿到数据",不解释数据内容——
 * 内容解释是 Rust 侧对应 frontendId 的 Parse 实现的职责,见
 * `crates/ctxrelay-frontend/src/lib.rs` 里 Acquire/Parse 的拆分,以及
 * `docs/architecture.md` §10.2。新增一个应用 = 新增一个实现这个接口的模块 +
 * 在 registry.ts 里注册一行,不需要改 background.ts 或桥接协议本身。
 */
export interface CaptureSource {
  /** 必须等于 Rust 侧对应 frontend crate 的 Parse::id(),是桥接协议里 frontend_id 字段的值。 */
  readonly frontendId: string;
  /** 便宜的同步判断:这个 tab 的 URL 是否属于本抓取源。 */
  matches(url: string): boolean;
  /** 做该应用私有的网络调用,拿到原始 payload;不是本源能处理的场景返回 null。 */
  capture(url: string): Promise<unknown | null>;
}
```

- [ ] **Step 2: 写 `extension/src/sources/claude-live.ts`(把 `background.ts` 里 claude.ai 专有的逻辑原样搬过来,只重新包成 `CaptureSource`)**

```typescript
import type { CaptureSource } from "./types.js";

interface Organization {
  uuid: string;
}

const CHAT_URL_PATTERN = /^https:\/\/claude\.ai\/chat\/([0-9a-f-]{36})/;

async function findSnapshot(conversationId: string): Promise<unknown | null> {
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
      return await res.json();
    }
  }
  return null;
}

/**
 * claude.ai 的抓取源:认领 /chat/<uuid> 页面,调认证态内部 API 拿整棵对话树。
 * frontendId 必须等于 fe-claude-live 这个 crate 的 Parse::id()
 * (crates/fe-claude-live/src/parse.rs)——两侧靠这个字符串配对,如同 Rust 内部
 * Acquire/Parse 靠 id() 配对一样(见 crates/ctxrelay-core/src/registry.rs)。
 */
export const claudeLiveSource: CaptureSource = {
  frontendId: "fe-claude-live",
  matches: (url) => CHAT_URL_PATTERN.test(url),
  capture: async (url) => {
    const match = url.match(CHAT_URL_PATTERN);
    if (!match) {
      return null;
    }
    return findSnapshot(match[1]);
  },
};
```

- [ ] **Step 3: 写 `extension/src/sources/registry.ts`**

```typescript
import type { CaptureSource } from "./types.js";
import { claudeLiveSource } from "./claude-live.js";

/**
 * 抓取源注册表——新增支持的应用时,在这里加一行就够,镜像 Rust 侧
 * Registry::with_defaults() 的"加一行"约定(crates/ctxrelay-core/src/registry.rs)。
 */
const SOURCES: CaptureSource[] = [claudeLiveSource];

export function resolveSource(url: string): CaptureSource | undefined {
  return SOURCES.find((source) => source.matches(url));
}
```

- [ ] **Step 4: 写 `extension/src/bridge.ts`(把 `background.ts` 里"打包 + POST + 设 badge"那部分原样搬过来,泛化字段名)**

```typescript
interface CaptureRequest {
  version: "1";
  token: string;
  frontend_id: string;
  captured_at: string;
  snapshot: unknown;
}

interface CaptureResponse {
  version: "1";
  status: "ok" | "error";
  message?: string;
}

/**
 * 把已经拿到手的 snapshot 打包成 bridge-protocol/schema.json 定义的
 * CaptureRequest,POST 给本地 ctxrelay listen,并把结果反映到工具栏 badge 上。
 * 这一步不关心 snapshot 内部长什么样(不同 frontendId 的 shape 完全不同),只负责
 * 传输和信号上报——这正是它能对所有抓取源通用的原因。
 */
export async function sendCapture(
  tabId: number,
  frontendId: string,
  token: string,
  port: number,
  snapshot: unknown
): Promise<void> {
  const captureRequest: CaptureRequest = {
    version: "1",
    token,
    frontend_id: frontendId,
    captured_at: new Date().toISOString(),
    snapshot,
  };

  try {
    const postRes = await fetch(`http://127.0.0.1:${port}/capture`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-CtxRelay-Token": token },
      body: JSON.stringify(captureRequest),
    });
    // 只看 postRes.ok 曾经是个 bug:ctxrelay listen 以前不管管线是否真的成功
    // 都回 HTTP 200,只在 body 里把 status 标成 "error"——所以这里必须解析
    // body、看 status 字段,而不是只信 HTTP 状态码。服务端现在已经改成失败回
    // 非 2xx 了,但两条信号都查一遍(postRes.ok 且 status === "ok")才算真正
    // 对齐 bridge-protocol 的契约,不会因为以后哪边单独改回去又变成静默失败。
    let succeeded = false;
    try {
      const responseBody = (await postRes.json()) as CaptureResponse;
      succeeded = postRes.ok && responseBody.status === "ok";
    } catch {
      // 响应体不是合法 JSON,理论上不该发生,但别让"解析响应体"本身变成又一个
      // 没人处理的异常——退化成只看 HTTP 状态码。
      succeeded = postRes.ok;
    }
    await chrome.action.setBadgeText({ text: succeeded ? "OK" : "ERR", tabId });
  } catch {
    // ctxrelay listen 大概率还没起,或者端口不对——用户需要先在终端跑 listen。
    await chrome.action.setBadgeText({ text: "N/L", tabId });
  }
}
```

- [ ] **Step 5: 重写 `extension/src/background.ts`,收缩成"按 URL 分发 + 配置检查",不再认识任何具体应用**

把整个文件替换成:

```typescript
import { resolveSource } from "./sources/registry.js";
import { sendCapture } from "./bridge.js";

async function captureAndSend(tabId: number, tabUrl: string): Promise<void> {
  const source = resolveSource(tabUrl);
  if (!source) {
    await chrome.action.setBadgeText({ text: "N/A", tabId });
    return;
  }

  const stored = await chrome.storage.local.get(["ctxrelayToken", "ctxrelayPort"]);
  const token = typeof stored.ctxrelayToken === "string" ? stored.ctxrelayToken : "";
  const port = typeof stored.ctxrelayPort === "number" ? stored.ctxrelayPort : 47651;

  if (!token) {
    await chrome.action.setBadgeText({ text: "CFG", tabId });
    return;
  }

  // 整个抓取阶段包进 try/catch——不只是最后一步 POST。`res.ok` 只覆盖"服务器
  // 返回了非 2xx",覆盖不了网络本身抛异常的情况(比如断网、DNS 失败),那种异常
  // 如果不接住会变成 service worker 里一个没人处理的 rejection,用户在图标上
  // 什么反馈都看不到,跟"什么都没做"没法区分。
  let snapshot: unknown | null;
  try {
    snapshot = await source.capture(tabUrl);
  } catch {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }
  if (snapshot === null) {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }

  await sendCapture(tabId, source.frontendId, token, port, snapshot);
}

chrome.action.onClicked.addListener((tab) => {
  if (tab.id === undefined || !tab.url) {
    return;
  }
  void captureAndSend(tab.id, tab.url);
});
```

- [ ] **Step 6: 改 `extension/manifest.json`,给 background service worker 声明 `"type": "module"`**

`background.ts` 现在 `import` 了别的文件,`tsc` 不会把它们打包成一个文件——`dist/background.js` 里会保留原样的 `import` 语句。Manifest V3 的 service worker 默认按 classic script 加载,遇到顶层 `import` 会直接报错("Cannot use import statement outside a module"),必须显式声明成 ES module 才能跑。把:

```json
  "background": {
    "service_worker": "dist/background.js"
  },
```

改成:

```json
  "background": {
    "service_worker": "dist/background.js",
    "type": "module"
  },
```

- [ ] **Step 7: 类型检查**

Run: `cd /Users/caoxinzhuo/code/ctxRelay/extension && npx tsc --noEmit`
Expected: 无错误输出。

- [ ] **Step 8: 编译产物**

Run: `cd /Users/caoxinzhuo/code/ctxRelay/extension && npm run build`
Expected: `dist/background.js`、`dist/bridge.js`、`dist/sources/types.js`、`dist/sources/claude-live.js`、`dist/sources/registry.js`、`dist/options.js` 全部生成;打开 `dist/background.js` 确认顶部有 `import { resolveSource } from "./sources/registry.js";` 这一行(不是被裁掉或改写)。

- [ ] **Step 9: 手工加载验证(没有测试运行时能替代这一步)**

1. Chrome 打开 `chrome://extensions`,开启开发者模式,"加载已解压的扩展程序",选 `extension/` 目录。
2. 点开该扩展的"service worker"检查器,确认控制台没有模块加载错误(验证 Step 6 的 `"type": "module"` 确实生效)。
3. 在终端跑 `ctxrelay listen --to claude-code --project <任意测试目录>`,把打印出的 token 粘贴进扩展的设置页。
4. 打开一个真实的 `https://claude.ai/chat/<uuid>` 对话页,点工具栏图标。
5. 确认行为和重构前完全一致:token 未配置时显示 `CFG`;非对话页显示 `N/A`;`ctxrelay listen` 没在跑时显示 `N/L`;正常时显示 `OK`,且终端能看到 `ctxrelay listen` 打出的 commit 结果。

- [ ] **Step 10: 提交**

```bash
git add extension/src extension/manifest.json
git commit -m "refactor(extension): split background.ts into per-site CaptureSource registry, mirroring the Rust Acquire/Parse split"
```

---

## Self-Review 摘要(编写计划时已核对,供执行者复核)

- **覆盖度**:Task 1 = 职责边界决策 + 文档(用户诉求 1);Task 2 = 接口约定的具体落地(用户诉求 2);Task 3 = 插件架构重构(用户诉求 3)。三者严格顺序依赖(Task 3 依赖 Task 2 的字段名),不适合拆成独立并行的计划。
- **未引入占位符**:所有代码块都是完整可运行的实现,没有"TODO: 处理其他站点"这类留白——`docs/architecture.md` §10.2 明确写了"新增站点需要做什么",但没有在代码里预先搭一个空的、没人用的抽象层。
- **命名一致性**:`frontend_id`(bridge-protocol / Rust `CaptureRequest.frontend_id` / TS 线上 JSON 字段)与 `frontendId`(TS 内部 `CaptureSource.frontendId`)是同一个概念在两侧语言各自惯例下的命名,`bridge.ts` 里显式做了 `frontend_id: frontendId` 的映射,不是不一致。
- **范围克制**:没有伪造一个 `fe-chatgpt-live`——那需要真实登录态实测 ChatGPT 的内部 API 才能写(参照 `fe-claude-live` 的先例),属于下一个独立的计划,不是本次"打好扩展点"的一部分。
