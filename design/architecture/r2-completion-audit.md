# R2 放行核对与剩余项

2026-09-21 当前推进：Worker 输出/字幕准入、子进程与 attempt 存活锁已接通；schema 15，coverage 十二类。真实超限/字幕/解码和自然租约恢复已通过，30 格历史迁移及分层/SDK 检查通过。最终候选 dev-443c76cb7b64 已构建，正在仓库外独立验收，尚未取代正式验收包 dev-4398ef6e2a9e。见 [本轮 QA](../qa/r2-worker-output-admission-2026-09-21.md)。R2 未整体完成，R3–R7 保持剩余范围。

2026-09-21 源码推进：子进程预留继承与内核单文件上限已实现，真实 FFmpeg 在父进程 SIGKILL 后继续写入、4 MiB 碰顶及恢复验证通过；原有 14 组产品动线回归通过。此机制尚未接入 Worker 输出/字幕/attempt 清理，coverage 仍十类、schema 14；没有宣称 R2 完成或新增发行包。见 [本轮验证](../qa/r2-child-reservations-2026-09-21.md) 与 [接入契约](native-child-write-contract.md)。正式交付仍为下方 dev-4398ef6e2a9e。

2026-09-21。以 `acceptance-driven-iteration-plan.md` 的完整 R2 条目核对，不以最近一轮待办代替范围。此前状态中的“只剩矩阵与发行”描述不完整：CLI 事件流及 stdio MCP resources/prompts/订阅已补齐；Streamable HTTP 已通过独立网络及官方 SDK 验收，工程/任务来源链已补充原子持久化，principal/project 授权已接通 CLI/stdio MCP 与 HTTP/Streamable HTTP，真实网络隔离和流式撤权已验，管理历史查询现已实现，媒体及插件发布／修复记录与恢复已接通；受限项目预检现已接通；工程范围素材导入公开动线和双 SDK 已接通；终态暂存发现／显式释放与上传预留配额已接通；项目范围 planner 授权和只读参数契约发现已接通并通过真实动线；共享 Host 拒绝日志已通过真实四入口、降级／隐私／撤权验证；完整物理磁盘治理仍需推进。

| 要求 | 当前证据/实现 | 状态 |
|---|---|---|
| 纯 Domain、Application 用例与原子端口 | Core/Time、工程/Job/租约/查询/媒体/插件/预检端口，纯依赖与源码门禁 | 已有本地验证 |
| SQLite/CAS 适配隔离 | sqlite-store、native-services；原子故障/并发/真实恢复记录 | 已有本地验证 |
| 公共任务和内部执行隔离 | 增量 JobView/产物引用，SDK 纯 DTO；旧 JobRecord 按 v1 兼容保留 | 已有本地验证，旧接口尚未退役 |
| full/direct/range 共享合成 | `opencut-composition` 与 `opencut.composition.v1` | 已有真实像素/PCM/取消验证 |
| 同源工具/Job Schema、OpenAPI、TS/Rust SDK | 59 工具、9 类 Job 描述、OpenAPI 62 路径；事件／资源／上传控制查询共享入口，导入通过 seal 接受，双 SDK 支持原始分块和扩展结果 | 已有本地验证 |
| 历史数据库、请求、成功结果矩阵 | 冻结支持格式；v1–v13 × legacy/v2 摘要、8 类结果、旧排队任务和失败回滚 | 已通过重建格式验收；不是旧发行二进制时间矩阵 |
| 物理占用观察 | owner 六类存储扫描、稀疏／硬链接、有界部分结果、四入口真实验收 | [本地已验](../qa/r2-storage-observation-2026-09-21.md)；写入准入未完成 |
| 持久写入准入 | v14 协议门槛、独立原生预留/存活锁、版本化策略与可信回执；CAS/分块/拼接/私有输入，以及全局媒体、插件和匿名下载，共十类 | [真实动线、跨进程及流生命周期恢复已验](../qa/r2-storage-writers-2026-09-21.md)，最终包 43 组与双 SDK 已验；未覆盖全部写入和硬物理配额 |
| CAS 孤儿回收 | owner 计划／分页发现／检查／执行、引用重检、目录锁、删除步骤记录、实际 SIGKILL 恢复 | [11 组真实动线已验](../qa/r2-cas-gc-2026-09-21.md)，新交付包验收进行中；产物／历史回收未完成 |
| 项目范围上传与暂存治理 | 分块／封存／资源查询／绑定、上传分页发现、逻辑容量预留和终态显式释放；v9，14 组真实动线、SIGKILL、撤权与迁移 | 已有本地验证，见 [暂存 QA](../qa/r2-upload-staging-2026-09-21.md)；CAS／产物／物理磁盘统一治理未完成 |
| 项目插件授权 | 精确包 SHA／贡献授权、内联 schema、受限规划、锁内撤权、CAS／原子审计、显式编辑 | 已通过 [真实 WASM 四入口验收](../qa/r2-project-plugins-2026-09-21.md)，完整 R6 插件类别仍未完成 |
| 拒绝审计 | v11 有界历史；可信身份／匿名失败、请求去重、四入口、流式撤权、降级 warning 和过期游标 | 已通过 [真实动线及双 SDK／交付验收](../qa/r2-denial-audit-2026-09-21.md) |
| 可安装本地交付 | release 宿主包、npm 包、Rust SDK 源码包、manifest/校验/诊断/升级说明 | macOS x86_64 本地开发包已通过 [实际验收](../qa/r2-history-delivery-2026-09-21.md)；公共分发及其他平台未验 |
| CLI JSON/NDJSON、退出码、等待/任务取消 | JSON/NDJSON、逐条游标、断线续读、管道背压、Ctrl-C/超时与显式取消 | 已通过 [真实进程及跨入口验收](../qa/r2-event-observation-2026-09-21.md)；新能力尚未重打 release 包 |
| MCP tools/resources/prompts 同一权威 | stdio tools/resources/templates/prompts/订阅已通过 [真实验收](../qa/r2-mcp-context-2026-09-21.md) | stdio 与 Streamable HTTP 均通过 [真实进程及官方 SDK 验收](../qa/r2-streamable-http-2026-09-21.md)；新版 release 验收进行中 |
| actor 来源、资源授权与追踪 | 本机可信来源、request/transaction/job/attempt/artifact 原子追踪已通过 [实际验收](../qa/r2-actor-audit-2026-09-21.md)；CLI/stdio MCP 的 principal/project 隔离已通过 [受限访问验收](../qa/r2-scoped-access-2026-09-21.md)；网络请求身份、credential/policy revision 会话绑定、公共 SSE 和产物字节撤权通过 [网络验收](../qa/r2-network-access-2026-09-21.md) | **跨 R2/R3/R7 待完成**，不声称远程多用户权限已实现 |
| 契约生成与依赖 CI gate | Moon 配置及本地命令通过 | 已配置/本地已验；未执行远程 CI |
| Web/GPUI 会话与宿主实现 | 接入契约保留 | 按用户指示暂缓实现与实际 UI 验收 |

原本地开发包验收已通过；新版事件/资源/网络 MCP 包正在独立验收。下一步补受限 principal/project 授权及插件/媒体维护审计，再按依赖推进 R3 协作/actor/选择性撤销、R4 持续预览、R5 分析、R6 完整编辑和工程包、R7 跨平台及整体验收。公共 registry、签名/公证、安装器、其他 OS 和移动宿主不能由本机压缩包交付抵扣。
