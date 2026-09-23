# R2 HTTP / MCP 受限身份增量验收

2026-09-21。项目范围授权从 CLI/stdio MCP 延伸到 REST 和 Streamable HTTP。Web/GPUI 实现继续按用户要求暂缓。R2 与完整 R0–R7 尚未整体放行。

## 实现与契约

- HTTP middleware 验证 bearer 后写入可信 Identity；业务 handler 必须提取 AuthorizedHost，缺失身份拒绝，不使用全局 owner 兜底。每次 Automation 调用携带 Caller，actor 不接受客户端自报。
- MCP tools/resources/prompts/subscriptions 共用 EditorMcp。网络身份来自 SDK 传递的请求扩展；stdio 保留显式 Caller。会话绑定具体 credential 和 policy revision，同主体另一凭证或 owner 也不能接管 GET/POST/DELETE。
- policy 变化使旧会话失效，撤销凭证或禁用主体关闭既有 MCP 连接。后台每 250 ms 复查，清理 session、订阅及缓存访问资格；调度和数据库等待意味着这不是硬实时截止时间。
- SSE/NDJSON/产物响应逐块复查，空闲流也复查；下载路径显式标记，不以 MIME 决定授权。已发送的客户端／内核缓冲字节无法收回。普通 JSON 按读取时的资格返回，写操作仍在业务事务内检查。
- `POST /v1/changes/stream` 使用共享 EventChangesInput 与 StreamRecord，SSE event id 为持久化 cursor；Last-Event-ID 优先于 body cursor。初始失败在建立流前返回结构化错误，流断开不取消工作。该端点已纳入同源 OpenAPI。
- capabilities.access 广告四种受限传输、七类项目动作、会话绑定和流路径。旧 22 工具 Schema 保持，工具总数 31，数据库仍为 v5。
- 旧 raw Job HTTP／SSE／工具保持 owner-only；受限客户端使用公共 Job 工具。全局媒体、任意路径、Worker、权限管理和插件入口不因网络接入自动开放。

## 实际验证

最终 debug SHA-256：`c6dd1b036393bb7183b1d98f5f6edbb315f447f2fb379d2981c0c723c230c49d`。

`scripts/verify-network-access.py` 的 7 组真实流程全部通过。证据目录 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-network-access-unpkygyq`，含 report、trace、独立客户端结果及实际 workspace/产物：

1. 两工程、三主体、四凭证，24 次并发 HTTP 请求仍保持各自身份；跨项目／历史／全局媒体／路径任务／只读回执拒绝，伪造 actor header 无效。
2. MCP 工具、resource、prompt 和订阅继承范围；另一工程凭证、同主体第二凭证、owner 均无法接管原会话，原凭证继续可用。
3. 公共 SSE 与共享事件查询的 cursor／范围／记录一致，Last-Event-ID 恢复正确，越权过滤和旧 raw Job SSE 拒绝。
4. 固定修订 Job 通过真实 Worker/FFmpeg 生成 1920×1080 PNG，SHA-256 和独立 RGB 解码通过；任务列表隔离，缺 read_artifacts 和跨工程下载拒绝。
5. 仓库外已安装 OpenCut SDK 与官方 MCP SDK 1.26.0 真实调用通过，包括结构化拒绝、资源访问和产物下载。
6. 实际 TCP 小接收窗口造成背压，下载读取开始后撤销 read_artifacts，5,676,209 字节产物仅收到 118,784 字节即结束；同一实验将产物声明 MIME 故障注入为 application/json，仍执行复查。活动 SSE 结束，旧 MCP 会话／缓存无法重放。
7. 单凭证撤销终止已经打开的 MCP GET；第二凭证可用直至主体被禁用；无关工程及 owner 会话继续工作。

相关 Rust 72 项、Clippy `-D warnings`、TS SDK 5 项、纯依赖／原生边界、冻结 22 工具兼容与生成契约一致性通过。旧 owner HTTP 20、MCP HTTP 9 和 CLI/stdio 权限 7 组回归通过；日志位于 `/tmp/opencut-network-owner-http.log`、`/tmp/opencut-network-owner-mcp.log`、`/tmp/opencut-network-access-local.log`。

## 交付与剩余项

最终开发包：`dist/local/opencut-0.1.0-dev-931ad331d2c8-x86_64-apple-darwin.tar.gz`，仅 macOS x86_64 本机目标。先前已验包保留不覆盖。

- Archive SHA-256：`5adbb3acbbe37e78490c69936914df2fe1b4b02ae3b11d6bde4660188a90f8b5`。
- Release binary SHA-256：`c8bd0da308f7f8111e390cd5451f7371dbb22eb6e2e29b1d61449b9d66395514`。
- 仓库外 18 组交付验收通过，23 条记录命令全部 exit 0。证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-ssz_4w1d/report.json`，逐条命令见同目录 `commands.json`。
- 解包 release 重新通过网络权限 7 组、CLI/stdio 权限、来源追踪、事件流、MCP 上下文／网络及官方 SDK 验收。该次真实背压下载从 5,676,209 字节中收到 122,880 字节后因撤权结束，见 `network-access/trace.json`。
- TS npm 包真实独立安装、Rust SDK 源码包在仓库外构建；两者以同一受限主体 sdk-agent 完成计划／提交、跨语言幂等重放和新修订审计。owner 单独完成初始化和原生 probe，未向受限调用开放任意路径。交付 CLI 精确回读，停服重开状态保留。
- 交付 release 的冻结历史矩阵 14 组／10 格通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-hyflxyej/report.json`；真实上传、预览、导出、修复和独立 RGB/PCM 解码动线通过。
- 打包内容核查：安装说明和 npm README 写明受限网络授权，OpenAPI 含 33 条路径，全部本地 schema 引用可解析；诊断篡改检测通过。
- 同 binary 的中间包 dev-15feefb883d5 也通过 18 组验收；最终包补齐 SDK README 和 OpenAPI 权限说明后完整重验，当前推荐上方 dev-931ad331d2c8。包摘要没有被覆盖。

下一步仍需权限管理事件查询、插件／媒体维护审计、工程范围导入与受限预检／插件能力；R3 分支／选择性撤销、R4 持续预览、R5 分析、R6 完整编辑／插件／工程包、R7 其他平台和整体验收继续保留。本轮只证明 loopback 应用层授权，不是外部 OAuth、OS 沙箱或无 bug 声明。
