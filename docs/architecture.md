# ctxRelay — 架构设计文档

> 项目代号 `ctxRelay`。
> 一个把 Web 端 LLM 对话的**未经压缩**上下文迁移到本地 CLI agent(Claude Code / Codex / …)、并在本地接着聊的工具。
> 设计目标:前端(Web 源)、中端(IR)、后端(CLI 目标)严格解耦,支持 N 个源 × M 个目标而不产生 N×M 的耦合。
> 仓库形态:单仓库(monorepo),Rust workspace(ir/frontend/backend/core/cli)与 TS 浏览器扩展共存,见 §10。

---

## 1. 需求

### 功能需求
- 输入:某个 Web 端 LLM 对话的导出物(账号级 `conversations.json`,或单次对话的粘贴/导出)。
- 输出:目标本地 CLI 的**原生会话存储**,使得 `<cli> resume` 能直接加载并接着对话。
- 内容保真:对话内容与推理链**不做摘要压缩**;工具调用可以丢失"可回放性",但不得丢失其人类可读产物。
- 可逆:每次导入产生一份 manifest,支持完整撤销(删掉写入的文件 / SQLite 行)。

### 非功能需求
- 解耦:加一个新 Web 源只写一个 frontend;加一个新 CLI 只写一个 backend;两者互不影响,中端不动。
- 版本韧性:某个 CLI 静默改 schema(前例:Codex 0.128 把 session picker 从 JSONL 换成 SQLite)时,只需改**一个** backend。
- 可验证:失败模式是**静默的**(文件写进目标永不读取的目录 / rollout 未被索引),必须有自动 verifier。
- 纯函数管线:除末端写盘外全部无副作用,可缓存、可 diff、可 dry-run。

### 约束
- 单人项目,增量推进,首发只做最小实现(各一个 frontend/backend 跑通往返)。
- 语言:Rust 为主(workspace + Cargo 依赖图承载解耦约束);浏览器扩展因运行环境强制为 TS,独立工具链,与 Rust 侧共处同一仓库,靠 §10.1 的 bridge-protocol 契约(而非编译器)维持解耦。
- 非目标:**不解决上下文窗口超限**。IR 与 commit 保留全文;若超过目标单轮窗口,由目标 CLI 在你发出第一条后续消息时自行 compaction。"磁盘上完整"与"模型第一轮真的全部注意到"是两个不同保证,后者不属于本工具职责。

---

## 2. 顶层架构:narrow waist

```
  Web 源(N)              中端(稳定)                CLI 目标(M)
 ┌───────────┐                                      ┌───────────┐
 │ claude-web│─┐                                  ┌─│claude-code│
 ├───────────┤ │   parse      ┌─────┐   lower     │ ├───────────┤
 │chatgpt-web│─┼──(纯)──────▶│ IR  │──(纯)──────┼─│  codex    │
 ├───────────┤ │             └─────┘             │ ├───────────┤
 │ gemini... │─┘   (可序列化)  legalize            └─│  ...      │
 └───────────┘                (纯)                  └───────────┘
   FRONTEND                    CORE                    BACKEND
                                                          │
                                                     commit(唯一副作用)
                                                          ▼
                                                    目标原生存储 + Manifest
```

波动性是**不对称**的:Web 端导出是合规功能,变得慢;CLI 后端变得快且无预兆。narrow waist 把稳定的 IR 放中间,让每次 CLI 断裂只波及一个 backend 的 lowering pass。

**M×N → M+N 不是主要收益**(现在 2×2 省得很少);真正的收益是**把不对称的脆弱性收缩进一个有明确契约的可替换模块**。这条论证成立的前提是"N 和 M 都会长"——若不长,见 §11 的诚实审视。

---

## 3. IR:中立、纯数据、带版本

IR 是整个设计的心脏。它的边界划在**所有 Web 源与所有 CLI 目标都真实存在的最小语义内核**上,而不是所有源格式的超集(超集会让每加一个 frontend 都得动 IR,解耦失败)。

