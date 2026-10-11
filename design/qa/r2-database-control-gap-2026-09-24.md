# R2 SQLite/WAL 控制写入准入：当前缺口实测（2026-09-24）

本轮使用已通过 36 项仓库外验收的 macOS x86_64 debug 二进制（SHA-256 `9e2d2f717e88a1005c4e1f8e1194fa5572a978bc08b86c7050475c4aecd94d91`），从公共 CLI 工具创建工程、设置 payload 准入余量为零，由独立 SQLite 连接固定一个读视图，再连续提交实际工程编辑。当时使用一次性审计脚本；该脚本已在交付清理中移除，当前回归由 [control VFS 验收](../../scripts/verify-control-vfs.py) 和 [预算政策验收](../../scripts/verify-control-budget-policy.py) 承担，原始摘要与请求／错误序列见[受控审计](evidence/r2-database-control-gap-2026-09-24.json)和[运行 trace](evidence/r2-database-control-gap-trace-2026-09-24.json)。

结果：`opencut_storage_budget_inspect.admission_headroom_bytes="0"`，但四次编辑都提交；固定读者使 WAL 从 0 增至 **4,021,152 字节／976 帧**。`opencut_database_checkpoint(TRUNCATE)` 返回 `blocked`，未虚报回收；读者释放后再次调用返回 `complete`，观察到 WAL 为 0。这里没有把 payload 额度当作数据库额度：当前公开契约本来只保护十二类 payload 文件，`enforces_all_workspace_writes=false` 和 `enforces_total_storage_limit=false` 均准确说明此边界。

同一二进制的[独立 64 MiB 卷盘满验收](evidence/r2-database-enospc-2026-09-24.json)把卷可用字节压到 0：编辑和任务取消返回 `DATABASE_CAPACITY_EXHAUSTED`，数据库行及工程 head 没有部分提交；移除测试填充文件后原幂等键可提交一次，取消可完成，`integrity_check` 为 `ok`。这证明失败回滚和故障后的恢复，**不证明盘满时取消有保障容量**。

下一实现依据[数据库控制写入契约](../architecture/database-control-admission.md)：控制字节必须与 payload 分账，SQLite 主库／WAL／SHM／journal 的增长在实际写入前受控，修复类操作保留独立容量；旧二进制、直接改写数据库的外部进程及其他平台边界须单独说明。上述两个实测结果是 R2 的红线基线，不是 R2 放行证据。
