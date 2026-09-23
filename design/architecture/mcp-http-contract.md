# MCP Streamable HTTP 契约

2026-09-21。协议固定为 `2025-11-25`。`serve` 在同一 loopback HTTP 宿主上提供 `/mcp`、原有 `/v1` API 和持久化 Worker；`mcp serve --transport streamable-http` 是同一宿主的入口别名。默认 `mcp serve` 仍是 stdio。

## 分层与权威

`mcp_http` 负责 HTTP framing、鉴权后的协议头检查、协商、配额与回放响应；`mcp_sessions` 将 SDK 流绑定到会话；`mcp_replay` 管理有界内存回放。MCP 工具发现仅对根 Schema 补 object 类型，并把顶层 property 的 true/false Schema 转为等价的 `{}`/`{not:{}}`，兼容官方客户端的工具解析；原始 v1 Schema 保持不变。传输模块不持有编辑规则。网络和 stdio 均实例化同一 `EditorMcp`，共享 tools、resources、prompts 和 Application 用例；31 个工具和 8 类 Job 不因 transport 分叉。

`system capabilities` 的 `mcp` 字段公开版本、两种 transport、`/mcp` 路径、`authenticated_loopback` 范围及会话/资源订阅上限。v1 capabilities 工具保持冻结、允许扩展字段的旧 Schema；新增 mcp 字段随 JSON 输出，完整扩展 DTO 的类型由 `opencut_resource_read` 的 capabilities 资源 Schema 提供。会话不是 principal，不能拿会话 ID 替代 bearer。

## 连接与请求

```sh
opencut --database /your/workspace/workspace.sqlite mcp serve \
  --transport streamable-http --listen 127.0.0.1:8788 \
  --token-file /your/workspace/api-token --session-timeout-seconds 300
```

所有业务方法 GET/POST/DELETE 都要求 `Authorization: Bearer <token>`。浏览器 Origin 必须出现在 `--allow-origin` 中；无 Origin 的本机客户端允许接入。Host 必须为 localhost 或 loopback IP，拒绝重复关键协议头及重复 Authorization/Origin。已允许 Origin 的 OPTIONS 可无 token；CORS 公开会话和协议响应头。

初始化 POST `/mcp`：Content-Type 为 application/json，Accept 同时包含 application/json 和 text/event-stream。发送 JSON-RPC initialize 后，读取响应的 `Mcp-Session-Id` 和 negotiated protocolVersion。后续每个请求携带该会话头及 `Mcp-Protocol-Version: 2025-11-25`。客户端请求其他版本时，初始化无版本头可协商为 2025-11-25；后续不支持的版本返回 400。初始化头和 body 版本不一致也返回 400。宿主不会因为 SDK 认识更新协议而自动切入未实现的无状态传输。

通知接受后返回 202 空 body；请求按 SDK framing 返回 JSON 或 SSE。GET 需要 Accept text/event-stream，承载独立服务器通知。单个 POST body 上限 2 MiB，包含 chunked body；超限 413。资源自身还有独立的逻辑 JSON 2 MiB 限制。

## 会话与退出

最多 32 个会话，每会话最多 32 个资源订阅；初始化准入串行检查防止并发穿透。超限返回 429，结束会话可复用槽位。每个会话有独立 EditorMcp/订阅集合。GET 或 POST 连接断开不自动删除会话，不等于取消编辑或 Job。

DELETE `/mcp` 结束当前会话及其订阅；以后使用其 ID 返回 404，其他会话继续有效。未初始化会话等待上限 10 秒。SDK 会话空闲期限默认 300 秒，`serve --mcp-session-timeout-seconds` 或别名的 `--session-timeout-seconds` 可设 1–3600 秒；收发协议活动会影响空闲判断，持续通知不等于空闲。进程重启全部会话失效，但持久化工程/Job 保留。

SIGINT 停止新准入、关闭 MCP SSE 和会话，并最多等 5 秒排空 HTTP 连接，未结束的 chunked request 不会无限卡住网络退出。Worker 仍按自身停止/恢复规则收尾；这不是所有媒体计算都在 5 秒内结束的承诺。

## 回放与一致性

SSE `id` 为不可解释的 `mcp-event:<uuid>`；它与 Application 持久化事件 cursor 不互换。事件入回放缓存后才发布。每条记录同时绑定真实会话和原始请求流，不能将一个 POST 的响应广播到其他 POST 或公共通知流。

客户端在同一会话 GET `/mcp` 时发送 `Last-Event-ID`，只得到该原始流在锚点之后的消息，保留原始 event ID。这是有限快照回放：响应末尾发送带最后 cursor 的空 data checkpoint 和 `retry: 1000` 并关闭；客户端保存最后处理的 ID，遵循重连间隔继续获取新消息。只收到 priming ID、尚未看到业务响应时，也可据此找回随后完成的响应。断开 POST 不代表写入没有发生；未知结果应以原 revision/幂等键重试或查询权威状态。

缓存为进程内、全局最多 2048 记录、每流最多 64 记录、逻辑序列化数据最多 32 MiB、保存最多 60 秒；任一限制都可能先淘汰锚点。有效但属于其他会话的 ID 返回 400/INVALID_CURSOR；未知、淘汰或过期锚点返回 410/CURSOR_EXPIRED，绝不把缺口当作空成功。已失效会话先返回 404。超限 event ID 拒绝；响应禁止缓存。重连 410 后重新读取权威资源，可靠历史补读使用 `opencut_event_changes`。资源更新通知可以合并中间变化，不能用它还原全部编辑历史。

## 验收与范围

`scripts/verify-mcp-http.py --binary <opencut>` 实际启动宿主，覆盖认证/Origin/Host、协商、真实编辑/CLI 回读、丢弃 POST 响应后回放、跨会话拒绝、通知溢出、删除/超时、配额和有活动连接的停机。包验收根据交付二进制能力自动运行它。另在仓库外安装 `@modelcontextprotocol/sdk@1.26.0`，执行 `node scripts/verify-mcp-client.mjs --binary <opencut> --sdk-root <外部 npm prefix>`，验证官方 SDK 发现、断开 POST 后自动回放、资源订阅和 DELETE；包验收可加 `--mcp-sdk-root` 合并此证据。官方 AJV 对 Rust 的 uint/float 等非标准 format 输出忽略提示，基础 type/范围/必填验证仍执行，这些提示不代表执行错误。

当前支持本机 owner bearer 及项目范围凭证；actor 持久化链已接通。受限 session 绑定具体 credential 和 policy revision，策略变化或凭证撤销使原 session 失效，详见 [授权契约](access-control-contract.md)。远程 OAuth 尚未实现。该 transport 不构成 R2–R7 整体放行；网页和 GPUI 的实现及实际 UI 验收继续暂缓。stdio 上下文语义见 [共享 MCP 上下文契约](mcp-context-contract.md)。