### 3.1 数据模型

```rust
struct Document {
    ir_version: SemVer,            // IR 自身的 ABI 版本
    source: SourceProvenance,      // 描述性:来自哪次导出
    turns: Vec<Turn>,
}

struct Turn {
    id: TurnId,                    // doc 内稳定
    role: Role,                    // User | Assistant | System
    origin: Origin,                // { vendor, model, surface } —— 仅描述,不驱动 IR 内部分支
    blocks: Vec<Block>,
    timestamp: Option<OffsetDateTime>,
}

enum Block {
    Text  { content: String },
    Code  { language: Option<String>, content: String },
    // 厂商专有工具(artifact / web_search / code_interpreter / grounding …)
    // 在 IR 里【不各自建模】,全部归一成一次"外部效应 + 人类可读产物"。
    ForeignAction {
        kind: String,             // 不透明标签,IR 不解释其语义
        summary: Option<String>,
        artifact: Option<Artifact>,
        caps: BlockCaps,
    },
    // 推理/思考:保留,但显式标注能力,由 backend 决定接受/丢弃
    Reasoning { content: String, caps: BlockCaps },
}

// 中立能力描述符 —— 解耦的关键:backend 只据此决策,永不问"来自哪个源"
struct BlockCaps {
    reasoning: bool,
    verifiable_signature: bool,   // 例如 Claude thinking 签名
    replayable: bool,             // ForeignAction 恒为 false
}
```

### 3.2 契约(写进 IR 文档,这就是 narrow waist 的定义)

> IR 只承诺 **content-effect** 的保真(对话内容、代码、推理链)。
> 对 **action-effect** 只承诺"标记其存在 + 携带产物",**绝不承诺可回放**。

伪造一个"读取了某文件"的 `tool_result` 是 replay hazard,会让 resume 后的模型误以为某个本地状态存在——因此 IR 层根本不提供可被误用成"回放"的 tool_use/tool_result 结构。

### 3.3 版本

- IR 自带 `ir_version`(SemVer)。frontend/backend 独立演进、独立发版,IR 就是它们的 ABI。
- 若第三个 frontend 需要一个 `Text/Code/ForeignAction/Reasoning` 都装不下的 block —— 那是**故意扩展 IR**(一次带版本的、慎重的变更)的信号,不是在某个 backend 里 special-case。

### 3.4 on-disk 规范形式

IR 有一个磁盘序列化形式(JSON 便于人读/diff,或 CBOR 求紧凑),带 schema 与版本号。`parse`/`emit` 是纯函数。因此 IR 文件本身可 checkin 进项目,成为"未压缩上下文"的可移植载体。

一个最小 IR 片段示例:

```json
{
  "ir_version": "0.1.0",
  "source": { "vendor": "anthropic", "surface": "claude.ai", "exported_at": "2026-07-05T..." },
  "turns": [
    {
      "id": "t1", "role": "User",
      "origin": { "vendor": "anthropic", "model": null, "surface": "claude.ai" },
      "blocks": [ { "type": "Text", "content": "我们把这个 IR 迁移工具设计一下" } ]
    },
    {
      "id": "t2", "role": "Assistant",
      "origin": { "vendor": "anthropic", "model": "opus-4.x", "surface": "claude.ai" },
      "blocks": [
        { "type": "Reasoning", "content": "...",
          "caps": { "reasoning": true, "verifiable_signature": true, "replayable": false } },
        { "type": "Text", "content": "核心是三层解耦..." },
        { "type": "ForeignAction", "kind": "artifact", "summary": "架构草图",
          "artifact": { "media": "text/markdown", "content": "# ..." },
          "caps": { "reasoning": false, "verifiable_signature": false, "replayable": false } }
      ]
    }
  ]
}
```

---

## 4. Frontend 契约

