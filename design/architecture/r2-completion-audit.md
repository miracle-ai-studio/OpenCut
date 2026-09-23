# R2 分层、契约与放行核对（2026-09-24）

**状态：部分完成。** 当前 Rust 宿主的业务数据库为 schema 15，公开注册表有 61 个叶工具和 9 个领域入口，生成 OpenAPI 有 76 条路径。CLI、HTTP、stdio MCP、Streamable HTTP 与双 SDK 使用同一 Automation/Application 规则；真实 macOS x86_64 debug 开发包已在仓库外通过[整包验收](../qa/r2-data-plane-openapi-2026-09-23.md)。这证明指定动线可用，不代表 R2 全部放行或 release／跨平台交付。

2026-09-24 最新收据：含物理 repair 预留和 Worker BUSY 修复的同一开发包在仓库外通过[34 项整包验收](../qa/r2-physical-repair-reserve-2026-09-24.md)。下方已验证能力按该包与各独立产品动线解释；完整 R2 仍缺以下边界。

## 权威分层

| 层 | 责任与边界 |
|---|---|
| Core 与纯契约 | 时间、工程操作、媒体与 Job DTO；不依赖 SQLite、FFmpeg、HTTP 或文件系统。 |
| Application | 工程、任务、权限、预算算式和审计用例；通过端口提交原子业务状态，公共结果不暴露 Worker 输入和原生路径。 |
| 原生适配器 | `sqlite-store` 是业务事务权威；`asset-store`、`job-runtime`、`plugin-host`、`storage-budget` 分别管理固定内容、执行、WASM 和原生写入预留。文件发布与 SQLite 提交通过显式 journal／恢复结果表达，不假装跨存储原子。 |
| 宿主与客户端 | `automation` 装配用例；CLI、HTTP 和 MCP 做传输／身份适配。TS/Rust SDK 消费生成契约，不持有另一套业务规则。 |

## 已验证的 R2 能力

- 工程创建、计划、原子编辑、幂等重放和固定修订读取；历史 v1–v15 × 两摘要的 30 格兼容矩阵。
- 项目上传原始字节、封存导入、项目资源查询、显式暂存释放；TS 和 Rust SDK 可在同一会话按确认偏移接续上传。公共 Job 视图、真实 Worker、产物下载和 SSE 由独立消费者验证。
- 工程范围身份和权限、受限预检、拒绝审计、发布维护日志；CLI、HTTP、两种 MCP 的授权与撤权由真实进程验收。
- CAS 固定、审阅式孤儿清理、真实 FFmpeg 输出与字幕的内核上限和 attempt 生存期；插件 planner 的真实 WASM 执行、安装／发布恢复和项目授权。
- 原生预算账本覆盖 12 类受控 payload 写入，保留锁跨进程和子进程生存；实际盘满、WAL 检查点、Worker 强杀、下载背压和目录别名拒绝均有独立动线。完整定义见[写入准入契约](storage-admission-contract.md)。

## 不满足 R2 完成条件的边界

1. `enforces_all_workspace_writes=false` 必须保持：管理型 SQLite 的主库、WAL、主 journal 与 SHM 已通过具名 VFS 做写前控制预留，本机 owner 也可绕开不可达数据库恢复预算策略。8 MiB 实际分配的 repair 预留已在真实 64 MiB 盘满卷上支持排队 Job 取消、终态上传释放和补足；但 SQLite 临时文件、预算／业务控制元数据的严格物理保证、外部直接写入、运行中 Worker／子进程、有活动连接时的 repair 切换及完整并发故障矩阵仍不在放行边界。此有界动线见[物理预留验收](../qa/r2-physical-repair-reserve-2026-09-24.md)，不能仅改能力位。
2. 产物、历史素材、插件版本和未来连续预览会话缺统一引用／租约与保留策略。删除前需要可审阅计划、活跃引用重检、下载与预览租约、失败恢复和跨机器工程包语义。
3. 当前包是本机 debug 包。release 构建、其他平台、升级／安装、真实旧用户数据库和长视频／VFR／中文访谈性能尚未形成完整证据；Web 与桌面还缺真实产品操作验收。

下一执行顺序是：补齐数据库临时文件与控制元数据准入、运行中任务与并发故障的 repair 恢复及预留容量定标，随后产物引用和保留，再做 release／跨平台及多宿主验收。R3–R7 的选择性协作、连续预览、素材理解、完整编辑和宿主交付按[总审计](../qa/r0-r7-current-audit-2026-09-23.md)另行推进。
