# OpenCut 本地开发交付包

这是 CLI、HTTP、stdio/Streamable HTTP MCP 与独立 SDK 的本地开发包，不是 R0–R7 完整产品发布。网页、桌面客户端和其他平台二进制不在本包中。具体 CPU/OS 目标、版本和文件 SHA-256 见 `manifest.json`；不包含工程、素材、认证 token 或用户配置。

2026-09-23 当前打包入口从被测二进制读取实际数据库 schema、操作数和领域数写入清单；不再使用历史硬编码版本。默认 MCP 发现九个领域工具，61 个旧叶操作仍可调用。生成 OpenAPI 覆盖 75 条叶操作、领域及原始上传／下载／SSE 路径；后面各节说明数据面的恢复语义。当前恢复源码的包仍须以独立验收报告判定，打包成功本身不是放行。

## 解包和启动

先用 archive 旁的 `.sha256` 文件校验下载内容，在一个新目录解包；保留目录结构与可执行权限。在解包后的根目录运行：

```sh
./bin/opencut --version
python3 doctor.py
./bin/opencut --json system schema
./bin/opencut --json system capabilities
./bin/opencut --json system domains
./bin/opencut --json system domains --domain edit --schemas
./bin/opencut --database /your/workspace/workspace.sqlite serve \
  --listen 127.0.0.1:8788 --token-file /your/workspace/api-token
```

`doctor.py` 需要 Python 3，用 JSON 报告本包文件校验、主程序能否执行以及 PATH 中的 FFmpeg/ffprobe；不会打开或修改 workspace。CLI 本身不依赖 Python。媒体操作需要 FFmpeg/ffprobe；字体由宿主提供，指定严格字体不存在时预检/任务会明确失败。检查通过不代表所有编码器、字体或功能可用。

`serve` 同时运行持久化 Worker。直接 CLI/MCP 接入时，可另开 `./bin/opencut --database /your/workspace/workspace.sqlite worker run`。CLI `--json` 输出一个结构化 envelope；stderr 留给诊断。MCP 入口是 `./bin/opencut --database /your/workspace/workspace.sqlite mcp serve`。新版入口协商 2025-11-25，支持 tools、resources/templates、prompts 与每连接最多 32 个资源订阅；资源通知用于重新读取，不是持久化日志。不要把 HTTP token 写进源码或 MCP 参数。

HTTP 公共事件流为 `POST /v1/changes/stream`，body 使用 EventChangesInput；SSE data 与 CLI 使用同一 StreamRecord。每条 event 的 id 可作为 Last-Event-ID 重连，优先于 body cursor；不要混用 MCP 回放 ID。

## 提交来源

新增 `opencut_audit_query` 可通过通用 call 查询工程事务、任务及执行 attempt 的持久化来源，例如 `{"target":{"kind":"project","id":"your-project"},"after_sequence":"0","limit":100}`。序号使用十进制字符串；审计与业务事务原子提交，幂等重试保留原来源，原始请求可沿 Worker 记录追踪公开产物 ID。旧数据不回填虚构 actor。

本机 OS 调用者及共享 token 当前统一表示 local-owner，source 区分 CLI/HTTP/两种 MCP；它不是具体人员或模型身份。Worker 表示 local-runtime。受限调用记录验证后的 principal；网络入口沿用相同项目授权。owner 可查询权限、媒体与插件发布历史；来源记录本身不授予访问权。

## 受限 CLI、HTTP 和 MCP

从数据库 v5 起支持项目范围的 principal policy 和可单独撤销的凭证。CLI 不指定凭证时仍代表本机 OS owner；指定 `--credential-file FILE` 后按当前授权执行。`access set-policy --input FILE` 原子更新策略（新主体 `expected_revision: "0"`）；`access issue PRINCIPAL --output NEW_FILE` 把凭证写入不存在的新文件，Unix 权限 0600，stdout 不输出 secret。`access revoke CREDENTIAL_ID` 撤销单个凭证，`access inspect` 或 `opencut_access_inspect` 读取当前权限。

policy 例：`{"principal_id":"reader","expected_revision":"0","enabled":true,"grants":[{"project_id":"demo","actions":["read"]}]}`。可选权限为 read/edit/restore/run_jobs/cancel_jobs/read_artifacts/read_audit；其他权限须伴随 read。CLI/stdio MCP 重新检查凭证与权限，写事务内再次检查；撤权后幂等回执也不能绕过授权。受限主体不能创建项目、提交任意宿主路径或运行 Worker/插件/全局媒体管理。