引入 URL 类 frontend(如分享链接)后,"frontend.parse 是纯函数"这条假设不再成立——网络请求会失败、超时、被 challenge、依赖服务器当下状态。与其破例,不如把 Frontend 拆成两段,与 Backend 的 `legalize/lower/commit` 对称:acquire 是唯一有副作用的一跳,parse 保持纯函数。

```rust
enum SourceRef {
    Url(String),          // 例如分享链接
    File(PathBuf),        // 例如账号导出的 conversations.json
}

trait Acquire {
    fn id(&self) -> &'static str;
    fn accepts(&self, input: &SourceRef) -> bool;              // 按 SourceRef 类型/URL 模式路由
    fn acquire(&self, input: SourceRef) -> Result<RawBytes>;   // 唯一的副作用:文件读取或网络 I/O
}

trait Parse {
    fn id(&self) -> &'static str;
    fn parse(&self, raw: RawBytes) -> Result<Document>;        // 纯函数,给字节吐 IR
}
```

职责:Acquire 只管"把 bytes 弄到手",不理解内容语义;Parse 把厂商专有结构 lower 进中立 IR,特别地——把 artifact / 搜索 / 代码解释器等**全部归一到 `ForeignAction`**,并如实填 `caps`。两者都**声明能力**,不关心任何 backend 的存在。

`File` 类 source(账号导出)的 Acquire 实现是平凡的(读文件),几乎不会失败,可以近似当纯函数处理测试。`Url` 类 source(分享链接)的 Acquire 才是真正需要隔离副作用的地方:

- **已知约束(V1 必须记录,不是留白)**:claude.ai 的分享快照本身就会丢内容——附件文件不含在快照内,MCP 工具调用的原始返回数据在快照里保持隐藏,只暴露最终对话产出。这是 Anthropic 服务端在你的 Acquire 拿到数据之前就已经做掉的裁剪,任何 frontend 设计都无法找回。若某类对话经常带文件/用 MCP,`fe-claude-share` 从定义上就不是该场景的忠实来源,需要 fallback 到账号导出 frontend(`fe-claude-export`,读 `conversations.json`)。两者不是互斥关系,是覆盖不同场景的两个 frontend,注册进同一个 core 即可。
- **目标态(单次操作,自动完成)**:同源策略决定 CLI 无法在用户毫无动作的情况下伸手进已认证的浏览器上下文——能突破这条边界的历史上只有两种机制:一次性授权的浏览器扩展(`host_permissions`),或者独立的、伪装成真实浏览器的自动化实例正面闯关反爬。两者中选**浏览器扩展 + 本地桥**,原因不是省一次安装,是它让 Acquire 彻底站在 Cloudflare 判定为"自己人"的一侧:
  - 扩展只需一次性授权 `host_permissions: ["https://claude.ai/*"]`,不需要可见标签页、不需要用户在浏览器里点任何东西。
  - 扩展的 **background service worker**(不是 content script!)用 `chrome.alarms` 周期轮询一个绑定 `127.0.0.1`、随机端口、短命的本地端点,拿到 `ctxrelay import <url>` 挂出的任务后,直接 `fetch(apiUrl, {credentials: 'include'})`——这次请求走的是真实浏览器进程的网络栈/TLS 指纹,不会触发 Cloudflare challenge,且因为该接口本就无需登录,不涉及读取或存储任何用户凭证。
  - **必须用 background script 发起 fetch,不能用 content script**:content script 运行在页面自身执行环境里,受页面 CSP 的 `connect-src` 约束,向 `http://127.0.0.1` 的出站请求可能被拦;background service worker 跑在扩展的特权上下文,不受页面 CSP 影响,绕开这个不确定性。
  - 端到端:`ctxrelay import <url>` 解析出对话 ID → 本地临时服务挂任务(建议带一次性 token 防本机其它进程冒充)→ 扩展轮询取到 → fetch → POST 回本地服务 → CLI 走 Parse→legalize→lower→commit → 返回。用户侧只有一条命令,浏览器需保持运行但无需任何手动点击。
  - 已评估并放弃的替代方案:无头浏览器自动化(patchright/camoufox 一类反检测方案)——技术上可行,但需要正面硬闯 Cloudflare managed challenge,是随其策略升级持续维护的军备竞赛,且比"扩展在已认证上下文里发一次干净请求"更明确地对抗站方已表达的意愿,不予采用。
  - **兜底态**:扩展未安装或浏览器未运行时,`SourceRef::File`(人工从浏览器另存为/DevTools Copy Response)始终可用,两条路径共享同一个 Parse 实现。
  - **排期**:核心管线先对着兜底态(人工另存为)打通、验证 Parse→legalize→lower→commit→Manifest→conformance test 全部正确,再把 Acquire 换成扩展方案——这个替换被 Acquire/Parse 的拆分完全隔离,不影响已验证的部分。
