# R2 媒体发布记录与中断恢复验收

2026-09-21。本轮实现共享原生发布协议并接入 workspace 媒体内容、元数据上传和修复。插件安装尚未接入；完整 R2／R0–R7 未放行，Web/GPUI 实现继续暂缓。

## 变更

纯 DTO／Application Journal/Read 端口、SQLite v6 追加事件与 Native Media 对象锁形成 [发布契约](../architecture/native-publication-contract.md)。prepared 先于文件修改提交，正常完成追加 completed。文件操作或完成记录失败返回 PUBLICATION_OUTCOME_UNKNOWN，保留 publication_id；显式核对或下一次协作写入在对象锁内读取现场，追加 observed_applied / observed_unchanged / observed_diverged，不伪造历史成功，也不覆盖现场。

新增 owner-only `opencut_maintenance_query`（只读）和 `opencut_maintenance_reconcile`（写记录），工具共 34 个，旧 22 工具 Schema 不变。内容和元数据分别记录，沿用同一个上传 response request_id 与可信 HTTP actor。库级兼容入口明确使用 UntrackedPublication，公共上传／修复宿主传入真实 Journal。

修复了上传去重只检查文件是否存在的问题：现在重新校验 staging 和已有内容 SHA-256，发现损坏时拒绝复用／覆盖。原 upload/repair 成功输出保持兼容；未改变文件的重复发布没有新记录。

## 真实流程

最终 debug SHA-256：`cc1e4a528f761d49128fa95f6c8f125215e54a80ab42292b6fb471fd223eebbd`。

`scripts/verify-maintenance.py --mcp-sdk-root <独立官方 SDK>` 的 8 组验收通过，证据目录 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-maintenance-nd8yyr9j`：

- 实际 WAV 上传、文件／元数据独立摘要、HTTP request/actor 对应，以及重复上传无新记录。
- 故意篡改内容级元数据，经真实 FFprobe 修复；before/after 指纹与 CLI 来源正确，独立 PCM 解码确认音频字节保留。
- SQLite prepared 插入故障使两个日志表一起回滚，实际内容文件没有发布。
- completed 插入故障发生在真实 rename 之后：文件确实存在、错误明确为未知结果，pending 保留；核对记录 observed_applied，并保留原始与核对 request ID，重复核对无新事件。
- 完成记录失败后再次改动磁盘元数据，核对记录 observed_diverged，原现场不被改写；之后使用当前指纹显式修复。
- 已有内容被同大小改写后，重复上传返回完整性错误；文件和记录都不被静默改变。
- 官方 MCP SDK 1.26.0 分别通过 stdio/HTTP 读取完全相同的记录并核对；受限凭证在两种传输均返回 NOT_AUTHORIZED。
- 独立十进制游标遍历全部阶段，无丢失／重复；无效对象路径、项目 read_audit 凭证拒绝；停止宿主后独立 CLI 重开记录保持。

## 进程中断与代码检查

`crates/sqlite-store/tests/publication_crash.rs` 使用真实子进程、SQLite、媒体目录与 OS 文件锁。测试专用 Journal 在 prepared 提交后或文件发布后暂停；父进程确认第二个核对进程已开始且被锁阻塞，再真实 SIGKILL 发布者（退出 signal 9）。核对取得释放后的锁，分别追加 observed_unchanged / observed_applied，原始与核对请求来源正确，重跑不重复记录。暂停逻辑只在测试适配器，未向生产宿主添加环境变量后门。

相关 Rust 83 项通过，包含上述实际进程测试；显示为 ignored 的 publication_child 是由父测试主动执行的辅助入口，不是遗漏的验收。Clippy `-D warnings`、TS SDK 5 项、生成契约、旧 22 工具兼容、纯依赖／原生边界门禁通过。日志 `/tmp/opencut-maintenance-final-tests.log`、`/tmp/opencut-maintenance-final-clippy.log`。

旧迁移单测的降级夹具增加 v6 表清理，首次修改误将第二次修复碰撞表的清理也重复扩大，导致 no such table；已限制为只删除碰撞表，完整重跑通过。生产迁移没有通过忽略 DDL 错误规避失败。

## 交付与兼容

数据库 v6 原子新增发布记录及索引，不回填更早操作；v1–v5 SQL 夹具保留，新增独立 db-v6.sql。历史矩阵扩到 12 格并新增 v5 迁移碰撞／回滚检查。最终 debug 的 15 组历史检查／12 格矩阵通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-slxrw16s/report.json`。新版开发包 `dist/local/opencut-0.1.0-dev-9a3f707b4702-x86_64-apple-darwin.tar.gz` 已通过仓库外 22 组验收，25 条记录命令全部 exit 0。仅本机 macOS x86_64，先前已验包保留。

- Archive SHA-256：`839aa3739a76ae9432f5f2384bb05d618f424896f4729b4debd5a248a1a6bb36`。
- Release binary SHA-256：`4131f7b2c16c49af5839c6fa09e1b9a8bc36bca58c9c32f35554d779ccdb5ce8`。
- 证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-y53dx0kg/report.json`，完整命令输出见同目录 `commands.json`。
- 解包 release 再次通过媒体发布 8 组（含官方 MCP SDK 四种身份／传输组合）、权限管理查询、受限网络／CLI/stdio、来源追踪、事件与 MCP 回归。
- TS npm 包实际独立安装，Rust SDK 源码包在仓库外构建；真实 HTTP 上传产生内容／元数据各一对 prepared/completed，两个 SDK 查询得到完全一致的 typed 记录，并拒绝项目范围凭证读取维护历史。原受限编辑、跨语言幂等重放、CLI 精确回读和停服重开保持通过。
- 交付 release 历史矩阵 15 组／12 格通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-e3wlnwc0/report.json`；真实上传、预览、导出、元数据修复和独立 RGB/PCM 动线通过。
- 包文件摘要、诊断篡改检测通过；manifest 标记数据库 6，安装说明包含 v1–v5 → v6 升级／备份回退，以及未知发布结果处理。OpenAPI 36 条路径的本地引用全部可解析。

本轮不是 SQLite／文件系统原子事务、硬件断电、其他平台持久化或插件发布完成的证明。下一步把插件包／授权文件发布接入相同协议，并继续工程范围导入／预检／插件授权；R3–R7 保持完整目标。
