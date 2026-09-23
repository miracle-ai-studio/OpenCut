# R2 上传暂存发现、配额与释放验收 — 2026-09-21

## 范围与结论

本轮接通上传暂存的发现／查询／显式释放和逻辑字节预留，数据库升至 v9，46 工具、9 类任务描述、OpenAPI 49 路径。提供项目 8 GiB／工作区 32 GiB 的上传源字节预留；不把它称作 CAS、产物或物理磁盘总配额。网页／桌面继续只维护接入契约。

创建时预留完整声明容量，文件实际删除并同步之后才释放。终态暂存默认保留到显式 release；失败／取消任务保留历史，修正输入后用新的创建键重传。没有后台 TTL 自动回收或复用原失败会话的重跑能力。完整 [契约](../architecture/upload-staging-contract.md) 定义状态、权限、错误、恢复与边界。

## 架构与发现的缺口

- 纯 Application 定义 UploadRetentionStore、清理授权回调、分页游标及释放顺序。SQLite 只负责权威状态／配额／原子事件；Native Services 调用 Asset Store 删除私有分块，不在数据库写事务中执行文件 I/O。
- v9 新增 upload_retention，不修改 v8 三张上传／素材表。所有既有会话按声明大小保守回填，aborted 也不能未经现场清理就释放额度。继承的超限保留量允许查询和清理，阻止新增，旧数据和工程历史不改写。
- 清理原先只在等待文件锁之前授权，可能在排队期间撤权后仍删除内容。现为初始检查、独占锁内实际删除前复查、释放额度事务内复查；真实场景与故障端口均验证。
- 仅有按 ID 清理不能处理 Agent 丢失回执后的容量恢复，因此增加 opencut_upload_query。默认只列保留项，可切换全部历史；游标固定项目内创建范围与过滤条件，状态按每页读取时更新。查询不修补状态或删除文件。
- 上传组装持共享 OS 锁，删除与写块持独占锁。aborted／terminal 才允许清理；queued/running/cancel_requested 不被当作可清理终态。清理不删除 CAS／素材记录，也不移动工程 head。
- TS hostCapabilities()／Rust host_capabilities() 读取 typed capabilities resource；新增 staging 可选分支包含四个工具、预留限制及明确的非物理磁盘／非自动清理标记。旧 v1 capabilities Schema 保持冻结。

## 真实动线

最终 debug SHA-256：`49fb413c91a46065cafcaeb224fad5e59b7f5e28ea7451a4bda9abec88edd362`。

`scripts/verify-project-import.py` 共 14 组通过：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-project-import-b9pz483y/report.json`。除既有上传／导入／预览动线外，本轮重点验证：

1. CLI 清理真实成功导入的源分块，HTTP 精确重试没有额外写入；read-only 清理被拒绝，跨项目 quota/cursor 被拒绝。素材仍可被受限 Agent 绑定；原源与暂存都删除后，实际预览产物仍通过独立 RGB 解码。
2. 官方 MCP SDK 在 stdio 和 Streamable HTTP 查询暂存／配额／上传页，并释放各自实际导入的暂存；只读主体不能释放。
3. 无效字节经过真实 FFprobe 导入失败，仍保留失败终态；显式释放删除私有源分块，没有生成素材记录。
4. 对同一私有锁持 Python fcntl 独占锁，启动真实受限 CLI 清理；撤销该专用 credential 后解锁。CLI 返回 NOT_AUTHORIZED，私有目录仍存在，预留未释放；另一有效凭据可以继续清理。
5. 注入 release 审计拒绝触发器，创建八个声明 1 GiB 的会话并 abort。测试不写 8 GiB 媒体，验证的是逻辑预留；文件不存在仍不能在提交失败时释放额度。项目预留达到 8 GiB 后新增请求返回 UPLOAD_QUOTA_EXCEEDED，失败不占用幂等键。
6. 只读分页发现上述遗留会话；开始后释放一个旧会话并创建新会话。后续页跳过已释放记录、排除后来创建、无重复，过滤条件或项目不匹配的 cursor 明确失败。刷新后通过发现结果完成清理，保留页变空，全历史仍可查询，额度归零，之前失败的创建键可正常使用。
7. 既有顺序上传、3 MiB 原始块、断线续传、摘要／长度拒绝、16 请求上限、传输中撤权、来源审计和停服重开继续通过。

## 崩溃、事务与迁移

SQLite 79 项通过；原生发布 SIGKILL 1 项、上传原生恢复 5 项通过。新增释放 SIGKILL 覆盖文件已删除但额度未提交、额度已提交但响应未返回两个窗口：重试后额度正确，释放审计恰好一次。子测试入口由父进程显式启动，ignored 标记不是跳过场景。

端口／真实存储检查还覆盖：两个独立连接竞争工作区最后 1 GiB，只有一个新项目获得额度；清理审计失败保留额度；queued/running 拒绝清理，成功后清理保留素材和 head；删除前及删除后撤权的不同结果与 owner 接手恢复。

`scripts/verify-project-import-recovery.py` 另有四组通过：真实 Worker 进程组 SIGKILL、30 秒自然租约恢复、恰好一次素材／成功审计与旧输入清理，以及 queued 导入拒绝 release、显式取消后允许释放。源码阶段证据 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-import-recovery-6su5owrb/report.json`；最终交付须由解包 binary 复验。

