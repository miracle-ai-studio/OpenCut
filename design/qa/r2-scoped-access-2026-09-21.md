# R2 受限 CLI / stdio MCP 增量验收

2026-09-21。本轮接通 principal/project 权限基础及 CLI、stdio MCP 动线。**R2 尚未整体放行，R0–R7 完整目标没有完成。** REST HTTP、MCP Streamable HTTP 仍只接受旧 owner bearer；本轮实际验证有效受限凭证在两个网络入口均被拒绝，没有隐式提权。Web/GPUI 实现与真实 UI 按用户要求暂缓。

## 设计与实现

- [授权契约](../architecture/access-control-contract.md)：纯 API DTO 与不可反序列化的 Caller；主体、多个可撤销凭证、显式七类项目 grant、字符串 policy revision。只存 secret 摘要，CLI 新建凭证文件在 Unix 使用 0600，stdout 不返回 secret。
- SQLite v5 原子迁移，独立 job_projects 验证固定 snapshot 与权威 revision 相等；不依赖审计记录授权，无法证明归属的旧任务保持 owner-only。迁移逐条读取历史输入，避免一次加载全部任务 JSON。
- Application 读端口包含计划授权；SQLite 在幂等回执读取前检查，并在编辑／恢复／任务接受／取消事务内再次检查。策略、凭证与管理事件原子保存，policy view 的 revision 和 grants 来自同一读事务。
- 项目／任务列表、分页及事件按存储查询过滤；隐藏任务不能成为分页或事件 cursor 锚点。审计查询要求 read_audit；有 Job 的来源记录还须符合独立任务归属。
- Automation 对受限主体采用工具白名单。原始 Job 工具、任意宿主路径、全局媒体、插件、Worker、owner host 和权限管理均不能由受限凭证调用。CLI 任务动线使用公共视图，任务事件使用公共投影。
- stdio MCP 工具、resource、prompt、订阅轮询携带同一 Caller。已建立连接的后续访问也检查撤权；已接受工作不因观察权限撤销自动取消。
- 新增 `opencut_access_inspect`，工具共 31 个，冻结的旧 22 工具 Schema 保持兼容；更新 TS、OpenAPI、Web 契约导出，不推进 UI 业务实现。

## 验证证据

最终 debug 二进制 SHA-256：`3daf576165d637e93be078b28d76bcd183ea10f8d7961be415797a5b52a85ad1`。

- `scripts/verify-access.py`：7 组真实进程动线通过。证据目录 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-access-y1qc4b4f`，含 report/trace、工作区、实际 PNG 与 HTTP 日志；secret 不进入 trace。
- 两个工程、只读／编辑主体、多个凭证：范围内 plan/apply/retry 成功，跨项目 inspect/revision、只读编辑、凭证自签发、路径任务及 Worker 拒绝；CLI 与 MCP 写入 actor 正确。
- 真实固定修订 Job 经 Worker/FFmpeg 生成 64×48 PNG。独立检查文件 SHA-256 和红色像素，受限 job wait 返回公共形状。read_artifacts 未授予时独立产物 URI 拒绝，更新策略后成功。
- 运行中 NDJSON 观察在撤权后返回 NOT_AUTHORIZED、退出码 3；既有 MCP 连接的 tools/resource/回执重试拒绝。撤销单凭证不影响另一凭证，禁用主体后另一凭证也拒绝。
- 6 组新增 SQLite 验证：准备／提交间撤权时全部业务表无新写入；回执不绕权限；隐藏任务分页／事件与原生路径隔离；并发策略更新仅一方成功；restore 是独立权限；管理事件失败回滚；旧任务归属与迁移失败回滚。
- 受影响 Rust 检查全部通过：SQLite 61、Application 3、Automation 1、CLI 2 unit＋5 真实进程检查。相关包 Clippy `-D warnings`、TS SDK 5 项及生成契约一致性、纯依赖/原生边界门禁通过。
- 最终 debug 的旧 HTTP、stdio MCP、事件流公共回归通过，分别为 20、19、11 组。日志 `/tmp/opencut-access-http.log`、`/tmp/opencut-access-mcp.log`、`/tmp/opencut-access-events.log`。
- 冻结 SQL 新增 v5，不修改 v1–v4；历史矩阵扩大为 10 格。迁移、旧回执／八类结果、旧 Worker 输入、20×4 并发开库和三类迁移失败回滚均纳入验收。最终交付二进制的矩阵证据见下方交付结果。

## 审查中修正的问题

公共任务错误转换原先将权限失败当作一般任务错误，现保留 NOT_AUTHORIZED。受限任务事件原始载荷可能含 Worker 私有诊断，因此受限 CLI 改用公共事件投影、原始事件工具保持 owner-only。编辑计划原先只在 native facade 检查权限，现由 Application 读端口执行，直接调用纯用例同样拒绝。

初次运行新验收脚本的失败来自错误 prompt 名和把无 stdout 的成功 Worker 命令当成 JSON 响应；改为从 MCP discovery 取得实际 prompt 名、单独检查 Worker 退出码后通过，未放宽权限或像素门槛。

## 下一步与未验边界

下一步接通 HTTP 请求 Caller、MCP HTTP 的 credential/session 绑定、订阅／SSE 重鉴权与产物字节下载授权，再验跨入口一致性。工程范围的素材导入、受限预检／插件能力、管理事件查询与插件／媒体维护审计仍需推进。当前不是网络多用户权限完成证据，也不是 OS 沙箱；具有数据库写权限的本机进程在信任边界内。

R3 分支／选择性撤销、R4 持续预览、R5 真实分析、R6 完整编辑／插件／工程包、R7 其他平台与整体交付门槛继续保留。不能由本阶段通过的动线推导“所有功能完善”或“没有 bug”。

## 本轮交付结果

开发包 `dist/local/opencut-0.1.0-dev-d9d15708a216-x86_64-apple-darwin.tar.gz`，仅本机 macOS x86_64 目标。

- Archive SHA-256：`e939603e671b47b6a4e90cd17b34a85f9ca053c65381c50cc79fb2f494fd1f81`。
- Release binary SHA-256：`f02a272e0ddd9366888fc83875e9e8c884cc48a8e957faf7b8ba036ede7ab023`。
- 仓库外 16 组验收通过，19 条记录命令全部 exit 0；证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-tejuwoag/report.json`，逐条输出见同目录 `commands.json`。
- 解包程序再次通过权限 7、来源 6、事件流 11、MCP 上下文 11、MCP HTTP 9、官方 MCP SDK 4 组；同时校验所有交付文件和诊断篡改检测。
- npm tarball 实际独立安装并编辑／重试／执行探测 Job；Rust SDK 源码包实际在仓库外构建，跨语言重放并提交新 revision，再由交付 CLI 精确回读。停服后重新打开状态保留。
- 交付 release 的历史矩阵 14 组／10 格通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-svfwtf_z/report.json`。包装验收的旧打印文字硬编码 eight-cell，已依据原始命令输出更正为实际 10-cell，原报告保留为 `report.original.json`；没有改变测试结果、包或二进制。
- 真实上传／预览／导出／媒体修复及独立 RGB/PCM 解码动线通过。该脚本仍明确标注 analysis.shots、analysis.transcribe、project.package 等剩余范围，不能将其成功计作这些功能完成。
- 交付说明包含 v1–v4 → v5 原子升级、备份和使用旧库副本回退的约束；受限 CLI/stdio 和 owner-only 网络边界明确区分。先前已验开发包保留，不覆盖旧压缩包。