受限 CLI 的 job start/get/list/cancel/wait/events 返回公共视图或游标页面，分页使用 opencut_job_query/opencut_job_changes 续查；owner 保留旧形状。HTTP REST 和 MCP Streamable HTTP 接受签发凭证作为 bearer；旧 raw Job 接口仍仅供 owner，受限客户端使用公共工具入口。产物下载要求 read_artifacts，逐块复查权限；SSE/下载在撤权后结束，已经发送的字节不能收回。当前只是应用层限制，不隔离具有数据库写权限的本机 OS 进程。

`access events --input QUERY.json` 或通用 `opencut_access_audit_query` 读取已提交的权限管理记录，只允许 owner。示例：`{"target":{"kind":"principal","id":"reader"},"after_sequence":"0","limit":100}`。HTTP／两种 MCP 及 SDK 共享该工具。结果包含原始 request/actor、credential ID 和策略提交快照，不含 secret 或摘要；重复撤销没有新记录，失败操作不记为已提交。序号与工程／任务审计分离，按 has_more 和 last_sequence 续查，最多 500 条且受 4 MB 记录预算限制。覆盖范围从 schema 5 开始，不回填更早操作。

## 项目范围预检

`opencut_render_preflight` 接收 project_id、revision、sequence_id、container、video_codec。受限调用要求 read + run_jobs，仅 read/edit 不足；capabilities.access.scoped_preflight 表示此能力。预检不创建任务、工程历史或产物，ready=true 之后仍需显式提交固定修订任务，提交时独立检查权限与内容。

受限预检仅解析已锁定的工程内容，原文件删除也可继续；旧未锁定素材返回 ASSET_LOCK_REQUIRED 和 asset_id，需要 owner 先固定内容。损坏／丢失的 CAS 不回退原 URI，诊断保留 code 与实体 ID，去除宿主路径、命令行和 stderr。owner 保留已有详细诊断。移除 run_jobs 或撤销凭证阻止后续请求；不取消已经获得读取资格的预检或既有 Job。

## 项目素材上传与导入

先调用 `opencut_upload_create`，提交 `project_id`、显示名称、整文件小写 SHA-256 `content_hash`、十进制字符串 `size_bytes` 和幂等键。创建会话不会改变工程 head。按 `capabilities.uploads` 给出的路径顺序发送原始分块：`PUT /v1/uploads/{upload_id}/chunks/{offset}?size_bytes=...&content_hash=...`，请求体是字节流，Bearer 身份需要项目 `read`、`edit`、`run_jobs` 权限。当前单文件上限 1 GiB、单块 4 MiB、最多 4096 块；实际并发数与接收超时以运行中宿主的 capabilities 为准。传输中撤权不会确认未完成分块；响应结果不明时先 `opencut_upload_inspect` 读取 `next_offset`，再重试相同未确认分块。

本地 CLI 使用 `upload put <upload_id> --offset <十进制偏移> --input <本地文件或 ->`。路径只由本地 CLI 读取，不进入服务端编辑请求。MCP 提供创建、查询、封存等控制工具；大字节数据通过 HTTP 或本地 CLI 传输，不能塞入 JSON base64。TypeScript SDK 的 `uploadChunk` 和 Rust SDK 的 `upload_chunk` 会计算分块摘要并发送认证请求。

完整会话由 `opencut_upload_seal` 接受为 `media.import` 持久 Job。成功后的公开 JobView 在 `extended_result` 中返回 schema 版本 1 的导入结果；TypeScript `publicJobResult` 和 Rust `extended_job_result` 可按类型读取。结果中的 `media_id` 只可在所属项目绑定，导入本身不修改时间线。`opencut_project_media_inspect`／`opencut_project_media_query` 用于有项目读取权的后续发现。

授权客户端用 `GET /v1/projects/{project}/media/{media_id}/content` 读取导入素材字节；服务端先按当前项目 `read` 权限解析资源，再从固定 CAS 版本打开并校验完整 SHA-256／长度，然后返回检测到的 MIME 和长度。读取中策略变更按持续响应规则复核。`media_id` 与资源 URI 不授予全局媒体目录访问权；旧 `/v1/media/{hash}` 仍是 owner-only 兼容接口。TypeScript SDK 提供 `downloadProjectMedia`，Rust SDK 提供 `project_media_url`。当前 Web 的原生素材播放器会把授权响应读入浏览器 Blob；长素材的流式播放与内存边界仍需 R4/R7 单独验收。

