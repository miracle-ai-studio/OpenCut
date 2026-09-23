# 受限主体与资源授权契约

目标为 R2/R3/R7 共同的访问隔离，Web/GPUI 暂不实现界面。当前本机 owner 路径保持兼容；接入受限凭据时必须走同一 Application 策略，不能根据传输或客户端自报 actor 提权。本文件先冻结权限语义，各入口实际完成情况由 QA 记录证明，不能仅凭文档视为实现。

## 主体、凭据与权限

principal 是 workspace 内的稳定标识。一个 principal 可有多个可单独撤销的随机 bearer credential；持久化只保存 SHA-256 摘要和随机 credential ID，不保存明文 secret。secret 仅在签发时交付。项目 grant 显式列出 read/edit/restore/run_jobs/cancel_jobs/read_artifacts/read_audit，不做字符串通配；除 read 外的权限须同时具备 read。owner 可以创建/更新 policy、签发与撤销 credential，受限主体不能给自己授权。

策略修改采用 expected policy revision，递增版本；旧版本修改返回冲突。新项目不自动继承现有 principal 的权限。禁用 principal 使全部 credential 无效；单凭 credential ID、principal ID、request_id、Job ID 或 URI 不能认证。随机 secret 通过宿主验证后生成不可由 JSON 反序列化的 Caller，actor 来自该 Caller，source 由入口确定。

每次操作重新读取 credential 状态及当前 grant。只读请求在权限检查时获得该次读取资格；权限撤销阻止之后的新读取。编辑、恢复、任务接受及取消在业务写事务内再次检查当前权限。幂等重试也先检查权限，不允许用已知 key 窃取回执或已完成结果；重试保留第一次提交的 actor，不改变既有幂等作用域。

## 资源范围

工程查询与编辑只在已授权 project 内执行。列表、事件、审计和分页在存储读取时过滤；分页边界不能从未授权 Job 派生。固定 revision 必须属于已授权工程。

Job 归属由提交时验证过的固定 project/revision 写入独立关系表，与 Job/回执/outbox 原子提交。旧 Job 仅在其保存的 snapshot 与权威历史修订一致时才能建立归属，否则只允许 owner 访问。不从审计记录、自报 input.project_id 或任意 URI 推断权限。产物继承 Job 的工程范围，并要求 read_artifacts。

受限主体不能运行 Worker、启动 owner 宿主、管理全局媒体或可执行插件，也不能提交 media.probe/preview.frame/render.export/artifact.verify 的任意本机路径。受限编辑不能通过 asset.bind/replace 引入路径；素材导入后续需使用明确的工程范围与宿主授予的资源引用。已有工程中由 owner 绑定的素材可按 run_jobs 权限生成固定修订产物。

## 入口及生命周期

CLI 的凭据文件与 HTTP bearer、MCP 会话认证最终生成同一 Caller；不得在一条受限调用链中退回默认 owner。MCP HTTP 会话须绑定 principal/credential，知道 session ID 不能接管。资源读取、prompt 上下文与订阅轮询重新鉴权；撤权后不再发布内容。已发出的字节无法收回，观察权限撤销不等于取消已经接受的工作。

本机 owner CLI 和对数据库文件具有写权限的 OS 进程位于信任边界内；本机制不是 OS 沙箱，不能阻止直接篡改 SQLite 的本机进程。远程 OAuth、跨主机传输和外部 Provider 授权仍需相应后续实现，loopback 绑定保持。

## 验收门槛

两个独立 project、只读和可编辑主体、多个 credential、旧回执、固定修订 Job、真实产物与事件共同验证。必须覆盖跨项目拒绝、猜测 ID、原生路径注入、范围内成功、撤销/禁用、准备期间撤权、并发 policy 更新、旧 Job 归属、列表/游标过滤和 HTTP/MCP/CLI 一致性。新增入口在实际接通前保持拒绝受限凭据，不得临时回落 owner。

## 当前接通边界（2026-09-21）

已接通原生 SQLite、Automation、CLI、stdio MCP、REST HTTP 与 MCP Streamable HTTP。CLI 无 `--credential-file` 时代表本机 OS owner；受限调用指定该参数。HTTP 两入口使用签发凭证作为 bearer，逐请求解析到可信 Caller；缺少身份上下文明确拒绝。owner bearer 保留兼容。实际验收见 [网络权限 QA](../qa/r2-network-access-2026-09-21.md)。

CLI 管理命令：`access set-policy --input policy.json`、`access get-policy PRINCIPAL`、`access issue PRINCIPAL --output NEW_FILE`、`access revoke CREDENTIAL_ID`、`access inspect`。policy 的 `expected_revision` 为规范十进制字符串，创建时用 `"0"`。签发文件必须不存在，Unix 权限为 0600；stdout 只返回 credential ID 和文件位置。管理事件与策略／凭证状态原子保存于 access_events，现在通过 owner-only `opencut_access_audit_query` 或 `access events --input FILE` 查询。

