# R2 并发预算扫描与 Worker 发布增量验收（2026-09-24）

状态：**已复现的两条并发故障路径已修复，并在同一 macOS x86_64 release 开发包上通过六路独立工作区真实媒体动线；R2 整体仍为部分完成。** 每路执行 HTTP／CLI 上传、编辑、预览、导出、产物保留与显式回收、审计失败恢复、服务重启及解码后回读，共 17 项检查。六路均通过；公开请求中仍观察到合计 16 次可重试 `DATABASE_BUSY`，未将锁竞争伪称为不存在。

故障定位来自连续的失败复现。原实现遇到 Worker 终态发布的瞬时 `DATABASE_BUSY` 就退出，HTTP 宿主因绑定 Worker 生命周期而断开；六路中两路在新预览 Job 处失败。改为发布前重新读取持久任务状态和租约，用新管理型连接在租约余量内重试；若无法确认提交结果，先查终态，避免重复发布。非容量故障的 BUSY 超过窗口后由运行中的宿主等待租约恢复，容量故障仍保留退出以供 owner repair。真实 SQLite 写锁持续超过一次 5 秒 timeout 的测试证明同一 attempt 最终成功且只有一条成功事件。

另一次六路复现中，两路 Worker 心跳报 SQLite `disk I/O error`，扩展码 778（`IOERR_WRITE`）。VFS 日志证明错误来自写前预算准入的 `STORAGE_BUDGET_ERROR`；调用点定位到预算扫描对产物目录先 `statat`、后 `openat`，与媒体 attempt 目录清理并发时把已消失的目录当作损坏。现在仅在 `openat` 返回 `NOENT` 且再次无符号链接跟随的检查确认名称确实不存在时跳过；符号链接、替换身份和其他打开错误仍拒绝。OS 锁的 `Interrupted` 另行重试。可选 `OPENCUT_TRACE_SQLITE=1` 诊断只写服务日志，不改变公共错误协议。

最终二进制 SHA-256 为 `12bb3362c7a42f95d2d2bc0bbc18ee97b120d906a47dcc52b28a2f2ae1f3e0f3`。[六路并发收据](evidence/r2-concurrency-six-2026-09-24.json)记录 6／6 通过、102 项动线检查、16 次 BUSY 响应及各自仓库外日志位置。同一二进制打成 `/private/tmp/opencut-r2-concurrency-delivery/opencut-0.1.0-dev-f7a3c4f15641-x86_64-apple-darwin.tar.gz`，归档 SHA-256 `6c79df00865336fe9b013534b71ff673ccb45c0a9f14fafea6a0cdf3801b935e`；[仓库外整包收据](evidence/r2-concurrency-package-2026-09-24.json)为 37 项通过，含双 SDK、MCP／HTTP、38 格历史格式、真实盘满／repair、Worker、项目导入恢复和包内音视频产品动线。可选 WASM component 未随包验，不能以此收据放行插件二进制。

源码验证：`opencut-job-runtime` 9 项库测试、`opencut-sqlite-store` 98 项库测试、`opencut-storage-budget` 全套测试通过；预算目录消失／符号链接替换测试通过。分层依赖检查、Rust 格式化、Python 编译检查和 `git diff --check` 通过。六路脚本为 `scripts/verify-concurrent-product-journeys.py`；可使用新的空 `--artifacts-dir` 重跑。测试使用合成音视频，不证明真实中文访谈、VFR、长视频、跨平台或全部多宿主故障矩阵。产物删除的下载并发／强杀／盘满回执、全工作区严格物理写入保证及 R0–R7 其余产品门槛仍未完成。
