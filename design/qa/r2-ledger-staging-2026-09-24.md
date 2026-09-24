# R2 原生账本暂存物理准入验收（2026-09-24）

状态：**有界增量通过，R2 未放行。** `.storage-budget/ledger.json` 采用写入并同步 `ledger.tmp` 后原子替换的持久协议。此前这份临时副本未在准入前计入物理余量；满盘时 owner 策略写入只能等文件系统报错，自动 Worker 输出上限也可能把全部空闲字节承诺给 FFmpeg，留下不足以登记预留的空间。

现在每次账本保存先按文件系统块向上取整下一版 JSON，并为目录和同步留 64 KiB；与活动 payload／control 预留核对可用卷空间。普通准入同时保留配置的最小空余；owner 的策略恢复不受可能过高的旧最小空余门槛阻断。自动 Worker 输出上限在选择文件额度前，为本次账本副本和锁文件另留空间。退出 repair 前序列化下一版 Ready 账本并核对补足 8 MiB 预留后仍能保存它；部分残留的预留文件也可按已分配块数重新补足。

真实 macOS HFS+ 64 MiB 卷验收：外部空闲为 0 时，普通编辑拒绝且工程 head 不变；owner 策略请求提前返回 `STORAGE_CAPACITY_LOW`，修订不变且未生成 `ledger.tmp`。释放物理 repair 预留后用同一策略请求成功，随后取消排队 Job、释放终态上传暂存；外部填充仍在时退出被拒绝，释放填充后补足预留、原编辑幂等重试与 SQLite `integrity_check=ok`。另在独立 HFS+ 卷上验证物理余量决定自动输出上限时，预留成功且仍有账本写入空间，释放后无残留预留。真实 CLI 固定 WAL 读者的紧额度拒绝、原键重试和 checkpoint 回归通过。

`cargo test -p opencut-storage-budget -p opencut-sqlite-store -p opencut-cli`、两原生 crate 的 Clippy `-D warnings`、格式与差异检查通过。一次中间回归暴露 repair Active 时部分残留预留文件仍应可重建；修正后对应测试及 64 MiB 实盘流程通过。另一次中间回归暴露自动输出上限不能用尽卷余量；改为预留账本／锁空间后在隔离卷通过，失败的中间候选包未作为交付。

最终 macOS x86_64 debug 开发包 [opencut-0.1.0-dev-d68ccc267851-x86_64-apple-darwin.tar.gz](../../dist/local/opencut-0.1.0-dev-d68ccc267851-x86_64-apple-darwin.tar.gz) 的 SHA-256 为 `4bf71f0a5e290b3a79e208d921fe7b0931193527ac18570bad47b374a501c694`，二进制 SHA-256 为 `82b5bea9d9e9243f5f0147d6bd4b41788f559bc2d16269983123a1b43e4ec4be`。仓库外解包后[34 项整包验收](evidence/r2-ledger-staging-package-acceptance-2026-09-24.json)全部通过，覆盖双 SDK、MCP／HTTP、30 格历史、真实盘满 repair、Worker、网络撤权、导入恢复、媒体产物解码与停服务重开。可选 WASM component 未随本包验收。

这是管理型写入的保守预检，不是严格物理硬配额：外部写者、文件系统写时复制、目录分配与 fsync 竞态仍可能导致预检后写入失败；全部控制元数据、运行中 Worker 的满盘 repair、跨平台发行和完整 R0–R7 尚未放行。两个全局强制能力标记继续为 false。