例如仅授予已有工程 `demo` 的读取权：

```json
{"principal_id":"reviewer","expected_revision":"0","enabled":true,"grants":[{"project_id":"demo","actions":["read"]}]}
```

```sh
opencut --database workspace.sqlite access set-policy --input policy.json
opencut --database workspace.sqlite access issue reviewer --output reviewer.secret
opencut --database workspace.sqlite --credential-file reviewer.secret project list
opencut --database workspace.sqlite --credential-file reviewer.secret mcp serve
```

受限 CLI `job start/get/cancel/list/wait/events` 返回公共 JobView／JobPage，owner 保留原有 v1 形状；`job list` 返回第一页，后续用 `opencut_job_query` 和 next_cursor 续查。原有 raw job 工具仍仅供 owner。JobView 中的产物引用不授予读取权，读取独立 artifact resource 要求 read_artifacts；网络字节下载同样要求 read_artifacts，并在流式传输中复查。受限 render preflight 已按 read + run_jobs 开放，限固定修订中的锁定内容；诊断去除原生路径，详见 [预检契约](scoped-preflight-contract.md)。插件执行仍保持拒绝。

数据库 schema 5 原子升级，旧 owner 工作区无需创建 policy；未知归属任务不会自动授权。v4 及更早程序不能打开已升级的库，本阶段不提供降级迁移。

## 网络会话与流契约

MCP HTTP session 绑定具体 credential 和初始化时的 policy revision。另一凭证（包括同 principal 的第二凭证或 owner）不能 GET/POST/DELETE 该 session；错误使用不会关闭原 session。策略版本变化使原会话失效，即使只是增加权限，也须重新 initialize。后台每 250 ms 检查受限会话；禁用主体或撤销 credential 会关闭已有连接并清理订阅。旧会话请求返回 403，已清理的会话返回 404；回放缓存同样受会话身份约束。

HTTP SSE、NDJSON 和产物字节响应逐块复查凭证及 policy revision，空闲时也每 250 ms 检查；失败以 EOF 结束，客户端应按未完成传输处理。产物路径显式启用该检查，不依赖 MIME（包括 application/json）。已交给客户端或内核缓冲区的字节不能收回，250 ms 是检查间隔而非调度或数据库等待下的硬实时保证。普通 JSON 控制响应按本次读取资格返回；修改事务仍按上文规则在提交内检查。

`POST /v1/changes/stream` 接收共享 EventChangesInput，输出 SSE ready/event/error，data 为 `opencut.events.v1` StreamRecord；每条 event 的 id 为持久化 opaque cursor。Last-Event-ID 优先于 body cursor，便于重连；二者均不是 MCP 的短期回放 ID。首批权限或输入错误在建立流前返回 JSON 错误。工程和任务范围与 CLI/工具查询一致，流断开不取消任务。该端点纳入同源 OpenAPI。

旧 raw Job REST、原始 Job SSE 和原始 Job 工具仍仅供 owner。受限客户端使用 `/v1/tools/opencut_job_*` 公共视图工具以及上述公共事件流。capabilities.access 广告支持的传输、项目动作、会话绑定及流路径；权限管理仍是 local_owner_cli。全局素材管理、受限工程导入、受限插件和管理写入不由网络接入自动开放。

## 权限管理历史查询

`opencut_access_audit_query` 为只读工具，CLI/REST/两种 MCP 和两个 SDK 共用纯 DTO 与 Application 查询用例。只有 owner 可读；项目 read_audit 只覆盖工程／任务来源，不能读取全局权限历史。target 为 workspace、principal、credential 或 request（后三者使用 id），支持 after_sequence（默认 "0"）与 limit（1–500，默认 100）。无匹配记录返回空页。credential 目标只包含该 credential 的签发／撤销；主体策略更新用 principal 查询。

按独立 access_events 序号升序查询；该序号是规范非负 i64 十进制字符串，不能与审计／事件／MCP cursor 混用。响应提供 last_sequence 与 has_more，空页保留输入位置。记录总编码大小最多 4,000,000 字节，预留 envelope 空间；页可以未满 limit 即终止，续查不能跳过尚未返回的候选记录。SQLite 在同一只读快照中按需逐行访问，不先加载全部历史或整页大型策略。

record 含 action、principal/credential ID、request_id、可信 actor、提交时 policy 快照和 UTC created_at；action 为 policy.updated / credential.issued / credential.revoked。policy 为更新后的 PrincipalView，其他动作无 policy。secret 和摘要不进入输出。查询不产生管理事件。覆盖仅限 schema 5 以来已提交的管理操作；原记录不重写，不把拒绝的登录／失败修改／幂等撤销虚构为成功记录。业务变更与管理事件插入仍在同一写事务，事件插入失败全部回滚。
