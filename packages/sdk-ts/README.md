# OpenCut TypeScript SDK

独立 ESM 包，零运行时依赖。支持 Node 22+ 的标准 fetch / AbortSignal；浏览器采用相同传输契约，但真实浏览器运行尚未验收。版本 `0.1.0` 对应 API `1`，两者独立版本化。本地打包安装已经验收，尚未发布到 npm registry。

```sh
cd packages/sdk-ts
npm ci
npm run contracts:generate
npm test
npm pack
```

将生成的 `.tgz` 安装到外部项目后：

```ts
import { OpenCutClient, publicJobResult } from '@opencut/sdk';

const client = new OpenCutClient({
  baseUrl: 'http://127.0.0.1:8788',
  token: '<host token>',
});
const snapshot = await client.callDomain('project', 'inspect', { project_id: 'demo' });
// 请求对象由调用方保存。重试必须保留 expected_revision、幂等键与操作内容。
const request = {
  project_id: snapshot.project.id,
  request: {
    expected_revision: snapshot.revision,
    idempotency_key: crypto.randomUUID(),
    label: 'Rename sequence',
    operations: [{ type: 'sequence.update_settings' as const, sequence_id: 'main', name: 'Final cut' }],
  },
};
const plan = await client.callDomain('edit', 'plan', request);
// 调用方在检查 plan 后显式提交；SDK 不替调用方作产品决策。
const committed = await client.callDomain('edit', 'apply', request);

// 原生 v1 media.probe 路径属于服务端文件系统，不能传浏览器本机路径。
const accepted = await client.submitJob({
  kind: 'media.probe', idempotency_key: crypto.randomUUID(),
  input: { input: '/server/workspace/clip.mp4' },
});
const finished = await client.waitForPublicJob(accepted.job_id, { timeoutMs: 30_000 });
const metadata = publicJobResult(finished, 'media.probe');
console.log(plan, committed.revision, metadata.streams);
```

## 契约

- `callDomain(domain, action, input)` 对领域注册表中的动作提供类型化输入／结果；`callDomainEnvelope` 额外保留 API 版本、request ID 和 warnings。`call`／`callEnvelope` 保留叶操作的完整兼容调用。两种形式源于同一 Rust registry，不维护手写的第二套编辑模型。
- 编辑使用明确 project/object ID、expected_revision 和稳定幂等键；SDK 不持有当前选区、工程 head 或第二份可写状态，不进行自动重试或自动刷新 revision。
- `OpenCutApiError` 保留 code、details、status、requestId、warnings。网络异常为 `OpenCutTransportError`；无效／不兼容响应为 `OpenCutProtocolError`。后两者均可能发生在服务端已提交后，`outcome` 为 `unknown`。
- `AbortSignal` 只中止调用方的传输／观察，不能证明服务端未执行。`waitForPublicJob` / `waitForJob` 的本地超时覆盖请求与轮询时间，并抛出 `JobWaitTimeoutError`；后台持久任务继续执行。只有显式 `cancelPublicJob` / `cancelJob` 才请求任务取消。
- 新接入使用 `submitJob`、`waitForPublicJob` 和 `publicJobResult(job, kind)`，得到旧八类具体公共结果及 media.import 扩展结果。失败／取消抛出 `JobCompletionError`，携带公共 JobView。既有 `waitForJob` / `jobResult` 保留 v1 JobRecord 兼容。
- `artifactUrl(reference)` 校验引用元组；`downloadArtifact(reference, {signal})` 返回认证后的流式 Response，错误返回结构化异常。引用绑定 job_id 与 SHA-256，成功任务的 availability=unchecked，下载时才校验实际文件。
- JSON 时间 ticks 始终为十进制字符串，禁止经 JS Number 中转。SDK 不重写请求时间或透传元数据，以免改变幂等身份。
- 当前 SDK 校验响应 envelope 与 API 版本，不做每个 DTO 的运行时 Schema 验证。开放 JSON/metadata/插件字段保持可扩展类型；TypeScript 不等于输入验证，权威校验仍在服务端。
- 认证 token 存在私有字段，默认 fetch 禁止 HTTP 重定向；自定义 fetch 必须遵守标准 fetch 的 signal 和 redirect 语义。

## 生成与验证

