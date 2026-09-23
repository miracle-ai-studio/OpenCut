# R2 权限管理历史查询验收

2026-09-21。补齐此前已持久化、尚无查询入口的 schema 5 权限管理事件。完整 R2／R0–R7 尚未放行；Web 和 GPUI 实现继续暂缓。

## 实现与边界

新增只读工具 `opencut_access_audit_query` 与 CLI `access events --input FILE`，两种 MCP、REST 和两个 SDK 使用同源 DTO／Schema。工具总数 32，旧 22 工具 Schema 不变；复用 schema 5 的真实 access_events，不修改数据库格式、不回填更早操作。

查询规则位于纯 Application 的 `access_audit` 用例；SQLite 适配实现按需逐行访问的读端口，单个查询使用同一读快照。Application 和适配层都检查 owner，项目 read_audit 不授予全局管理历史权限。target 支持 workspace/principal/credential/request，序号为独立的规范 i64 十进制字符串；它不是工程／Job 审计序号或 MCP 游标。

每页最多 500 条且记录总编码大小最多 4,000,000 字节。返回 has_more／last_sequence；遇到字节上限提前分页时不跳过未返回的记录。提交策略保留当时的完整 PrincipalView；凭证动作只返回 ID，不返回 secret/digest。查询本身不追加事件，重复撤销不重复记录；覆盖仅为 schema 5 以来已提交的管理动作，不包含被拒绝的登录或失败修改。

## 本地证据

Debug SHA-256：`d66363ed38299051c22c0fd44d73056c88dc68f461bf52bc74a07e5950d86262`。

`scripts/verify-access-audit.py` 的 5 组真实进程验收通过，证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-access-audit-gt1jipg0`：

- 实际创建／更新策略、签发和撤销凭证，原 request ID／可信 actor 与管理结果对应，更新不覆盖旧策略快照；含引号的主体 ID 被参数化精确过滤。
- 逐条分页无丢失或重复，空页保留位置，未知目标为空；非规范序号、越界页大小、伪造身份字段拒绝。失败的 policy CAS 不产生已提交事件。
- 专用 CLI、通用工具、REST、stdio MCP 与 Streamable HTTP 返回相同历史；具备项目 read_audit 的凭证在所有入口仍被拒绝，伪造 header 不能提权。
- 真实 SQLite trigger 故障注入：审计插入失败时策略更新、凭证签发和撤销全部回滚；失败签发不遗留可用凭证文件。恢复后撤销只记一次，禁用记录保留最终策略。
- 关闭宿主、独立 CLI 重开仍得到相同记录；所有响应和 trace 无 secret 或摘要。

新增 SQLite 场景验证适配层不能被显式 owner 参数绕过，并使用三份含 1000 个长 ID 的历史策略快照检查实际字节预算分页，页大小低于 4 MiB，后续记录无跳过。相关 Rust 共 73 项（SQLite 62）、Clippy `-D warnings`、TS SDK 5 项、契约生成／旧 Schema／纯依赖及边界门禁通过。

第一次新验证脚本运行因局部 helper 名称遮蔽 Python http 模块而失败；修正脚本名称后完整重跑通过，生产授权和检查条件没有放宽。

## 交付验收

开发包 `dist/local/opencut-0.1.0-dev-f17740658938-x86_64-apple-darwin.tar.gz` 已通过仓库外 20 组验收；24 条记录命令全部 exit 0。仅本机 macOS x86_64 目标，数据库仍为 v5，先前已验包保留。

- Archive SHA-256：`165a6ac60e3a306d7b23ad7556fad314885fe094a6da0b0378a1c9dd352a1b5f`。
- Release binary SHA-256：`d3f89fc5df651ba929af983630e73b638e26eeb7849f85b0a31bce99c8479ee3`。
- 证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-2v4773d2/report.json`，完整命令输出见同目录 `commands.json`。
- 解包 binary 再次通过权限管理查询 5 组、网络授权 7 组、CLI/stdio 权限、来源追踪、事件、MCP 上下文／网络及官方 SDK 验收。
- npm SDK 实际独立安装、Rust SDK 源码包在仓库外构建。两者以 owner 读取完全相同的 typed 管理历史，并检查受限身份返回 NOT_AUTHORIZED（HTTP 403）；TS 输出见 `ts.json`，Rust 断言见独立消费者源码和成功命令记录。
- 两个 SDK 继续以 sdk-agent 受限身份编辑／跨语言重试／审计，交付 CLI 精确回读，停止宿主后重开保留状态。
- 交付 release 的冻结历史矩阵 14 组／10 格通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-rnvgr3lu/report.json`。真实上传／预览／导出／修复及 RGB/PCM 解码动线通过。
- 打包文件摘要和诊断篡改检测通过；交付安装说明／两个 SDK README 已描述管理查询。OpenAPI 包含 34 条路径，本地 schema 引用全部可解析。

## 继续推进的范围

权限管理查询现已实现。仍需插件／媒体维护操作的来源审计，以及工程范围素材导入、预检和插件能力；跨文件系统与 SQLite 的变更不能声称具有单数据库事务原子性，后续必须明确执行前记录、结果与恢复语义。R3 分支／选择性撤销、R4 持续预览、R5 分析、R6 完整编辑／插件／工程包、R7 跨平台和整体交付保持原范围。
