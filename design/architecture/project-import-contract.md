# 项目素材导入契约（实施中）

本文件定义后续上传与导入的完整目标，不把端口、状态机或局部检查当作已交付导入能力。Web/GPUI 仅冻结接入约定。

## 动线与身份

创建项目上传会话 → 写入有界块 → 封存并接受持久化 media.import 任务 → Worker 校验完整内容、探测并固定 CAS → 原子发布项目素材记录与任务成功 → Agent 以项目素材引用绑定／替换资产 → 显式编辑、预检、预览或导出。导入不会隐式移动项目 head，素材元数据来源于实际字节而非文件扩展名或调用者自报 kind。

创建、写块、封存和取消要求项目 read + edit + run_jobs；查询要求 read。每一步按上传记录里的权威 project_id 检查当前凭据，不能由客户端改写归属。幂等回执也先鉴权。同项目已授权编辑者可接手续传；credential 是可撤销身份，不是写入请求中的 actor 字段。owner 的全局媒体上传保留兼容，但不由项目权限开放。

## 控制面与字节数据面

控制面为纯 DTO／Application 用例，CLI/REST/MCP/SDK 共用。创建提供 project_id、name、size_bytes、content_hash、idempotency_key；名称仅作显示，不用来拼接文件路径。size/offset 使用规范十进制字符串，完整内容和每块指纹均为小写 SHA-256。客户端不能仅凭已知 hash 引用其他项目内容，必须持有本项目的上传／素材记录。

字节通过 `PUT /v1/uploads/{upload_id}/chunks/{offset}?size_bytes=<规范十进制>&content_hash=<SHA-256>` 发送，body 为 application/octet-stream。单块上限 4 MiB，单会话上限 1 GiB／4096 个已确认块，大小和完整 hash 在创建时确定。最多 16 个并发块请求，接收预算 60 秒，接收中约每 250 ms 及结束时复查授权；超时／撤权不 ACK。首版顺序写入，offset 必须等于已确认 next_offset；已确认块的精确重放允许成功，偏移相同但长度／指纹不同返回冲突。上传控制面不接收视频 base64；CLI 的本地输入路径只由本地客户端读成字节，不传给宿主执行。

创建幂等按 project + key 识别；同键参数不同拒绝。状态 receiving / sealed / aborted，封存后禁止再写块，取消不能把已接受的导入任务伪装成未发生。断开数据连接不取消会话；只返回已完整持久化并记账的偏移，Agent 先查询偏移再重试。查询不创建目录或修补会话。

## 一致性与恢复

原生适配以不可变私有块暂存，每块实际长度／hash 与声明一致，文件及目录同步完成后，SQLite 在短写事务内重新鉴权、检查状态／偏移并原子记录块和 next_offset。临时块不是已发布素材；数据库失败不能确认偏移。文件先落地、记录未提交时可留下私有孤儿块，精确重试重新验证字节并复用，不能把文件存在当成会话已确认。不同进程通过 per-upload OS 锁串行化协作写入；读取可看到此前完整确认的状态。

封存与任务接受／任务工程归属在同一写事务提交。Worker 使用 attempt 私有副本重新校验每块及完整 hash，实际探测媒体并固定不可变内容；成功注册项目素材必须与合法当前 attempt 的任务完成原子提交。取消、过期 attempt、崩溃不能发布可见素材或移动工程修订。未引用 CAS 与块清理按引用关系和明确保留策略执行，不能靠进程 PID 或不可靠 mtime 猜测。

## 资源与契约兼容

项目素材记录返回 opaque media_id、project_id、已验证内容版本和媒体元数据，不返回宿主安装／暂存路径。后续资产绑定必须验证 media_id 的项目归属及内容，原有任意宿主 URI 的绑定仍对受限主体拒绝；不可通过自造 AssetVersion 或猜测全局 CAS hash 提权。

原 22 工具和媒体发布查询保持兼容。新增 media.import 必须在同源 Job 注册、Worker、公共结果／错误、CLI/MCP/SDK 和能力发现中一致表达；旧 Job 的结果与输入继续可读，不用编辑事务承载媒体字节。上传配额、大小、块限制和支持状态由能力发现明确返回，仅广告已接通并验收的阶段。

## 验收门槛

