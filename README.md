## ctxRelay

把 Web 端 LLM 对话(目前支持 claude.ai 的分享快照)导入本地 CLI agent 的原生会话存储,让你能直接用 `claude --resume` 接着聊,而不是把上下文复制粘贴一遍。完整设计动机和架构见 `docs/architecture.md`。

### 构建

```bash
cargo build --release -p ctxrelay-cli
```

编译产物是 `target/release/ctxrelay`。下面的示例假设你已经把它加进 `PATH`,或者直接用 `cargo run -p ctxrelay-cli --` 代替 `ctxrelay`。

### 用法

从 claude.ai 的分享链接页面手动"另存为"拿到一份 `chat_snapshots` JSON(V1 还不支持直接传分享链接自动抓取,见下面的已知限制),然后:

```bash
# 只把导出文件 parse 成中立 IR,看看解析出来是什么样,不碰任何 CLI 会话状态
ctxrelay ir conversation.json --output thread.ir.json

# 完整导入:解析 → 合法化 → lower → 写进 Claude Code 的会话存储
ctxrelay import conversation.json --to claude-code --project ./my-project

# 先看看会写成什么样、会丢什么,不真的落盘
ctxrelay import conversation.json --to claude-code --project ./my-project --dry-run

# ./my-project 从来没在 Claude Code 里打开过?加这个让 ctxrelay 花一点真实 API
# 额度帮你起一次一次性会话、找到该写到哪个目录——不加的话会直接报错提示你手动
# 先跑一次 claude,或者显式加上这个参数
ctxrelay import conversation.json --to claude-code --project ./my-project --bootstrap

# 撤销一次 import(只有文件内容和 commit 时完全一致才会真删,已经被你在
# Claude Code 里续聊过的会话会被跳过,不会误删)
ctxrelay undo ./my-project/.ctxrelay/manifests/<session-id>.manifest.json

# 冒烟测试:确认这次 commit 出来的会话真能被 resume(见下面"verify 是什么")
ctxrelay verify ./my-project/.ctxrelay/manifests/<session-id>.manifest.json
```

`import` 默认把导入记录(manifest,`undo`/`verify` 都要靠它)写在 `<project>/.ctxrelay/manifests/<session-id>.manifest.json`,也可以用 `--manifest-out <path>` 指定别的位置。如果 `<project>` 本身是个 git 仓库,`.ctxrelay/` 目录不会自动被忽略——如果不想让它出现在 `git status` 里,自己在项目的 `.gitignore` 加一行。

### `verify` 是什么、不是什么

`verify` 只是个诚实的冒烟测试:确认 `claude --resume` 能正常加载这次 commit 出来的会话、给出一个回应,不会去核对回应内容对不对。它做不到这一点,也不该假装能做到——真实用户的对话没有"标准答案"可以拿来断言。如果你想验证的是"内容真的完整迁移过去了",最可靠的办法是自己 resume 之后问模型一个只有原对话里才会出现的细节,肉眼确认。

### 已知限制

`fe-claude-share`(目前唯一的 frontend)只支持从本地文件解析(`ctxrelay ir`/`import` 后面跟的是文件路径),还不支持直接传一个 claude.ai 分享链接自动抓取——这需要一个浏览器扩展配合本地桥接,尚未实现。另外,claude.ai 的分享快照本身(不是 ctxRelay 的问题)不包含附件文件,也不包含 MCP 工具调用的原始返回数据,只暴露最终的对话文本;如果你的对话大量依赖附件或 MCP,分享快照从定义上就丢了这部分内容,导入结果会如实反映这一点,不会假装完整。

推理链(thinking)目前不会被写成 Claude Code 真正的 thinking block——IR 现在没有字段能装下真实的 thinking signature 字节,而没有合法签名的 thinking block 会直接触发 Claude API 的报错。但内容不会丢:会被内联成普通文本(加 `[Thinking]` 前缀),你仍然能在会话里看到当时的思考过程,只是失去了"这是一段思考"的结构化身份。

`ctxrelay import` 只在目标 backend 是 `claude-code` 时能自动发现该写到 `~/.claude/projects/` 下的哪个目录;这套发现逻辑目前是为 Claude Code 的目录规则量身定做的,还没有支持第二个 backend(比如 Codex)。
