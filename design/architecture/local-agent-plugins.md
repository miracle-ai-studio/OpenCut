# 本机外部 Agent 插件

外部 Agent 负责对话和创作规划，OpenCut 提供同一套工程、编辑、预览、历史和导出用例。
当前插件生成器版本为 0.4.0；CLI、HTTP、MCP、SDK 和 Web 的操作含义来自 Rust 唯一注册表。
实际构建和限定验收以 `headless-completion-ledger.md`、`acceptance.json` 记录的候选身份为准。
历史插件 0.2.0/0.3.0 的证据保留原候选，不代表当前客户端安装或真实模型会话已验收。

## 连接方式与任务寿命

默认使用已启动的独立 OpenCut 服务。服务持有 Worker，Agent 只启动自己的 stdio MCP
连接；关闭 Agent 不会关闭独立服务。连接前检查服务 ready、同一工作区和相同二进制内容。
这是一台机器上的共享工作区连接，不是把 stdio 转发到远程 HTTP。具体权限仍在每次操作时校验。

```sh
/absolute/path/to/opencut --database /absolute/workspace.sqlite \
  server --directory /absolute/service start

python3 scripts/distribution/agent-plugin.py \
  --binary /absolute/path/to/opencut \
  --database /absolute/workspace.sqlite \
  --service-directory /absolute/service \
  --output /absolute/new-plugin-bundle
```

生成器需要 Python 3.11+。省略 `--service-directory` 时绑定 `~/.opencut/server`，
不推断其他工作区的服务目录。生成配置不会启动服务、打开数据库、读取凭据内容或修改客户端配置。
没有运行服务、连接错工作区或二进制不一致时明确失败，不悄悄创建另一个工作区。
输出目录必须不存在；空格/Unicode 通过 argv 传递；拒绝字面 `${` 路径，避免客户端展开改变目标。

可用 `--credential-file` 引用已有受限凭据；省略时是所选本机工作区的 OS-owner 权限。
如需由会话临时运行 Worker，显式使用 `--with-worker`，且不能同时提供受限凭据或
`--service-directory`。这种便携模式退出会话就停止其 Worker；持久任务仍在库中，后续由其他
Worker 继续处理。默认模式解决的是会话退出影响任务执行的问题，不是自动启动或恢复操作系统服务。

## 工具和人类编辑的发现

默认公布 11 个业务领域入口，工作区维护保留在显式管理入口。每个领域工具的 action 列表
覆盖该领域所有已注册动作；`inputSchema` 用按 action 匹配的分支给出精确 Rust 请求类型，
共用定义去重。单动作输出类型仍从 `opencut://schemas/domains/{domain}` 读取。
例如 `edit/timeline.plan`、`observe/event.changes` 和 `project/package.export` 不再隐藏。
owner-local 能力被发现不表示远程或受限调用获得该权限。

Agent 订阅 `opencut://projects/{project}/branches/main/summary` 可收到当前主线变化通知，
随后读取新固定修订和所需时间线。固定修订资源保持不变。
`observe/event.changes` 提供带游标的补读：先建立游标，再读取摘要并补读变化，可覆盖首次观察
期间的编辑；重连后从已处理游标继续。通知不是持久消息队列，也不会自动改写旧提案。
提交仍携带原 `expected_revision`，过期就重新读取和规划，不能覆盖人类的新编辑。

常用 CLI 工程、编辑、任务、事件、插件、上传命令经 `--endpoint` 连接服务；本地和远程复用
同一个命令到注册操作的映射。远程原始上传从本机读字节，不把文件路径当服务端素材路径。
`server`、存储恢复、访问管理及宿主启动仍是本机管理命令；二进制音频流 schema 工具仅本机可用。

```sh
/absolute/path/to/opencut --endpoint http://127.0.0.1:8788 \
  --credential-file /absolute/service/api-token --json project inspect PROJECT_ID
```

