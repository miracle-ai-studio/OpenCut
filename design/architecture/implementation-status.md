# 实现与验收状态

2026-09-24 R2 物理修复预留增量：管理型工作区的原生账本 v3 和 8 MiB 实际分配／同步预留已接通；本机 owner CLI 在停掉管理型连接后可进入有限 repair 状态，经专用 VFS 取消排队 Job、释放终态上传暂存，再补足预留。64 MiB 真盘满卷、原键回放、SQLite 完整性与受影响测试已通过；Worker 的瞬时 SQLite BUSY 不再直接取消任务或终止内嵌 HTTP 宿主。最终 macOS x86_64 debug 开发包在仓库外通过 34 项验收，详见[物理修复预留 QA](../qa/r2-physical-repair-reserve-2026-09-24.md)。全工作区写入准入仍为 false，运行中任务、SQLite 临时文件、完整资源保留、跨平台发行与 R3–R7 仍未完成。下方各段按日期保留历史增量，不代表最新完整状态。

2026-09-23 恢复后的当前源码增量：纯 Rust 的 `contracts/time/editor-core/composition` 已构成精确时间、工程校验／编辑计划和固定修订合成语义内核；`editor-application` 通过端口编排原子提交，SQLite／CAS／FFmpeg／Worker 位于适配层。五个相关 crate 的 37 项测试与依赖边界门禁通过，具体责任及不足见 [Rust 内核核对](../qa/r0-r7-current-audit-2026-09-23.md#rust-内核与外层适配)。默认 MCP 入口按 `system/project/edit/media/task/observe/plugin/access/workspace` 九个领域发现，61 个原操作作为可调用兼容层；CLI、HTTP、MCP 共用同一动作解析、叶操作鉴权与审计。生成 OpenAPI 已覆盖 75 条控制／数据面路径。独立安装的 TS SDK 13 项、Rust SDK 12 项及官方 MCP SDK 5 项真实互通通过，v1–v15 × 两种摘要的 30 格历史兼容通过；详见 [最新数据面验收](../qa/r2-data-plane-openapi-2026-09-23.md)与 [恢复状态](../../RECOVERY_STATUS.md)。当前 debug 二进制已打成 macOS x86_64 本地开发包并在仓库外通过 21 项验收，相同二进制 Worker 输出准入另过 4 项；当前恢复源码由本地检查点封存，该包不是 release／跨平台发行或 R0–R7 整体放行。R2 的全工作区写入准入／保留治理及 R3–R7 目标仍未完成。下方 2026-09-21 各段是历史迭代记录。

2026-09-21 当前推进：Worker 输出/字幕准入、子进程与 attempt 存活锁已接通；schema 15，coverage 十二类。真实超限/字幕/解码和自然租约恢复已通过，30 格历史迁移及分层/SDK 检查通过。最终候选 dev-443c76cb7b64 已构建，正在仓库外独立验收，尚未取代正式验收包 dev-4398ef6e2a9e。见 [本轮 QA](../qa/r2-worker-output-admission-2026-09-21.md)。R2 未整体完成，R3–R7 保持剩余范围。

2026-09-21 源码推进：子进程预留继承与内核单文件上限已实现，真实 FFmpeg 在父进程 SIGKILL 后继续写入、4 MiB 碰顶及恢复验证通过；原有 14 组产品动线回归通过。此机制尚未接入 Worker 输出/字幕/attempt 清理，coverage 仍十类、schema 14；没有宣称 R2 完成或新增发行包。见 [本轮验证](../qa/r2-child-reservations-2026-09-21.md) 与 [接入契约](native-child-write-contract.md)。正式交付仍为下方 dev-4398ef6e2a9e。

2026-09-21 持久写入准入增量：SQLite 仍为 v12，59 工具／9 类任务／OpenAPI 62 路径。独立原生账本、预留存活锁、owner 策略版本/幂等回执已接通 CAS 捕获、上传分块/拼接和任务私有输入；七组真实流程、旧宿主拒绝、26 格历史及跨进程争用/SIGKILL 恢复已验，见 [准入 QA](../qa/r2-storage-admission-2026-09-21.md)。新包验收进行中，上一份已验交付仍为 dev-0bf1481f531f。enforces_all_workspace_writes=false；渲染输出、媒体/插件、匿名下载副本和数据库增长仍待接入，完整 R2 及 R3–R7 保持未完成。以下保留阶段历史。

2026-09-21 CAS 回收增量：数据库 v12，56 工具／9 类任务／OpenAPI 59 路径。显式计划、未完成计划发现、引用重检、持锁删除与持久步骤恢复已实现；11 组真实动线、双 SIGKILL 窗口、24 格历史迁移及契约／分层门禁已过，见 [回收 QA](../qa/r2-cas-gc-2026-09-21.md)。新包仓库外验收进行中，上一份已验交付仍为 dev-3b89cbfbff85。统一物理配额、写入准入、产物保留及 R3–R7 仍未完成。以下保留阶段历史。

2026-09-21 存储盘点增量：数据库仍为 v11，52 工具／9 类任务／OpenAPI 55 路径。新增 owner-only 六类原生占用观察，包含 inode 去重、稀疏文件、合作式预算和显式部分结果；CLI／HTTP／官方 MCP 两种传输六组真实验收已过，见 [存储盘点 QA](../qa/r2-storage-observation-2026-09-21.md)。新包验收待完成，上一已验交付保留 dev-7d1c8c705b60。完整物理配额、引用保护 GC 和 R3–R7 仍未完成。以下保留阶段历史。

2026-09-21 拒绝审计增量：数据库 v11，51 工具／9 类任务／OpenAPI 54 路径。CLI／HTTP／MCP 的工具、认证／Origin、资源及流式撤权拒绝进入有界日志，按服务端 request_id 去重；Owner 可分页和关联身份查询。七组真实动线、22 格历史、100 项相关 Rust 和分层／旧契约门禁已过，见 [拒绝审计 QA](../qa/r2-denial-audit-2026-09-21.md)。新版 dev-7d1c8c705b60 已通过仓库外 35 组验收，36 条记录命令全成功；独立 TS/Rust SDK 拒绝关联、真实 HTTP 头部降级传递、22 格历史及实际媒体回归已验。旧归档保留。完整磁盘治理和 R3–R7 继续推进。以下保留阶段历史。

2026-09-21 项目插件增量：数据库 v10，50 工具／9 类任务／OpenAPI 53 路径。Owner 可授权准确包哈希与贡献；受限 Agent 可发现内联 schema、固定修订规划并显式提交，文件锁内及返回前重验项目授权／principal policy revision。真实 WASM 四入口六组、20 格历史迁移和相关分层／兼容门禁已过，见 [项目插件 QA](../qa/r2-project-plugins-2026-09-21.md)。新版 dev-7e6beabc18df 已通过仓库外 32 组验收，33 条记录命令全部成功；独立 TS/Rust SDK 实际规划、显式提交及 CLI 重开回读通过。旧归档保持不变。拒绝审计、完整磁盘治理及 R3–R7 剩余范围继续推进。以下保留阶段历史。

2026-09-21 暂存治理增量：数据库 v9，46 工具／9 类任务／OpenAPI 49 路径。项目上传可分页发现、查询保留与配额、显式释放终态源分块；创建时预留，实际清理并同步后释放，文件锁内和最终事务重新鉴权。14 组真实导入／清理／分页／撤权与 18 格迁移通过，相关 Rust 118 项、TS 7 项通过，见 [暂存 QA](../qa/r2-upload-staging-2026-09-21.md)。新开发包 dev-46d52b41f70b 已通过仓库外 30 组验收，32 条记录命令全成功；包含独立双 SDK 的发现／清理／预留查询、18 格历史及真实媒体回归，旧归档保持不变。该配额只覆盖上传源字节预留，完整物理磁盘／CAS GC、项目插件授权和拒绝审计及 R3–R7 继续推进。

2026-09-21 公开导入增量：42 工具／9 类任务、HTTP 有界原始分块、CLI 续传、项目素材分页及 TS/Rust SDK 助手已接通。四入口真实导入／绑定／RGB、传输中撤权与并发上限、abort 清理 SIGKILL 和导入 Worker 30 秒自然租约恢复已验；117 项相关 Rust、TS 7 项、旧契约／生成／分层门禁通过。最终开发包 dev-c924e6d7272b 已通过仓库外 29 组验收，32 条记录命令全成功；包含实际安装的双 SDK 跨语言续传、17 组／16 格历史兼容与真实媒体回归。它取代 dev-0046d31b1081 成为最近已验开发交付，旧归档保持不变。详见 [公开导入 QA](../qa/r2-public-import-2026-09-21.md)。终态暂存回收、总磁盘配额及剩余 R2–R7 继续推进。

2026-09-21 开发中增量：原生导入核心已接通封存→Worker→CAS→任务成功／项目素材原子登记→受限绑定；真实图片删除原源后预览并独立解码通过，视频／音频导入通过。修正导入取消发布语义、限制上传探测的外部引用，新增可扩展 Job 结果读取。相关 Rust 122 项、TS 5 项、16 格历史与旧契约门禁通过，见 [导入核心 QA](../qa/r2-import-core-2026-09-21.md)。公开上传入口、双 SDK 助手、清理／磁盘配额和新包验收未完成；R2 继续进行，最近交付仍为 dev-0046d31b1081。

2026-09-21 开发中增量：项目上传的纯规则、SQLite v8 和原生持锁分块存储已实现；86 项相关 Rust 检查、真实双 SIGKILL／并发／撤权竞态，以及 v1–v8 共 16 格历史兼容通过。新公开上传接口、封存／导入任务和项目素材绑定尚未接通，因此 R2 项目导入仍未完成，未打新交付包；最近已验收交付仍为 dev-0046d31b1081。详见 [上传基础 QA](../qa/r2-upload-foundation-2026-09-21.md)。

2026-09-21 当前增量：受限预检已按项目 read + run_jobs 接通四入口；授权与固定修订读取同一只读快照，原生调用前检查素材锁定，诊断保留问题代码／实体并移除宿主路径。真实预检→Worker→下载→RGB 解码 8 组及相关 Rust 78 项通过，见 [预检验收](../qa/r2-scoped-preflight-2026-09-21.md)。新版开发包 dev-0046d31b1081 已通过仓库外 26 组验收，29 条命令全部成功，独立双 SDK、历史 14 格及真实媒体回归通过；项目范围导入／插件授权和 R3–R7 继续推进。

2026-09-21 当前增量：插件包／授权文件已接入泛型发布 Journal，媒体和插件的 wire 类型、序号及查询保持独立。数据库 v7、36 工具；真实插件 8 组、四个 SIGKILL 窗口、媒体 8 组和 14 格历史矩阵通过。修复暂存路径别名误判、包指纹漏算与不完整安装状态。新版开发包 dev-a4808c3781b2 已通过仓库外 24 组验收，26 条命令全部成功，TS/Rust 插件来源查询、受限调用隔离、历史升级及真实媒体回归通过；详见 [插件发布 QA](../qa/r2-plugin-publication-2026-09-21.md)。项目范围资源能力与 R3–R7 剩余目标继续推进。下方为阶段历史。

2026-09-21 当前增量：原生发布 Journal 协议已接入媒体上传／元数据修复，包含明确的未知结果与持锁核对；修复重复上传未验证已存在内容的问题。数据库 v6、34 工具；真实流程 8 组、发布前／后 SIGKILL 及相关 Rust 83 项通过。新版开发包 dev-9a3f707b4702 经仓库外 22 组验收通过，25 条记录命令全部成功；TS/Rust 真实发布记录查询、12 格历史迁移和媒体回归通过，见 [本轮证据](../qa/r2-media-publication-2026-09-21.md)。插件安装接入、工程范围资源能力与 R3–R7 剩余需求继续推进。下方为阶段历史。

2026-09-21 当前增量：权限管理历史查询已通过纯 Application 读端口接通 CLI/HTTP/两种 MCP；owner-only，32 个工具、数据库仍为 v5。真实查询／跨入口隔离／审计失败回滚 5 组及相关 Rust 73 项通过。新版开发包 dev-f17740658938 经仓库外 20 组验收通过，24 条命令全部成功；TS/Rust 管理历史查询、受限编辑及真实媒体回归通过，见 [本轮报告](../qa/r2-access-audit-2026-09-21.md)。R2 剩余插件／媒体维护审计及工程范围资源能力，完整 R3–R7 范围保留。下方为历史阶段记录。

2026-09-21 当前增量：受限 Caller 已贯穿 REST 与 Streamable HTTP；MCP session 绑定 credential/policy revision，公共事件 SSE 和产物下载动态复查权限。真实网络 7 组及 72 项相关 Rust、Clippy、TS 5 项、旧 22 工具兼容与契约门禁通过。新版开发包 dev-931ad331d2c8 经仓库外 18 组验收通过，23 条记录命令全部成功；受限 TS/Rust 跨语言编辑与真实媒体动线通过。最终证据见 [本轮网络 QA](../qa/r2-network-access-2026-09-21.md)。R2 尚缺管理审计查询、插件／媒体维护审计和范围化资源能力；R3–R7 完整目标保留。以下历史记录不代表最新网络边界。

2026-09-21 当前增量：受限主体／项目权限已接通 SQLite、Automation、CLI 和 stdio MCP；数据库 v5，31 个工具，旧 22 工具 Schema 保留。真实权限、撤权和 FFmpeg 产物动线通过；开发包 dev-d9d15708a216 经仓库外 16 组验收通过，详见 [本轮验收](../qa/r2-scoped-access-2026-09-21.md)。REST 和 Streamable HTTP 仍只接受 owner bearer，R2 继续未完成。后续段落是各阶段历史证据。

2026-09-21 当前增量：可信 Host 来源与 request ID 已贯穿 CLI/HTTP/两种 MCP，并在工程/任务业务事务内原子持久化。新增第 30 个工具 `opencut_audit_query`；数据库 v4，旧 22 工具 Schema 不变。真实来源动线、八格历史矩阵、80 个并发打开进程和回滚/attempt 检查通过；新版 release 包正在独立验收，见 [本轮报告](../qa/r2-actor-audit-2026-09-21.md)。目前仍是本机共享主体，principal/project 访问隔离尚未完成；完整 R0–R7 目标不变。

2026-09-21 当前增量：Streamable HTTP `/mcp` 已实现，同一 EditorMcp 共享工具、资源、提示与订阅。新增会话隔离、有界回放、显式过期、DELETE/空闲/停机规则；真实进程协议验收通过。正在完成独立官方 SDK 互通及新版 release 包验收，交付状态以本轮 QA 报告为准。R2 尚缺 actor/principal/project 授权与追踪，R3–R7 剩余范围未缩减。见 [网络契约](mcp-http-contract.md)。下方保留各轮历史记录。

2026-09-21 最新增量：stdio MCP 资源发现/模板/读取、三个提示模板、连接隔离与有界订阅已完成。新增共享 `opencut_resource_read`，当前 29 工具；真实慢管道 EOF 关闭问题已复现并修复。见 [上下文契约](mcp-context-contract.md) 与 [验收报告](../qa/r2-mcp-context-2026-09-21.md)。R2 仍缺 Streamable HTTP 与 actor 授权链；新能力尚未重新打 release 包，Web/GPUI 暂缓。


2026-09-21 最新增量：CLI NDJSON/watch、逐事件断点、统一 `opencut_event_changes`、中断/超时及显式取消已完成真实进程验收；HTTP/MCP/CLI 查询一致，当前注册表 28 工具，旧 22 工具兼容不变。见 [事件观察契约](event-observation-contract.md) 和 [验收记录](../qa/r2-event-observation-2026-09-21.md)。R2 仍缺 MCP resources/prompts/订阅/Streamable HTTP 与 actor 授权链；现有 release 包是上一轮版本，本轮新能力尚未重新打包。Web/GPUI 继续暂缓。以下为按阶段保留的历史记录。


2026-09-21 最新进展：冻结历史格式矩阵通过 v1/v2/v3 × legacy/v2 六格及八类结果读取；本地 macOS x86_64 release 开发包已交付并通过仓库外实际解包、TS/Rust SDK、跨语言重放、历史矩阵和真实媒体动线，MCP 18 项/真实插件 14 项也通过。见 [本轮交付报告](../qa/r2-history-delivery-2026-09-21.md) 与 [安装/升级说明](local-distribution.md)。**完整核对修正：R2 还缺 CLI NDJSON、MCP resources/prompts；actor/资源授权跨 R2/R3/R7 仍待完成。** 详见 [放行核对](r2-completion-audit.md)，不再沿用此前“仅剩矩阵与发行”的简化描述。当前包为开发交付，R0–R7 整体未完成。

2026-09-21 最新 R2 进展：共享合成描述已落地。纯 `opencut-composition` 编译固定修订，`media-contracts` 持有泛化输入原语；full/direct/range 使用同一编译与时间选择规则，文件和编码选项留在原生适配层。相关 Rust 56 项、真实编码帧 16 项、直接帧 11 项、区间音视频 10 项、带中英字幕导出 15 项，以及 Clippy/契约/依赖门禁通过。见 [合成契约](composition-contract.md) 与 [验收报告](../qa/r2-composition-2026-09-21.md)。**R2 仍待完整历史兼容矩阵与发行；R3–R7 的其他需求尚未全部完成。** Web/GPUI 实现与实际 UI 暂缓。下方为此前阶段记录。

2026-09-21 最新 R2 进展：预检、媒体维护及插件计划已归位到纯 Application 用例，通过 Native Services 装配原生能力。修复只读能力创建目录、插件语义错误归因及非 UTF-8 安装路径发布边界。最终构建的只读 CLI 7 项、真实 WASM 14 项、媒体动线 12 项、HTTP/MCP 各 18 项，以及受影响 Rust 27 项、Clippy、契约/依赖门禁通过。见 [能力端口契约](native-capability-ports.md) 与 [验收报告](../qa/r2-native-capabilities-2026-09-21.md)。**R2 仍未整体完成：后端无关合成描述、完整历史兼容矩阵与发行待推进。R3–R7 的剩余范围继续有效；Web/GPUI 实现与真实 UI 按用户要求暂缓。** 以下为各阶段历史记录。

2026-09-21 当前 R2 进展：SQLite／CAS 原生装配已迁至 `opencut-sqlite-store`；Application 不再依赖 SQLite、AssetStore 或 tempfile。查询／分页／历史遍历和公开事件规则通过读端口保留在 Application，SQLite 多行任务页及事件页使用单个读事务。CLI、HTTP、Automation、Worker、预检及插件的原生入口已切换；原有 66 项检查与 Clippy、契约生成／旧 Schema 兼容／纯依赖门禁通过。见 [原生存储边界](native-storage-boundary.md) 与 [验收报告](../qa/r2-storage-split-2026-09-21.md)。**R2 尚未整体完成：预检／插件等剩余用例归位、后端无关合成、完整版本矩阵及发行仍待推进。** Web/GPUI 实现继续暂缓。

2026-09-21 上一阶段 R2 进展：任务生命周期已拆为纯状态机、JobService 和原子 JobLifecycleStore；Worker 在 SQLite 获锁后读取生产时钟，修复等锁期间过期仍可能用旧时间发布的风险。66 项相关 Rust 检查、Clippy、八类真实任务、MCP/HTTP、两类真实 SIGKILL／自然租约恢复及活动 FFmpeg 取消均通过。见 [生命周期验收](../qa/r2-job-lifecycle-2026-09-21.md) 与 [端口契约](job-lifecycle-port-contract.md)。**R2 仍在进行：查询／分页端口、SQLite 包级迁出、剩余用例归位和后端无关合成尚未完成。** Web/GPUI 实现与真实 UI 继续暂缓。

2026-09-21 上一阶段 R2 进展：工程 create/plan/apply/restore 已通过 ProjectRead／ProjectStore／AssetPinning 与 SQLite 原子写入分离；共享 v2/legacy identity，并修复预查回执与读取 head 间的并发误冲突。独立 CLI 故障／并发验收六项、TS 11／Rust 12 项、真实任务动线 11 项、MCP/HTTP 各 18 项及 Rust 54 项、Clippy、契约门禁均通过。见 [本轮证据](../qa/r2-project-ports-2026-09-21.md) 和 [工程事务契约](project-transaction-port-contract.md)。**R2 仍在进行：任务租约／查询等端口、SQLite 包级迁出、剩余用例归位和后端无关合成尚未完成。** Web/GPUI 实现与真实 UI 继续暂缓。

2026-09-21 上一阶段 R2 进展：公共 JobView 五个增量工具、八类结果投影、固定修订 target、内容绑定产物下载、双 SDK 公共任务入口已落地。任务接受用例迁入 Application，通过事务级 JobSubmissionStore 接口提交；入队和渲染共享 Core 素材依赖规则。真实八类 Worker／六类产物、事件恢复及失败动线 11 项，独立 TS 消费 11 项、MCP/HTTP 各 18 项、相关 Rust 90 项和 Clippy 通过。旧 22 工具 Schema 保持不变，当前总数 27。详见 [公共任务与应用层验收](../qa/r2-public-jobs-2026-09-21.md) 和 [契约](public-job-view-contract.md)。**R2 仍在进行：SQLite 具体实现尚未整体移出、工程事务及任务租约等端口未完成、后端无关合成未完成。** Web/GPUI 实现与真实 UI 验收继续暂缓。

2026-09-21 上一阶段 R2 进展：Rust SDK 与纯 API／媒体／Job 契约已落地，prepared input 移到内部 `job-execution`；生成 Schema 无漂移。仓库外 Rust 消费及同一事务跨 TS/Rust 重试 11 项、TS 消费 11 项、MCP/HTTP/Plugin、真实媒体动线及 129 项相关 Rust 检查通过。详见 [Rust SDK 与契约验收](../qa/r2-rust-contracts-2026-09-21.md)。当时公共 Job 视图、存储端口、用例归位与后端无关合成仍未完成；公共视图与入队端口的后续状态见上方。

2026-09-21 早期 R2 进展：独立 TypeScript SDK、22 个工具／八类 Job 的同源类型和 OpenAPI 已落地；仓库外真实包消费及 HTTP/CLI 动线 11 项、SDK 传输检查五项、现有 Web 类型检查通过。详见 [R2 SDK 验收](../qa/r2-sdk-progress-2026-09-21.md)。Rust SDK、公共 DTO/Worker 隔离与存储端口仍未完成。Web/GPUI 实现和真实 UI 验收按用户决定暂缓，契约见 [客户端接入约定](client-integration-contract.md)。以下早期记录为各阶段证据，不代表当前完整交付。

这份文件记录工作区实现，不替代完整范围 `agent-friendly-editor-v1.md`。未提交代码、组件测试通过和产品整体交付是不同状态。

2026-09-21 后续：发布前真实崩溃窗口与旧元数据修复已验；共享媒体检查／修复工具使当前工具数为 22。公共动线扩至 12 项，错误映射与插件注册约束已收拢。详见 [后续验收记录](../qa/r1-followup-2026-09-21.md)。R1 的真实浏览器门槛及 R2–R7 完整范围仍未通过。

2026-09-21：验收驱动的 R1 已落地首批修复，公共 HTTP／CLI 动线 11 项和实际组件场景 8 项通过；R1 及整体验收仍未完整放行。具体版本、证据与剩余门槛见 [R1 开发与验收进展](../qa/r1-progress-2026-09-21.md)，不要沿用下文历史检查直接推断当前完整产品完成。

## 当前模块边界

| 模块 | 职责 | 当前边界 |
|---|---|---|
| `contracts` / `time` / `editor-core` | 工程 DTO、精确时间、原子编辑计划、参数与操作发现 | 单主分支；部分可编辑参数尚不能渲染 |
| `editor-application` | 工程／任务／租约用例、读写端口、分页游标与公开投影 | 已移除 SQLite、CAS 与原生执行依赖；浏览器适配尚未实现 |
| `sqlite-store` | 原生 workspace facade、数据库初始化／迁移、工程及任务原子适配、查询快照、CAS 装配 | 现有单主分支本地宿主；其他存储后端尚未实现 |
| `job-contracts` / `media-contracts` | 公开任务输入、具体成功结果、八类 JobKind/Schema、纯媒体字段 | 已移除原生媒体与素材库依赖；v1 路径语义仍是宿主本机路径 |
| `api-contracts` / `job-execution` | 前者持有公开 DTO 与 typed Tool registry，后者持有内部 prepared input | 两者已独立；新增 JobView 投影，旧 JobRecord 保留兼容；源任务本地路径和旧接口退役仍待迁移 |
| `native-services` | 预检、媒体目录及插件的原生能力适配与装配 | 生产依赖无 SQLite；只读能力不创建对应目录，业务规则与计划验证在 Application |
| `composition` | 固定修订的纯合成描述、视觉/音视频范围、精确帧/区间选择 | 不含本地路径、编码器或执行后端；full/direct/range 已接入，第二个渲染后端尚未实现 |
| `automation` | 各入口共享分派、原生宿主装配、Schema/TS 类型生成 | 已去除对媒体/插件/CAS 实现的直接依赖；生成一致性 gate 已接 Moon CI，历史支持格式矩阵、事件能力和本地发行已有验收 |
| `packages/sdk-ts` | 独立 TS SDK、结构化错误、显式 Job 等待／取消与同源类型 | 仓库外包安装与真实 HTTP/CLI 动线已验；registry 发布、跨平台、SSE/媒体传输封装待完成 |
| `sdk-rust` | 59 工具关联类型、具体 Job 结果、阻塞 HTTP、任务观察与取消 | 外部源码消费及 TS/Rust 同事务重试通过；本地源码包已验；公共发行、异步取消和跨平台仍有限制 |
| `job-runtime` | 任务领取、心跳、取消、恢复、固定修订渲染／工程帧、结果发布、持久化阶段进度 | 提供编码后抽帧、直接 PNG 帧与区间音视频预览；仍非连续播放，进度比例是阶段比例 |
| `asset-store` | 绑定／替换素材时固定版本；工程任务 SHA-256 清单、执行前校验和私有复制 | 新版本素材不回退来源 URI；旧未固定素材保留兼容；显式孤儿 CAS 回收已验；字体/插件依赖锁及统一配额/保留仍待实现 |
| `media` / `project-render` | FFmpeg 探测、抽帧、多视频轨／图片／变换、混音、Unicode 字幕、clip Hold/Linear 动画 | 七项 clip 参数已通过真实像素／PCM 与公共分割验证；贝塞尔、轨道／字幕动画和完整效果仍待完成 |
| `plugin-host` | 版本化组件包、显式授权、Wasmtime 计划生成、宿主验证 | 当前 planner 插件；效果 ABI、Provider 和 UI 插件待完成 |
| `apps/cli` | CLI JSON/NDJSON、stdio MCP tools/resources/prompts/订阅、认证 HTTP、上传、产物下载、SSE | 本地宿主已支持项目范围凭证；远程 OAuth 与 OS 级隔离尚未实现 |
| `apps/web` | 真实 API 编辑台、工程帧请求、clip 关键帧编辑 | 源素材、工程帧和导出产物区分显示；构建／逻辑测试已验收，浏览器视觉验收仍待完成 |
| `apps/desktop` | GPUI UI 壳 | 尚未接入共享应用服务 |

## 本地运行入口

```sh
cargo build -p opencut-cli
target/debug/opencut --database .opencut/workspace.sqlite serve \
  --listen 127.0.0.1:8788 \
  --token-file .opencut/api-token \
  --allow-origin http://localhost:5173
```

HTTP 服务会运行共享 Worker；媒体任务要求环境提供 `ffmpeg` 和 `ffprobe`。CLI/MCP 单独运行时可另启 `opencut worker run` 消费持久化队列。关闭宿主会停止计算，下一次 Worker 启动可在租约过期后恢复未完成任务；需要持续后台计算时应保持 Service Host 运行。

```sh
target/debug/opencut --json system schema
target/debug/opencut --database .opencut/workspace.sqlite mcp serve
target/debug/opencut --database .opencut/workspace.sqlite --json job list
```

HTTP、CLI 和 MCP 调用相同应用用例。HTTP/MCP 每请求使用独立连接，依靠 SQLite 写事务及 expected_revision 协调写入，插件运行不持有全局编辑互斥锁。插件计划不会自动提交。

## 可重复的验收入口

| 检查 | 验证内容 |
|---|---|
| `cargo test -p opencut-cli` | CLI JSON、持久化、幂等、冲突、输入及具体输出 Schema |
| `scripts/verify-mcp.py` | 真实 stdio 握手、工具发现、计划／提交／重试／冲突、CLI 独立回读 |
| `scripts/verify-http.py` | 本地认证、Origin、工程写入、CLI 回读、持久化 SSE 事件及游标 |
| `scripts/verify-render-job.py` | 真实上传、工程编辑、固定修订任务、后续修改隔离、下载与哈希、独立音视频工具回读 |
| `scripts/verify-composited-frame.py` | 直接 PNG、固定修订、视觉依赖范围、音频尾段黑场、真实 FFmpeg 活动中取消及产物下载 |
| `scripts/verify-project-range.py` | 区间音视频预览、固定修订、完整素材锁、RGB／PCM 独立窗口验证与真实合成中取消 |
| `scripts/verify-project-preview.py` | 动画工程帧、序列格点、预览／导出像素对照、末帧与精确时长、编码中取消和私有文件清理 |
| `scripts/verify-asset-lock.py` | 原素材替换／删除、并发幂等、独立像素回读、CAS 损坏拒绝、旧任务兼容 |
| `scripts/verify-asset-recovery.py` | 真实 Worker SIGKILL、自然租约过期、新 attempt 恢复、已删除源与私有输入清理 |
| `scripts/verify-plugin.py` | 真实 WASM 包安装、计划无副作用、事务应用、重试和过期计划拒绝 |

这些命令的存在不是通过证据。每轮交付必须记录实际执行结果；插件脚本要求先构建 `crates/plugin-host/examples/planner-guest` 的 `wasm32-wasip2` 产物。

### 先前阶段的实际观察

- 最新 `cargo build -p opencut-cli` 已通过，产出包含 19 个工具的主程序；新增 `opencut_job_events`，独立任务 SSE 位于 `/v1/job-events/stream`。
- `verify-mcp.py` 已通过，使用真实 stdio 连接及 2025-11-25 协议握手。
- `verify-http.py` 已通过，包括实际 SSE 读取以及 `Last-Event-ID` 重连后的下一条事件。
- `verify-render-job.py` 已通过：后续删除活动工程中的片段，不影响已接受的固定修订导出；下载文件 SHA-256 匹配，并用独立 FFmpeg/ffprobe 验证红色画面、尺寸和时长。
- `verify-plugin.py` 修复受限 WASI Host 后已由主 Agent 独立复验通过：真实组件生成非空计划，计划不直接修改工程，提交重试保持幂等，过期计划明确冲突。组件只获得空 WASI 上下文，未继承宿主文件、网络、环境和标准流。
- 上一轮媒体 Agent 报告 8 项真实媒体／工程渲染测试与 Clippy 通过；覆盖非零源起点、VFR、音轨、混音、字幕、黑场、多视频轨／图片的像素位置与层叠。cosmic-text 替换后需要重新验收，不能沿用旧字幕通过记录。
- 修订历史、固定历史快照和整工程恢复已通过应用层、CLI/MCP/HTTP 验证；恢复创建新 revision，不改写历史。选择性撤销、多分支尚未实现。
- 最新应用层 Agent 报告 schema v3 与 25 项测试通过：任务进度／生命周期事件事务化，过期 attempt 拒绝，v1/v2 数据库升级保留数据；初始化及迁移在单个写事务中重新判断版本。两项并发初始化／迁移测试各重复 20 次通过。
- 本轮主 Agent 独立执行 contracts 3、core 10、application 22、CLI 5、job-runtime 2 项测试通过。根契约测试发现并修复 strict 字体策略吞掉未知字段的问题；新增默认字体字段不改变旧请求序列化内容。
- 字幕 Agent 完成 media unit 6、真实 FFmpeg 5、真实工程渲染 3 项测试及 Clippy。主 Agent 独立查看中英、阿拉伯 RTL、组合字符、多行左／中／右对齐 PNG，确认实际排版；这不替代浏览器交互验收。
- 最新 MCP 真实握手验证通过，工具数 19，包含任务事件及游标。HTTP Agent 真实验证通过独立任务 SSE、过滤、续接和 CLI 回读。Web 的 4 项 SSE 测试、类型检查、client/SSR 构建及最新 Schema 一致性检查通过。
- 新增公共字幕导出场景先暴露两处集成问题，均已修复后重新验收：正常打开 v3 WAL 库不再执行写事务；字体来源不再通过 Debug 展开字体字节。并发测试实际更新未提交 Job，其他连接仍读到原已提交状态，回滚后数据保持原样。
- 主 Agent 最新执行 `verify-render-job.py --caption-font Arial --caption-fallback-font 'Arial Unicode MS'` 通过全部 15 项检查：真实阶段进度、独立 CLI 事件回读、固定修订、媒体哈希／尺寸／时长、中英字幕实际字体及解码像素、字体元数据大小上限，以及缺字体时任务失败且不发布产物。没有通过延长请求超时或测试重试掩盖上述问题。
- 修复存储后，主 Agent 重新执行 HTTP、MCP、Plugin 三条公共脚本全部通过；最新二进制的 Web 生成契约 `--check --skip-build` 通过。相关 Rust 模块 Clippy `-D warnings` 通过。Moon CI 已配置生成一致性任务，未把配置存在宣称为远程 CI 已执行。
- Web Agent 报告生产构建与全量 TypeScript 检查通过；主 Agent 已独立确认 `/editor` HTTP 响应中包含真实编辑台。CUA 返回 `apps=[]`、`browsers=[]`，视觉与交互验收尚未完成。
- 本轮之后仍有并行开发。上述结果只证明对应执行时的工作区版本，后续变更需要重验受影响链路。

### 工程帧预览本轮验证

- 主 Agent 实际构建 `opencut-cli` 通过，最终 `verify-project-preview.py --artifacts-dir /tmp/opencut-preview-qa` 通过 16 项公共检查，报告 `status=passed`。使用真实上传、固定历史修订、Worker、认证下载和独立 FFmpeg RGB 解码，没有用模拟渲染代替。
- 位置关键帧按闭式预期像素中心验收：片段非零起点、0.5 倍速、多个中间帧及最后有效帧。预览与同修订正式导出的对应帧逐像素差不超过 2；主 Agent 另外查看了前、中、后三张 PNG。
- 新增真实 `clip.split` 事务验收最初发现浮点边界导致个别边缘像素差异，已修复后重跑通过：分割前／边界／分割后六个实际帧逐通道差不超过 5，像素中心差小于 1。另验证起点为 1.05 秒的 clip 在 10 FPS 序列中的局部时间相位，不能先把 clip-local 时间舍入到帧格点。
- Media Agent 的单参数像素检查覆盖 opacity 强度、scale 边界、0°／90°长方形、position_y 中心；低帧率源、Hold 片段内部零值、线性 PCM 增益与透明 contain 下层透出均已检查。主 Agent 最终独立运行 5 项 project-render 测试全部通过；新增“大整数时间段不能因相同 f64 被丢弃”的 adapter 单测通过。能力接口现声明 `keyframes=true`，范围为七项 clip 参数的 Hold/Linear。
- `nearest` 等距选择较晚帧，`floor` 向前；`exact` 非格点、请求到 end、ceil 超出 end 均拒绝。真实 NTSC 编码帧返回精确 `1001/30000`；`1/3` 秒序列终点不会因微秒舍入被误纳入。
- 取消场景等待真实编码 media_time 大于零后再请求取消，任务最终 cancelled，没有公共 artifact，且没有残留 `preview-source.mp4`／`frame.png`。新增 RAII 清理测试验证后端 panic 时这两个私有文件也会删除。
- 主 Agent：time 5 项、job-runtime 4 项测试通过；相关 crate 的 Clippy `-D warnings` 通过。Application Agent：automation 新增 3 项与 application 25 项回归及 Clippy 通过。
- Application Agent 使用统一新二进制执行真实 MCP 与 HTTP 脚本均通过。MCP 保持 19 个工具，新 Job variant 在输入 Schema 中可发现，覆盖固定 revision 入队、幂等、负时间拒绝及 CLI 独立回读。
- 主 Agent 重跑含中英字幕的 `verify-render-job.py`，15 项检查全部通过。
- Web Agent 使用新二进制生成契约，7 项逻辑测试、TypeScript、client/SSR 生产构建及契约一致性检查通过。代码包含连接／工程／revision／请求代次检查、下载中止与重试、请求与实际时间显示、错误详情和能力门控。主 Agent 再查 CUA 仍返回 `apps=[]`、`browsers=[]`，不宣称浏览器视觉与交互已经验收。
- 此阶段工程帧质量标记为 `encoded_project_frame`，需要完整渲染后抽帧，不具备低延迟连续播放能力；当时尚未锁定外部素材。后续媒体锁进展见下一节。

### 媒体内容锁与渲染预检

- 新工程导出和工程帧任务保存 `asset_lock`，包含 version、revision、sequence_id、实际使用的 asset_id/SHA-256/size。原始 snapshot URI 保留，Worker 验证 CAS 并复制到 attempt 私有目录；损坏或缺失不能回退源路径。结果显式标记 `media_content_locked`，旧任务显式标记 `legacy_unlocked`。
- 幂等身份保持原有 JobRequest 哈希；相同键在捕获前读取既有任务，提交时再次在数据库事务内判定。文件复制不持有数据库写锁。目录发布同步包含 CAS 根和哈希前缀目录，并发捕获方也独立同步发布目录后才能接受任务。
- 只读预检通过 MCP、CLI、HTTP 共享固定 revision 的类型化输入和报告，逐对象收集问题；Web 展示问题，并在导出／工程帧提交前检查。预检不生成 Job、不锁定文件、不承诺后续一定成功。
- Worker 在正常结束、错误、协作取消和 unwind 时清理私有输入；启动和租约恢复后基于数据库 attempt/state 清理已放弃输入，不删除当前运行中的输入或公开产物。
- 主 Agent 实际运行素材库 5 项、runtime 5 项测试通过；runtime 包括恢复清理时保留当前／未来 attempt 与产物的检查。相关 crate Clippy `-D warnings` 通过。
- 主 Agent 的 `verify-asset-lock.py --report /tmp/opencut-asset-lock-report.json` 已通过 9 项公共检查：入队后真正替换／删除源文件，导出与工程帧仍独立解码为原红色，新键捕获蓝色；并发同键只有一个 Job/queued event；CAS 损坏失败且无产物；旧无锁任务明确兼容。报告 `status=passed`。
- Application Agent 执行 `verify-asset-recovery.py --report /tmp/opencut-asset-recovery-report.json` 通过 5 项真实检查，主 Agent 已读回报告：强杀本次测试 Worker 进程组，等待自然租约过期，attempt 2 从已锁媒体恢复导出；原源文件已删除，独立红色像素与哈希通过，旧／新 attempt 私有输入均清理。未通过改数据库租约或模拟 renderer 代替该场景。
- Media Agent 最终报告 media 6 unit＋5 real、project-render 7 unit＋5 real 及 Clippy 全通过；预检新增实际 ffprobe、损坏媒体、真实流类型和源范围检查，图片与可选原声保持渲染语义。Application Agent 最终 automation 6、application 27、CLI 5 项测试通过。
- 该验收最初发现首帧 PTS 解析错误：ffprobe 的首行 `0,` 被整行整数解析丢弃。修复为读取首个 CSV 字段，并等待 stdout 读取线程结束后排空剩余 PTS；没有通过把验收请求改为第 0.1 秒避开问题。
- 最终原生 binary 重建通过，主 Agent 用它重新执行素材锁 9 项与工程帧 16 项公共检查全部通过；后者还检查协作取消后无私有 inputs 遗留。MCP 最终 20 个工具和 HTTP 验收均通过，生成契约一致性通过。并发编译／真实恢复渲染期间曾出现启动和编码进度等待超时；观察到高 CPU load 后停止并发重负载，保持原 15 秒启动／45 秒编码等待门槛串行复验通过，不把当时失败的运行记为通过。
- 最终 binary 的 `verify-render-job.py --caption-font Arial --caption-fallback-font 'Arial Unicode MS'` 15 项公共检查通过，包含中英字幕、实际字体报告、独立像素验证以及缺失严格字体时不发布产物。
- Web Agent 报告生成契约一致性、10 项逻辑／请求取消测试、TypeScript 与 client/SSR 构建通过。预检响应和创建 Job 后均有上下文隔离，返回的 revision/sequence 必须匹配请求。主 Agent 本轮 CUA 仍无浏览器窗口，视觉／交互验收未完成。
- 后续 Web 审查修正：已接受的正式导出独立于当前 UI 上下文继续运行，过期响应只不写入新视图；临时工程帧仍可取消旧请求。锁清单验证 SHA-256 格式、唯一素材 ID、非负安全整数大小及可取得的修订／序列绑定。
- 上述 Web 审查修正后的最终验证：3 个测试文件共 11 项通过、`tsc --noEmit` 通过、生成契约一致性通过、Vite client/SSR 生产构建通过。没有引入仅包装身份比较的冗余导出策略函数；正式导出保留运行的分支已源码审查，浏览器行为仍待真实交互验证。
- 此阶段的任务媒体锁不等于修订级依赖锁：当时使用新幂等键对同一 revision 提交仍可能捕获原路径的新字节。后续素材版本化进展见下一节；字体／插件／工具依赖、源素材任务、工程包和 CAS GC 仍属于完整目标。

### 修订级素材版本化：原生链路与 Web 逻辑验收

- 新 `asset.bind`／`asset.replace` 在提交时写入 `Asset.version { id, content_hash, size }`，内容版本 ID 为 `av_<sha256>`；逻辑素材 ID 保持独立。替换同 ID 保留 clip 引用，并由完整工程验证拒绝失效范围或类型。
- `edit.plan` 不读取或固定文件，返回 proposed document 中所有 `unresolved_asset_ids`；该集合不是当前渲染的必需素材集合。实际预检／执行仍由 `required_asset_ids` 确定范围。
- 新工程 schema 2；旧 schema 1 无版本素材继续可读可编辑，None 序列化不新增字段以保留旧请求身份。旧修订不隐式迁移，schema 2 也可能包含未固定的旧素材。
- Application 在原请求幂等／head 检查后于数据库写事务外 pin，事务内重复检查再提交准备好的操作。相同原请求在源删除或 head 前进后重试仍返回原结果。
- 固定版本的预检从只读 CAS 验证执行 URI；工程任务从版本生成媒体锁，Worker 校验锁与版本一致后复制到 attempt 私有目录。缺失／损坏不能回退原 URI；必需版本素材的无锁任务返回 `ASSET_LOCK_REQUIRED`，旧无版本无锁任务继续兼容。
- Application 是 CAS 路径的单一来源，Automation 复用实际实例路径；内存实例使用私有临时 CAS，关闭时清理。文件数据库保持同级 `asset-store`。
- Web 新增素材版本信息与同 ID 替换流程，图片导入使用 graphic 轨道；历史视图显示对应历史素材 hash，保持禁止替换。源预览与工程预览区分显示。
- 主 Agent 统一运行受影响 Rust 测试通过：contracts 5、time 5、core 15、asset-store 7、application 28、automation 7、runtime 6、CLI 5、media unit 6＋real 5、project-render unit 8＋real 5，共 102 项。相同范围 Clippy `-D warnings` 与 CLI 构建通过。project-render 新版本测试初次因 schema 1 和错误版本 ID 的夹具失败；修正合法夹具后复验通过，未放宽生产校验。
- 主 Agent 使用本轮新 binary 执行 `verify-asset-version.py --report /tmp/opencut-asset-version-report.json`，5 项全部通过：只读 plan、源删除后事务重试、同修订不同任务键、替换与旧修订、恢复及损坏 CAS 拒绝，并独立解码红／蓝像素。
- 主 Agent 执行更新后的 `verify-asset-lock.py`，10 项全部通过；新增同修订新键保持原内容、显式 replace 新内容、缺原源仍可预检、损坏版本预检关联错误且不创建 Job，保留并发幂等、私有输入清理与旧任务兼容。
- 新 binary 的 `verify-project-preview.py` 16 项、`verify-render-job.py --caption-font Arial --caption-fallback-font 'Arial Unicode MS'` 15 项全部通过，主 Agent 另查看本轮中间／最后预览 PNG。MCP 20 工具与 HTTP/SSE 公共验收全部通过。
- 本轮 `verify-asset-recovery.py` 5 项通过：真实 SIGKILL 本次 Worker 进程组、自然租约到期后 attempt 2 成功，源已删除，独立产物哈希和红色像素正确，旧／新私有 inputs 均清理。报告 `job_4b282f635342406e9c1fcc3d5305738b`、`attempts: 2`；脚本异常现在写 failed，避免失败留作 running。
- Core/Web Agent 已生成新契约并将客户端与编辑台操作收敛为生成的 `Operation[]`。主 Agent 审查后补齐导入／替换源预览与事务完成回读的 API／工程／revision／访问代次隔离：过期 URL 回收、旧 snapshot/history 不写新视图、独立 busy 代次防止旧请求解除新请求状态。源预览下载失败仍完成已提交事务回读，并显示预览警告。
- 最终 Web Agent 报告 5 个测试文件共 20 项通过，包括延迟响应、同工程离开再返回、旧 URL 回收、下载失败及有效响应的正向验证；TypeScript、生成一致性、client 2157／SSR 2233 模块构建全部通过。主 Agent 已复查最终 act/import/replace 接线。浏览器工具本轮仍返回空 apps/browsers，不将逻辑测试当作交互验收。

### 共享任务契约与已解析工程类型

- `job-contracts` 统一公开任务输入、prepared 工程导出／帧输入、artifact 和四类封装结果；probe/verify 复用媒体 DTO。Automation 强类型准备后序列化，Worker 使用同一类型读取、构造结果；`job-runtime` 已移除对 `automation` 的依赖。
- prepared 输入无 lock 时省略字段，保持旧请求身份 JSON；存储层仍使用原有 JobRecord JSON 和 SQLite schema，不增加媒体执行依赖。相同 key 的早期返回仍先于 CAS 捕获。
- 结果缺两项锁字段时保守解释为 `legacy_unlocked`；矛盾锁模式、修订或工程帧序列绑定不符均以 `INVALID_JOB_RESULT` 拒绝。Worker 发布工程结果前调用同一校验。请求拒绝未知字段，结果／artifact 读取可忽略新增字段。
- `capabilities.job_kinds` 发布六类任务 input payload 与成功 result 的 JSON Schema，不增 MCP 工具。当前 Schema 描述字段结构；锁清单与结果的跨字段一致性由 Rust 校验，不将结构检查冒充完整语义证明。
- `ResolvedProjectSnapshot` 内部字段私有、不可 Serde 构造，序列固定；普通 snapshot 不能传入 resolved 请求。显式 resolver 仅处理 required 版本素材，预检聚合每素材错误，plan 对任一解析失败拒绝。实际字节校验仍由 Host 的 CAS／任务私有副本流程负责，类型不保证不可信 callback 的正确性。
- 主 Agent 运行 project-render 9 unit＋5 real＋1 compile-fail、job-contracts 最终 10、runtime 6、automation 7、CLI 5 项测试通过；受影响 crate 的 Clippy 与 CLI 构建通过。Core 对历史结果与新增字段兼容规则完成了手写 fixture 验证。
- 新 binary 的素材版本 5 项与素材锁 10 项公共验收通过，涵盖显式替换、旧修订、缺源、损坏 CAS、并发幂等与真实 legacy 无锁任务。MCP 20 工具和 HTTP/SSE 验收通过；新增公共 Schema 检查覆盖六类名称、关键字段、时间字符串、工程帧质量及 legacy 默认模式。
- 生成器同时读取 tools schema 和 capabilities.job_kinds，校验任务名称、重复与命名空间冲突及 Schema 缺失，生成六组 Input／Result 共十二个 TypeScript 命名空间；客户端导出六类常用结果类型。Core/Web Agent 最终报告 20 项逻辑测试、TypeScript、契约一致性、client 2157／SSR 2233 模块构建全部通过。主 Agent 已审查生成器和类型导出；本轮没有改 UI 交互业务。
- 新 binary 的工程预览 16 项与含中英字幕导出 15 项公共检查通过；预览继续满足时间、分割、像素对照与实际编码中取消约束。真实 Worker 恢复 5 项通过，Job `job_c659605dc9634e1baea9c14d5c4f5d3d` 自然租约到期后 attempt 2 成功，源已删除，产物哈希／像素及清理通过。
- 原生任务 contracts 仍复用 native media/asset-store DTO；独立可发布 SDK、完整历史版本矩阵和后端 DTO 的进一步隔离仍未完成。该阶段结束时，`direct-frame-preview-plan.md` 仍是设计；后续直接帧实现与证据见下节。

### 直接合成工程帧

- 新增 `preview.composited_frame`，输入沿用 `ProjectFrameInput`，结果是独立 `CompositedProjectFrameJobResult`，quality 为 `composited_project_frame`；旧 `preview.project_frame` 和 `encoded_project_frame` 语义／枚举保留。capabilities 公开第七类任务 Schema 与独立 direct 能力。
- 视觉依赖由 `required_visual_asset_ids` 确定，使用私有字段 `ResolvedVisualProjectSnapshot`；compile-fail 验证其不能传入完整导出请求。素材锁只覆盖视觉输入，音频仍按原序列结束时间语义贡献时长，音频文件不被读取／探测。
- 媒体后端复用原绝对时间视觉图，以精确序列 frame index 选择单帧并编码 PNG，不构造音频 graph 或 MP4。当前仍可能解码前缀，不是低延迟 seek 或连续播放。共享帧对齐函数保留旧错误 code。
- 新帧 staging／caption 临时文件有 RAII 清理；FFmpeg 进度回调 panic 先终止并回收子进程，再展开栈。Worker 对过期的新帧 attempt 清理整个未发布目录，成功当前 attempt 的 PNG 保留。
- 本轮受影响 Rust 测试累计通过 94 项：automation 8、CLI 5、application 29、job-contracts 10、runtime 7、media unit 6＋real 8、project-render unit 13＋real 6＋compile-fail 2。受影响七个 crate 的 Clippy `-D warnings` 和 CLI 构建通过。
- 新增真实测试验证 VFR／非零源 PTS／B 帧的 0.5 倍映射（ffprobe 实际帧类型、非零首 PTS 和非等距 PTS 断言均已复验）、直接帧与实际源帧及编码帧对照、字幕半开边界、panic 清理、动画分割前／边界／后逐像素连续，以及缺失音频和音频尾段黑场。首次失败来自非法绝对 source_range、夹具全局 ID 冲突及不适用于色度边缘的逐通道阈值；保持原生产时间前缀，通过真实源目标／相邻帧误差判别建立正确时刻证据，没有放宽工程范围校验。
- `verify-composited-frame.py --report /tmp/opencut-composited-frame-report.json --artifacts-dir /tmp/opencut-composited-frame-qa` 已通过 11 项：真实 HTTP 导入／事务／固定修订、幂等、独立 CLI 回读、PNG 哈希／大小／像素、观察期间无 MP4、视觉锁排除缺失音频、旧完整任务拒绝缺失音频和 3 秒音频尾段黑场。取消在同一合成 FFmpeg 子进程连续两次采样存活后触发；Job cancelled、不发布 PNG、私有输入清理均通过，观察到 PID 47823。
- `verify-asset-recovery.py --job-kind preview.composited_frame --report /tmp/opencut-composited-recovery-report.json` 通过 5 项：确认合成 FFmpeg PID 47863 连续存活后 SIGKILL 本次 Worker 进程组，自然租约到期后 attempt 2 从已删除源文件对应的 CAS 恢复；80 秒处 PNG 的哈希、红色像素、旧 attempt 整目录清理及成功 attempt 输入清理全部通过。Job 为 `job_7030195afb544ae9969cdee12e03b04c`。
- 固定红色样例直接／编码帧最大与平均通道差均为 0，不推广为通用逐像素一致。复杂 VFR testsrc 的直接／源帧 mean 为 1.171、max 为 43，目标 PTS 8.3 的平均差明显低于相邻帧；高对比色度边缘差异不作为选帧错误。四张实际下载 PNG 保存于 `/tmp/opencut-composited-frame-qa`，主 Agent 已独立查看 direct.png。
- 旧 `verify-project-preview.py` 在本轮最终 binary 上通过全部 16 项，原有像素门槛不变，覆盖 NTSC、非格点起点、分数终点、真实分割与编码中取消。MCP 仍为 20 个工具，新增直接帧固定修订／幂等／排队取消调用通过；HTTP 认证、SSE／续接、七类任务 Schema 和跨入口回读通过。
- Web 提供两种显式预览入口、生成的新任务类型和不同画面标记；直接帧跳过包含音频依赖的完整 preflight，仍有请求上下文隔离。主 Agent 实际执行 21 项 Web 测试、TypeScript、最终 client／SSR 构建和生成一致性检查全部通过。
- 浏览器检查再次返回 `apps=[] / browsers=[]`，没有浏览器交互验收。直接帧专门的完整 NTSC／非格点起点／2 倍速与跨字体组合矩阵、低延迟定位、缓存、区间音视频预览和连续播放仍需继续；本节不宣称完整目标完成。

### 区间音视频预览（2026-09-20）

- 第八类任务 `preview.project_range` 已贯通共享 DTO／Schema、Automation、Worker 和 Web。输入绑定固定 revision 与 sequence，指定精确 requested_range 和帧对齐策略；结果公开 actual_sequence_range、quality `encoded_project_range`、完整音视频锁与 MP4 artifact。仅编码选中区间，但仍可能解码前缀；目前不是连续播放，也不做区间素材依赖裁剪。
- Adapter 使用有理数分别对齐端点，拒绝零／负／塌缩／越界范围，不静默 clamp。Media 保留绝对序列动画、字幕与音频时间后裁取，输出从零播放。完整渲染的旧音频 PTS 行为保留；首轮真实 hold/linear 回归发现该问题后修复，未放宽原 PCM 门槛。
- 主 Agent 最终重跑 runtime 7、media unit 8＋real 10、project-render unit 16＋real 7＋compile-fail 2，共 50 项通过。包括新 range 恢复清理、caption clip-backed 预检／规划一致性、NTSC 精确三帧及旧完整渲染回归。此前该轮 automation 9、application 30、job-contracts 10、CLI 5 项通过；之后这些模块未再改代码。七个受影响 crate 的 Clippy `-D warnings` 与 CLI 构建通过。
- NTSC 真实测试验证非零区间输出恰好三帧，timebase=1/30000，PTS=[0,1001,2002]，PCM 保留绝对静音／有声窗口。审查同时修复 range end 的 timebase 乘积无谓拒绝，极值运算使用 checked arithmetic；修正原有“两帧容差”遗漏 fps denominator 的计算，不用容器毫秒级时长冒充逐帧时刻。
- 主 Agent 的 `verify-project-range.py --report /tmp/opencut-project-range-report.json --artifacts-dir /tmp/opencut-project-range-qa` 通过 10 项公共检查。真实 1–3 秒请求输出 2.0 秒 MP4，独立解码前段绿色、后段蓝色；音频前段 880 Hz、后段 440 Hz，RMS 分别 407.37 与 1638.11。包含后续编辑隔离、同键重试、完整媒体锁、音频 CAS 删除后新键拒绝、认证下载哈希和 CLI 回读。主 Agent 已查看独立抽出的两张 PNG。
- 首轮公共计频用简单过零计数，被 AAC 零点附近噪声误判为 1103.75 Hz；独立频率分析证明 880 Hz 主分量远大于其他候选，改用 10% RMS 滞回过零计数后重新运行通过，保留原频率范围。另修复取消夹具的 track/clip 全局 ID 冲突，生产工程校验保持不变。
- 活动中取消确认本服务／本 Job 路径下同一 FFmpeg PID 53247 连续两次存活，取消后无公共结果、MP4 或私有 inputs。正常任务 Job `job_70af0e31ca034d96b53115bec895ed71`，revision `rev_263e43431b0db7189713f119b712c0ecae8fa1f4547b129ea436804ab90f2f2f`。
- 主 Agent 的 `verify-asset-recovery.py --job-kind preview.project_range --report /tmp/opencut-range-recovery-report.json` 通过 5 项：观察稳定 FFmpeg PID 53298 后 SIGKILL 本次 Worker 进程组，等待自然租约到期，attempt 2 从已删除源文件的 CAS 恢复 30–60 秒区间；旧 attempt 整目录清理、当前成功输出保留、私有输入清理与独立哈希／红色像素均通过。Job `job_a3846f47122443488ba74b3ec33d73dd`。
- MCP 20 个工具与 HTTP/SSE 公共验收通过，均发现八类任务；MCP 额外实际创建／重试／取消区间任务，拒绝零时长，并由 CLI 回读 prepared input 与状态。主 Agent 使用最终二进制再次检查生成契约一致性通过。
- Web 生成区间 Input／Result；新增范围长度及音视频预览入口，标记绝对序列范围和播放器零起点。拆开 workspace 与 source 请求代次，避免预览请求淘汰已接受编辑的 snapshot/history 回读；主 Agent 审查发现残留跨代次域比较并修复。Core Agent 最终 23 项逻辑测试、TypeScript、生成一致性及 client/SSR 构建通过；主 Agent 另独立重跑 23 项测试通过。新的异步流程覆盖交错的两次预览与并行编辑回读；正式导出跨导航继续运行。CUA 本轮再次返回 apps=[]、browsers=[]，不宣称浏览器交互已验收。
- 最终 binary 的旧 `verify-project-preview.py --artifacts-dir /tmp/opencut-preview-qa` 16 项全部通过，保留原像素、NTSC、分割、非格点相位、分数终点和真实编码中取消门槛，revision `rev_4b7fda56c3fb6aee274a6fde28e8a2f781439d82aa5a3bba3d1d6656b0f4db78`。该回归证明区间改动没有替换旧编码帧语义。
- 下一阶段连续预览契约见 `continuous-preview-plan.md`，目前仅设计；区分服务端 producer 进度和 owner 客户端 presentation telemetry，不将短区间 Job 循环冒充连续播放。

### 连续预览准备：共享编译与精确时间边界

- `project-render` 将重复的视频／图片素材映射、变换、图层序号、clip-local 曲线编译和字幕样式映射收敛为私有 `compile_visual_clips`／`compile_captions`。完整导出与直接帧实际调用同一实现，区间预览继续复用完整规划。视觉路径仍不生成音频图，也不求值视频音量；独立音轨仅贡献启用、未静音片段的序列时长。opaque resolved 类型和素材执行路径检查保持原边界。
- 此步仍输出现有 media Timeline DTO，只是减少执行路径分叉，不能称为后端无关合成描述已经完成。后续 `EvaluatedComposition`、持续 producer 与 Session 尚未实现。
- 另修复 `RationalTime::checked_scale` 的假溢出：当 numerator=denominator=`i64::MAX/u32::MAX` 时，对可表示时间乘等比例原先可能先在 i128 中溢出。现在跨分子／分母因子先约分，再 checked multiply 并检查最终 i64/u32；保留零、负号和非法分母行为，真正超出公开表示仍报 TIME_OVERFLOW。
- 主 Agent 在两项改动后独立运行 time 8、editor-core 15、project-render unit 16＋real 7＋compile-fail 2，共 48 项通过。包含真实完整渲染、视觉单帧缺失音频、动画与分割、区间音视频一致性。对应三个 crate Clippy `-D warnings`、CLI 构建与最终生成契约一致性通过。
- 最终 binary 的直接帧公共脚本通过 11 项，job `job_0bd8168f941b46d3aae0e7a0978db3c1`、revision `rev_7b765e5b696d7387cc9fd9c269a512c61c5fc85a59e6d45d0629c41063203b26`；活动取消观察同一 FFmpeg PID 54899，仍无 MP4，缺失音频不阻塞视觉请求，音频尾段保持黑场语义。

- 共享编译改动后的最终区间公共脚本再通过 10 项，job `job_eb85812d64704e00a956b9da15e045df`、revision `rev_964ff93846acdb0d0e55108934207fb8593d2f2b69eab6812b5ed572bd02bb35`。独立输出仍为 2.0 秒、880→440 Hz 与正确色段，活动中取消 PID 55045；报告和视频分别在 `/tmp/opencut-project-range-report.json` 与 `/tmp/opencut-project-range-qa/project-range.mp4`。

## 完整交付仍需覆盖

Classic 行为迁移及工程导入、低延迟连续工程预览、完整动画和效果、复杂文字排版跨后端一致性与字体依赖锁定、选择性撤销与分支、资源内容锁定及工程包、独立 SDK 与历史版本兼容性门禁、插件扩展类别、GPUI 接入、浏览器独立存储与 WASM、脚本面板和移动端适配，均继续属于正式目标。已支持的工程帧任务、多轨合成、原生字幕、任务事件和整工程恢复不能替代其余要求；Web 事件消费及预览的浏览器交互验收也仍待完成。

可选的真实中英字幕公共链路验证（字体必须在宿主安装；本轮 macOS 使用这些字体）：

```sh
python3 scripts/verify-render-job.py --caption-font Arial --caption-fallback-font 'Arial Unicode MS'
```

该场景同时检查实际字体报告、独立解码后的字幕像素，以及严格模式缺字体时失败且不发布产物。