`abort` 仅处理仍在接收的会话；`UPLOAD_CLEANUP_PENDING` 表示状态已提交但私有字节尚未清完，应重试 abort。封存后的任务通过 Job 取消。终态上传暂存的保留和释放使用独立工具；完整物理配额与自动回收仍未完成。备份整个 workspace 时必须包含 `uploads` 目录。

## 媒体发布与未知结果

从数据库 v6 起记录已接入的媒体内容／元数据发布和修复。`opencut_maintenance_query` 查询 owner-only 追加日志；`opencut_maintenance_reconcile` 在同一对象锁内读取实际指纹，核对 pending 后追加 observed_applied / observed_unchanged / observed_diverged。它不改写文件，也不把现场相同伪称为历史调用已正常完成。

当上传／修复返回 PUBLICATION_OUTCOME_UNKNOWN（HTTP 500、CLI 8）时，文件可能已经改变。保留 details.publication_id，通过通用 call 查询 `{"target":{"kind":"publication","id":"..."}}`；核对输入为 `{"object":{"kind":"media_metadata","content_hash":"..."}}` 或 media_content。先核对再决定重试。正常 completed 与 prepared、事后观察分别记录，独立序号分页。重复内容上传必须验证已存在字节，发现损坏不会静默覆盖。

这不是文件系统与 SQLite 的原子事务；媒体记录从 v6 开始，不回填历史。插件发布后来接入独立 Journal。旧程序不能打开已迁移的库，回退须使用整份升级前备份副本。

## SDK 与契约

- TypeScript：在消费项目运行 `npm install /path/to/bundle/sdk/opencut-sdk-0.1.0.tgz`；运行时要求 Node 22+。包内含编译 JS、类型和 README，没有仓库路径依赖。
- Rust：解开 `sdk/opencut-rust-sdk-0.1.0.tar.gz`，保留整个目录。将消费项目的 `opencut-sdk` path 指向解包目录的 `crates/sdk-rust`。SDK 与四个纯契约 crate 一起交付，第三方依赖由 Cargo 按 lockfile 获取；不是离线 vendor 包，也未发布 crates.io。
- `contracts/` 含工具 Schema、OpenAPI 和跨 SDK 编辑夹具。API 版本为 1；重试必须保留 revision、幂等键与操作内容。观察取消与后台 Job 取消是不同操作。
- `dependency-inventory.json` 列出宿主构建依赖版本和声明的许可；本包仅作本地开发交付，未执行应用商店签名、公证、包管理器或公共 registry 发布。

## 升级、失败与回退

1. 停止旧服务、Worker 和其他写入同一 workspace 的进程。
2. 备份整个 workspace 目录，包括 SQLite/WAL 文件、asset-store、uploads、media、plugins 和任务产物；不要只复制主 `.sqlite` 文件，也不要在写入过程中复制目录。
3. 保留旧二进制和备份，在新目录解开新包并先运行诊断。启动新程序指向原 workspace；支持的 v1–v15 历史格式依次以原子迁移升级到 `manifest.json` 中的 `database_schema`，同版本原样打开。未知版本明确拒绝。
4. 升级验证失败时停止新进程，把整份备份复制到另一个 workspace 路径，用保留的旧二进制打开该副本。不要让旧二进制直接降级打开已升级的库；本包不提供逆向 schema 迁移。

兼容验收证明支持格式的读取、升级与回执保留，不证明任意历史发布版本的双向互通。迁移失败应保留原数据；故障排查保留 stderr、结构化错误、Job ID、revision 和诊断结果，分享前移除 token 与私有文件路径。已接受的任务保留在库中，进程退出不等于取消；重启后按租约与恢复规则继续。

## 构建与验收入口

仓库中运行 `python3 scripts/package-local.py` 构建 release 二进制并产生 `dist/local/*.tar.gz`、校验文件和机器可读清单。打包成功仅标记 `packaged_not_yet_accepted`。

随后运行 `python3 scripts/verify-local-package.py <archive>`，在仓库外校验/解包、执行诊断、安装 TS SDK、编译 Rust SDK、启动解包程序并走共享编辑和真实媒体任务。历史格式另由 `scripts/verify-history-compatibility.py --binary <解包的 bin/opencut>` 检查。只有实际验收结果可以支持对应平台的交付声明。


## 项目插件

