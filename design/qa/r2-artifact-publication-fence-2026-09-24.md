# R2 产物引用发布围栏增量验收（2026-09-24）

状态：**schema 18 已建立强杀后仍生效的持久引用发布围栏；公开删除执行与恢复尚未完成，R2 仍部分完成。** 五类产物引用权威都由同一个 SQLite 业务库写入：工程修订、Job 输入／结果、项目素材、插件发布和项目插件授权。`artifact_gc_removals` 出现 `removing` 后，SQLite 触发器拒绝这些表的新增或引用字段改写；与引用无关的 Job 状态更新仍可提交。未来执行必须在 `BEGIN IMMEDIATE` 写事务内完成即时扫描并原子提交 `removing`，随后由原生尝试排他锁删除；未记录终态回执前，强杀不会打开新引用发布空档。当前公开 API 不会创建删除意图，`execution_available=false`。

受影响的 SQLite crate 98 项单元测试及 SIGKILL 集成测试通过。双连接回归证明，写锁期间另一个连接不能插入 Job；意图提交后，五类权威的新增和引用字段更新都被拒绝，普通 Job 状态可更新，终态后写入恢复。[独立进程专项收据](evidence/r2-artifact-publication-fence-2026-09-24.json)把删除意图从子进程提交后强杀，再由真实 CLI 尝试创建工程：整笔事务以 `ARTIFACT_COLLECTION_RECOVERY_REQUIRED` 拒绝，没有工程、修订或幂等回执；将意图标为 `changed` 后用同键重试，只产生一条修订和一条回执。冻结 v1–v18 两摘要共 36 格历史矩阵和 25 项迁移检查通过，包括 v17→v18 DDL 碰撞回滚。

release 编译的 macOS x86_64 本地开发包为 [`opencut-0.1.0-dev-385c68cd4494-x86_64-apple-darwin.tar.gz`](../../dist/local/opencut-0.1.0-dev-385c68cd4494-x86_64-apple-darwin.tar.gz)，归档 SHA-256 `ae003140a0fb391651ccb50d5fe699612fc6d196116c124d8f6494b2957d3e2a`，包内二进制 SHA-256 `7421191dbaa4f1c8b87fbbf87759d746e7ded1347c256611198b0c9e41799a57`。[仓库外 36 项整包验收](evidence/r2-artifact-publication-package-acceptance-2026-09-24.json)全部通过，其中包括包内 36 格历史矩阵、强杀围栏专项、双 SDK／MCP／HTTP、真实盘满和 Worker 恢复。[包内媒体动线](evidence/r2-artifact-publication-product-2026-09-24.json)通过 13 项，覆盖真实上传、预览 RGB／PCM、导出、计划与重开。三个收据的二进制 SHA-256 一致。包的 manifest 仍标记 `development`，此处的 release 指 Rust 优化构建，不等于正式发行或跨平台验收。

围栏只解决数据库内持久引用的发布空档，不替代执行前重新解析原生路径别名、尝试目录排他锁、下载／Worker 租约、文件身份复核、同步删除、终态回执及 `removing` 的自动恢复。直接在数据库之外篡改文件系统的进程也不受此围栏约束；这些门槛未通过前不得宣称产物回收能力已交付。