- **风险定性**:对"账号内自己的对话、手动触发、单条链接"这种低频、非批量的使用模式,和大规模自动爬取属于不同量级,但既然没有官方 API 承诺(anthropics/claude-code 仓库上有一条尚未关闭的 feature request 正是在要求这个能力),该 backend 应设计成用户手动触发的一次性调用,不做成常驻轮询服务——这不是能力不够,是刻意的风险定界。

---

## 5. Backend 契约

```rust
trait Backend {
    fn target(&self) -> TargetSpec;          // { tool, version_range } —— 版本是 backend 的一个维度
    fn required_caps(&self) -> CapPolicy;     // 声明需求:接受/拒绝哪些 caps

    fn legalize(&self, doc: &Document) -> (Document, LoweringReport);  // 纯:合法化 + 报告丢弃了什么
    fn lower(&self, doc: &Document) -> Result<LoweredSession>;         // 纯:IR → 目标原生序列(bytes/结构)
    fn commit(&self, lowered: LoweredSession, dest: &Dest, report: LoweringReport, ir_digest: String) -> Result<Manifest>; // 唯一副作用
}
```

> **实现落地时的偏离(已在 `ctxrelay-backend`/`be-claude-code` 落地,§12 步骤 3/4)**:`commit` 实际比这里多两个参数——`report`(`legalize` 产出的 `LoweringReport`)和 `ir_digest`(对**原始**、legalize 之前的 `Document` 求的内容摘要)。根因是 `lower(doc) -> LoweredSession` 只拿得到已合法化的 `Document`,天然不知道 legalize 阶段丢弃了什么,也没资格代表原始 IR 的身份——这两份信息只有调用方(持有原始 `Document` 的那一方)知道,必须显式传给 `commit` 才能填出一份诚实的 `Manifest`。

- **legalize** 是 LLVM legalization 的搬运:遇到本目标不合法的 IR 构造,负责丢弃/转译,而不是反过来要求 frontend 预先适配。典型动作:
  - `verifiable_signature: false` 的 `Reasoning` → 内联成 `Text`(否则强行写成 thinking block 会触发 `400 Invalid signature in thinking block`;但内容本身不销毁,跟 `ForeignAction` 一样降级保留);
  - `ForeignAction` → 内联成 `Text`/`Code`(内容一字不丢,只剥掉工具外壳);
  - 根据 `origin` 合成一段 preamble(personality migration):"以下为从 Web 对话导入的讨论,工具调用已内联为文本,从此处继续"。`origin` 是**描述性**读取,不驱动 IR 内部逻辑。
- **lower/commit 分离**:lower 纯、可缓存、可 diff;commit 是唯一写盘处。这带来 dry-run、可逆、可测。
- **目标版本是 backend 的维度**:"Claude Code backend" 不是一个东西,是 "CC v2.1.x backend" / "v2.2.x backend"。避免在一个 backend 里堆 `if cc_version < X`(那是 LLVM target 逻辑外溢的复现)。

