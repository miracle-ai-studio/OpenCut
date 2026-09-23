# R2 拒绝审计验收 — 2026-09-21

## 本轮交付范围

新增 Owner-only `opencut_denial_query`，统一记录共享工具与提前结束的传输拒绝。数据库 v11，51 个工具／9 类任务描述／OpenAPI 54 路径。[拒绝审计契约](../architecture/denial-audit-contract.md) 明确保留、身份、请求关联、失败降级和覆盖边界。Web/桌面只维护接入约定。

成功编辑／任务的现有审计保持独立；拒绝日志不修改工程 head／outbox，不把“日志写入成功”解释成原操作成功。直接 SQLite port 调用方需要通过 Host 报告接口接入，不声称网络解析器之前或操作系统级事件均被捕获。

## 逻辑审查与修正

- 原 Automation 只返回 NOT_AUTHORIZED/PERMISSION_DENIED，HTTP 认证、媒体路径、流式撤权、CLI 管理／观察也可能提前返回。现在共用纯 Application DenialStore／DTO 和 SQLite 有界适配器，传输只报告可信元数据。
- request_id 使用服务端 UUID，主键去重保留第一次观察；HTTP 的外层 403 不能重复记入工具内部已经记录的拒绝。输入中的 actor/request_id、原始未知工具名、URL、参数和 token 不进表。
- 匿名认证失败不推断身份；已通过认证后被撤权的调用保留可信 principal/credential。MCP resource/prompt 现在传入真实 mcp_stdio/mcp_http 来源；不再被记为 embedded。
- 日志追加、去重和保留淘汰处在独立短事务，原拒绝操作已经退出／回滚。日志失败保持原拒绝，返回一次 DENIAL_AUDIT_UNAVAILABLE；实际验收发现多层可能重复追加 warning，已修复并重跑。
- MCP metadata 请求重验受限身份；Owner 的纯 metadata 发现保留无额外存储行为。资源订阅首次撤权后通知并停止该观察任务，避免每 250 ms 重复拒绝与写日志。
- 上限为工作区 10000 条记录；不是物理磁盘、WAL 或永久安全归档配额。过期非零游标返回 CURSOR_EXPIRED，Owner 可从 0 重新发现最早保留位置。

## 真实公共入口

`scripts/verify-denials.py` 在真实二进制和新工作区完成七组：

1. CLI 工具、未知工具、跨项目、受限查询审计、Owner 管理以及无效 credential file；每次拒绝可按真实 request_id 查询，业务工程未变。
2. HTTP 工具拒绝经过两层仍只有一条；客户端伪造请求头无效。缺失／错误 bearer、Origin、全局素材读取、MCP Host 拒绝都记录正确来源，匿名路径不冒充用户。
3. 真实 SQLite 触发器拒绝日志插入；CLI warning 和 HTTP body/header 明确降级，原操作仍被拒绝，行数和工程状态没有伪成功。撤去触发器后恢复正常。
4. 外部官方 MCP SDK 在 stdio 与 HTTP 验证工具／资源错误的请求关联和来源。stdio 另以独立 credential 订阅资源后撤权，metadata 立即拒绝，订阅通知一次后停止；继续等待时拒绝记录不继续增长。
5. 实际空闲公共 SSE 的 credential 被撤销，连接关闭；原响应头 request_id 恰好对应一条 policy_changed/stream 记录。
6. Owner 按 request/principal/credential 查询与分页；直接检查数据库记录不含真实 token、请求体标记、伪 actor/request_id 或原始 URL。
7. 独立构造超过保留上限的历史，再通过公开拒绝触发淘汰；保留恰为 10000，旧游标显式过期，新拒绝可被查到；服务停机重开后工程和日志一致。

