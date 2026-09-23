# R2 Streamable HTTP 与客户端互通验收

2026-09-21。本轮实现了 `/mcp` 网络入口，复用 stdio 的工具、资源、提示和订阅用例；具体边界见 [协议契约](../architecture/mcp-http-contract.md)。这是 R2 增量，不是 R0–R7 整体完成声明。

## 真实进程验收

最终 debug 候选 SHA-256：`c0b0c670f6fa14e626ec36e456c90bdb5fe8a25e260ee3d082e07522bed516af`。使用不可变的临时拷贝运行回归，避免构建期间二进制变化影响证据。

| 动线 | 结果 | 关键观察 |
|---|---|---|
| Streamable HTTP | 9 组通过 | bearer/Origin/Host、头重复、协商、Accept、Content-Length/chunked 超限；真实创建后 CLI 回读；丢弃 POST 后找回响应；跨会话拒绝、溢出 410；DELETE、空闲、32 会话准入；带活动 SSE/未完成 body 的 SIGINT |
| stdio 资源与提示 | 11 组通过 | 同源资源、固定修订提示、独立连接、真实产物及失败 Job、资源大小/订阅上限和慢管道 EOF |
| stdio 工具 | 19 组通过 | 29 工具发现、计划/提交/幂等、冲突、历史和实际 CLI 回读 |
| 原有 HTTP | 20 组通过 | 鉴权、资源/事件查询一致性、编辑和固定修订、SSE 断点 |
| CLI 事件流 | 11 组通过 | 逐条续读、不跳不重、范围拒绝、超时/Ctrl-C、等待与取消分离、慢读与 broken pipe |
| 官方 MCP TypeScript SDK 1.26.0 | 4 组通过 | 仓库外安装真实 SDK，工具/资源/提示发现；用真实 SQLite 写锁延迟提交，仅交付 priming 后切断 POST，由 SDK 自己跨多次空回放续接恢复提交结果；独立 CLI 编辑触发订阅通知；SDK DELETE 后 404 |

命令记录保存在 `/tmp/opencut-streamable-final-{mcp-http,mcp-context,mcp,http,event-streams}.json`，各自指向原始临时证据目录；官方 SDK 结果在 `/tmp/opencut-streamable-interop.json`，含版本、二进制哈希、检查和证据路径。临时目录不保证长期保留，最终交付包另附机器可读 acceptance 文件。

## 验收发现并修复的问题

1. SDK 在协议协商前依据客户端请求版本选择 transport。客户端请求更新版本时，本服务虽协商 2025-11-25，却可能丢失有状态会话语义。入口现在在 SDK 分支选择前绑定受支持版本，并检查初始化头/body 一致性。
2. SDK 的请求流 ID 不含真实 session ID；按字符串猜测会话所有者不可行。每个会话使用绑定 owner 的事件适配器，回放同时限制 session 与原始 stream。
3. SDK 默认回放无法明确区分过期缺口。新增有界缓存及 400/410 语义，独立验证跨会话、其他 POST 流及记录淘汰。
4. 官方 TypeScript SDK 在 tools/list 阶段拒绝两个缺根 object 类型的 union Schema，并拒绝顶层参数的布尔 Schema。MCP 适配层现在输出等价表示，核心 Schema 不改。该问题由独立客户端发现，原有原始 JSON 协议脚本不能单独证明互通。
5. 扩展能力描述最初改变了冻结 v1 capabilities 输出 Schema。已恢复旧 DTO/Schema，扩展内容使用 HostCapabilities 和新增资源的类型；旧 22 工具兼容门禁通过。
6. 持续 SSE 或未完成 chunked 请求可能阻碍 graceful drain。会话取消及有界 HTTP 排空已落实；不能将网络排空上限误写为全部 Worker 计算的停止上限。

7. 实际写锁延迟暴露空回放丢游标：SDK 在新 SSE 流中不保留前一个解析器的 ID，而 Axum 的空 data 不输出 data 行。已改成精确编码含最后 cursor 和单个空 data 字段的 checkpoint。修复前官方 SDK 实际超时 15 秒；修复后必须跨多次空回放得到原响应且 SDK 错误列表为空。新测试同时检查原始 SSE 报文，避免只检查最终写入成功。

官方 SDK 的 AJV 会提示忽略 Rust uint/float 等非标准 format；互通运行的 SDK 错误列表为空，仍执行 Schema 的基本类型、范围及必填校验。这些提示未通过关闭 Schema 验证来隐藏。

## 契约与代码检查

CLI、API contracts、Automation 的 Clippy `-D warnings` 通过；生成契约一致性、旧 22 工具 Schema/read_only 不变、纯依赖/源码边界门禁通过。TS SDK 五项检查通过。受影响的 Rust 检查 61 项通过（CLI/Automation/SDK 9 项，Application/SQLite 52 项）。完整工作区 rustfmt 会报告已有桌面文件格式差异；未修改暂缓范围的桌面源码，受影响包格式检查通过。

## 交付与剩余范围

新版 release 包正在构建/独立验收；本段将在拿到实际交付证据后更新。不能用此前开发包或 debug 通过结果代替本次 release 验收。

本机共享 bearer 不是多用户 principal。actor 来源、项目授权和持久化追踪仍是 R2/R3/R7 的待办。R3 分支/选择性撤销、R4 持续预览、R5 分析、R6 完整编辑/工程包、R7 平台和整体验收继续有效。Web/GPUI 实现和真实 UI 按用户要求暂缓。