### 后端落地要点(工程层面,避免踩坑)
- **Claude Code**:会话是 `~/.claude/projects/<slug>/` 下按 UUID 命名的 append-only JSONL。**不要逆向目录 slug 编码规则**(冒号/斜杠/空格→连字符、盘符大小写保留——这是逆向出来的、会变)。稳妥做法:commit 前在目标工作目录起一个一次性真 session,观察新建了哪个目录,往那儿写;用 `--session-id` 指定 UUID + `--resume <uuid>` 验证,尽量不碰"自动生成、勿手改"的 `sessions-index.json`。
- **Codex**:优先用 `codex -c experimental_resume="<rollout.jsonl>"` 从 JSONL 直接加载,少碰会变的 `state_*.sqlite` threads 表。(注意:Codex 自带官方 external-agent 导入器,但**本设计不 piggyback**——见 §11。)

---

## 6. 能力协商(解耦真正活着的地方)

不允许 frontend 和 backend 私下商量,否则就有 N×M 的隐性耦合。流程:

1. frontend 在每个 block 填 `BlockCaps`(**声明我产出了什么**)。
2. backend 用 `required_caps()` 声明 `CapPolicy`(**声明我接受什么**)。
3. core 对一个 `(Document, Backend)` 对调用 `backend.legalize(doc)`,产出合法 `Document` + `LoweringReport`。
4. `LoweringReport` 呈现给用户(透明:丢了哪些 reasoning、内联了哪些 artifact),这是"对理解的可逆性"。

backend 的判断永远是 `if !block.caps.verifiable_signature { drop }`,**从不**是 `if origin.vendor == "anthropic"`。这条规则是解耦是否成立的试金石。

---

## 7. 管线与编排

```
SourceRef(Url | File)
  └─ Acquire(路由 + 拉取) ──▶ RawBytes          ← 唯一副作用 #1(网络/文件 I/O)
       └─ Parse(纯) ──▶ Document(IR,可落盘)
                          └─ Backend.legalize(纯) ──▶ (LegalDoc, Report)
                               └─ Backend.lower(纯) ──▶ LoweredSession(纯数据)
                                    └─ Backend.commit(dest) ──▶ Manifest   ← 唯一副作用 #2
```

两端对称:入口的副作用集中在 Acquire,出口的副作用集中在 commit;中间从 RawBytes→Document→LegalDoc→LoweredSession 全程是可 diff、可缓存、可离线测试的纯数据变换。

core 是一个薄 driver + frontend/backend 注册表。CLI 形态(示意):

```
ctxrelay import <export-file> --to claude-code --project ./myproj [--dry-run]
ctxrelay import <export-file> --to codex       --project ./myproj
ctxrelay undo   <manifest-file>
ctxrelay verify <manifest-file>          # 冒烟测试:resume 一次真实会话,确认这条路没坏
ctxrelay ir     <export-file> -o thread.ir.json   # 只 parse 出 IR,不 commit
```

---

## 8. 效果边界与可逆性

commit 是对 CLI 状态存储的一次 effect。为它配一份 **Manifest**,记录写了哪些文件 / 哪些 SQLite 行 / 起了哪个 session-id:

```rust
struct Manifest {
    ir_digest: String,          // 溯源:由哪份 IR 生成
    target: TargetSpec,
    writes: Vec<WriteRecord>,   // 文件路径 + 哈希 / SQLite 表+主键
    created_session_ids: Vec<String>,
    report: LoweringReport,     // 丢弃/转译了什么
}
```

`ctxrelay undo <manifest>` 据此完整回滚。这呼应 reversibility 偏好:导入不是不可逆的黑箱。

---

## 9. 测试策略(两层契约 → 两层测试)

- **IR 层 property test(不碰任何真实 CLI)**:任意合法 IR 经 `lower → parse` 往返后 **content-effect 守恒**。可 fuzz,不起真进程,快。这是解耦额外送的礼物——中端可独立验证。
- **Backend conformance suite(LLVM lit 式,端到端)**:`emit → <cli> resume → 问"我们上一件讨论的是什么"`,断言答案命中。这是唯一能抓"静默失败"的手段。
- **round-trip / 暗号实验**:在 trunk 埋一个随机暗号词,commit → resume → 问模型该词。答对即证明上下文真的到位。fork 隔离测试(问 A 关于 B 的标签,应答"unknown")验证不串味。