Owner 安装插件后，用 `opencut_project_plugin_grant` 以 target(project/id/version)、实际 package_sha256、operations、enabled、expected_revision、idempotency_key 设置项目策略。受限 Agent 可通过 list/inspect 发现内联参数契约，并用 plan 获取经验证的操作。执行需 read/edit/run_jobs，发现需 read；包身份和全局权限同时校验。授权更改不改变工程 head，计划必须显式 edit_apply；查看随源码提供的 `project-plugin-contract.md` 了解锁内撤权、预算和错误语义。完整隔离及其他插件类别不属于本轮 planner 范围。


## 拒绝审计与故障排查

Owner 使用 `opencut_denial_query` 查询服务端 request_id、principal 或 credential 的拒绝历史，最多保留 10000 条，不保存请求体、token 或原始 URL。HTTP 返回 `X-OpenCut-Request-Id`，MCP 工具/资源错误携带请求标识；详细约定见包内 `denial-audit-contract.md`。日志写入故障仍拒绝原操作，并通过 `DENIAL_AUDIT_UNAVAILABLE` warning／HTTP 头或后台 stderr 诊断报告；这不代表原写入成功。保留窗外查询显式过期，数据库备份涵盖当前保留窗，不等于完整永久安全日志。

## 存储占用观察

Owner 可以用 `opencut_storage_inspect` 查询六类托管数据的实际文件长度和分配块；`max_entries` 可降低扫描预算。结果为 live_non_atomic，必须检查 complete、stop_reason 和遗漏计数，不得将观察当成预留成功、独占空间或删除计划。该版本提供显式 CAS 孤儿回收，尚不执行全局物理配额或自动 GC，Unix 以外平台未提供扫描，内存数据库不支持；完整范围见包内 `storage-governance-contract.md`。

## 显式 CAS 回收

Owner 通过 cas_gc_query 发现未完成计划，通过 cas_gc_plan 创建可审阅计划，随后用 cas_gc_execute 明确提交准确 plan_id/expected_plan_hash。断线或进程退出后用 cas_gc_inspect 核对并重试同一计划。工具名均带 `opencut_` 前缀。务必先停止不参与新锁协议的旧 Host/Worker，再升级整个专用工作区。多数据库/别名布局明确拒绝回收。完整范围、根保护和逐项状态见包内 `cas-gc-contract.md`。


## 持久写入准入

本版同时提供 `opencut_storage_budget_inspect/configure/recover`。默认 64 GiB payload 逻辑准入和 64 MiB 卷观察余量，当前覆盖 12 类受控写入；SQLite/WAL、控制元数据和完整保留／回收仍未接入，`enforces_all_workspace_writes=false`。配置使用策略 revision 和稳定幂等键，返回可信 request_id/actor 回执。恢复只释放已失去 OS 存活锁的预留，不删除实际文件。详见包内 `storage-admission-contract.md`。

2026-09-24 控制额度增量：`opencut_storage_budget_configure` 可选 `control_limit_bytes`，默认 4 GiB，旧请求省略时保留现值；`inspect.control` 单独报告控制限额、已观察字节、活动预留及实际拦截状态。控制账本和原生预留已实施，但 SQLite 事务尚未接入写前拦截，`write_interception_enabled=false`；配置控制限额**不会**限制当前数据库写入。详见包内 `database-control-admission.md`。

升级必须停掉旧 Host/Worker，旧进程不遵守新预留协议。备份/恢复必须包含 `.storage-budget` 和其他工作区内容；其账本版本独立于 SQLite schema，当前宿主 schema 由包清单给出。不要手工删除活动预留的锁文件或仅复制策略文件。新请求可能明确被准入额度拒绝；已有文件/任务历史保留，检查错误后调整策略或显式清理再重试。

全局媒体上传/修复、插件包/授权与产物下载验证副本已纳入准入。匿名下载副本的额度会保留到响应流结束或断开；取消响应等待不代表后台 IO 已停止，只有文件关闭后才释放。详细生命周期与剩余未覆盖写入见 storage-admission-contract.md。


Worker 输出与字幕现已接入准入。Owner Configure 可选 media_output_limit_bytes（十进制正数字符串，默认 1073741824，省略保留当前值）；inspect/新变更回执在顶层返回该值，Policy 原结构保持兼容。实际单文件 ceiling 还会受当前逻辑与卷余量限制。MEDIA_OUTPUT_LIMIT_EXCEEDED 表示没有发布成功产物，应核对容量/策略后显式提交新任务。预留携带 job_id/attempt，FFmpeg 与只读探测继承 attempt 存活锁，旧 Worker 被杀后不能仅因租约超时释放其存活子进程的额度或删除目录。详见包内 native-child-write-contract.md；全工作区硬物理配额仍未完成。
