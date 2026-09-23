# R2 项目导入核心验收 — 2026-09-21

## 结论

原生核心已接通“封存上传 → 持久任务 → 校验／探测／固定 CAS → 原子登记项目素材 → 受限主体绑定 → 实际预览”。图片、视频、音频通过真实媒体检查；图片还经过删除原源、绑定、Worker 预览与独立 RGB 解码。

这不是项目导入整体交付：公开上传控制工具、HTTP 字节路由、CLI/双 SDK 导入助手、清理／总磁盘配额和仓库外新包验收尚未完成。当前公共发现仍为 36 工具／8 种任务；`media.import` 目前通过原生 ImportStore 接受并由 Worker 执行，尚未作为公共可启动能力注册。Web/GPUI 实现按要求暂缓。本轮未打新包，最近已验收包仍为 `dev-0046d31b1081`。

## 实现边界

- `editor-application/uploads.rs` 定义 ImportStore、封存清单和结果一致性规则；没有原生 I/O。内部 PreparedMediaImportInput 在 job-execution，不由客户端提供。
- SQLite 在同一写事务内检查当前 read + edit + run_jobs、完整确认偏移与连续块清单，然后封存会话、创建唯一任务、保存权威 job_projects、事件和审计。重复封存先授权，再返回同一个 job_id；导入不移动项目 head。
- Worker 逐块校验长度／SHA-256，再校验整文件 SHA-256。attempt 私有文件在成功、错误及 unwind 时清理；成功探测后固定 CAS，输出 `opencut-media://media_id`，不返回暂存路径或 owner 全局媒体 URL。
- 合法 attempt 的任务完成事务内，校验结果与封存会话、job_id、内容版本和登记身份一致，再插入 project_media、媒体事件及任务成功审计。任一步失败全部回滚。job audit 改读权威 job_projects，媒体登记审计关联原始提交 request ID。
- 素材登记是工程修订之外的资源。受限 asset.bind／asset.replace 只接受当前项目登记的资源，URI、版本、kind、duration 和元数据必须一致；允许编辑逻辑 asset ID 与显示名。准备前和实际提交事务里均检查，不能凭已知 CAS hash 或其他项目 media_id 绑定。

## 通过动线发现并修正

1. 旧 Job 完成语义允许未观察到取消的后端提交成功。这与新导入的资源发布要求冲突。现在 media.import 在同一个事务里观察到 cancel_requested 时转 cancelled，不写结果／素材。旧八类任务保持既有 cooperative 语义。成功先提交时，随后取消不回滚已有资源。
2. 上传内容可能是引用其他文件的播放列表。新增 probe_upload 使用受限 demuxer 白名单、file 协议和禁用 MOV 外部 data reference 的选项；没有开放 concat/HLS 等播放列表。探测只输出所需字段，stdout 1 MiB、stderr 64 KiB、30 秒预算，支持取消并终止／回收子进程。这是本次探测边界，不宣称已完成通用操作系统沙箱。
3. 扩展新任务不能让旧结果枚举导致整个 Job 查询解码失败。JobView 新增可选 extended_result 和 resource_target；八类旧 JobResultView 枚举保持不变。新导入结果在 Host 先做类型与身份校验，再投影为 kind + schema_version + data。非法字段不原样透传。

## 实际证据

本机 FFprobe：8.1.2，Apple clang 16.0.0。相关 Rust 111 项（含 1 项 doctest）通过，另媒体库 11 项通过，总计 122；TS SDK 5 项及类型构建通过。

- SQLite 73 项：新增封存／入队原子回滚与幂等、撤权后拒绝封存回放；完成审计失败时任务和素材一起回滚；伪造结果 URI／元数据被拒绝；取消、过期和已被接管的 attempt 不登记资源；成功后迟到取消不改变结果。检查覆盖 job_events/job_projects，而非只数 upload 表。
- `job-runtime/tests/project_import.rs` 三项真实动线：
  - 生成 PNG、71 字节分块上传并封存、删除原源、真实 Worker 导入；受限主体重命名绑定，精确编辑重放；两项目均有权限时仍不能直接绑定另一项目资源；伪造 URI、hash、kind、metadata 被拒绝。实际预览后用独立 FFmpeg 解码 RGB，核对 64×48 图像及红色中心像素。
  - 损坏已确认分块、错误整文件 hash、引用实际宿主 PNG 的 ffconcat 清单全部任务失败；无项目素材记录，无 head 变化，私有 attempt 输入已清理，公开 Job 不含宿主根路径。
  - 生成 MP4/H.264 和 WAV/PCM，删除原源后导入。实际字节决定 video/audio 分类和 duration；重新读取 CAS 与原字节逐字节一致，公开结果不含宿主路径。
- `media/process.rs` 使用真实子进程验证 stdout／stderr 超限、运行中取消、超时及正常输出回收；既有媒体逻辑测试通过。
- 原有上传双 SIGKILL 窗口、并发块冲突及落盘后撤权／中止复验通过。**本轮没有把它们称作导入 Worker 的 SIGKILL 验收**；导入进程自然租约恢复尚待公开动线阶段补齐。
- 历史兼容 17 组／16 格（v1–v8 × legacy/v2）通过：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-e7k_1tg0/report.json`。
- Clippy、fmt、纯依赖／分层检查、生成契约一致性通过；原 22 工具和两个媒体发布工具的 Schema 保持不变。SDK／OpenAPI／Web 契约导出已同步。

Debug binary SHA-256：`a29456e9e234cad26d154d4bd4932f87672cba94ab10534cd7fe10ddc3c80a7b`。

本机执行记录：`/tmp/opencut-import-final-tests.log`、`/tmp/opencut-import-media-tests.log`、`/tmp/opencut-import-journey.log`、`/tmp/opencut-import-final-clippy.log`、`/tmp/opencut-import-build.log`、`/tmp/opencut-import-history.log`。这些结果证明本轮对应源码和二进制，不代表后续修改自动通过。

## 剩余交付

公开控制工具与字节路由、资源列表分页、CLI 流式／续传助手、双 SDK 类型化导入结果与调用、能力与错误／限额发现、MCP 数据面交接、终态与孤儿块清理、磁盘总配额、导入 Worker 崩溃恢复、失败恢复策略以及仓库外新包验收仍需完成。完整 R0–R7 目标保持不变，本轮不将 R2 标记完成。
