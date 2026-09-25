# R2 产物回收：真盘满回执与权限撤销（2026-09-24）

状态：**修复后的 macOS x86_64 release 开发二进制在 64 MiB 独立 HFS+ 卷的两个真实 ENOSPC 窗口均安全失败并可恢复；Unix 权限撤销不造成误删。R2 仍未整体放行。** 二进制 SHA-256 是 `ad0ffa140ca0fdea946ceee1a1eb6454663343551d20bff7432c730bface20b0`。测试使用 FFmpeg 生成实际预览产物，通过公开 CLI 提交 Job、release、审阅计划和执行，没有直接调用内部删除函数。

[真盘满收据](evidence/r2-artifact-full-volume-2026-09-24.json)记录独立卷容量 67,067,904 字节，两次故障时外部可用字节均为 0。测试数据库的慢触发器仅放大目标事务窗口，不改变产品代码。第一处在原生 unlink 已完成、终态审计提交之前填满卷：公开执行返回 `DATABASE_CAPACITY_EXHAUSTED`，SQLite 中仍是持久 `removing`，无 `removed` 审计，新增工程引用被围栏拒绝。移除外部填充文件后，同一计划得到持久 `absent`，同键回放一致，工程发布恢复。第二处在 attempt 独占租约取得、删除意图提交之前填满卷：同样返回容量错误，原文件字节、删除意图与对应审计均未改变；释放外部容量后，原计划成功删除。独立 SQLite 完整性检查为 `ok`。脚本 `scripts/verify-artifact-gc-full-volume.py` 在旧交付二进制上完整重跑两次，新修复版又通过一次。

[权限收据](evidence/r2-artifact-permissions-2026-09-24.json)覆盖两个不同语义。撤销 attempt 目录访问使执行安全失败、不建意图、不删字节；恢复目录访问后，原计划成功。撤销文件读取权限会改变其 ctime 文件身份，旧计划持久记录 `changed` 并保留字节；恢复权限后重新审阅的新计划可删除。脚本 `scripts/verify-artifact-gc-permissions.py` 在非 root Unix 用户下运行。

**仍需修复的公开口径：**目录撤权通过存储预算扫描返回笼统的 `STORAGE_ERROR`／SQLite `disk I/O error`，而非指向权限或安全盘点失败的结构化恢复建议。测试证明了安全拒绝与恢复，不证明 Agent 可从当前错误直接判断原因。跨进程并发故障矩阵、非 Unix 等价实现和全工作区物理写入保证仍未放行。新修复版 macOS 仓库外整包复验正在执行，结果须用其独立收据确认。