这次接线的边界理由：CLI 的 `connection` 模块只在宿主层选择现有 Application 或现有 Rust SDK，
不新增业务实现、crate 或依赖；不从内部模块调用 shell CLI。领域输入 schema 直接由共享声明生成，
不手写第二套 DTO。远程完整目录通过既有 `/api/operations`、`/api/domains` 专用接口读取，
SDK 使用共享 DTO 和既有响应大小限制，不放宽工程上下文资源的 2 MiB 上限。
MCP schema 比原动作子集更大，真实模型上下文开销及各客户端的 schema 支持仍需实测。

## 插件内容与加载

```text
bundle/
  README.md                       当前机器的命令和连接目标
  manifest.json                   文件与二进制 SHA-256
  configs/                        独立 MCP 客户端配置片段
  codex/.agents/plugins/marketplace.json
  codex/plugins/opencut/           .codex-plugin、.mcp.json、skills/edit/SKILL.md
  claude/.claude-plugin/marketplace.json
  claude/plugins/opencut/          .claude-plugin、.mcp.json、skills/edit/SKILL.md
```

两个客户端使用相同 stdio 配置和 Skill 源，分别保留自己的封装格式；不包含 hooks 或自动批准设置。
生成后的 README 给出选定目录的加载命令。客户端安装由用户选择，不由验证脚本修改全局配置。
宿主的媒体依赖需要在服务环境中可用；换机器或移动二进制、数据库后需重新生成连接。
本机路径配置不适用于云端 Agent。

## 限定验证

```sh
python3 -m unittest discover -s scripts/tests -p 'test_agent*.py' -v
python3 scripts/verify-agent-interfaces.py \
  --binary /absolute/qualified/opencut --artifacts-dir /absolute/new-interface-report
```

第二条需要 `jsonschema`，始终使用显式二进制，不构建产品。只建立一个专用空工作区和一个极小 WAV
探测夹具，验证真实 MCP 发现及独立 schema 校验、当前修订订阅、远程常用 CLI、幂等与过期提交、
事件游标、原始上传、断开后独立服务及 Worker、错误连接拒绝。输出候选 hash 与范围，结束时关闭测试服务。
它不代表真实 Codex/Claude 模型会话、全部权限/平台矩阵、媒体渲染或 UI 交付验收。

历史 `verify-agent-plugin.py` 明确使用 `--with-worker` 便携模式回放既有编辑及成功任务，
保留原验收工作区和候选约束；不能把其结果当成默认独立服务模式的证据。
历史客户端格式核对参见对应回执。外部连接插件也不同于以下工程 WASM 规划器。


## 工程 WASM 规划器

外部Agent的MCP连接包与工程WASM规划器是两种能力。唯一 `opencut_project_plugin_plan` 接收 `{plugin:{id,version,operation_name,project_id,revision,parameters},idempotency_key,label,branch_name?}`。所有调用方使用原exact-package工程grant，并具有Read/Edit/RunJobs；安装不自动授予规划权。MCP读取 `opencut://schemas/domains/plugin`，调用 `opencut_plugin` 的 `project.plugin.plan`；CLI `plugin plan` 与SDK `planProjectPlugin` 使用同一请求。

插件接收完整公开工程并返回普通公开编辑操作。原全局/基础/开发代际规划入口和旧编辑JSON退出；没有旧模型投影、协议探测或失败回退。实际安装包的固定ABI只决定私有函数绑定，必要元数据适配留在宿主内；依赖身份和历史证明不改名。返回旧开发操作的包明确失败，不能把旧操作混入当前事务。`transaction/preview` 是提案，显式 `edit.apply` 才提交；原授权、幂等、head、锁轨、资源和原子围栏保持。

默认示例与固定ABI夹具共用同一提案实现。默认 `planner` 函数使用完整公开契约；`pinned-editor-abi` feature只为原已发布的固定依赖函数生成夹具，不是业务版本或另一套实现。私有WIT的原符号保持精确，现有组件不在静态阶段重编。

当前静态整合尚未构建或运行。`scripts/verify-wasm-planning.py`复用已有专用工程、组件、授权和同候选服务，粗粒度验证提案/跨入口/显式提交/重放/恢复；不复制工程、安装包或生成中间候选。可选故障证明及最终连接配置只在实际对应阶段运行时成立。原 `agent_v4_wasm_isolation_20261017`等证据保留原候选和范围，不改标为当前树通过；完整发行及渲染插件矩阵仍开放。
