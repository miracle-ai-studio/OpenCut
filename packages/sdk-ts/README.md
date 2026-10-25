# OpenCut TypeScript SDK

独立 ESM 包，零运行时依赖，使用标准 fetch / AbortSignal。业务契约只有一个，由 Rust 生成；包和第三方依赖的实际发行身份与业务协议选择无关。当前静态集成尚未统一生成、构建或验收，不把历史安装和浏览器证据改标为当前通过。

```sh
cd packages/sdk-ts
npm ci
npm run contracts:generate
npm test
npm pack
```

将生成的 `.tgz` 安装到外部项目后：

```ts
import { OpenCutClient, jobResult } from '@opencut/sdk';

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

// owner 本机源任务的输入属于宿主文件系统；浏览器素材使用工程上传/资源接口。
const accepted = await client.submitJob({
  kind: 'media.probe', idempotency_key: crypto.randomUUID(),
  input: { input: '/server/workspace/clip.mp4' },
});
const finished = await client.waitForJob(accepted.job_id, { timeoutMs: 30_000 });
const metadata = jobResult(finished, 'media.probe');
console.log(plan, committed.revision, metadata.streams);
```

## 契约

- `callDomain(domain, action, input)` 对领域注册表中的动作提供类型化输入／结果；`callDomainEnvelope` 额外保留 request ID 和 warnings。`call`／`callEnvelope` 保留叶操作的当前叶操作调用。两种形式源于同一 Rust registry，不维护手写的第二套编辑模型。
- 编辑使用明确 project/object ID、expected_revision 和稳定幂等键；SDK 不持有当前选区、工程 head 或第二份可写状态，不进行自动重试或自动刷新 revision。
- `OpenCutApiError` 保留 code、details、status、requestId、warnings。网络异常为 `OpenCutTransportError`；无效／不兼容响应为 `OpenCutProtocolError`。后两者均可能发生在服务端已提交后，`outcome` 为 `unknown`。
- `AbortSignal` 只中止调用方的传输／观察，不能证明服务端未执行。`waitForJob` / `waitForJob` 的本地超时覆盖请求与轮询时间，并抛出 `JobWaitTimeoutError`；后台持久任务继续执行。只有显式 `cancelJob` / `cancelJob` 才请求任务取消。
- 新接入使用 `submitJob`、`waitForJob` 和 `jobResult(job, kind)`，得到旧八类具体公共结果及 media.import 扩展结果。失败／取消抛出 `JobCompletionError`，携带公共 JobView。既有 `waitForJob` / `jobResult` 保留 v1 JobRecord 兼容。
- `artifactUrl(reference)` 校验引用元组；`downloadArtifact(reference, {signal})` 返回认证后的流式 Response，错误返回结构化异常。引用绑定 job_id 与 SHA-256，成功任务的 availability=unchecked，下载时才校验实际文件。
- JSON 时间 ticks 始终为十进制字符串，禁止经 JS Number 中转。SDK 不重写请求时间或透传元数据，以免改变幂等身份。
- 当前 SDK 校验响应 envelope 与 API 版本，不做每个 DTO 的运行时 Schema 验证。开放 JSON/metadata/插件字段保持可扩展类型；TypeScript 不等于输入验证，权威校验仍在服务端。
- 认证 token 存在私有字段，默认 fetch 禁止 HTTP 重定向；自定义 fetch 必须遵守标准 fetch 的 signal 和 redirect 语义。

## 生成与验证

V4源音频分离/恢复先调用`opencut_project_source_audio_plan`，检查返回的`transaction`后原样提交`opencut_edit_v4_apply`。规划只读且要求当前read/edit权限、固定head及可用源音轨；提交继续复查权限与写入围栏。分离复制精确源范围、速率、增益及音量通道并关闭视频音频；恢复重新启用视频音频，保留已分离元素。幂等重试沿用同一transaction，不重新规划新ID。此工具不代表已接入Web播放或完整音频导出。

`npm run contracts:check` 检查 SDK 类型、工具 Schema、OpenAPI 3.1 和 Web 兼容导出是否漂移。已构建正确原生 binary 时可加 `-- --binary /path/to/opencut --skip-build`。Web 旧生成命令仍可调用相同生成实现。

仓库根运行 `node scripts/verify-sdk.mjs`：打包、在仓库外安装、编译外部消费者、启动真实认证服务、经 SDK 编辑、通过独立 CLI 回读、注入真实响应丢失、验证冲突及真实 FFmpeg Worker。使用隔离临时数据库，证据目录保留报告、包与服务日志。

