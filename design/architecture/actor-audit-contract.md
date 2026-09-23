# 调用来源与持久化审计契约

本契约推进 R2/R3 的可信来源链。当前本机 OS 调用者及共享 bearer 代表 `local-owner` 权限主体，不声称识别了某个人或模型。Worker 的服务主体为 `local-runtime`。后续受限 principal/项目授权沿此边界接入，当前实现不得广告已有多用户权限。

## 来源与追踪

Host 构造不可从 JSON 反序列化的 ExecutionContext。source 为 cli/http/mcp_stdio/mcp_http/embedded/worker；正文、请求头、MCP clientInfo 中自报的 actor/principal/request_id 不改变它。Automation 在每次调用开始生成新的 request_id，同一个 ID 用于 Response 和本次提交的审计。

工程创建、apply/restore，以及 Job 接受、产生事件的生命周期/进度变更，在业务事务内写 audit_records。失败回滚不能留下成功审计；审计写失败必须阻止业务提交。幂等重试不重写原 actor，不产生另一条成功提交记录；重试响应有新的 request_id，原记录通过 transaction/job 查询。

Job 的接受记录建立 origin_request_id；后续 Worker claim/progress/complete/fail/recover 带自己的 request_id、attempt 和执行 worker_id，并关联原接受请求。旧任务没有原始来源时 origin_request_id 为 null，不虚构历史身份。取消记录使用取消请求自己的上下文，同时保留任务的原始接受请求。重复取消、纯 heartbeat、没有状态变化的重复操作不产生成功变更审计。

审计保留 project/revision/transaction/job/attempt/公开 artifact ID，不保存素材路径、完整请求、字幕、转写、错误诊断或凭据。产物 ID 来自成功公共 JobView 的真实引用，失败不能伪造成功产物。插件安装、媒体文件维护、授权拒绝日志和完整耗时指标不由本轮数据库变更审计替代，仍待后续接入。

## 数据与兼容

数据库升级为 v4，新增独立 audit_records 表，原工程、任务、回执及 outbox 格式不变；v1/v2/v3→v4 原子升级。历史记录不回填伪造 actor。旧版本不能打开 v4，回退须使用整个 workspace 备份。

旧 22 工具 Schema 不变。新增只读 `opencut_audit_query`，共享 CLI generic call、HTTP tools 路由、stdio/网络 MCP 及两种 SDK 的 typed registry。

```json
{"target":{"kind":"job","id":"job_example"},"after_sequence":"0","limit":100}
```

target 为 workspace/project/transaction/job/request；request 同时匹配 request_id 和 origin_request_id，因此可以追踪发起请求之后的 Worker 变化。after_sequence 为规范非负十进制字符串（SQLite i64 范围），limit 1–500。结果按 sequence 升序，带 last_sequence、has_more 及 `coverage: recorded_mutations_since_schema4`。空页保留请求的位置；客户端只在处理完成后推进位置，查询范围不是访问凭证。未来授权策略必须先验证目标权限，再执行本查询。

## 验收要求

真实 CLI/HTTP/两种 MCP 的响应 request_id 与提交记录一致；伪造头及正文不能获得身份；跨入口幂等重试保留第一次 actor；真实 Worker 产物关联接受请求及 attempt。数据库触发器故障覆盖创建、编辑、接受和完成，证明业务/审计同生共死。历史矩阵覆盖 v1–v4，并明确旧数据的未知来源。完整 principal/project 访问隔离仍需独立实现与验收。

## 后续授权集成约束（尚未实现）

- 审计日志不是授权数据库，也不能凭 audit 中的 project_id 或旧 Job 的自报输入授予访问权。主体和 grant 来自可信凭据解析及权威资源归属端口。
- 先校验当前 principal 的 action/resource 权限，再查幂等回执或返回历史结果；跨入口重试继续保留首次提交来源。权限不足不得借重试读取他人的结果。
- 同一策略需覆盖工具和 REST、MCP resources/prompts、Job/产物下载、分页事件及审计查询；列表过滤不能泄露未授权记录，URI/ID 不是 capability。
- 网络 MCP 会话须绑定已认证主体，不能让另一凭据仅凭 session ID 接管；订阅重新读取时也必须重新检查权限，不能永久保留连接建立时的旧授权。
- 受限主体不得直接提交任意宿主路径、安装可执行插件、维护全局媒体或运行 Worker。已接受任务的执行来源与当前观察者身份分别记录，撤销观察权限不自动等于取消任务。
