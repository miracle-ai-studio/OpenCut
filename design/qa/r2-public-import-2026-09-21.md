# R2 公开上传与项目素材导入验收 — 2026-09-21

## 本轮范围

接通 CLI、HTTP、stdio/Streamable HTTP MCP 的项目上传控制面及原始字节入口，提供项目素材分页和双 SDK 分块／扩展结果读取。数据库 v8，42 工具、9 类任务描述、OpenAPI 45 路径。Web/GPUI 只更新接入契约，未实施 UI。

本轮不将完整 R2 或全部 R0–R7 标为完成。终态上传暂存清理、磁盘总配额和失败任务重试策略仍待实现；项目插件授权及后续 R3–R7 范围保持。

## 实现与代码审查

- 纯 Application 定义上传控制、素材分页、清理端口；SQLite 在同一只读快照内授权并固定素材分页上界。跨项目 cursor 不能读取其他项目；查询不触发写入或修复。
- 六个新增工具复用相同 Application 权限和 DTO。创建／写块／封存／abort 要求 read + edit + run_jobs，查询要求 read。旧 22 工具和两个媒体发布工具冻结 Schema 保持不变。
- HTTP 原始 PUT 与 JSON 控制面分离，限制单块 4 MiB、16 个并发块和 60 秒接收时间；接收期间约每 250 ms 及结束时复查凭据与状态，落盘和 ACK 再复查。阻塞落盘闭包持有并发名额，客户端断开不能提前释放仍在执行的写入名额。
- CLI upload put 只在客户端读取文件／stdin；控制请求不接收宿主路径或 base64。上传失败先 inspect 确认偏移；不能由网络错误推断未提交。
- abort 先持久提交 aborted，再用与写块相同的 OS 锁清除私有目录；失败返回 UPLOAD_CLEANUP_PENDING，可重复 abort 完成清理。Application 校验清理对象身份与终态；私有临时文件在后续持锁写入时回收。
- TS uploadChunk 在 await 前复制字节并计算摘要，避免调用方改写缓冲区导致 hash/body 不一致；Rust upload_chunk 使用借用字节和本地 SHA-256。两者不自动重试或封存。TS publicJobResult 和 Rust extended_job_result 拒绝错误任务、未完成和未知版本；旧八类结果保持兼容。
- media.import 由 upload_seal 原子接受，旧 JobStartInput 不增加枚举。capabilities.uploads.accept_tool 和工具描述明确入口；扩展结果版本为 1。

## 真实产品动线

Debug SHA-256：`9b33799dfd658c7d7552e180a621571c3abd02f902e50e33d91d43e1b8f803ea`。

`scripts/verify-project-import.py` 九组通过，证据 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-project-import-scpnp8nm/report.json`，同目录保存 trace、MCP 输出、预览和服务日志：

1. CLI 创建／HTTP 重放同一会话；改参数冲突；read-only、只有 read+edit、跨项目、额外路径和猜测身份被拒绝。
2. 实际分块 HTTP 传输，精确重放不改数据库；冲突字节、错误 hash／长度被拒绝；中途断开不推进确认偏移，CLI 原始 stdin 完成续传。
3. seal 重放保持同一任务；真实 Worker 导入 image，公共结果无宿主路径，工程 head 不变；只读身份可查本项目资源。
4. 官方 MCP SDK 分别用 stdio/HTTP 创建／查询／封存，真实字节交接使用认证 PUT；只读和跨项目写入均拒绝。
5. 素材分页开始后新增导入，不进入旧 cursor 范围；分页没有数据库写入，其他项目 cursor 被拒绝。
6. 删除原文件后受限主体绑定、精确编辑重放、预检、真实 Worker 预览及认证下载通过；SHA-256 和独立 FFmpeg 的 64×48 红色 RGB 检查通过。
7. 大于 JSON 控制上限的 3 MiB 原始块正常上传；abort 清除私有目录，重复 abort 稳定，随后写入被拒绝。
8. 16 个真实未完成 body 占满 advertised 并发数，第 17 个返回 429；撤权使所有请求返回 403，名额释放且没有 ACK，正常凭据随后可继续。
9. 传输中撤权不产生文件／确认偏移；已提交块的审计区分真实 HTTP/CLI 来源与 principal；停服后独立 CLI 回读素材一致。

首次大块拒绝场景暴露验收客户端在服务端提前拒绝时遇到 BrokenPipe。脚本现在仍读取并严格验证 HTTP 状态与 envelope，不通过只捕获异常来假定成功。服务端在读取完整大 body 前拒绝已终止上传是预期行为。

## 崩溃与自然恢复

`upload_crash` 实际子进程检查新增“写入已落盘但未 ACK → abort 提交 → 清理等待 OS 锁 → 杀死清理进程 → 释放旧写入 → 再次 abort”。旧写入返回 UPLOAD_STATE_CONFLICT，确认偏移为 0，清理重试成功；原有 ACK 前／后 SIGKILL、不同块竞争和撤权竞态复验通过。测试子入口标记 ignored，由父测试显式拉起执行，不能解释为遗漏此检查。

`scripts/verify-project-import-recovery.py` 三组通过，证据 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-import-recovery-tztaxuq1/report.json`。只在测试 PATH 放探测边界暂停包装器，实际组装输入后杀死 Worker 进程组；不修改生产故障开关、数据库租约或系统时间。删除源 PNG 后重启正常 Worker，约 30.08 秒自然租约到期，attempt 2 用真实 FFprobe 完成。恰好一条素材记录、一次 media.imported 与 job.succeeded 审计；seal 身份和工程 head 不变，旧私有输入清理完成。