OpenAPI 从同一注册表生成，覆盖叶操作、领域动作与发现入口，以及原始上传分块、产物字节下载和公共 SSE；具体范围以生成声明为准。领域 action 与精确输入 schema、叶操作映射保持同源，事件记录通过 `x-opencut-data-schema` 关联到 SSE。SDK 不自动重连或重试写请求。

唯一 JobView 隐去执行输入、租约、原生路径和原始诊断；CLI/HTTP/MCP/SDK/UI 使用同一结果。`jobResult(job,kind)` 只接受匹配且成功、可用的类型化结果；不维护结果版本表或原始任务入口。原任务的存储身份和校验器只留在执行/读取边界。当前原验证脚本的结果消费适配仍在收尾，不将旧八类结果验收当作当前联合结果已通过。

`opencut_resource_read` 读取已发布 `opencut://` URI，返回 typed ResourceView；例如 `opencut://projects/project_demo/branches/main/summary`。固定修订 URI 不随 head 漂移，Job/产物沿用安全视图。变量按 RFC 3986 百分号编码；资源不是任意文件读取入口或授权凭证。MCP 资源订阅只是重新读取提示，持久化补读仍使用事件游标。

owner 可调用 `opencut_access_audit_query` 读取权限管理历史；target 支持 workspace/principal/credential/request，after_sequence 为十进制字符串，与工程／任务审计序号不可混用。响应含提交时 policy 快照，不含 secret/digest；页受条数和 4 MB 记录预算共同限制，has_more 为 true 时使用 last_sequence 续查。项目 read_audit 不授予管理历史读取权。

owner 可通过 `opencut_maintenance_query` 读取媒体文件发布的 prepared/completed/observed_* 事件，通过 `opencut_maintenance_reconcile` 持锁核对未知结果。PUBLICATION_OUTCOME_UNKNOWN 不代表文件未修改；保存 publication_id，先核对再决定重试。核对只追加观察结果，不改写内容；项目 read_audit 不授予该权限。发布日志从数据库 v6 开始。

项目范围预检使用现有 `opencut_render_preflight`（Rust：`tools::RenderPreflight`），要求 read + run_jobs；能力资源 access.scoped_preflight 默认 false，新宿主为 true。受限调用只读取固定修订中的锁定内容；ASSET_LOCK_REQUIRED 需要 owner 固定素材，其他失败保留问题 code／实体 ID，去除原生路径。预检不会创建 Job，之后提交仍独立鉴权与验证内容。


项目上传统一使用 `opencut_upload_begin`（project_id、显示 name、规范字符串 size_bytes、稳定 idempotency_key，可选 content_hash）。普通文件无需先整文件读取：通过 `uploadChunk(uploadId, offset, bytes, {signal})` 顺序发送 1 byte–4 MiB 分块，同时累计整文件 SHA-256。SDK 复制字节并计算每块摘要，使用认证 PUT；本地路径不进入服务端请求。断线时先用 `opencut_upload_inspect` 查询已确认偏移，再重试相同分块；SDK 不自动重试写入。摘要必须覆盖整文件，不能仅累计重连后的后缀。

完整上传后调用 `opencut_upload_complete({upload_id, content_hash})`，原子固定完整摘要并接受一个持久导入任务；worker 在发布前独立核验全部字节。Rust 对应 `tools::UploadBegin/UploadComplete` 与 `upload_chunk`。`opencut_upload_query` 包含摘要尚未声明的会话，分页跟随 next_cursor；取消使用 `opencut_upload_abort`，已封存任务通过任务取消入口处理。

完成请求返回 job_id，用 `waitForJob` 观察，再以 `jobResult(job, 'media.import')` 取得类型化素材结果；导入不修改工程 head。`opencut_project_media_query` 按 next_cursor 查询，查询期间的新导入不加入原分页范围。

已绑定到工程修订的音频／视频可提交本地转写。宿主需安装 whisper.cpp，并设置 `OPENCUT_WHISPER_MODEL`（模型路径）；`OPENCUT_WHISPER_CLI` 可选，默认 `whisper-cli`。这些是宿主配置，不属于客户端请求：

```ts
const accepted = await client.call("opencut_analysis_transcribe", {
  idempotency_key: "transcribe-source-v1",
  project_id: "interview", revision: snapshot.revision, asset_id: "source",
  language: "zh",
});
const finished = await client.waitForJob(accepted.job_id);
const analysis = jobResult(finished, "analysis.transcribe");
// analysis.asset_version 固定内容哈希；segment.start/end 是素材时间，不是时间线位置。
for (const segment of analysis.transcription.segments) console.log(segment.text);
```

