# 数据库盘满错误与当前开发包复验（2026-09-23）

## 结论与修复

真实 64 MiB 独立镜像盘满时，`opencut_edit_apply` 已返回 `DATABASE_CAPACITY_EXHAUSTED`，但 `opencut_job_request_cancel` 经公共 Job 错误投影被折叠成 `JOB_FAILED`。这会让 Agent 失去可恢复原因。`editor-application::job_views::public_failure` 现在保留规范的容量错误码和不泄漏宿主路径的提示；单元测试覆盖即时错误和历史失败 Job 投影。`verify-database-recovery.py` 也修正了测试 SQLite 连接在镜像卸载前未关闭的问题。

新二进制在实际 HFS+ 测试卷填满后，编辑和取消均返回准确容量错误且 SQL 行、工程 head、排队 Job 状态不变；移除测试填充文件后，同一编辑幂等键成功提交且重放一致、取消只发生一次、`PRAGMA integrity_check` 为 ok。持有读事务时 passive/truncate 检查点明确报告 blocked；读者释放后 truncate 清空 WAL，逻辑数据不变。原始数值和二进制 SHA 见 [数据库容量证据](evidence/database-capacity-2026-09-23.json)。此验收证明错误与恢复路径，不代表 SQLite/WAL 已接入全工作区写入准入。

## 仓库外交付检查

本地 macOS x86_64 **debug 开发包** `dist/local/opencut-0.1.0-dev-90befbf82ac1-x86_64-apple-darwin.tar.gz` 的 SHA-256 为 `0463dc46210cca9b91e1e4cb4ba0f72b9e34214fe191c608e289b5b325e35709`；包内 CLI SHA-256 为 `cccee081dc81b714be5e24f6bea4718941e432798b987036014a3233aca5d6ee`。仓库外解包后 20 组检查通过，包含双 SDK 独立消费、官方 MCP SDK、v1–v15 × 两种历史摘要 30 格、权限与管理审计、存储观察、写入准入与强杀恢复、真实盘满/WAL、项目导入与 Worker 强杀恢复、RGB/PCM 真实媒体动线及重启重开。完整检查名与原始验收路径见 [包验收报告](evidence/current-package-2026-09-23.json)。复制到 `dist/local/` 后重新计算了同一 archive 摘要。

对相同 CLI SHA 另跑 [Worker 输出准入](evidence/worker-output-current-2026-09-23.json) 4 项：实际 FFmpeg 输出上限、字幕与多种预览／导出、父 Worker 强杀后子进程与 attempt 锁、目录别名拒绝。测试环境没有旧 schema-14 归档，脚本的旧二进制拒绝新库及旧账本升级分支未计入这 4 项。

这是恢复后的源码与开发包子集验收：并未验证真实旧用户数据库、长视频/VFR/中文访谈、真实浏览器与桌面操作、release 优化构建、跨平台发行，或 R0–R7 整体完成。`enforces_all_workspace_writes=false` 仍然成立。
