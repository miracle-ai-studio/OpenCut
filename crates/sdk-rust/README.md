# OpenCut Rust SDK

`opencut-sdk` 是同步 HTTP 客户端。输入、输出、工具名来自 `opencut-api-contracts` 的同一个注册表，八类 Job 结果来自 `opencut-job-contracts`。它不依赖 Application、SQLite、Worker、FFmpeg 执行库、图像／文字引擎或 Wasmtime。

```rust
use opencut_sdk::{Client, api::{self, tools}};

let client = Client::new("http://127.0.0.1:8788", "<host token>")?;
let snapshot = client.call::<tools::ProjectInspect>(&api::ProjectInput {
    project_id: "demo".into(),
})?;
// 保存完整请求。网络失败重试时保留 revision、幂等键和操作内容。
let request = api::EditInput {
    project_id: snapshot.project.id,
    request: opencut_sdk::project::TransactionRequest {
        expected_revision: snapshot.revision,
        idempotency_key: "caller-owned-stable-key".into(),
        label: "Rename".into(),
        operations: vec![opencut_sdk::project::Operation::UpdateSequenceSettings {
            sequence_id: "main".into(), name: Some("Final".into()),
            width: None, height: None, fps: None,
        }],
    },
};
let plan = client.call::<tools::EditPlan>(&request)?;
// 调用方检查计划后显式提交。
let transaction = client.call::<tools::EditApply>(&request)?;
```

`call` 的输入／结果关联类型由 sealed Tool 约束，不能把 EmptyInput 当成 EditApply 参数。`call_envelope` 保留 request ID 和 warnings。JSON 时间 ticks 保持字符串；SDK 不进行浮点换算或请求规范化。

## 失败和任务

