# 公共任务与产物引用契约

2026-09-21。本文定义已实现的增量接口；旧 v1 JobRecord 接口仍保留。新入口通过允许字段清单投影内部记录，不向调用方暴露 prepared snapshot、Worker 租约、attempt、原生输出路径或原始后端诊断。它不是多租户授权边界：持有本地宿主 token 的客户端仍可调用遗留接口。

## 工具与职责

| 工具 | 输入 | 结果与语义 |
|---|---|---|
| `opencut_job_submit` | 既有 JobStartInput | JobView；与旧 start 共用幂等作用域，不创建第二份任务 |
| `opencut_job_view` | job_id | 当前公开视图 |
| `opencut_job_query` | cursor、limit（默认 50，1–100） | JobPage；固定集合上界的分页 |
| `opencut_job_changes` | cursor、可选 job_id、limit | 持久化公共事件页；可续接 |
| `opencut_job_request_cancel` | job_id | 取消请求后的 JobView；已完成任务保留原终态 |

这五个工具与旧 22 个工具共用 Rust registry、CLI/MCP/HTTP Dispatcher、TS/Rust 关联类型和 OpenAPI。旧 22 个工具的输入／输出 Schema 和 read_only 属性由冻结 fixture 检查；新增入口没有删除或收窄旧字段。

## 任务事实与失败

JobView 包含 job_id、kind、target、state、cancel_requested、progress、result、result_unavailable、failure、artifacts。四种工程任务的 target 固定 project_id、revision、sequence_id，排队期间即可读取。源媒体任务没有工程 target。target 当前从内部持久输入的允许字段投影，不要求数据库迁移。

八种成功结果通过 `{kind, data}` 区分：探测、源抽帧、工程抽帧、直接合成帧、工程区间、源导出、工程导出、产物校验。媒体／时间／验证结果、实际字体 face、排版指标和素材锁保留；文件位置和诊断详情移除。source_pts、时间 ticks 和 artifact size_bytes 保持十进制字符串，客户端不得用浮点数中转。

公开失败保留稳定分类、固定说明及 diagnostic_job_id，不透传 FFmpeg stderr、文件名或宿主错误 details。未知错误归为 JOB_FAILED。未知／损坏的历史成功结果保持原 succeeded 状态，返回 result_unavailable，不能伪造空成功结果。进度是阶段信息，不保证是全任务线性百分比；未知阶段归为 unknown。

## 分页与事件

Job 查询按持久插入顺序分页。首个游标记录集合上界及已读位置，之后创建的任务不会插入这一轮分页；每页的任务状态仍是读取当时的观察，并非全库状态快照。next_cursor 为 null 表示此轮集合读完；刷新列表需重新开始。

事件游标绑定 job_id 过滤范围（或 all）、持久序号和 event_id。接口验证锚点及过滤条件；错格式／错过滤返回 INVALID_CURSOR，无法找到锚点返回 CURSOR_EXPIRED。已有锚点跨服务重启有效。空页保留原游标，调用方可继续轮询。公共事件使用 queued、started、progress 等类型，不返回底层 payload、attempt 或 lease。游标是续接标识，不是授权凭证；复制同一数据库会复制其事件身份。

## 产物契约

ArtifactReference 包含 artifact_id、job_id、role、content_hash、size_bytes、mime_type、availability。当前仅 primary，identity 为 `artifact:<job_id>:<sha256>`，绑定已提交任务结果及内容。客户端应将 artifact_id 作为不透明值交给 SDK。

`GET /v1/artifacts/{artifact}` 使用与工具相同的 Bearer 认证。宿主验证引用与任务结果匹配、文件在授权产物目录内、实际大小和 SHA-256；随后从经校验的私有复制文件发送，避免校验与读取内容不一致。错误哈希、不存在／损坏的文件明确失败。

Job 的 succeeded 表示历史执行已成功，不能保证文件永久存在；availability 固定为 unchecked。下载失败不能把历史成功任务改成 failed。六种有文件的结果提供引用；probe 和 verify 不发布新文件。旧 `/v1/jobs/{job_id}/artifact` 下载路由继续兼容。

TS SDK 提供 submitJob、waitForPublicJob、cancelPublicJob、publicJobResult、artifactUrl 和认证流式 downloadArtifact。Rust SDK 通过 typed Tool 调用新增工具，提供 wait_for_public_job、cancel_public_job、artifact_url；尚无 Rust 流式下载封装。两者本地等待超时／取消观察均不请求取消持久任务。

## 应用层与存储端口

`editor-application::jobs::submit` 拥有校验、固定修订、素材锁和接受任务的编排。Automation jobs 模块只保留兼容导出。入队与实际渲染共同使用 Core 的素材依赖选择：禁用轨不参与，静音音轨不参与，静音视频轨仍需视觉素材，直接合成帧只需视觉依赖。

内部 JobSubmissionStore 提供固定快照读取、幂等预查和原子 accept_job。accept_job 必须在同一事务中再次检查幂等身份，写入任务、幂等回执和 queued 事件；任一失败整体回滚。并发相同请求返回最先接受的任务，不能用后来的 prepared input 覆盖它。JobAssetCapture 只准备持久素材锁，不允许自行写队列。素材捕获在事务外，失败／竞争后可能留下未引用 CAS 数据，后续 GC 负责回收。

后续 SQLite 实现已迁至独立 `sqlite-store` 包，见 [原生存储边界](native-storage-boundary.md)。工程编辑事务的后续拆分见 [工程事务契约](project-transaction-port-contract.md)；任务租约后续见 [生命周期契约](job-lifecycle-port-contract.md)；查询端口已随存储迁出落地，预检和插件等剩余用例仍需继续分离。

## 明确的剩余范围

源媒体 Job 输入仍采用 v1 宿主本地路径，尚未统一为已授权 asset identity；旧任务和诊断接口也尚未版本退役。新视图不是完整远程资源模型。字体／插件锁、产物生命周期与 GC、后端无关合成计划、SSE 自动恢复、SDK 发布和完整历史版本矩阵继续按 R2–R7 跟踪。Web/GPUI 实现与真实 UI 验收按用户指示暂缓。
