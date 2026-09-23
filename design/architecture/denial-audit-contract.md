# 拒绝审计契约

2026-09-21 代码及本地真实动线已验，发行状态另行记录。成功事务审计和权限管理历史保持原协议；新增拒绝历史不写入工程 outbox、不产生 revision。

## 记录来源与隐私

数据库 v11 denial_records 接收可信 Host 在拒绝操作退出／回滚后的独立追加。入口包括 CLI／Automation、HTTP 认证和 Origin、工具、媒体／上传／下载、MCP 工具／资源、会话和持续响应的撤权。HTTP 请求只在应用路由层开始记录，不承诺 TCP／HTTP 解析器之前的网络审计；直接调用 SQLite port 的嵌入者须使用 Host 的报告接口。

每条记录包含 sequence、服务端 request_id、CallSource、可选可信 principal_id／credential_id、固定动作分类、拒绝原因和服务端时间。不存 token／token 摘要、密码、请求体、参数、原始 URL／路径、Origin 文本、MCP session ID、客户端 actor 或 request_id 声明。未建立身份的认证／Origin 拒绝为匿名，不凭客户端声明猜测用户。曾通过验证、随后被撤销的身份仍可作为本次拒绝的来源。

动作包含注册工具名；未知工具统一记 unknown_tool，不能把任意工具名当作可信日志文本。传输拒绝按 authentication/http_request/mcp_request/stream 等固定分类记录。MCP resource/prompt 通过共享资源查询记录，并携带实际 mcp_stdio/mcp_http 来源，不再默认为 embedded。

## 结果关联、幂等与失败

相同服务端 request_id 至多一条；首条可信观察为准。外层适配器观察到同一个 403 不再产生第二条或消耗序号。HTTP `X-OpenCut-Request-Id` 由服务端设置；REST 工具响应头与 JSON 信封 request_id 相同；MCP HTTP 传输请求 ID 与内部工具调用 ID 分别关联各自边界。MCP 工具使用 structuredContent.request_id，资源错误在 JSON-RPC error.data 返回 request_id。CLI NDJSON 的拒绝 error.details 包含 request_id；后台 session 撤权没有正在返回的请求，使用新的服务端观察 ID，可按 credential／principal 查询。

拒绝日志失败不能放行原操作，也不能伪称已经记录：JSON 信封 warnings 添加一次 `DENIAL_AUDIT_UNAVAILABLE`；HTTP 返回同名 `X-OpenCut-Audit-Warning`。已发出头部的流或后台 session 只能关闭流并将固定 warning 和服务端请求 ID 写到诊断 stderr。拒绝码保持原分类，记录失败不递归调用审计。不会因记录失败重试原编辑。

成功请求不记录到拒绝历史。工具 read_only 标记继续表示不修改工程和业务资源，不排除失败时 Host 的审计副作用。权限与审计查询都不能绕过实时 credential 校验。

## 保留和查询

单工作区最多保留 10000 条；新追加、去重检查和按 sequence 淘汰同一短 SQLite 事务完成。淘汰失败回滚新记录。无后台定时器；升级 v10→v11 创建空表，不回填无法证明的历史拒绝。这是逻辑行数上限，不是 SQLite/WAL／物理磁盘字节配额。

Owner-only `opencut_denial_query` 输入 target(workspace/request/principal/credential)、规范十进制 after_sequence（默认 0）、limit（1–200，默认100）。输出 records、last_sequence、has_more、oldest_available_sequence、max_retained_records 和 coverage=`bounded_host_denials_since_schema11`。按同一读快照升序分页；过滤不会暴露其他主体给受限 Agent，因为受限 Agent 不得查询此历史。拒绝查询本身也产生一条 Host 拒绝记录。

非零游标早于最早保留序号减一时返回 CURSOR_EXPIRED，调用者从 0 重新发现并显式处理历史缺口；不能声称保留窗外完整无丢失。新请求不可无限扩大历史。实时鉴权与记录发生在不同短事务，记录不会复活已回滚操作或持有原生文件锁。

## 撤权后的观察状态

HTTP 持续响应继续在字节块和空闲周期重验身份，拒绝记录沿用响应的请求 ID，已进入客户端缓冲的字节不能追回。MCP 资源订阅在首次权限失败后报告更新并停止该观察任务，避免每 250 ms 重复失败与审计洪泛；权限恢复后需 unsubscribe/subscribe 或重新连接。后台 MCP session 关闭记录固定动作和 policy_changed；它不是编辑撤销或持久 Job 取消。

网页／桌面只维护协议约定：展示原拒绝与记录降级信息，Owner 可按 request_id 查询；禁止把匿名失败归因给某个用户、把审计落库当作原写入成功，或自动重发失败写入。
