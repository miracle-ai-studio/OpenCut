# R2 项目范围插件验收 — 2026-09-21

## 本轮范围

新增 Owner 项目授权、受限主体列表／参数契约发现／固定修订规划四个工具。数据库 v10、50 个工具、9 类持久任务描述、OpenAPI 53 条路径。项目授权绑定插件 id/version/包 SHA 和贡献名，授权 revision 与工程 revision 分开；规划不自动提交。Web/桌面维护接入协议，暂不实现 UI。

[项目插件契约](../architecture/project-plugin-contract.md) 给出输入输出、权限、CAS／幂等、限额、错误、升级和客户端动线。旧 Owner 插件接口继续兼容，完整 R2/R6 尚未放行。

## 架构与代码逻辑

- api-contracts 提供纯 DTO；Application 定义 ProjectPluginStore／ProjectPluginRuntime 和权限 ticket、规划顺序、后置操作校验。SQLite 承担短事务、CAS／幂等／事件／审计；Native Services 仅装配 WASM Host 和进程内并发限制，没有生产 SQLite 依赖。
- v10 只创建项目策略表，升级不授权已有全局包。策略写入不修改 head。回执前检查 Owner；匹配回执先于 CAS；授权、回执、outbox、审计一次提交，审计故障必须全部回滚。
- 项目授权与 principal policy revision 在同一读事务获得。原生文件锁内、编译前、执行前和返回结果前复查；本轮重点避免“排队前有权限、拿锁后已撤权”的窗口。执行中撤权后即使原生返回成功，也不能交付旧 ticket 的输出。
- WASM 只获得不可变 snapshot 和参数；提议的素材绑定仍须经过项目资源权威检查，再由 Core 生成计划。没有可信插件直写入口。
- 发现只输出内联 schema、贡献版本、实际限额和策略，不输出 install_path。原生错误清理路径、内部参数值和 WASM 栈；稳定 error.code 保留。

## 真实公共入口

脚本 `scripts/verify-project-plugins.py` 使用真实编译的 WASM component，没有以 mock 代替执行。六组通过：

1. Owner 安装和精确授权；受限 Agent／reader 可发现契约，reader 和缺 edit 的 runner 无法执行；工程 head 未因授权／规划变化。授权精确重放及错误键／CAS 分类正确。
2. WASM 实际返回无效 JSON、无效操作、领域无效操作及死循环；对应错误或 fuel trap，未写入工程。参数上限、跨项目和跨项目 revision 被拒绝。
3. 全局权限撤销、准确包哈希不匹配独立生效；注入真实 SQLite 审计拒绝触发器，策略、幂等和 outbox 全部回滚。
4. Python fcntl 持插件版本排他锁，真实 CLI 子进程打开同一锁文件后（用 lsof 观测）撤销项目授权；解锁返回 NOT_AUTHORIZED，撤权事务未被原生等待占住。
5. HTTP 的 schema／plan 与 CLI 一致；Agent 显式 edit_apply 后创建的轨道可回读并重开。历史修订规划仍固定；旧 revision 新键提交返回 409。
6. 外部官方 MCP SDK 在 stdio 和 Streamable HTTP 真正发现并执行上述工具；reader 能 inspect、不能 plan。

Debug 二进制 SHA：`2335fc0ebdea4d7c76ea23e3261b9257e7b9f6071d1ac621e5cff3ca552d4a6b`。
Component SHA：`595093637aa4a09030e2d1d6c77073382614d1e7f54868f4af2d7edc2ff2608e`。
证据：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-project-plugins-yj5ozmvt/report.json`；同目录 trace.json／mcp.stdout／host.log。

第一次运行在 HTTP 阶段因 QA Python 函数名遮蔽 http 模块而停止，已修复脚本并从新工作区完整重跑；失败记录保留在 `opencut-project-plugins-pt77a8uh/failure.json`，没有将失败运行计为通过。

## 迁移、契约和回归

- 冻结 v1–v10 × legacy/v2 的 20 格矩阵、19 组检查通过。新增 v9 项目策略表名碰撞导致整次升级回滚，修复后干净重试；升级结果没有隐式授权且历史工程一致。报告：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-cv4rij51/report.json`。
- Application、Native Services、Plugin Host、SQLite 及实际崩溃恢复合计 108 个相关 Rust 测试通过（最后追加的运行中撤权后置检查单独运行；未重复累计先前 3 项）。插件发布和上传恢复的 ignored child 是实际 SIGKILL 父用例启动的入口。
- 四个针对性存储／应用检查覆盖原子审计失败、授权回执与 CAS、两个连接竞争同一 revision、principal 撤销再恢复／grant revision 变化、运行返回前撤权；不以这些替代上面的真实公共入口。
- TS SDK 7 项通过；Clippy 所涉及 crate 的 all-targets/-D warnings、fmt、纯依赖与存储／原生分层、生成漂移门禁通过。
- 原 22 工具及两个媒体发布工具的冻结兼容检查通过；总工具数 50。与已验 v9 开发包 dev-46d52b41f70b 的完整比较：46 个旧工具的输入／read_only 全部相同，45 个输出完全相同；仅 resource_read 增加可选 project_plugins 能力及定义，证据 `/tmp/opencut-project-plugins-prior-schema.json`。

日志位于 `/tmp/opencut-project-plugins-{tests,fencing,journey,history,wire,clippy,sdk,boundaries,drift}.log`。

## 交付状态

新交付 `dist/local/opencut-0.1.0-dev-7e6beabc18df-x86_64-apple-darwin.tar.gz` 已通过仓库外 **32 组验收／33 条记录命令全部 exit 0**，此前已验 dev-46d52b41f70b 归档保持不变。

- 包 SHA-256：`c617aff4a0638ba745835fe287a65890ff88440ee01672fd13f47ef71ef7e6cb`。
- release 二进制 SHA-256：`5f982a4b3316a34b7f11a6d52db59617b1cd5296c2aaf49ae276574a84e733e9`。
- 完整报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-5rcjchtf/report.json`；同目录 commands.json 保存全部命令、exit code 与输出。归档旁 `.acceptance.json` 保存验收摘要。
- 实际解包二进制重跑项目插件 6 组、项目导入 14 组、Worker SIGKILL／自然租约恢复 4 组，以及媒体／插件发布、作用域权限、SSE、MCP 生命周期／官方 SDK 和实际 RGB/PCM 导出检查。
- 仓库外 npm 安装的 TS SDK 设置准确项目授权、读取内联 schema 并生成真实 WASM 计划；仓库外源码安装的 Rust SDK 对同一固定修订获得相同计划，显式提交创建轨道。随后继续跨语言旧事务重放、新事务提交、CLI 独立回读、服务停止后重开，轨道仍保留。纯 SDK 依赖检查通过。
- 解包 release 的 20 格历史／19 组检查也通过：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-jh2i1lq6/report.json`。
- 包含项目插件独立契约、诊断脚本、生成的 Schema/OpenAPI、TS 包和 Rust SDK 源码包。平台范围为本机 macOS x86_64 开发交付，未声称其他平台、签名、公证或公共发行验收。

## 剩余边界

项目 planner 授权闭环不等于完整插件系统。WASM epoch 不覆盖锁等待、schema／component 编译，输出上限在返回后检查；没有声称 OS 沙箱、原生进程内存上限或整个请求硬超时。四槽是进程内上限，不是跨进程额度。其他插件类型、analysis ABI、持久任务取消、全局拒绝审计、CAS／产物／物理磁盘统一治理及 R3–R7 继续推进，完整产品未完成。
