# R2 SQLite 磁盘临时文件准入验收（2026-09-24）

状态：**有界增量通过，R2 未放行。** 管理型 SQLite VFS 现在按工作区、数据库名和 normal／repair 模式绑定。SQLite 以空文件名打开的 `TEMP_DB`、`TEMP_JOURNAL`、`TRANSIENT_DB`、`SUBJOURNAL` 被定向到工作区的 `sqlite-temp/`，使用与主库相同的控制预算和写前闸门；未知或不能归属的文件打开请求拒绝。控制目录观察包含可见的临时残留。SQLite 打开 `DELETEONCLOSE` 临时文件后立即 unlink，活动字节由文件句柄持有的预留表示，关闭后释放。

原生实际 I/O 回归：`PRAGMA temp_store=FILE` 创建并写入 4 MiB 临时表，在仅有 16 KiB 控制余量时得到 `SQLITE_FULL`；扩大额度后，先前拒绝的语句没有留下行，再写入成功，活动预留至少覆盖 4 MiB，连接关闭后为零。把 `sqlite-temp` 预先替换成指向工作区外的符号链接时，写入得到 SQLite I/O 错误，外部目录无文件。`cargo test -p opencut-sqlite-store -p opencut-storage-budget` 通过（SQLite crate 92 项、预算 crate 的跨进程／SIGKILL／repair 用例），`cargo test -p opencut-cli`、两 crate `cargo clippy --all-targets -- -D warnings`、格式和差异检查通过。最终二进制的真实 CLI 固定 WAL 读者动线还验证了紧额度拒绝、工程 head 不动、原幂等键一次提交、checkpoint 和 `integrity_check=ok`。

macOS x86_64 debug 开发包 [opencut-0.1.0-dev-88ac94654450-x86_64-apple-darwin.tar.gz](../../dist/local/opencut-0.1.0-dev-88ac94654450-x86_64-apple-darwin.tar.gz) 的 SHA-256 为 `cc21639b0cb9b3f09e8669e0a3fea1c73943e21889ed598d0f9c35f63bcc5445`，二进制 SHA-256 为 `6a013c61cb619ad4073074d5de080f2113d0cf4cc1ea025d5443049ce4edce1f`。仓库外解包的[34 项整包验收](evidence/r2-sqlite-temp-vfs-package-acceptance-2026-09-24.json)全部通过，覆盖 MCP/HTTP、双 SDK、30 格历史格式、64 MiB 盘满与 repair、Worker、网络撤权、上传导入、媒体产物解码和重开。可选 WASM component 未随本包验收。

第一次运行整包脚本时，已通过 MCP 和归档校验，但宿主启动探针的单次 1 秒 HTTP 请求抛出 `TimeoutError`；宿主日志显示已监听，真实 CLI VFS 动线独立通过。未把这次运行计作通过。第二次运行同一归档完整通过 34 项；验收脚本随后将 `TimeoutError` 与连接未就绪一样，在既有 20 秒启动截止时间内重试，以消除单次探针抖动。

本轮没有证明全部 SQLite 具名／未来新增临时文件类型、预算和业务控制元数据的严格物理占用、外部直接写入、运行中 Worker／FFmpeg 的盘满 repair、活动连接下的 repair 切换、跨平台发行或完整 R0–R7。`enforces_all_workspace_writes=false` 与 `control.write_interception_enabled=false` 仍正确。