V4源音频分离/恢复先调用`opencut_project_v4_source_audio_plan`，检查返回的`transaction`后原样提交`opencut_edit_v4_apply`。规划只读且要求当前read/edit权限、固定head及可用源音轨；提交继续复查权限与写入围栏。分离复制精确源范围、速率、增益及音量通道并关闭视频音频；恢复重新启用视频音频，保留已分离元素。幂等重试沿用同一transaction，不重新规划新ID。此工具不代表已接入Web播放或完整音频导出。

`npm run contracts:check` 检查 SDK 类型、工具 Schema、OpenAPI 3.1 和 Web 兼容导出是否漂移。已构建正确原生 binary 时可加 `-- --binary /path/to/opencut --skip-build`。Web 旧生成命令仍可调用相同生成实现。

仓库根运行 `node scripts/verify-sdk.mjs`：打包、在仓库外安装、编译外部消费者、启动真实认证服务、经 SDK 编辑、通过独立 CLI 回读、注入真实响应丢失、验证冲突及真实 FFmpeg Worker。使用隔离临时数据库，证据目录保留报告、包与服务日志。

OpenAPI 从同一注册表生成，覆盖叶操作、领域动作与发现入口，以及原始上传分块、产物字节下载和公共 SSE；具体范围以生成声明为准。领域 action 与精确输入 schema、叶操作映射保持同源，事件记录通过 `x-opencut-data-schema` 关联到 SSE。SDK 不自动重连或重试写请求。

新增公共 JobView 隐去 prepared input、租约和原生路径；源任务输入仍是 v1 宿主路径，旧 JobRecord 接口仍存在，不构成新的权限边界。完整约定见仓库 `design/architecture/public-job-view-contract.md`。运行 `python3 scripts/verify-public-job-views.py` 会验证八类真实结果、产物字节、游标恢复及外部安装包中的公共 SDK 流程。

`opencut_resource_read` 读取已发布 `opencut://` URI，返回 typed ResourceView；例如 `opencut://projects/project_demo/branches/main/summary`。固定修订 URI 不随 head 漂移，Job/产物沿用安全视图。变量按 RFC 3986 百分号编码；资源不是任意文件读取入口或授权凭证。MCP 资源订阅只是重新读取提示，持久化补读仍使用事件游标。

owner 可调用 `opencut_access_audit_query` 读取权限管理历史；target 支持 workspace/principal/credential/request，after_sequence 为十进制字符串，与工程／任务审计序号不可混用。响应含提交时 policy 快照，不含 secret/digest；页受条数和 4 MB 记录预算共同限制，has_more 为 true 时使用 last_sequence 续查。项目 read_audit 不授予管理历史读取权。

owner 可通过 `opencut_maintenance_query` 读取媒体文件发布的 prepared/completed/observed_* 事件，通过 `opencut_maintenance_reconcile` 持锁核对未知结果。PUBLICATION_OUTCOME_UNKNOWN 不代表文件未修改；保存 publication_id，先核对再决定重试。核对只追加观察结果，不改写内容；项目 read_audit 不授予该权限。发布日志从数据库 v6 开始。

项目范围预检使用现有 `opencut_render_preflight`（Rust：`tools::RenderPreflight`），要求 read + run_jobs；能力资源 access.scoped_preflight 默认 false，新宿主为 true。受限调用只读取固定修订中的锁定内容；ASSET_LOCK_REQUIRED 需要 owner 固定素材，其他失败保留问题 code／实体 ID，去除原生路径。预检不会创建 Job，之后提交仍独立鉴权与验证内容。


Web 的单次读取导入使用 `opencut_upload_stream_begin`（不要求整文件摘要），以 `uploadStreamChunk` 顺序发送分块，同时累计整文件 SHA-256；最后 `opencut_upload_stream_complete({upload_id, content_hash})` 原子封存并接受导入任务。Rust 对应 `upload_stream_chunk` 与生成的 `tools::UploadStreamBegin/UploadStreamComplete`。会话的 `content_hash` 在封存前为 null，worker 在发布素材前独立验证全文件。使用 stream_inspect/query/abort 查状态、发现会话和取消；旧 query 不显示未定摘要会话，分页仍须跟随 next_cursor。断线时仍先检查已确认偏移，再发送相同块；摘要必须覆盖整文件，不能仅累计重连后的后缀。旧协议继续可用：

项目上传先调用 `opencut_upload_create`（project_id、显示 name、规范字符串 size_bytes、完整 SHA-256 content_hash、稳定 idempotency_key），再 `uploadChunk(uploadId, offset, bytes, {signal})` 顺序发送 1 byte–4 MiB 的 Uint8Array。助手复制字节后计算每块 SHA-256，使用认证 PUT；不把本地路径交给服务端。确认偏移来自 `opencut_upload_inspect`，传输失败先查询，再重试相同块；SDK 不自动重试或封存。