紧凑入口为 `client.callDomain("media", "analysis.transcribe", input)`。新任务复用既有 Job 观察／取消，通过 `result` 版本 1 返回段级时间和来源；旧 `submitJob` 枚举保持不变。Rust 使用 `tools::AnalysisTranscribe` 和 `extended_job_result::<jobs::kinds::AnalysisTranscribe>`。转写不会修改工程；Agent 将素材时间映射到剪辑时间线、提交 `edit_plan` 后再显式 `edit_apply`。当前 `words` 为空，无说话人识别；长素材、识别精度和真人访谈需单独验收。等待超时只停止观察，不取消服务器任务。

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

V4 工程使用 `opencut_project_v4_*` 和 `opencut_edit_v4_*`，输入/输出类型从 Rust 同源生成。inspect/create/copy 返回完整公共快照；promote/restore/apply 返回事务结果。资源绑定选择同一授权工程的固定资产/字体记录或已发布 media ID/version，不接受私有路径。undo/redo/navigation 使用版本化因果历史，计划只公开摘要和冲突原因。字体导入/派生、场景WAV/PNG/MP4/WebM、音视频区间和工程包均有版本化入口；实际准入范围与证据见迁移台账，不从类型声明推断完整支持。旧执行工具明确拒绝 V4。


`opencut_project_resource_inspect` 描述固定修订中的资产/字体；`projectV4ResourceUrl` 按范围重新计算 URL，`downloadProjectV4Resource` 以授权请求流式读取服务端校验过的 bytes。描述本身不保证 CAS 仍可用，hash 不是访问凭证。服务端复查当前授权与工程存活；查询参数不会成为路径片段。

`opencut_project_media_import_plan` 返回原样可提交的 `opencut_edit_v4_apply` 事务与明确的设置/放置决定。计划无 receipt，`original_request_digest` 不代替提交事务的幂等身份；apply 再检查素材范围、固定 head 和权限。Classic 首视频可能覆盖为较低全局 FPS；旧 Legacy scene profile 保持独立规则。

`opencut_project_audio_gain_plan` 在固定 head 上规划元素音量或独立静音。`set_decibels` 使用 −600…200 的整数 `decibels_tenths`，仅编辑静态 Decibels gain；−60dB 保留非零增益，静音和曲线另行保留。有元素音量关键帧或 Legacy linear gain 时拒绝静态改值，静音仍保留原音量语义。Audio 和已知有源音频的 Video 可用，Video 不会因此重新启用源音频。无变化返回 `transaction: null`，否则原样提交唯一 ElementPut；计划本身不写修订，不代表提交成功。

## 场景音频输出

`opencut_render_capability` 返回音频可用性与采样率。整段和范围共用 `opencut_render_submit` 的 audio 请求；可选 range 使用目标采样率下的半开样本区间。固定修订先按原混音和母带规则处理，再裁切区间，区间之外的峰值仍可能影响增益。可用性不代替提交时的 Read/RunJobs、资源和预算检查。

```ts
const capability = await client.call('opencut_render_capability', {});
if (!capability.audio.available || !capability.sample_rates.includes(48000)) {
  throw new Error(capability.audio.unavailable_reason ?? 'Audio export is unavailable');
}
const fixed = await client.inspectEditorProject({project_id:'demo'});
const accepted = await client.renderRevision({kind:'audio',request:{
  idempotency_key:crypto.randomUUID(),project_id:fixed.project.id,revision:fixed.revision,
  sequence_id:fixed.project.default_sequence_id,sample_rate:48000,
  range:{start_sample:'48000',frame_count:'48000'},
}});
const finished = await client.waitForJob(accepted.job_id);
const result = jobResult(finished,'render.audio');
const reference = finished.artifacts.find(a=>a.role==='primary');
if (!reference || reference.content_hash!==result.artifact.content_hash ||
    reference.size_bytes!==result.artifact.size_bytes) throw new Error('Missing matching WAV artifact');
const response = await client.downloadArtifact(reference);
// 大输出应流式消费 response.body；不要把整段结果一次性放入内存。
```

### Editing and evidence

Project inspection, planning, atomic apply, analysis, quality and rendering share the same Rust declarations and fixed-revision semantics. Planning does not commit or silently rebase. `jobResult` narrows the one successful and available result union; no result protocol version is selected.

Domain discovery uses one registry. The initial view is concise and selecting a domain expands its current supporting actions. Generate SDK/OpenAPI/tools from the same explicit integrated binary; do not edit generated DTOs by hand or run generation against a historical candidate as current-source evidence.

Interactive audio uses one OCAP PCM envelope and the audio-preview MIME. Its generated header binds the fixed identity, owner generation, overlay epoch, exact sample range, sequence and PCM hash. These authority clocks remain; development counters are absent. The client reports actual consumption, while buffering and producer progress never prove audible output. Existing scope/lease/cancellation/recovery and bounded queues remain.
