# MCP 资源、提示与订阅契约

2026-09-21。当前 stdio 宿主明确协商 MCP `2025-11-25`。采用该版本的 [Resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources) 和 [Prompts](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts) 接口；不因 SDK 默认认识更新协议就宣称本产品已实现新版订阅语义。

## 唯一读取权威

`opencut_resource_read {"uri":"…"}` 是只读注册工具。CLI call、HTTP `/v1/tools/opencut_resource_read`、SDK typed tool 和 MCP resources/read 共用 Application 用例。输出 `ResourceView {uri,mime_type,payload}`；payload 用 `kind/data` 标记纯契约 DTO。MCP 将同一值编码为 `application/json` text resource，不读取任意路径或让 URI 绕过应用查询。

- `api-contracts::context`：ResourceReadInput、ResourceView、summary/timeline/operation DTO；既有 JobView、ArtifactReference 与 Capabilities 直接复用。
- `editor-application::resources`：URI 校验、项目修订与序列选择、只读投影、操作 Schema 和 2 MiB 内容上限；只依赖 ProjectRead/JobRead 端口。
- `sqlite-store`：连接及读适配装配。没有新增表或更改 schema v3。
- `automation`：注册和分派；宿主能力文档按需提供，动态资源轮询不重复构造全部 Schema。
- `apps/cli::mcp_context`：MCP 参数/错误映射、提示模板和连接内订阅生命周期，不实现编辑规则。

URI 不携带访问权限。当前 stdio 是显式选择 workspace 的本机信任边界，HTTP 沿用宿主 bearer/Origin 校验；principal/project 授权链仍未完成。未来授权必须在同一资源用例/宿主入口执行，不能仅验证 URI 格式。

## 发现与资源身份

resources/list 返回三个静态资源；resources/templates/list 返回六个动态模板。两个集合在当前运行版本固定，均为单页，不接受未知 cursor；不广告 listChanged。动态工程/任务从现有项目/Job 查询工具获得 ID 后展开模板。

| URI | payload.kind | 语义 |
|---|---|---|
| `opencut://system/capabilities` | capabilities | 与能力工具同源 |
| `opencut://schemas/tools` | tools | 与注册表同源的输入/输出 Schema |
| `opencut://schemas/event-stream` | event_schema | CLI NDJSON 同源 Schema |
| `opencut://projects/{project}/branches/{branch}/summary` | project_summary | 当前主分支摘要，可变化 |
| `opencut://projects/{project}/branches/{branch}/revisions/{revision}/summary` | project_summary | 指定修订，不随 head 更新 |
| `opencut://projects/{project}/revisions/{revision}/sequences/{sequence}/timeline` | timeline | 指定修订的完整序列及精确时间 DTO |
| `opencut://jobs/{job}` | job | 公共 JobView，可变化 |
| `opencut://artifacts/{artifact}` | artifact | Job/hash 绑定的描述，不是媒体 bytes |
| `opencut://schemas/operations/{name}/{version}` | operation_schema | 对应版本的操作描述及准确输入 variant，携带本地 `$defs` |

当前 branch 只支持 `main`；未知分支返回不存在，不能回落到 main。序列必须属于该工程的指定修订。summary 包含序列设置和轨/clip/字幕数量，不包含原始素材路径。timeline 保留用户工程内容，不能推断这些内容没有敏感文本。

变量按 RFC 6570 简单变量展开：UTF-8 字节中，仅字母数字及 `-._~` 直接保留，其余百分号编码。字符 `/`、`?`、`#`、`%` 在变量内必须编码；编码后只解码一次作为对象标识，不拼接文件路径。URI 总长最多 16384 字节，每个解码段最多 1024 字节，非法百分号/UTF-8、任意外部 scheme/query/fragment 被拒绝。

资源值序列化后的逻辑 JSON 内容最多 2 MiB；MCP 对 text 的外层 JSON 转义不计入此值。过大的完整 timeline 明确返回 `RESOURCE_TOO_LARGE`，不会截断后假装完整。原设计中的 analyses 资源尚依赖 R5，当前不广告虚构模板。产物描述使用 `availability: unchecked`，真实下载仍需宿主验证；不可凭历史成功推断文件仍存在。媒体不在本轮内联。

## 订阅与关闭

MCP 声明 `resources.subscribe=true`；resources/subscribe 先用同一用例验证资源能读。每个连接最多 32 个 URI；重复订阅同一 URI 不增加观察器，重复取消有效 URI 无副作用，取消后可复用配额。URI 按请求字符串计配额，等价编码可以占用不同槽位。

当前 head 与 Job 资源每 250 ms 重新读取，值或可读状态变化后通知 `notifications/resources/updated`，只携带 URI。不可变资源可以订阅，但无需轮询。通知允许合并中间变化，不是持久化事件日志；客户端收到通知后重新 resources/read。需要断线后的可靠补读时，使用 `opencut_event_changes` 的事件 ID/cursor。

每个观察器最多一个待发送通知；写端背压暂停观察，不积累无限队列，也不因临时慢读静默注销订阅。取消订阅中止并 join 对应任务；之前已进入传输层的通知可能仍在途。stdio 断连释放连接订阅，不更改 Job 状态。Streamable HTTP 的单个连接断开保留会话订阅，直到 DELETE、会话到期或宿主退出，详见 [网络契约](mcp-http-contract.md)。

实际验收发现 SDK stdio 关闭会等待 writer flush，慢读端可能让 EOF 关闭挂起。宿主用 EOF 观察 reader 独立捕获断连，终止观察器；stdio 入口在协议会话结束后退出进程，不等待无人读取的 Tokio stdout helper。此规则仅用于 stdio 生命周期，不应照搬成网络会话的进程退出。写请求在断连时结果可能未知，必须以原幂等键查询/重试，不能自动重新生成意图。

## 提示模板

prompts/list 提供 `interview_rough_cut`、`caption_review`、`vertical_adaptation`。均要求字符串参数 project_id、revision、sequence_id；拒绝未知参数、缺参、错误类型和不存在的固定修订/序列。get 返回说明和嵌入的同源不可变 timeline resource，读取不会提交编辑。

模板引用已发布的能力、计划、提交、公共任务观察/取消工具，标明 apply 和预览 Job 的持久化效果；模板本身不增加权限。当前未提供的转写、说话人分析或自动主体检测会在说明中明确指出，要求有来源证据，不伪造执行。参数和工程内容作为数据处理；不会提升成系统指令。

## 错误和未完成范围

resources/read/get_prompt 的资源不存在为 MCP `-32002`；非法 URI、参数、发现 cursor、资源过大或订阅超限为 `-32602`；内部读取失败为 `-32603`，不暴露原生诊断路径。业务 code 保存在 error.data.code。Job 为 failed 的公共视图仍是资源读取成功，失败保存在 payload.data.failure 中。

旧 tools/call 继续使用结构化 Response 和 isError；二者语义不混用。新增 ResourceRead 是第 29 个工具，旧 22 工具的冻结 Schema/read_only 保持不变。Streamable HTTP 已由后续 [独立契约与验收](mcp-http-contract.md) 覆盖；远程 OAuth/多用户权限和更新协议版本仍未实现。Web/GPUI 实现仍暂缓。