**`ctxrelay verify` 和上面的 conformance 测试不是一回事**(实现落地时明确过这条界限,`ctxrelay-core` 的 `verify.rs` 有对应文档注释):conformance 测试自己造内容、自己埋暗号词,所以知道"正确答案"是什么,能断言答对与否;`ctxrelay verify` 面对的是真实用户对话,没有已知的"正确答案"可断言,所以它只是个诚实的冒烟测试——确认 `<cli> resume` 这条路没坏、能正常加载并给出回应,不对回应内容做任何断言。两者都必要,但职责不同:conformance 测试是开发期"这个 backend 没做错"的证据,`verify` 是用户日常"这次 commit 真的能用"的信心检查。

---

## 10. 单仓库布局(Cargo workspace + TS 扩展 —— 两种解耦执行机制并存)

```
ctxRelay/                       # 单仓库,前端插件与中后端全部在此
├── Cargo.toml                  # workspace 根:members = ["crates/*"]
├── crates/
│   ├── ctxrelay-ir/            # 纯:类型 + schema + serde + 版本 + property test。【不依赖任何其他 crate】
│   ├── ctxrelay-frontend/      # Acquire/Parse trait + 注册表            依赖: ir
│   │   ├── fe-claude-share/    #   V1 起步:File 兜底;目标态:配合 extension/ 自动 fetch  依赖: ir, frontend
│   │   ├── fe-claude-export/   #   账号导出 conversations.json(Acquire=读文件,近纯)      依赖: ir, frontend
│   │   └── fe-chatgpt-web/     #   ChatGPT 账号导出解析器                                依赖: ir, frontend
│   ├── ctxrelay-backend/       # Backend trait + 注册表                  依赖: ir
│   │   ├── be-claude-code/     #   → ~/.claude/projects JSONL            依赖: ir, backend
│   │   └── be-codex/           #   → ~/.codex 或 experimental_resume     依赖: ir, backend
│   ├── ctxrelay-core/          # pipeline driver + 能力协商 + manifest   依赖: ir, frontend, backend
│   ├── ctxrelay-cli/           # 薄 CLI:含本地临时 job 端点(127.0.0.1,随机端口+一次性 token) 依赖: core
│   └── ctxrelay-conformance/   # lit 式端到端测试harness(dev)
├── extension/                   # 浏览器扩展(TS),独立 package.json/tsconfig,不进 Cargo workspace
│   ├── src/background.ts        # background service worker:轮询本地 job、fetch(credentials:'include')、回传
│   ├── manifest.json
│   ├── package.json
│   └── tsconfig.json
├── bridge-protocol/              # CLI ↔ extension 唯一的契约来源(见 §10.1)
│   └── schema.json
└── docs/architecture.md          # 本文档
```

**依赖规则(Cargo 编译期强制,只覆盖 `crates/` 内部)**:
- `ctxrelay-ir` 出度为 0 —— 想往它塞 target 味道的字段,会污染一个谁都依赖的 crate,立刻显眼。
- frontend crates 与 backend crates **之间没有依赖边** —— 想在 backend 里引用某个 frontend 的类型,直接编译失败。
- 只有 `core` 同时见到两侧;`cli` 只见 `core`。

解耦从"约定"升级为"物理事实":耦合是编译错误,不是 code review 才抓的疏漏。**但这条纪律的强制力止步于 Cargo workspace 边界**——`extension/` 是另一个语言、另一个编译器,Cargo 的依赖图管不到它。单仓库不等于单一强制机制,`crates/` 与 `extension/` 之间的解耦需要一套不同的、显式设计的机制,见下。