开发动线报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-denials-46aoclkh/report.json`；同目录 trace.json／mcp.stdout／host.log。该次开发报告没有记录二进制指纹，最终交付需以解包 release 重跑的带指纹报告为准。

失败记录保留：`opencut-denials-dg1dikd8`／`opencut-denials-1hq2htd4`／`opencut-denials-erxv2o2j`。其中包含已修复的重复 warning 问题，以及 QA 重用 credential 输出文件、SSE 解析器对字段顺序假设的夹具问题；失败运行不计入通过。

## 迁移与回归

- 冻结 v1–v11 × legacy/v2 的 22 格矩阵、20 组检查通过；v10 的 denial_records 表名碰撞使整次迁移回滚，修复后重试不改历史、不伪造过去拒绝。报告：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-cxyrn0ly/report.json`。
- SQLite、Automation、CLI 及真实发布／上传崩溃恢复合计 100 项通过。三个新存储场景覆盖同一请求重复／并发观察、可信动作校验、Owner-only 查询、淘汰失败的完整回滚。
- TS SDK 八项通过。Clippy all-targets/-D warnings、fmt、纯契约／Application／存储和原生分层、生成漂移门禁通过。
- 原 22 工具及两个媒体发布工具冻结兼容检查通过。与已验 v10 包 dev-7e6beabc18df 的 50 工具比较：输入与 read_only 全部不变，49 个输出原样不变；resource_read 仅新增可选 denials 能力。
- 最终 debug 的只读能力七组通过，数据库不变，没有创建插件／媒体／素材目录。二进制 SHA `5102e115b036b8a03a31bdd969d149f0b60752d451cae2640555bdcbf6e44c53`；证据 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-read-only-58emj1ml`。

补充修正：提前 HTTP 拒绝的审计降级可能只在响应头，两种 SDK 现均合并到 API 错误 warnings，并与已有信封 warning 去重。TS 同时覆盖只有头部与头部／信封并存的场景；最终仓库外验收已注入真实日志写入故障，两种已安装 SDK 均收到一次明确 warning。

主要日志：`/tmp/opencut-denials-{journey4,tests,history,clippy2,wire,boundaries,sdk,readonly,drift}.log`。

## 交付状态

新包 `dist/local/opencut-0.1.0-dev-7d1c8c705b60-x86_64-apple-darwin.tar.gz` 已通过仓库外 **35 组验收，36 条记录命令全部 exit 0**。

- archive SHA-256：`1cbc44e9bd9fef89a4695000b40c9a9ac02b49a67f4e8f64b0b3194ade5e2910`。
- release SHA-256：`63559a7de0aaaa0c65a51817f56ab25b6186de0ab6e21d36195b9453c7e79851`。
- 报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-lda_2gop/report.json`；commands.json 记录全部命令和输出；归档旁 `.acceptance.json` 保存验收结果。
- 解包二进制真实重跑拒绝审计七组，报告 `denials/report.json` 记录上述 release 指纹；同一包的项目插件六组、项目导入十四组、Worker SIGKILL／自然租约恢复四组、媒体与插件发布、作用域权限、SSE、官方 MCP SDK 和实际 RGB／PCM 导出全部通过。
- npm 独立安装的 TS SDK 返回拒绝 requestId；Owner 能查询对应 typed DenialPage。独立源码安装的 Rust SDK 查询同一记录得到一致结果，自己的受限审计查询也被拒绝并准确记录。无原生执行／数据库依赖进入 SDK。
- 另对同一实际宿主注入拒绝日志插入故障，用错误 bearer 触发 HTTP 认证提前拒绝；TS 和 Rust SDK 均从只在 HTTP 头部的降级信息得到一次 DENIAL_AUDIT_UNAVAILABLE。恢复触发器后继续 CLI 回读、媒体导出、停止服务与重开。
- release 的历史报告：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-ulqinjvo/report.json`，22 格／20 组通过。

此前 dev-7e6beabc18df 已验归档保持不变。中间候选 dev-ad9c88298056 未包含 SDK 头部 warning 合并修正，没有接受或推荐。平台范围仍为本机 macOS x86_64 开发交付；其他平台、公开分发和签名／公证不在本轮已验范围。

## 尚未完成

完整物理磁盘／CAS／产物治理仍未完成，R2 不整体放行。R3 选择性撤销／分支、人机协作，R4 持续预览，R5 真实分析，R6 完整编辑／插件类别／工程包，R7 平台与整体验收继续执行；Web/桌面按用户要求暂缓实现。此处只对明确版本、平台和场景提供通过证据，不声明产品无 bug。