- `Error::Api` 保留服务端 code/message/details、HTTP status、request ID、warnings。
- `Error::Transport` 和 `Error::Protocol` 不证明服务端尚未提交。SDK 不自动重试，也不读新 head 后修改请求；同一幂等键只能用于同一请求。
- `wait_for_job` 仅查询 Job，终态失败与取消分别为 `JobFailed` 和 `JobCancelled`，携带完整 Job。`job_result::<jobs::kinds::MediaProbe>(&job)` 检查成功状态和 kind，返回对应的具体结果。
- `WaitOptions.timeout` 覆盖请求与轮询；超时返回 `WaitTimeout`，不取消后台任务。取消观察的 AtomicBool 在阻塞 HTTP 调用之间检查、休眠期间至多每 25 ms 检查一次。进行中的 HTTP 调用最长受 request_timeout 限制；这不是即时网络中断 API。
- 只有显式 `cancel_job` 会请求任务取消。关闭客户端不会自动取消持久任务。
- 默认单次请求上限 10 秒、响应体 32 MiB，可通过 Options 调整。HTTP 重定向不跟随。使用 [ureq 3.2 阻塞 HTTP 接口](https://docs.rs/ureq/3.2.0/ureq/)；异步宿主应在阻塞执行器使用此客户端。

## 接入和验收

目前通过仓库 path dependency 或复制 SDK 与四个纯契约 crate 接入，未发布到 crates.io：

```toml
[dependencies]
opencut-sdk = { path = "/path/to/OpenCut/crates/sdk-rust" }
```

```sh
cargo test -p opencut-sdk
python3 scripts/check-contract-boundaries.py
python3 scripts/verify-rust-sdk.py
```

真实验收会复制 SDK 和契约源码到仓库外，编译运行、检查传递依赖、调用真实服务、用独立 CLI 回读、在后端提交后丢弃响应、验证手动重试与等待取消。随后安装 TS SDK，用同一 fixture 重放 Rust 提交，检查两个客户端取得同一事务。

新增公共工具与原有接口共计 27 个。新接入可用 `tools::JobSubmit` / `tools::JobView` / `tools::JobQuery` / `tools::JobChanges`，以及 `wait_for_public_job` / `cancel_public_job`。公共终态错误为 `PublicJobFailed` / `PublicJobCancelled`，携带 JobView；八类 `api::job_views::JobResultView` 可通过 Rust enum 匹配。`artifact_url` 校验任务／哈希元组并构造内容绑定下载地址；调用方仍需 Bearer 认证，目前没有 Rust 流式下载封装。

新视图不返回 prepared input、租约和原生路径；v1 JobRecord 兼容入口仍可调用。源任务输入中的路径字符串依旧表示服务端本机路径，不是远程 asset identity。SSE、上传下载封装及独立发行继续跟踪。完整约定见仓库 `design/architecture/public-job-view-contract.md`。

`tools::AccessAuditQuery` 是 owner-only 的权限管理历史查询；`api::access::AccessAuditTarget` 和 `AccessAuditAction` 提供 typed 目标／动作，策略快照为提交时的值，序号使用独立的十进制字符串空间。条数未满也可能因 4 MB 记录预算提前分页，按 has_more/last_sequence 续查。

`tools::MaintenanceQuery` 和 `tools::MaintenanceReconcile` 提供 owner-only 媒体发布日志与现场核对。`api::maintenance::PublicationPhase` 区分 prepared/completed 和事后 observed_*，后者不声称恢复了历史调用结果；PUBLICATION_OUTCOME_UNKNOWN 必须先检查 publication_id 对应记录与现场。

项目范围预检使用现有 `opencut_render_preflight`（Rust：`tools::RenderPreflight`），要求 read + run_jobs；能力资源 access.scoped_preflight 默认 false，新宿主为 true。受限调用只读取固定修订中的锁定内容；ASSET_LOCK_REQUIRED 需要 owner 固定素材，其他失败保留问题 code／实体 ID，去除原生路径。预检不会创建 Job，之后提交仍独立鉴权与验证内容。


项目素材导入使用 `tools::UploadCreate` 和 `api::uploads::CreateUpload` 创建会话，再用 `client.upload_chunk(&upload_id, offset_u64, &bytes)` 顺序写入原始分块。助手本地计算 SHA-256；单块为 1 byte–4 MiB，无自动重试，未知结果先调用 `tools::UploadInspect` 查询确认偏移。`tools::UploadSeal` 原子接受一个 media.import 任务；任务通过 `wait_for_public_job` 观察，`extended_job_result::<jobs::kinds::MediaImport>(&view)` 解出版本 1 的强类型结果，拒绝错误 kind、未完成和未知版本。旧 JobStartInput 八类枚举不包含导入。

`tools::ProjectMediaInspect` / `tools::ProjectMediaQuery` 返回仅本项目的素材；查询按 next_cursor 固定范围分页。创建／写块／封存／abort 要求 read + edit + run_jobs，查询要求 read。abort 的 UPLOAD_CLEANUP_PENDING 表示取消状态已提交而私有文件清理待重试。终态暂存可显式释放，上传预留配额已接通；完整工作区物理磁盘治理仍未完成。


`client.host_capabilities()` 获取类型化的增量宿主能力；可选 uploads.staging 描述实际配额和工具。`tools::UploadRetention` / `tools::UploadQuota` 是只读查询；`tools::UploadRelease` 清理 aborted 或终态 Job 的私有上传分块。新状态为 api::uploads::UploadRetentionState；它表示预留是否释放，不是文件存在证明。

上传按完整声明大小预留项目 8 GiB／工作区 32 GiB 额度；删除并同步后才在短事务内释放预留。UPLOAD_RELEASE_PENDING 需要查询并重试。默认不自动回收终态暂存，失败／取消任务保留历史；修正输入须用新创建幂等键。该配额不覆盖 CAS、导出产物和物理磁盘块。


丢失上传 ID 时，用 opencut_upload_query（Rust tools::UploadQuery）分页发现本项目会话，默认 retained_only=true；false 可读释放历史。页内同时返回 upload 与 retention，按 release_eligible 判断是否可释放。next_cursor 固定创建上界与过滤条件，状态仍取每页的当前值；查询期间新创建的会话不进入原分页，已释放记录会从 retained 筛选中消失。查询只读，不能把结果当成执行授权。


项目范围插件：`tools::ProjectPluginGrant/List/Inspect/Plan` 和 `api::project_plugins` 提供类型化契约。`host_capabilities()?.project_plugins` 是增量能力发现。Owner 授权精确包 SHA 和贡献，Agent 读取内联 JSON Schema 后对固定工程修订生成计划。`PlanView.result.operations` 只代表建议，须经独立 `EditApply` 提交；授权 CAS revision 与工程 revision 不可混用。


拒绝审计：`api::denials` 与 `tools::DenialQuery` 提供 Owner-only 分页历史。用 `Error::Api(failure).request_id` 关联拒绝，`failure.warnings` 明确日志降级；拒绝不因日志故障变成允许。`host_capabilities()?.denials` 发布 10000 条保留上限。查询不能越过保留窗口，`CURSOR_EXPIRED` 需要显式重新发现。

Owner-only `opencut_storage_inspect` returns typed, bounded storage observations. Byte counts are decimal strings; inspect `complete`, `stop_reason`, category omissions and `volume_observed`. Live observations do not reserve capacity or authorize garbage collection. Use `hostCapabilities().storage` / `host_capabilities()?.storage` for availability and limits.

CAS collection: discover with `opencut_cas_gc_query`, persist a review with `opencut_cas_gc_plan`, then explicitly execute the returned `plan_id` and exact `plan_hash` using `opencut_cas_gc_execute`. Inspect/retry the same plan after transport failure; `removing` is unresolved. Never infer deletion from acceptance, or reclaimed physical space from logical sizes. Query can return an empty filtered page with a next cursor. Discovery is in `hostCapabilities().cas_gc` / `host_capabilities()?.cas_gc`.


全局 owner 可用 `opencut_storage_budget_inspect/configure/recover` 管理持久写入准入。configure 使用 expected_revision 与稳定幂等键，返回带原 request_id/actor 的策略回执；重放不创建新策略。字节量是规范十进制字符串。默认 64 GiB payload 准入上限和 64 MiB 卷余量；当前只覆盖 CAS 捕获、上传分块/拼接和任务私有输入复制，`enforces_all_workspace_writes=false`。inspect 不恢复或创建状态，recover 是显式写操作；已写内容与完整活动预留可能重复计入，headroom 不是物理容量承诺。公开任务和上传保留明确的容量错误；终态 Job 需要新的显式尝试。