完整上传后调用 `opencut_upload_seal({upload_id})`，使用返回的 job_id 调用 `waitForPublicJob`，再 `publicJobResult(job, 'media.import')` 取得项目素材。该任务由封存入口接受，不能通过旧 `submitJob` 输入枚举启动。结果检查成功状态、kind 和 schema_version=1；未知版本拒绝。导入不修改工程 head。`opencut_project_media_query` 使用返回的 next_cursor 分页，查询期间新导入的素材不加入原分页范围。

已绑定到工程修订的音频／视频可提交本地转写。宿主需安装 whisper.cpp，并设置 `OPENCUT_WHISPER_MODEL`（模型路径）；`OPENCUT_WHISPER_CLI` 可选，默认 `whisper-cli`。这些是宿主配置，不属于客户端请求：

```ts
const accepted = await client.call("opencut_analysis_transcribe", {
  idempotency_key: "transcribe-source-v1",
  project_id: "interview", revision: snapshot.revision, asset_id: "source",
  language: "zh",
});
const finished = await client.waitForPublicJob(accepted.job_id);
const analysis = publicJobResult(finished, "analysis.transcribe");
// analysis.asset_version 固定内容哈希；segment.start/end 是素材时间，不是时间线位置。
for (const segment of analysis.transcription.segments) console.log(segment.text);
```

紧凑入口为 `client.callDomain("media", "analysis.transcribe", input)`。新任务复用既有 Job 观察／取消，通过 `extended_result` 版本 1 返回段级时间和来源；旧 `submitJob` 枚举保持不变。Rust 使用 `tools::AnalysisTranscribe` 和 `extended_job_result::<jobs::kinds::AnalysisTranscribe>`。转写不会修改工程；Agent 将素材时间映射到剪辑时间线、提交 `edit_plan` 后再显式 `edit_apply`。当前 `words` 为空，无说话人识别；长素材、识别精度和真人访谈需单独验收。等待超时只停止观察，不取消服务器任务。

`opencut_upload_abort` 只取消 receiving 会话并清除私有分块；返回 UPLOAD_CLEANUP_PENDING 时状态已经 aborted，应重试 abort 完成清理。sealed 任务需显式请求 Job 取消，已完成素材保持可读。终态暂存可通过 opencut_upload_release 显式释放，上传预留量受项目／工作区配额约束；显式 CAS 孤儿回收见下文；统一物理配额、产物保留和其他类别回收仍待实现。


`hostCapabilities()` 通过 `opencut://system/capabilities` 读取同源 HostCapabilities，提供类型化的可选 uploads.staging；旧 v1 capabilities 工具 Schema 保持兼容。`opencut_upload_retention` 查询状态／预留，`opencut_upload_quota` 查询本项目预留量和公开限制。`opencut_upload_release` 只清理 aborted 或终态导入的私有分块；release_eligible 是观察，不是权限凭据。

默认按创建时的完整 size_bytes 预留，项目 8 GiB／工作区 32 GiB，只有实际删除与最终记账成功才释放容量；该数字不测量 CAS、产物和物理磁盘。UPLOAD_RELEASE_PENDING 表示需查询／重试，不证明文件仍在。默认没有终态自动删除；失败／取消任务不原地重跑，修正输入后使用新的创建幂等键。成功资源在清理后仍能绑定／预览。


丢失上传 ID 时，用 opencut_upload_query（Rust tools::UploadQuery）分页发现本项目会话，默认 retained_only=true；false 可读释放历史。页内同时返回 upload 与 retention，按 release_eligible 判断是否可释放。next_cursor 固定创建上界与过滤条件，状态仍取每页的当前值；查询期间新创建的会话不进入原分页，已释放记录会从 retained 筛选中消失。查询只读，不能把结果当成执行授权。


项目插件使用 `opencut_project_plugin_list/inspect/plan`：read 可以读取项目策略与内联参数 schema，执行 plan 还需 edit/run_jobs。Owner 用 `opencut_project_plugin_grant` 绑定准确包哈希、贡献名和授权 CAS revision。plan 返回 `{grant,result}`，`result.operations` 仍须用原工程 revision 显式 `edit_apply`；授权 revision 不是工程 revision。`hostCapabilities().project_plugins` 提供限额和工具发现，详见仓库的 `design/architecture/project-plugin-contract.md`。


