# R2 SQLite 控制写入 VFS 增量验收 — 2026-09-24

状态：**部分完成，不是 R2 或完整 R0–R7 放行**。在独立控制额度账本之上，`sqlite-store` 增加具名 `opencut-control-v1` VFS。管理型 CLI、HTTP、MCP 与 Worker 数据库连接统一走它；嵌入式 `EditorApplication::open` 保留非管理入口。VFS 委托原生 SQLite VFS 处理实际 I/O、锁与同步，在主库／WAL／主 journal 的写入、扩大截断、size hint 和 SHM 映射扩展前扩展持久控制预留。每个管理型工作区绑定一个数据库文件名，另一个数据库会在文件创建前得到 `STORAGE_BUDGET_ERROR`。

当前源码和 debug 二进制 SHA-256 `11946e176975cfa8b032fac395c711f8358a573bb05832b077a5ee85` 的证据：

- [`verify-control-vfs.py`](../../scripts/verify-control-vfs.py) 经真实 CLI、独立 SQLite 固定 WAL 读者运行通过，收据见[JSON](evidence/r2-control-vfs-2026-09-24.json)。控制额度紧缩后，256 KiB track 编辑返回 `DATABASE_CAPACITY_EXHAUSTED`，工程 head 不变；扩大额度后原幂等键只提交一次。读者在场时 truncate checkpoint 阻塞，释放后完成；独立 `integrity_check=ok` 且交易数为一。第二数据库绑定拒绝也走真实 CLI 验证。
- `cargo test -p opencut-sqlite-store`：89 项单元测试、发布／上传 SIGKILL 与并发恢复集成测试通过。新增 VFS 单测覆盖真实 WAL 拒绝和策略调整后恢复、同目录第二数据库在打开前拒绝，以及不安全的 payload 目录不会误报为磁盘容量不足。`cargo test -p opencut-job-runtime`、`cargo clippy -p opencut-sqlite-store -p opencut-storage-budget -p opencut-automation -p opencut-job-runtime -p opencut-cli --all-targets -- -D warnings`、`cargo fmt --check`、生成契约检查及 Python 语法检查通过。
- [`verify-history-compatibility.py`](../../scripts/verify-history-compatibility.py) 的 30 格冻结格式矩阵和 22 项检查通过。原夹具曾把不同数据库放进同一目录，与新的一工作区一数据库约束冲突；现在每个独立夹具有自己的工作区，同一夹具仍保留双进程／四进程迁移竞态与失败回滚测试。
- 128 MiB 独立 HFS+ 卷实际填至 0 可用字节时，编辑与取消返回容量错误且未改变 SQL 行、工程 head 或 Job 状态；清除填充文件后原键重试、取消、WAL checkpoint 和 `integrity_check` 通过，[独立收据](evidence/r2-control-recovery-2026-09-24.json)与当前二进制哈希一致。原 64 MiB 夹具在默认 `minimum_available_bytes=64 MiB` 下无法初始化，因此扩为 128 MiB；这**不**满足目标契约中的 64 MiB repair 预留门槛。
- [Worker 实测收据](evidence/r2-control-worker-2026-09-24.json)验证真实 FFmpeg 输出上限、合成字幕临时文件清理、孤儿 FFmpeg 与 attempt 锁恢复；artifacts 根目录被别名替换时在 VFS 准入或 Worker 清理处拒绝，恢复目录后真实帧任务成功。heartbeat 失败现在保留本地诊断，避免无故把底层原因丢成取消状态。
- macOS x86_64 debug 开发包 [`dist/local/opencut-0.1.0-dev-8181a01bc0f6-x86_64-apple-darwin.tar.gz`](../../dist/local/opencut-0.1.0-dev-8181a01bc0f6-x86_64-apple-darwin.tar.gz) 的 SHA-256 为 `91a3dcef4791f256754b68b891b85e352c9b4c2afa67941140ff5c5dc0f32c48`；[仓库外 32 项验收收据](evidence/r2-control-vfs-package-acceptance-2026-09-24.json)与包内二进制哈希一致，包含双 SDK、HTTP/MCP、30 格历史格式、控制 VFS、真实盘满、Worker、网络撤权、项目导入和音视频产物重开。此轮未提供可选 WASM component，收据的 `component_sha256=null`，不能用本轮收据证明插件二进制验收。

`opencut_database_inspect.managed_vfs_enabled` 报告连接入口；`enforces_total_storage_limit`、`control.write_interception_enabled` 和 `enforces_all_workspace_writes` 仍为 false。当前尚未覆盖所有 SQLite 临时文件、外部直接写入、元数据文件增长的严格物理保证、预分配 repair 容量、并发 Host／SIGKILL 全矩阵及跨平台 release。旧宿主与新宿主不可同时写同一工作区。开发包验收只支持上述本机、此归档和这 32 项实际检查，不能替代完整 R2 或 R0–R7 放行。

恢复缺口：owner 若把共享的 `minimum_available_bytes` 设得高于当前可用空间，下一次 CLI 可能在 SQLite 的 SHM／WAL 打开阶段就得到 `DATABASE_CAPACITY_EXHAUSTED`，连同一工具的预算检查与重新配置也无法执行。旧 payload 验收脚本原先使用极大空余门槛来制造拒绝，因而暴露这一问题；脚本现以有效 96 MiB 音频和 64 MiB 控制余量验证同样的真实 payload 物理拒绝。产品仍需独立、可审计的 repair 通道与实际预分配容量，不能把夹具调整当作修复。