历史 v1–v9 × legacy/v2 共 18 格、18 组通过：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-k00i6b9k/report.json`。新增独立 v8 fixture 中的 DDL 碰撞整体回滚；修正后将十个已 aborted 的 1 GiB 会话保守回填为超限保留量，阻止创建；释放三项后恢复创建，原工程快照保持一致。

## 契约和相关回归

相关 Rust 合计 118 项（SQLite／恢复 85，Application／Asset Store 12，Runtime／Native Services／Automation／CLI／SDK 21，含 compile-fail doctest）；TS 7 项及构建通过。Clippy -D warnings、fmt、纯依赖与原生／存储分层、生成一致性通过；HTTP 20 组、stdio MCP 19 组通过。

与已验 v8 包 c924e6d7272b 比较：全部 42 工具的输入及 read_only 不变，41 个输出 Schema 原样一致；resource_read 仅新增可选 uploads.staging 及其类型定义。原 22 工具＋两个媒体发布工具冻结门禁仍通过。

主要日志：`/tmp/opencut-staging-authority-tests.log`、`/tmp/opencut-staging-runtime-tests.log`、`/tmp/opencut-staging-authority-journey.log`、`/tmp/opencut-staging-authority-clippy.log`、`/tmp/opencut-staging-final-sdk-ts.log`、`/tmp/opencut-staging-final-wire.log`。

## 交付状态

最终交付 `dist/local/opencut-0.1.0-dev-46d52b41f70b-x86_64-apple-darwin.tar.gz` 已通过仓库外 30 组验收，32 条记录命令全部 exit 0，取代 c924e6d7272b 成为最近已验开发交付。Archive SHA-256：`2db9bee7a7d8318c80a768ebb0a243dfd35dc3d612342b93a925a9f59209b727`；release SHA-256：`8d96de6a92b25f44d835926491c1789133c171f03cc45904f6b6156e81947076`。中间候选 a00510a7f5c6 与 50aff261903b 未通过本轮最终放行，不作为推荐包；最终包已包含上传发现入口和锁内撤权修正，并通过仓库外双 SDK／实际媒体／迁移回归。

统一报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-395thoov/report.json`；同目录 commands.json 保存命令和完整输出，归档旁 `.acceptance.json` 保存摘要。

- 解包的 release 执行全部 14 组上传／暂存／撤权动线、4 组自然恢复／取消动线；自然租约恢复约 30.41 秒，attempt 2 成功，记录／清理保持一致。
- npm 包在仓库外实际安装，Rust SDK 源码包独立构建。TS 实际导入 WAV 后查询、释放暂存，读取预留 0 和默认上传页为空，全部历史仍有一条；Rust 接续另一个上传并成功导入，再读取 typed capabilities、释放、预留 0、保留页空、历史恰好两条，原素材仍可读。
- SDK 依赖树不含 SQLite、Worker、原生媒体或插件执行器。旧跨 SDK 编辑／幂等／审计、媒体／插件发布、权限／会话／事件／预检与实际预览／导出／修复／RGB/PCM 回归通过。
- 解包 release 的历史 18 组／18 格通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-0p1sw68c/report.json`。停服后独立 CLI 回读一致。

## 保留边界

上传预留不等于工作区物理磁盘配额；CAS、产物、历史引用与工程包的统一预算及可恢复 GC 继续归入后续工作。锁文件／数据库历史仍保留，未声称全量垃圾回收完成。项目插件授权、拒绝审计及完整 R3–R7 仍需推进；不把本轮局部交付当作 R2 或完整产品完成。