### 10.1 bridge-protocol:跨语言那道边界靠什么维持纪律

`ctxrelay-cli` 和 `extension/`(插件)是两个不同运行时、不同语言的进程,唯一的接触点是 `ctxrelay listen` 起的那个 `127.0.0.1` 本地一次性端点。插件侧持有这份契约的代码具体落在 `extension/src/bridge.ts`(见 §10.2),不是 `background.ts`——后者只做按站点分发。这条边界如果只靠"两边约定好格式"心照不宣地维护,就是整个设计里唯一一处失去编译期保障、退化回口头约定的地方——这和 §6 那条"IR 是 frontend/backend 唯一沟通媒介"的原则在精神上是一回事,只是这次没有共享的类型系统能帮你兜底,所以必须显式补一层:

- `bridge-protocol/schema.json` 是这条契约**唯一的权威来源**,定义 `CaptureRequest`(插件 POST 给本地服务的请求体)和 `CaptureResponse`(处理结果)两个形状,带独立于 `ir_version` 的自己的版本号字段——它和 IR 是两条不同的 ABI,不要合并。
- **`CaptureRequest` 只携带三类信息,不掺入任何具体应用的语义**:`token`(配对凭证)、`frontend_id`(路由键,必须等于 Rust 侧某个已注册 `Parse::id()`,例如 `"fe-claude-live"`——`ctxrelay listen` 收到请求后据此在 `Registry` 里查出对应的 `Parse` 实现,不再写死)、`snapshot`(不透明 payload,具体形状完全由 `frontend_id` 对应的 `Parse` 决定,桥本身不解释、也不应该解释其内容)。早期版本的 `CaptureRequest` 还带过 `conversation_id`/`org_id` 两个 Claude 专有字段——这两个字段除了在 Rust 侧反序列化之外从未被下游任何逻辑读取过,是纯粹的死字段,泛化契约时已删除;插件如果需要人类可读的调试标识,应该放进它自己拥有的 `snapshot` 内容里,不属于桥协议本身。
- **两侧的类型都手写投影,严格照抄这份 schema。** V1 不引入 `typify`/`json-schema-to-typescript` 代码生成,靠 `crates/ctxrelay-cli/tests/bridge.rs` 里一条"反序列化插件会发出的样例 JSON"的测试,作为两边没有漂移的最小兜底验证——字段名/必需性任何一次不同步,这条测试会先炸。
- `frontend_id` 这个路由键选在"和 Rust 侧 `Parse::id()` 完全相同的字符串"上,不是巧合:这样插件侧新增一个抓取源时,只需要知道"我对应哪个已经注册好的 Rust frontend",不需要发明一套独立的应用标识体系。见 §10.2。

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

### 插件化(为 N/M 增长预留)
- v0:编译期注册表 + trait object,足够。
- 后续外部贡献者要加 frontend/backend 时,再引入动态加载(优先 **WASM 组件**而非 dylib:沙箱、跨平台、ABI 稳定),把注册表变成运行时发现。此举不改 IR 契约,只改 core 的加载方式。

---

## 11. 权衡与诚实的反面审视

