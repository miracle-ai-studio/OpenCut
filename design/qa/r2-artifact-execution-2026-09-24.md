# R2 已发布产物显式回收执行验收（2026-09-24）

状态：**Unix 本机单产物删除执行已接通，R2 整体仍为部分完成。** schema 19、69 个叶操作和共享 Automation 入口包含 `opencut_artifact_gc_execute` 与 `opencut_artifact_gc_execution_inspect`。Owner 以准确的计划 ID／哈希执行；旧计划、默认保留或带保护根的计划不能删除。执行锁定尝试目录，在 SQLite 写事务中即时重扫引用与 inode 别名，将 `removing` 和审计原子提交，然后按计划中的目录／文件身份删除并同步目录。终态回执跨进程可重放，历史成功 Job 不因字节回收而删除。

本机 Rust 测试、生成契约、权限与分层检查均通过；同一 macOS x86_64 release 开发包在仓库外通过 [36 项整包检查](evidence/r2-artifact-execution-package-2026-09-24.json)。包内真实 HTTP／CLI 合成音视频动线通过 [17 项检查](evidence/r2-artifact-execution-product-2026-09-24.json)，二进制 SHA-256 一致。强制删除意图审计失败时文件仍存在且无删除行；强制最终审计失败时文件已删除但 `removing` 保留，新的工程发布被围栏拒绝；重启后同计划重试记录 `absent` 并解除围栏。另一个无保护产物由 CLI 删除为 `removed`，HTTP 查询跨重启重放。v1–v19 两种摘要身份的 38 格历史矩阵通过。按旧编码去掉目录身份的计划仍可读取，执行明确返回 `CAPABILITY_UNAVAILABLE`。

此收据还不能放行完整产物回收：同一交付包尚须实测下载中独占锁、替换／别名、执行各窗口 SIGKILL、磁盘满回执、权限撤销及跨进程并发；非 Unix 尚无等价原生锁和文件身份协议。全工作区物理准入、控制元数据严格保证、多宿主与正式跨平台发行仍属 R2 独立缺口。

整包复验过程中并行执行的另一条独立 release 媒体动线出现过一次可重试 `DATABASE_BUSY`，另一次在 Worker 心跳处返回 SQLite `disk I/O error`；数据库完整性为 `ok`、卷空间充足。整包结束后的无并行负载同二进制[独立重放](evidence/r2-artifact-execution-isolated-2026-09-24.json)又通过 17 项，但并行异常仍未解释或核销；须在并发宿主压力下复现定位。不能用包内和隔离重放通过证明并发宿主稳定性。
