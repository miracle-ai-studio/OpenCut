# 实施契约与分工

状态：进行中。以 `agent-friendly-editor-v1.md` 为完整范围，以下内容明确并行开发边界，不缩减最终交付目标。

## 架构修订

Application API 是中心；Embedded、Headless、Service 是三种宿主。独立 daemon 是可选宿主，不是 Domain 的运行前提。所有宿主执行同一事务／版本／权限契约。Web 使用 React/Vite 静态页面并连接独立 Rust 服务；保留 GPUI 外壳与 Moon 工具链，移除无编辑能力的云 API 示例。脚本 SDK 与移动端适配属于正式设计范围。

## 第一轮共享 Rust 接口

全部 crate edition 2024，继承 workspace package。公开接口先实现以下稳定骨架，字段扩充必须通知相邻负责者。

`opencut-contracts`（crates/contracts）负责 serde DTO：

- `ProjectDocument { id: String, name: String, schema_version: u32, default_sequence_id: String, sequences: Vec<Sequence>, assets: Vec<Asset> }`
- `ProjectSnapshot { project: ProjectDocument, revision: String }`
- `CreateProject { id: String, name: String, sequence_id: String, width: u32, height: u32, fps: FrameRate }`
- `TransactionRequest { expected_revision: String, idempotency_key: String, label: String, operations: Vec<Operation> }`
- `TransactionResult { transaction_id: String, base_revision: String, revision: String, created_ids: Vec<String>, changed_ids: Vec<String>, deleted_ids: Vec<String> }`
- `EditPlan { base_revision: String, proposed_project: ProjectDocument, created_ids: Vec<String>, changed_ids: Vec<String>, deleted_ids: Vec<String> }`
- `EditorError { code: String, message: String, details: serde_json::Value }`，实现 Error/Display；`EditorError::new(code, message)`。
- `Time { ticks: String, timebase: u32 }`、`FrameRate { numerator: u32, denominator: u32 }`、`TimeRange { start: Time, duration: Time }`。
- 第一轮仅主分支，由应用层显式拒绝不支持的分支；不能伪造已经实现多分支。

`opencut-editor-core`（crates/editor-core）：

- `create_project(input: &CreateProject) -> Result<ProjectDocument, EditorError>`
- `validate_project(project: &ProjectDocument) -> Result<(), EditorError>`
- `plan_transaction(snapshot: &ProjectSnapshot, request: &TransactionRequest) -> Result<EditPlan, EditorError>`
- 全部操作在克隆快照上计算，失败不暴露部分状态。禁止 UI、文件、数据库依赖。

`opencut-editor-application`（crates/editor-application）：

- `EditorApplication::open(path: impl AsRef<Path>) -> Result<Self, EditorError>`
- `create_project(&mut self, input: CreateProject, idempotency_key: &str) -> Result<ProjectSnapshot, EditorError>`
- `list_projects(&self) -> Result<Vec<ProjectSnapshot>, EditorError>`
- `snapshot(&self, project_id: &str) -> Result<ProjectSnapshot, EditorError>`
- `plan(&self, project_id: &str, request: &TransactionRequest) -> Result<EditPlan, EditorError>`
- `apply(&mut self, project_id: &str, request: &TransactionRequest) -> Result<TransactionResult, EditorError>`
- SQLite 原子保存修订、事务、请求哈希／幂等结果与事件。幂等匹配先于版本检查。相同键不同请求必须冲突。

`opencut-media`（crates/media）：

- 独立媒体探测／渲染后端，结构化调用 FFmpeg/ffprobe，不拼接 shell。
- 先提供自身清晰的请求／输出 DTO 和真实媒体测试，随后由 Application 适配为 Job，避免并行期间绑定未确定模型。
- 能力如未实现必须显式拒绝，不能忽略效果／字幕／音轨后返回“完整成功”。

## 文件所有权

| 负责者 | 独占编辑范围 |
|---|---|
| 子 Agent core | crates/contracts、crates/editor-core、crates/time |
| 子 Agent application | crates/editor-application |
| 子 Agent media | crates/media |
| 主 Agent | 根 Cargo.toml／锁文件协调、apps/cli、协议适配、设计文档、全局集成与验收 |

子 Agent 不修改根 workspace、不格式化其他负责者文件、不删除现有项目或 tmp。需要接口变更先相互发消息。完成一轮后继续分配下一轮，不把首轮骨架当完整项目。

## 本轮验收

原子编辑与回滚、重复请求去重、过期修订冲突、保存重开、真实 ffprobe/FFmpeg 产物读取；之后接通 CLI/MCP 与 UI 并继续完整功能矩阵。

## 后续集成补充：修订素材版本

本节补充上面的第一轮骨架；实际验收证据维护在 `implementation-status.md`。

