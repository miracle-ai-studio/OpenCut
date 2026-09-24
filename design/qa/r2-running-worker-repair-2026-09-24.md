# R2 运行中任务的有界盘满 repair 验收（2026-09-24）

状态：**本动线通过，R2 整体仍为部分完成。** 在真实 64 MiB HFS+ 独立卷上，Worker 已领取 `media.probe` 并启动独立 ffprobe 进程。卷被外部写者填到可用 0 字节时，仍持有管理型 SQLite 连接的 Worker 阻止 owner 进入 repair，返回 `STORAGE_BUDGET_BUSY`。强杀 Worker 后，ffprobe 继续持有继承的尝试目录锁；owner 可释放物理 repair 预留，但对该任务的取消仍返回 `STORAGE_BUDGET_BUSY`，不会提前发布终态。终止孤儿 ffprobe 后，同一恢复命令原子提交 `cancelled` 状态和审计事件，重试返回同一终态。

同一盘满卷继续验证排队任务取消、终态上传暂存释放、普通编辑拒绝、外部写者重新占满后提前补足 repair 预留被拒绝。移除外部填充后，预留重新分配并同步，数据库 `integrity_check=ok`，工程重开、同键编辑重试及两条取消审计均通过。[机器收据](evidence/r2-running-worker-repair-hfs-2026-09-24.json)保存容量、二进制摘要及每项断言；可复验脚本为 [`scripts/verify-repair-reserve.py`](../../scripts/verify-repair-reserve.py)。

受影响的媒体、应用、SQLite、Worker 和 CLI 测试通过；新增的阻塞 ffprobe 取消测试通过，五个 crate 的 Clippy `-D warnings`、格式和差异检查通过。最终 macOS x86_64 debug 开发包 [opencut-0.1.0-dev-033d8c37563a-x86_64-apple-darwin.tar.gz](../../dist/local/opencut-0.1.0-dev-033d8c37563a-x86_64-apple-darwin.tar.gz) 的 SHA-256 为 `4a4628cabff9f5283a50c33f6c4f1208fb1d8326cb65d89ed4989c4f8d326657`，二进制 SHA-256 为 `9feb66843d671300c46e3f4d6f74b5a18b6cb8978b1c89edd84a737e9961abd1`。仓库外解包后的[34 项整包验收](evidence/r2-running-worker-repair-package-acceptance-2026-09-24.json)全部通过，其中重新执行了上述盘满 repair 脚本，并由交付包二进制产生相同摘要的收据。

追加独立[Worker 容量故障收据](evidence/r2-worker-capacity-quiesce-2026-09-24.json)：对**同一已交付二进制**运行 [`verify-worker-capacity-quiesce.py`](../../scripts/verify-worker-capacity-quiesce.py)。Worker 已执行被阻塞的 ffprobe 时，owner 将数据库控制额度调到 1 字节；Worker 以容量错误退出且释放连接，owner 立即进入 repair、原子取消遗留运行态，无须等待 30 秒租约过期；补足预留、恢复策略、幂等重试、审计及 SQLite 完整性均通过。这一追加检查不计入前述 34 项整包收据；下一次整包验收已将它纳入默认顺序。

实现边界：`storage recovery cancel` 在 repair 模式中，对排队任务保持原行为；对运行态任务先检查尝试目录的独占锁，只有无活跃写者时才直接撤销其旧租约。普通会话不能调用这个立即撤销操作。Worker 在任务结果或持久化出现容量故障时会退出并释放数据库连接；`media.probe` 使用可取消且有时限的子进程读取，避免退出被无界 ffprobe 阻塞。

本收据证明**停机／强杀后的运行态任务恢复**，不证明运行中 Worker 可在不断开的情况下进入 repair。尚缺非协作子进程、同时运行的多个宿主与 Worker、控制元数据严格物理硬配额、外部直接写者以及全故障矩阵。`enforces_all_workspace_writes` 继续为 `false`；完整 R2 和 R0–R7 不放行。
