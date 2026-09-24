# R2 已发布产物保留状态与显式 release（2026-09-24）

状态：**成功 Job 产物的保留状态查询与显式 release 已实现；完整回收仍未完成。** Owner 通过共享 Automation 的公开工具按 `artifact_id` 查询 `retained / released`，并用显式幂等键撤销默认保留根。release 与审计在一个 SQLite 事务提交；相同键与对象原样重放，键指向另一对象或对象已由另一键释放时给出明确冲突。受限主体不能查询或释放。release 不删除文件，成功 Job 历史和下载仍独立存在。

schema 从 15 升至 16，新增保留表；公开操作从 61 增至 63，生成 OpenAPI 从 76 增至 78 条路径。原生精确删除围栏仍未被公开用例调用。契约与仍缺的引用图、可审阅计划、执行和回执见[产物保留契约](../architecture/artifact-retention-contract.md)。

最终 macOS x86_64 **debug 开发包**为 [`opencut-0.1.0-dev-0f545bfd6898-x86_64-apple-darwin.tar.gz`](../../dist/local/opencut-0.1.0-dev-0f545bfd6898-x86_64-apple-darwin.tar.gz)，归档 SHA-256 `896ad55c867fcc7dd754669fd3edc13ba6753fdaf5e155e88d5ce531cfbd2535`，包内二进制 SHA-256 `d2ced025aced3a4c8e4f6726dcf74c9561e5eb7554360dc2610a7ba7826a97f8`。包内附有产物保留语义契约与生成协议；从该归档在仓库外解包的[35 组整包验收](evidence/r2-artifact-retention-package-acceptance-2026-09-24.json)通过。相同二进制摘要见[真实 HTTP／CLI 媒体动线](evidence/r2-artifact-retention-product-2026-09-24.json)、[v1–v16 两摘要历史矩阵](evidence/r2-artifact-retention-history-2026-09-24.json)、[物理盘满 repair](evidence/r2-artifact-retention-repair-2026-09-24.json)和[真实 Worker 锁动线](evidence/r2-artifact-retention-worker-2026-09-24.json)。媒体动线验证成功产物初始保留、错误 ID 拒绝、审计失败原子回滚、同键重放只审计一次、冲突键拒绝、另一个产物仍保留、原始字节可下载及重启后状态不变。样本仍为合成音视频，未验证真实访谈理解。

本轮首次媒体动线曾在进入 release 前出现 `STORAGE_ERROR`（SQLite disk I/O error）；同一工作区随后重跑通过、磁盘空间和 SQLite 完整性正常，无法从该次结果单独确认根因。后续真实并发验收发现预算目录锁争用被 VFS 映射为终止性 IOERR，已改为可重试 BUSY；取消请求的短暂 `DATABASE_BUSY` 被 Job 公共层误转成 `JOB_FAILED`，已公开脱敏的 `DATABASE_BUSY` 和 HTTP 429。Worker 的心跳重试窗口延至距 30 秒租约到期仍有一个 5 秒 SQLite 忙碌超时的位置。夹具检查租约须在到期前续上，取消客户端对 429 作 1 秒退避与有界重试。

首次整包验收在 64 MiB 盘满夹具中途停下：夹具在 Worker 刚持有短暂预算锁时发出 `SIGSTOP`，冻结后的 Worker 无法释放该锁，后续 `inspect` 返回 `STORAGE_BUDGET_BUSY`。夹具现在确认 Worker 已停止且预算门闩可独占；若停在门闩内则先恢复 Worker，再重新冻结。修正后的[盘满恢复单项动线](evidence/r2-artifact-retention-repair-2026-09-24.json)通过，含物理零空闲、运行任务子进程锁、排队任务取消、暂存释放和修复预留补足。

此前 7 秒预算目录锁与真实 FFmpeg 进度、外部取消交错时，Worker 偶发在锁释放后持续获得 `DATABASE_BUSY` 并丢失租约。采样确认心跳提交 SQLite WAL 时可能持有数据库写锁等待预算目录独占锁；同时进度持续写入会与心跳争用。现在只对 SQLite 控制预留给予 15 秒有界等待，并让进度写入在心跳待提交期间让行；普通预算门闩仍是 5 秒。清理诊断代码后的最终包通过独立 Worker 动线与上述 35 组整包验收，包含真实 7 秒双锁、租约续约、取消及孤儿 FFmpeg 恢复。这是所列场景和当前归档的证据，不证明所有调度交错均无缺陷。

格式、相关 Rust 测试和受影响后端 crate 的 Clippy `-D warnings` 通过。全工作区 Clippy 受本机 Command Line Tools 缺少 `metal` 编译器阻断于 GPUI 桌面依赖的构建脚本；桌面端静态检查在本轮保持未验。

完整 R2 仍受制于统一引用图、删除计划／执行／崩溃恢复、控制元数据物理保证和跨平台 release 验收；R3–R7 各产品动线另行放行。