- `Asset.version: Option<AssetVersion>`：版本包含 `id, content_hash, size`。逻辑 `Asset.id` 保持稳定，内容版本 ID 由宿主生成；None 在 JSON 中省略，旧请求身份保持兼容。
- `Operation::ReplaceAsset { asset }`：对应 `asset.replace`，要求已有逻辑 ID，保留 clip 引用；最终工程范围／类型验证失败则不提交。
- `EditPlan.unresolved_asset_ids`：纯计划中尚未固定版本的素材 ID；空集合在 JSON 中省略。它不是渲染依赖清单，也不是文件可用性证明。
- Application 是资产存储路径的权威。Automation 使用 `EditorApplication::asset_store_root()`，不得重新推导内存实例的路径。
- 提交顺序：原请求幂等查找、head 检查、事务外 pin、写事务内再次幂等／head 检查、纯 core 计划与原子提交。请求哈希使用原请求；操作日志和修订记录已准备好的版本。
- 预检只读解析固定内容；Job 保存必需素材锁，Worker 校验其与修订版本一致后复制私有输入。必需版本素材无锁时报错；旧无版本任务保留明确的 legacy 语义。
- Web 导入／替换提交捕获的工程和修订。界面切换不得撤销已提交事务，过期预览及完成回读不得覆盖另一连接／工程／历史视图。

后续已落盘 `job-contracts`：Automation 和 Worker 共同依赖 prepared inputs/results，Worker 移除对 Automation 的依赖；Application 继续持久化 opaque JSON，没有引入媒体执行依赖或数据库迁移。prepared 输入的可选 lock 为 None 时省略，以保留旧幂等请求身份。八类任务成功结果 Schema 由 capabilities.job_kinds 公开。

`ResolvedProjectSnapshot` 已改为内部字段私有且不支持 Serde 的类型；构造必须经过显式 resolver 流程，resolved 请求固定其 sequence。它封闭普通快照误传的入口；字节验证仍由宿主 CAS／私有副本 resolver 负责。当前验收进度见实现状态，完整范围不因此缩减。

## 直接合成工程帧接口

- `preview.composited_frame` 复用公开 `ProjectFrameInput` 和 prepared `PreparedProjectFrameInput`，新增独立 `CompositedProjectFrameJobResult`／`CompositedProjectFrameQuality`，旧编码帧 quality 枚举保持不变。
- `ResolvedVisualProjectSnapshot` 只证明视觉依赖经过显式解析；它与全导出的 `ResolvedProjectSnapshot` 是不同类型，不能混用。
- `required_visual_asset_ids` 只包含启用的视频／图片轨素材。音频轨参与序列结束时间，音频文件不参与视觉素材锁、探测或媒体 graph。
- `plan_frame_resolved` 固定修订和 sequence，调用与旧编码帧共享的精确帧选择函数；请求和对齐结果都必须在半开范围内。
- `TimelineFrameRenderRequest { timeline, sequence_time }` 复用视觉合成语义，直接输出 PNG。Worker 校验结果时刻、构造结构化产物，沿用租约和取消契约。
- 新帧 attempt 被回收后清理整个未发布私有目录；成功当前 attempt 的公开 PNG 保留。普通返回和 panic 由临时文件 guard 清理。
- Web 直接帧不运行包含音频依赖的完整导出 preflight，由视觉规划器验证；保留请求上下文防止旧结果覆盖新工程。

具体通过证据见 `implementation-status.md`，接口落盘不等于完整预览系统完成。

时间坐标补充：源帧 `preview.frame` 的 requested_time 是实际源流 PTS 时间域；工程帧请求是 sequence 时间域。工程 clip 的 source_range 使用素材相对时间，不能把容器的非零 PTS 起点直接填入 source_range；本轮 VFR 样例的 source_start=0、sequence=0.6、0.5 倍速，对应实际源 PTS 8.3。多流起点偏移的进一步矩阵验收不能由单视频流样例替代。

## 区间音视频预览

- 新增第八类任务 `preview.project_range`。`ProjectRangeInput` 为 project/revision/sequence、requested_range 与 alignment；`PreparedProjectRangeInput` 保存不可变 snapshot、同形范围与可选素材锁。公开请求不接受 codec/container，首版固定 H.264/MP4。
- `ProjectRangeJobResult` 返回 requested_range、actual_sequence_range、alignment、独立 quality `encoded_project_range`、media、artifact 与锁字段。保留结果向前读取及旧结果 legacy_unlocked 规则；发布前执行同一跨字段验证。
- `plan_range_resolved` 只接受完整 `ResolvedProjectSnapshot`，不能接收 visual-only 类型。先使用正式导出规划，再用精确有理时间对齐区间端点；后端在绝对序列时钟执行动画、字幕、音频，末端裁取并归零输出 PTS。
- 不渲染完整中间 MP4；可以解码／处理输入前缀。当前依赖集合是完整序列必需音视频素材，不做区间依赖裁剪。
- 正常完成只保留公共 preview.mp4；失败、取消、unwind 清理私有输出／字幕／inputs。恢复时清理旧 preview attempt 整目录，保留当前成功产物。正式导出仍保持原清理边界。
- Web 的区间预览使用完整音视频预检；直接 PNG 使用视觉解析。所有预览与源预览共享意图代次隔离，旧下载不得覆盖新视图；已接受的正式导出仍独立继续运行。
- 验收入口 `scripts/verify-project-range.py` 检查实际非零区间的 RGB、音频频率与包络、固定修订、幂等、锁和活动中取消；`verify-asset-recovery.py --job-kind preview.project_range` 检查真实进程终止、自然租约恢复及私有目录清理。执行结果单独记录，不以脚本存在作为通过证据。