## 回归与契约

相关 Rust 117 项通过（首批 116 项，加新崩溃场景 1 项；包含 1 项 compile-fail doctest），TS 7 项及构建通过。Clippy -D warnings、fmt、纯依赖／源码分层门禁和生成一致性通过。HTTP 20 组、stdio MCP 19 组（42 工具）通过。OpenAPI 全部 45 路径及组件引用可解析。

日志：`/tmp/opencut-upload-public-tests.log`、`/tmp/opencut-upload-abort-crash.log`、`/tmp/opencut-upload-clippy.log`、`/tmp/opencut-upload-sdk-ts.log`、`/tmp/opencut-upload-http-regression.log`、`/tmp/opencut-upload-mcp-regression.log`、`/tmp/opencut-upload-wire.log`。历史 v1–v8 的升级／回滚由解包后的 release 继续验收。

## 新交付包

最终交付为 `dist/local/opencut-0.1.0-dev-c924e6d7272b-x86_64-apple-darwin.tar.gz`，仓库外 29 组验收通过，32 条记录命令全部 exit 0。Archive SHA-256：`5234a7b818068b02f1d41edfbf8dbcb8729bbec6b33dbc59b0500123166d9c28`；release SHA-256：`af4255392abb84e18028313792cee5e08a71df9851560c768162e300ec53350f`。

统一报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-_kdwq9ap/report.json`，同目录 commands.json 保存逐条命令／输出；归档旁 `.acceptance.json` 保存摘要。解包 binary 的导入九组和恢复三组通过，恢复耗时约 30.26 秒。

仓库外实际安装 npm 包；TS 以项目凭证上传真实 WAV、封存并读取强类型结果，再上传第二会话的前半段。Rust SDK 源码包在仓库外独立构建，以同一项目凭证续传后半段、封存并读取版本 1 的 MediaImportJobResult；两个资源的摘要一致，分页得到恰好两条，错误 kind／未知版本拒绝。SDK 传递依赖不含 SQLite、原生媒体、Worker 或插件运行时。

同包历史 17 组／16 格（v1–v8 × legacy/v2）通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-dj6clnhf/report.json`。既有发布 Journal、插件、权限、审计、MCP 会话／事件、预检、真实预览／导出／修复、独立 RGB/PCM 和停服重开回归通过。仅证明本机 macOS x86_64 开发包，不代表跨平台发行或全部目标完成。

早期候选 d1f263039922 的程序／SDK 动线已通过，但随包说明仍写 v7 升级目标；已修正为 v8 并加入导入／暂存边界说明后重新打包，归档保持不可变。它不作为本轮最终推荐包。

## 下一步

先补上传终态保留／重试／清理策略与可恢复磁盘配额，再接通项目范围插件授权和拒绝审计。随后按现有完整计划推进协作与选择性撤销、连续预览、素材理解、完整编辑与工程包、跨平台交付。已验的是上述指定平台和场景，不表示所有格式、所有宿主或无 bug 保证。