真实 CLI/HTTP/官方 MCP 控制面与独立 SDK；跨项目、猜测 upload/media ID、仅 read/edit、撤权和准备期间撤权；精确重试、冲突重试、超限、错误长度／hash、分块中断；实际 SIGKILL、并发写块、文件已落盘但数据库失败；非法媒体、真实 FFprobe、素材绑定／替换、独立 RGB/PCM 与源文件删除后重开；任务取消与过期 attempt 不发布素材；历史迁移、旧工具兼容和仓库外安装包验收。满足端到端门槛前不标“项目导入完成”。

## 任务完成与读取契约

media.import 的封存使用 upload_id 作为稳定接受身份，同一次上传最多接受一个任务。接收失败不封存；任务接受后重试封存返回同一 job_id。sealed 不能通过 abort 伪装成任务未发生，取消走既有 Job 权限。若取消请求先于导入完成事务提交，完成转 cancelled 并且不登记素材；若成功事务先提交，迟到取消保持成功资源。旧八类任务维持既有 cooperative 语义，不改变旧输入／结果枚举。

JobView.result 保留旧八类的封闭枚举。新增任务使用可选 extended_result：`{kind, schema_version, data}`，当前导入版本为 1；Host 必须先按对应已知类型验证，再输出 data，不能直接公开任意 Worker JSON。result 与 extended_result 不同时出现；非法／无法投影结果仍通过 result_unavailable 表达，不把已完成任务改成失败。新消费者应读取支持的 kind/version，旧消费者忽略附加字段而无需反序列化新的枚举变体。resource_target 是 `{project_id, resource_kind, resource_id}`，导入指向 kind=upload 和权威 upload_id；旧固定修订任务仍使用 target。

导入只探测白名单中的自包含二进制格式，禁用网络协议和 MOV 外部 data reference，不接受 concat/HLS 播放列表作为素材。探测预算为 30 秒、stdout 1 MiB、stderr 64 KiB；可取消。该边界不等同于完整 OS 进程沙箱。媒体分类依据实际字节而非显示名称。

## 当前实施状态（2026-09-21）

上传基础已验证，见 [当前交付台账](headless-completion-ledger.md)。本轮已接通原生 ImportStore 封存、Worker 整体校验／实际探测／CAS、合法 attempt 成功与素材登记原子提交、项目素材查询和受限 asset.bind／asset.replace。真实图片删除原源后导入／绑定／预览／独立 RGB 解码、视频／音频分类、取消和回滚均通过，见 [当前交付台账](headless-completion-ledger.md)。

公开入口及暂存增量已接通 46 个工具／9 种任务描述、HTTP 原始分块、CLI upload put、固定范围项目素材分页和双 SDK 分块／扩展结果读取。六个新增工具为 upload_create、upload_inspect、upload_abort、upload_seal、project_media_inspect、project_media_query（均带 opencut_ 前缀）。media.import 由 seal 接受，旧 JobStartInput 不增加枚举变体。能力发现 uploads 返回实际接入路径、限制与结果版本；OpenAPI 49 条路径。

abort 先持久提交 aborted 再持与写块相同的 OS 锁删除该会话私有分块；失败为 UPLOAD_CLEANUP_PENDING，重复 abort 继续清理。崩溃残留写块临时文件在下一次持锁写入时回收。只读查询不执行清理。sealed 的成功／失败／取消任务暂存默认保留，可显式 release；upload_query 可重新发现遗留会话，retention／quota 查询提供释放状态和逻辑预留。项目 8 GiB／工作区 32 GiB 的上传预留在实际清理后释放；完整物理磁盘／CAS／产物治理仍待完成，不能把源字节预留称为总磁盘配额。详见 [暂存契约](upload-staging-contract.md)。

导入 Worker 的真实进程组 SIGKILL 已验证：在实际组装输入后、探测前终止，不修改数据库时钟；30 秒自然租约到期后 attempt 2 用真实 FFprobe 恢复，恰好一份资源和成功审计，工程 head 不变，旧私有输入被清除。证据由 `scripts/verify-project-import-recovery.py` 重现。

源码及最近已验交付库格式为 v9。dev-46d52b41f70b 已通过仓库外 30 组验收，32 条命令全成功，含独立双 SDK 的发现／释放／预留查询、历史 18 格和真实媒体回归；见 [当前交付台账](headless-completion-ledger.md)。此前公开导入记录见 [当前交付台账](headless-completion-ledger.md)，完整磁盘治理等边界保持。
