# R2 已发布产物回收：下载互斥与强杀恢复（2026-09-24）

状态：**同一 macOS x86_64 release 开发二进制通过本机真实 HTTP 下载、文件身份变化与两处 SIGKILL 动线；产物回收及 R2 整体仍未放行。** 二进制 SHA-256 为 `12bb3362c7a42f95d2d2bc0bbc18ee97b120d906a47dcc52b28a2f2ae1f3e0f3`。测试从真实 FFmpeg 帧预览 Job 得到公开产物，由 Owner release、创建精确计划，再从 HTTP 或 CLI 执行；没有直接调用 Rust 内部删除函数。

[下载与替换收据](evidence/r2-artifact-download-and-replacement-2026-09-24.json)共 6 项：HTTP 下载在产物目录持有共享 inode 租约期间，精确计划执行返回非持久 `busy` 且不删原字节；下载核对 SHA-256 后，同一计划删除成功。同大小不同 inode 的文件替换、attempt 目录替换但把原文件移回、新增外部硬链接，分别返回持久 `changed` 并保留文件。脚本 `scripts/verify-artifact-gc-download.py` 用预算门闩暂时卡住真实 HTTP 下载的验证阶段，以确定地覆盖下载与删除交错；测试对同一二进制独立重跑四次通过。

[强杀收据](evidence/r2-artifact-sigkill-2026-09-24.json)共 4 项：测试库的慢 SQLite 触发器只放大事务时间窗，不修改产品代码。在 attempt 独占锁取得后、删除意图持久化前 SIGKILL，文件和数据库均保持原状，同一计划重试得到 `removed`。在 `removing` 已提交、文件已 unlink、终态回执提交前 SIGKILL，重开仍能查到 `removing`；新工程发布被 `ARTIFACT_COLLECTION_RECOVERY_REQUIRED` 围栏拒绝；同一计划重试得到持久 `absent`，同键回放一致，工程发布恢复，数据库完整性检查为 `ok`。入口是交付二进制的 CLI 与 Worker，退出方式是真实 SIGKILL；同一二进制独立重跑两次通过。

[仓库外整包收据](evidence/r2-artifact-download-package-2026-09-24.json)为同一 macOS x86_64 release 开发归档的 38／38 项通过，含双 SDK、MCP／HTTP、v1–v19 两摘要历史矩阵 38 格、真实盘满／repair、Worker、媒体产品动线和新增 HTTP 下载互斥／替换动线。归档 SHA-256 为 `6c79df00865336fe9b013534b71ff673ccb45c0a9f14fafea6a0cdf3801b935e`。强杀动线是针对归档内同哈希二进制的独立验收，未计入 38 项。

当前证据只覆盖遵守锁协议的单机 Unix 宿主。尚需盘满时 `removing`／终态回执的失败与恢复、权限撤销、符号链接在执行窗口变化、跨进程多宿主争用及非 Unix 等价实现；不能由本轮单机合成媒体推断完整资源治理或正式发行。
