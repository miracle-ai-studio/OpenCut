# 可恢复事件观察契约

2026-09-21，增量契约 `opencut.events.v1`。实现文件分别为 `api-contracts::event_stream`、`editor-application::events`、`sqlite-store::sqlite_queries` 与 CLI `event_streams`。客户端观察状态不持有编辑会话，也不隐式修改任务。

## 权威、分层与分页

`opencut_event_changes` 是注册表中的只读工具，HTTP `/v1/tools/opencut_event_changes`、MCP tools/call 与双 SDK typed Tool 使用同一个 Application 查询。CLI NDJSON 也调用该查询。DTO/Schema 在纯契约层，游标与过滤规则在 Application，SQLite 适配负责同一读事务内检查 anchor 与读取下一页。没有为 CLI 创建另一套事件表或任务状态机，也没有更改 schema v3。

请求：

```json
{"scope":{"kind":"projects","project_id":"project-id"},"cursor":null,"limit":100}
```

`scope.kind` 为 `projects` 或 `jobs`；分别可指定 `project_id`、`job_id`，省略/null 表示该类全部事件。`limit` 必须为 1–500。回应 `events` 中每项包含 `cursor` 与 `event`，后者按 `type: project|job` 和 `data` 区分。每条事件的 cursor 支持页内断点；`next_cursor` 是本页最后事件的 cursor，空页保留传入值，初始空流为 null。不同类型的流之间没有统一总顺序。

首次无游标查询从保留历史的开头开始。CLI 有限读取一直排到当前空页，并非只取 `--limit` 条；持续写入时调用者应使用 `--max-events` 或 `--timeout-seconds` 限制总工作量。watch 每 200 ms 轮询空页，有积压时连续读取；不是实时播放通道。

## 游标与恢复

游标为 opaque string。当前实现保存流类型/过滤条件、十进制字符串序号和事件身份；适配器检查 anchor 与记录身份，不使用 JSON number 暴露数据库位置。客户端不得解析、重建、修改或跨过滤条件重用游标。

- 编码、格式或过滤范围不匹配：`INVALID_CURSOR`，CLI 2 / HTTP 400。
- anchor 已删除或相同序号属于另一事件历史：`CURSOR_EXPIRED`，CLI 4 / HTTP 410。不会默默从开头重放。
- 项目/任务过滤对象不存在：`NOT_FOUND`。
- 游标不是权限凭证。受限身份的项目／任务过滤在存储读取时应用当前 grant；CLI、HTTP 与两种 MCP 沿用同一授权。HTTP 公共流为 POST /v1/changes/stream，Last-Event-ID 优先于请求 cursor；SSE data 使用下方 StreamRecord。撤权或 policy revision 变化结束既有 HTTP 流，客户端只保存已处理的 event cursor。
- 当前绑定的是事件身份，不包含独立 workspace UUID。复制同一数据库且保留相同事件历史时，旧游标仍有效。不能宣称所有数据库副本间的游标都互斥。

消费方只在事件已处理完成后保存其 cursor；断线后从该 cursor 续读。处理与保存之间发生崩溃可能重放，应按 `event_id` 去重。服务器不承诺消费方 exactly-once；没有 anchor 的断线不能凭 `ready` 判断哪些事件已处理。历史保留策略未来须继续遵守明确过期的错误契约。

## CLI 输出与退出

```sh
opencut events --project-id project-id --watch --format ndjson
opencut events --format ndjson --cursor '<saved-cursor>' --max-events 50
opencut job events --job-id job_id --watch --format ndjson --timeout-seconds 30
opencut system events-schema --json
opencut job wait job_id --json --cancel-on-interrupt
```

每行有 `format: "opencut.events.v1"`，按以下类型解释：

| type | 字段/意义 |
|---|---|
| ready | `scope`、初始 `cursor`；第一轮查询校验成功后发出 |
| event | 本条 `cursor`、`event`；不包含浮点化数字序号 |
| end | 最后实际输出的 `cursor`、`reason: caught_up|limit_reached` |
| error | 最后实际输出的 `cursor`、结构化 `EditorError` |

stdout 只有 NDJSON；诊断留在 stderr。每条记录写出后 flush，页内提前停止不会把未输出事件算入 cursor。`--format ndjson` 与单信封 `--json` 互斥；数字 `--after`/`--after-sequence` 只用于旧批量接口。格式可识别的 CLI 参数错误也输出一条 error，解析失败不创建数据库。正式 JSON Schema 为 `packages/contract-fixtures/event-stream.generated.json`，TS 同源类型为 `EventStream.StreamRecord`。

正常完成/下游关闭管道返回 0；Ctrl-C 返回 130；观察超时返回 124；其余错误沿用统一映射。中断/超时优先结束进程，即便输出管道已阻塞也不等待清空。因此可能没有 end，最后一行也可能未写全；消费方只接受完整换行并成功解析的记录。坏管道不视为任务失败。

任务事件使用现有 JobChange allowlist 投影，不输出 Worker input、lease、错误原文和私有路径。工程事件继续传递提交 payload；不可据此假设用户填写的工程内容没有敏感信息。

## 等待与取消

`job wait` Ctrl-C 默认只结束观察，以一个 JSON envelope 报告 `USER_INTERRUPTED` 并退出 130。只有 `--cancel-on-interrupt` 才通过现有 `opencut_job_cancel` 请求持久化取消；响应 details 保存取消请求结果，成功接受取消不代表运行中任务已经终止。超时不触发取消，即使带该选项也保持后台任务。

既有 `opencut_events_read`、`opencut_job_events`、`opencut_job_changes` 及批量 CLI 输出保留，不能把新游标传给旧数字接口。此轮不修改旧 SSE 的 numeric Last-Event-ID，也不声称已实现 MCP resource subscriptions。