| 决策 | 收益 | 代价 | 何时重估 |
|---|---|---|---|
| 中立 IR(拒绝 piggyback Codex 官方导入器) | 不被任一目标 schema 劫持;加第 3、4 个后端时 IR 不动 | 每个 backend 都要自己写 lowering;首发工作量更大 | 若最终只支持 CC+Codex 两个后端且都稳定,piggyback 反而更省 |
| lower/commit 分离、IR 落盘 | 可缓存/diff/dry-run/可逆;IR 可 checkin | schema 维护 + 版本负担 | 永远值得(与 state externalization 一致) |
| crate 图强制解耦 | 耦合变编译错误 | 5 个 crate 比 4 个直接转换器重 | 见下方"负收益"审视 |
| ForeignAction 归一(不建模具体工具) | frontend/backend 都不需认识"什么是 artifact" | 丢失可回放性 | 若某目标真需要回放某类工具,那是扩展 IR 的慎重信号 |
| Acquire 用扩展+本地桥而非无头浏览器自动化 | 站在 Cloudflare 判定"自己人"一侧,不触发 challenge;无需存储任何用户凭证;避免持续维护的反检测军备竞赛 | 需要一次性安装个人用扩展并保持浏览器运行;引入 CLI↔扩展的本地 IPC(轮询延迟、需要一次性 token 防本机冒充) | 若要支持无浏览器/无人值守环境(如 CI、服务器),扩展方案不适用,需重新评估无头浏览器方案 |
| V1 用 `chat_snapshots` 分享快照而非账号导出 | 零延迟(不用等邮件)、结构贴近前端实际渲染用的数据 | 无官方 API 承诺;分享快照本身丢附件与 MCP 工具原始数据,并非真正"未压缩" | 若场景常涉及文件/MCP,应转向 `fe-claude-export`(账号导出),或改用 `fe-claude-live` 读取所有者自己认证视角下的对话(可能不受此裁剪,需实测) |

**负收益审视(必须诚实对待)**:严格解耦在两侧都极小时是**负收益**——现在 2 web × 2 CLI,五个模块的解耦架构确实比四个直接转换器重。它的全部正当性建立在"N 和 M 都会长"这个前提上;只要前提成立,解耦就赢,且赢面随规模扩大。

因此推进策略不是"别解耦",而是:**把 IR 的 schema 和 content/action effect 契约当成真正的产品**——一开始就写规范、上版本号、配 property test;而 frontend/backend 的**数量**增量地加,先各做一个能跑通 round-trip 的最小实现。架构上严格解耦,交付上增量推进,二者不冲突。

---

## 12. 建议的构建顺序

0. **仓库脚手架**:建好 `ctxRelay/` 单仓库骨架——根 `Cargo.toml`(`members = ["crates/*"]`)、空的 `extension/`(`package.json` + `tsconfig.json`)、`bridge-protocol/schema.json` 占位。此时两边工具链互不干扰这件事就该验证一次:`cargo build` 不该因为 `extension/` 里有 `node_modules` 而受影响,反之 `npm install` 也不该碰到 `crates/`。
1. `ctxrelay-ir`:定死类型 + on-disk schema + 版本 + effect 契约文档 + round-trip property test。**这是地基,先钉死。**
2. `fe-claude-share`:V1 的第一个 frontend。Acquire = 人工从浏览器另存为 `chat_snapshots` JSON(裸 HTTP 客户端会撞上 Cloudflare managed challenge,已实测确认)+ 读文件;Parse 解析该 JSON 为 IR,`caps` 里如实标注是否含 thinking、是否只保留了被选中的分支(需要用一条真实分享链接实测确认这两点)。
3. `be-claude-code`:JSONL 格式直接,当第一个 backend;先把 `--session-id`/`--resume` 验证跑通。
4. `ctxrelay-core` + `ctxrelay-cli`:串起 `import` / `ir` / `undo` / `verify`。
5. `ctxrelay-conformance`:埋暗号词的端到端测试。**在此之前不要相信它真的 work**——失败是静默的。
6. **`bridge-protocol/schema.json` + `extension/`**:先把 job/response 的 schema 定型、跑通两侧的类型生成和 conformance 测试,再实现 `background.ts` 的轮询+fetch 逻辑,替换掉 `fe-claude-share` 的人工另存为路径。这一步验收标准除了 §9 的两层测试,还要加一条:关掉扩展,确认 `fe-claude-share` 照样能靠 `SourceRef::File` 兜底跑通——自动化路径失效时不应该让整个工具停摆。
7. 之后再增量补 `fe-claude-export`(账号导出,作为 `fe-claude-share` 丢文件/MCP 数据场景的 fallback)/ `fe-chatgpt-web` / `be-codex`,每个新模块的验收标准就是通过 §9 的两层测试。