拒绝审计：`hostCapabilities().denials` 提供保留上限和查询入口。受限调用被拒绝后，`OpenCutApiError.requestId` 可交给 Owner 用 `opencut_denial_query` 的 request target 查询。日志写入失败仍拒绝操作，`warnings` 含 `DENIAL_AUDIT_UNAVAILABLE`；不能把这个 warning 当作重发编辑的依据。查询保留窗外返回 `CURSOR_EXPIRED`，从 0 重新发现并处理历史缺口。

Owner-only `opencut_storage_inspect` returns typed, bounded storage observations. Byte counts are decimal strings; inspect `complete`, `stop_reason`, category omissions and `volume_observed`. Live observations do not reserve capacity or authorize garbage collection. Use `hostCapabilities().storage` / `host_capabilities()?.storage` for availability and limits.

CAS collection: discover with `opencut_cas_gc_query`, persist a review with `opencut_cas_gc_plan`, then explicitly execute the returned `plan_id` and exact `plan_hash` using `opencut_cas_gc_execute`. Inspect/retry the same plan after transport failure; `removing` is unresolved. Never infer deletion from acceptance, or reclaimed physical space from logical sizes. Query can return an empty filtered page with a next cursor. Discovery is in `hostCapabilities().cas_gc` / `host_capabilities()?.cas_gc`.


全局 owner 可用 `opencut_storage_budget_inspect/configure/recover` 管理持久写入准入。configure 使用 expected_revision 与稳定幂等键，返回带原 request_id/actor 的策略回执；重放不创建新策略。字节量是规范十进制字符串。默认 64 GiB payload 准入上限和 64 MiB 卷余量；当前覆盖 12 类受控写入（详见仓库的 `design/architecture/storage-admission-contract.md`），但 SQLite/WAL、控制元数据及完整保留/回收尚未覆盖，`enforces_all_workspace_writes=false`。inspect 不恢复或创建状态，recover 是显式写操作；已写内容与完整活动预留可能重复计入，headroom 不是物理容量承诺。公开任务和上传保留明确的容量错误；终态 Job 需要新的显式尝试。

`callWithAdmissionRetry(name, input, options)` explicitly defers only a structured
`STORAGE_BUDGET_BUSY` response, using the same captured JSON and idempotency key.
The default deferral deadline is five seconds (configurable from 1–30000 ms).
`signal` cancels the wait; `beforeAttempt` can recheck the caller's captured view
before every send. Capacity, revision, transport and protocol errors are returned
without retry. Ordinary `call`, domain calls and raw upload chunks remain one-shot;
a network failure still requires inspecting the confirmed state.

V4 工程使用 `opencut_project_v4_*` 和 `opencut_edit_v4_*`，输入/输出类型从 Rust 同源生成。inspect/create/copy 返回完整公共快照；promote/restore/apply 返回事务结果。资源绑定选择同一授权工程的固定资产/字体记录或已发布 media ID/version，不接受私有路径。undo/redo/navigation 使用版本化因果历史，计划只公开摘要和冲突原因。字体目前只能复用固定项目记录；完整场景 WAV 和 normal 场景静帧 PNG 已有版本化 Job 与 Web 动线，完整视频、工程包和其他视觉类型仍待完成。旧执行工具明确拒绝 V4。


`opencut_project_v4_resource_inspect` 描述固定修订中的资产/字体；`projectV4ResourceUrl` 按范围重新计算 URL，`downloadProjectV4Resource` 以授权请求流式读取服务端校验过的 bytes。描述本身不保证 CAS 仍可用，hash 不是访问凭证。服务端复查当前授权与工程存活；查询参数不会成为路径片段。

`opencut_project_v4_media_import_plan` 返回原样可提交的 `opencut_edit_v4_apply` 事务与明确的设置/放置决定。计划无 receipt，`original_request_digest` 不代替提交事务的幂等身份；apply 再检查素材范围、固定 head 和权限。Classic 首视频可能覆盖为较低全局 FPS；旧 Legacy scene profile 保持独立规则。

`opencut_project_v4_audio_gain_plan` 在固定 head 上规划元素音量或独立静音。`set_decibels` 使用 −600…200 的整数 `decibels_tenths`，仅编辑静态 Decibels gain；−60dB 保留非零增益，静音和曲线另行保留。有元素音量关键帧或 Legacy linear gain 时拒绝静态改值，静音仍保留原音量语义。Audio 和已知有源音频的 Video 可用，Video 不会因此重新启用源音频。无变化返回 `transaction: null`，否则原样提交唯一 ElementPut；计划本身不写修订，不代表提交成功。

