## ctxRelay

把 Web 端 LLM 对话(目前支持 claude.ai)导入本地 CLI agent 的原生会话存储,让你能直接用 `claude --resume` 接着聊,而不是把上下文复制粘贴一遍。完整设计动机和架构见 `docs/architecture.md`。

现在有两条获取对话的路径。推荐的一条是浏览器扩展直接抓取当前打开的、已登录的 claude.ai 对话页面,连 thinking 内容都能带过来,抓完自动回传给本地的 `ctxrelay listen`,整条链路不需要手动导出文件。另一条是老路径:从 claude.ai 的分享链接页面手动"另存为"一份 `chat_snapshots` JSON,再用 `ctxrelay ir`/`import` 喂给它——这条路径不需要装扩展,但拿不到 thinking,也拿不到没公开分享过的对话。两条路径最终都会汇聚到同一套 legalize → lower → commit 管线,产出同样的 Manifest,`undo`/`verify` 对两者一视同仁。

### 构建

```bash
# CLI
cargo build --release -p ctxrelay-cli
```

编译产物是 `target/release/ctxrelay`。下面的示例假设你已经把它加进 `PATH`,或者直接用 `cargo run -p ctxrelay-cli --` 代替 `ctxrelay`。

如果要用浏览器扩展这条路径,还需要编译一下扩展:

```bash
cd extension
pnpm install
pnpm run build
```

然后在 Chrome 里打开 `chrome://extensions`,开启右上角的"开发者模式",点"加载已解压的扩展程序",选中 `extension/` 目录。

### 用法一:浏览器扩展 + 本地桥(推荐,能带 thinking)

先在你要导入的项目目录里起一个一次性的本地桥,它会打印一个 token 并等着接收一次抓取,处理完就自动退出:

```bash
ctxrelay listen --to claude-code --project ./my-project
```

打开扩展的设置页(在 `chrome://extensions` 找到 ctxRelay,点"扩展程序选项"),把终端打印出来的 token 粘贴进去,端口默认 47651,跟 `listen` 保持一致就行,不需要改。然后打开你想导入的那个 claude.ai 对话页面(必须是已登录状态,普通登录态就够,不需要走分享链接),点一下浏览器工具栏上的 ctxRelay 图标。扩展会拿这个对话的真实内容(含 thinking)POST 给本地正在监听的 `ctxrelay listen`,`listen` 收到之后跑完整个导入管线、写 Manifest、退出。工具栏图标会用一个简短的 badge 反馈结果:`OK` 表示成功,`ERR` 表示抓取或写入失败,`N/A` 表示当前标签页不是一个 claude.ai 对话页,`CFG` 表示还没在设置页填 token,`N/L` 表示没连上本地的 `listen`(通常是还没在终端起这个命令,或者端口对不上)。

`listen` 也支持 `--dry-run` 之外的所有 `import` 参数(`--bootstrap`、`--manifest-out`,以及测试用的 `--claude-projects-root`),用法和下面 `import` 命令的对应参数完全一致。

### 用法二:文件导入(不需要装扩展,但没有 thinking)

从 claude.ai 的分享链接页面手动"另存为"拿到一份 `chat_snapshots` JSON,然后:

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
```

### `undo` 和 `verify`

```bash
# 撤销一次 import(只有文件内容和 commit 时完全一致才会真删,已经被你在
# Claude Code 里续聊过的会话会被跳过,不会误删)
ctxrelay undo ./my-project/.ctxrelay/manifests/<session-id>.manifest.json

# 冒烟测试:确认这次 commit 出来的会话真能被 resume(见下面"verify 是什么")
ctxrelay verify ./my-project/.ctxrelay/manifests/<session-id>.manifest.json
```

不管走的是扩展路径还是文件路径,导入记录(manifest,`undo`/`verify` 都要靠它)默认写在 `<project>/.ctxrelay/manifests/<session-id>.manifest.json`,也可以用 `--manifest-out <path>` 指定别的位置。如果 `<project>` 本身是个 git 仓库,`.ctxrelay/` 目录不会自动被忽略——如果不想让它出现在 `git status` 里,自己在项目的 `.gitignore` 加一行。

`verify` 只是个诚实的冒烟测试:确认 `claude --resume` 能正常加载这次 commit 出来的会话、给出一个回应,不会去核对回应内容对不对。它做不到这一点,也不该假装能做到——真实用户的对话没有"标准答案"可以拿来断言。如果你想验证的是"内容真的完整迁移过去了",最可靠的办法是自己 resume 之后问模型一个只有原对话里才会出现的细节,肉眼确认。

### 已知限制

`fe-claude-live`(扩展抓取的那个 frontend)只实现了 Parse,没有实现 Acquire——它的数据只能通过扩展主动 POST 过来触发导入,`ctxrelay ir`/`import` 这两个直接吃文件路径的命令用不了它,只能走 `ctxrelay listen`。反过来 `fe-claude-share`(文件路径那个 frontend)只支持从本地文件解析,不支持直接传一个分享链接自动抓取。另外,claude.ai 的分享快照本身(不是 ctxRelay 的问题)不包含附件文件,也不包含 MCP 工具调用的原始返回数据,只暴露最终的对话文本;如果你的对话大量依赖附件或 MCP,分享快照从定义上就丢了这部分内容,导入结果会如实反映这一点,不会假装完整。

推理链(thinking)目前不会被写成 Claude Code 真正的 thinking block——IR 现在没有字段能装下真实的 thinking signature 字节,而没有合法签名的 thinking block 会直接触发 Claude API 的报错。但内容不会丢:会被内联成普通文本(加 `[Thinking]` 前缀),你仍然能在会话里看到当时的思考过程,只是失去了"这是一段思考"的结构化身份。

浏览器扩展目前是面向作者自己用的 V1:工具栏图标的 badge 反馈只有几个简短代码,没有更友好的提示文案,也没有自动重置;`ctxrelay listen` 是一次性服务,处理完一次抓取就退出,如果要连续导入好几个对话,需要重新起一次 `listen`(重新起会换一个新 token,记得同步更新扩展设置页里的值)。

`ctxrelay import`/`listen` 只在目标 backend 是 `claude-code` 时能自动发现该写到 `~/.claude/projects/` 下的哪个目录;这套发现逻辑目前是为 Claude Code 的目录规则量身定做的,还没有支持第二个 backend(比如 Codex)。