## V4 范围音频 WAV

先用 `opencut_project_v4_audio_range_capability` 查询当前宿主的执行配置；返回版本为 1，采样率仅支持 44100/48000 Hz。`available` 不代替当前工程的 read/run_jobs 权限、资源可读性或预算校验。整场工具 `opencut_job_v4_audio_wav` 保持原请求，不接受 `range`；范围任务使用独立工具 `opencut_job_v4_audio_range_wav` 和 kind `render.project_audio_range.v4`，不通过旧 `submitJob` 枚举提交。

区间是目标采样率下的半开样本区间 `[start_sample, start_sample + frame_count)`，两个字段均为规范无符号十进制字符串；禁止前导零，`frame_count` 必须大于零，结束位置不能超过整场帧数。工程先按固定修订完整混音和母带处理，再裁切该区间，因此区间之外的峰值也可能影响输出增益。这是范围导出，不是实时播放、decoder seek 或局部母带模式。

以下沿用已创建的 `client`，假设 `demo` 的默认场景至少两秒：

```ts
import type { ToolInput } from '@opencut/sdk';

const capability = await client.call('opencut_project_v4_audio_range_capability', {});
if (capability.schema_version !== 1 || !capability.available ||
    !capability.sample_rates.includes(48000)) {
  throw new Error(capability.unavailable_reason ?? 'Range WAV is unavailable');
}
const fixed = await client.call('opencut_project_v4_inspect', { project_id: 'demo' });
const rangeRequest: ToolInput<'opencut_job_v4_audio_range_wav'> = {
  idempotency_key: crypto.randomUUID(),
  project_id: fixed.project.id,
  revision: fixed.revision,
  sequence_id: fixed.project.default_sequence_id,
  sample_rate: 48000,
  range: { start_sample: '48000', frame_count: '48000' }, // [1s, 2s)
};
const rangeJob = await client.call('opencut_job_v4_audio_range_wav', rangeRequest);
const rangeFinished = await client.waitForPublicJob(rangeJob.job_id);
const rangeResult = publicJobResult(rangeFinished, 'render.project_audio_range.v4');
if (rangeResult.schema_version !== 1 ||
    rangeResult.identity.project_id !== rangeRequest.project_id ||
    rangeResult.identity.revision !== rangeRequest.revision ||
    rangeResult.identity.sequence_id !== rangeRequest.sequence_id ||
    rangeResult.identity.sample_rate !== rangeRequest.sample_rate ||
    rangeResult.identity.channels !== 2 ||
    rangeResult.identity.sample_format !== 'ieee_float32_le' ||
    rangeResult.identity.start_sample !== rangeRequest.range.start_sample ||
    rangeResult.identity.frame_count !== rangeRequest.range.frame_count) {
  throw new Error('Unexpected range WAV identity');
}
const reference = rangeFinished.artifacts.find(a => a.role === 'primary');
if (!reference || reference.content_hash !== rangeResult.artifact.content_hash ||
    reference.size_bytes !== rangeResult.artifact.size_bytes) {
  throw new Error('Missing matching WAV artifact');
}
const response = await client.downloadArtifact(reference);
const wavBytes = await response.arrayBuffer(); // 本例只有一秒；大结果应流式消费 response.body。
console.log(rangeResult.identity.whole_scene_frame_count, wavBytes.byteLength);
```

也可调用同源领域动作 `client.callDomain('project', 'v4.audio.range.capability', {})` 和 `client.callDomain('task', 'v4.audio.range.wav', rangeRequest)`。结果通过 `extended_result` 版本 1 投影，包含固定 project/revision/sequence、输出 start/frame count、整场帧数、stereo Float32 格式、完整输入摘要、asset lock、执行摘要和无路径产物元数据；生成类型为 `JobRenderProjectAudioRangeV4Result.V4AudioRangeWavResultV1`。

保存原始请求及幂等键；提交响应未知时重发同一请求恢复接受回执，不更新 revision 或重新生成键。等待超时可继续观察同一 job_id，不自动取消后台任务；取消使用 `cancelPublicJob(job_id)` 并等待终态。服务端在接受、执行和发布时复查原提交者授权、工程存活及任务围栏；下载仍独立鉴权和校验实际文件。
