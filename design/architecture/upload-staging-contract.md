# 上传暂存配额与释放契约

数据库 v9 在不修改 v8 会话／分块／素材结构的前提下新增 upload_retention。API 版本仍为 1；UploadView、旧任务和原工具输入保持兼容。

## 预留模型

新会话按声明的完整 size_bytes 预留容量，同一事务提交会话、幂等身份、预留、事件和审计。项目上限 8 GiB，工作区上限 32 GiB；之前的单文件 1 GiB、单块 4 MiB、4096 块、8/128 个 receiving 会话上限继续独立生效。精确创建重放先鉴权，再返回原会话，不因当前容量已满而拒绝；容量不足的创建全部回滚，不占用幂等键。

该指标是上传源字节的逻辑预留，包含尚未写入的承诺容量，不是文件系统块数或实际空闲磁盘。它不覆盖 CAS、Worker attempt 私有副本、导出产物、工程包、数据库或锁文件；写块时短暂存在的旧／新文件也不等同于预留计数。完整工作区物理磁盘治理与可恢复 CAS GC 仍是后续要求。

v8 迁移为所有既有会话（包括 aborted、sealed）按声明容量建立 retained 记录；不在 SQLite 迁移内检查／删除文件。已有用量即使超过新上限也保持可读和可清理，新创建暂被拒绝；显式释放后恢复接入。迁移中任何 DDL／回填错误整体回滚，旧字段、成功记录、工程历史不改写。

## 状态、权限和释放

opencut_upload_query 提供按项目发现会话的分页入口，默认 retained_only=true；false 查询全部历史。响应每项包含 upload 和 retention，limit 为 1–100，默认 50。游标绑定项目内的创建上界、最后一项和筛选条件，不绑定不可变状态快照；新创建被排除，后续释放会从 retained 查询消失。相同条件继续游标，刷新从空 cursor 开始。这样 Agent 丢失创建回执后仍能发现可释放项；查询不删除文件或释放配额。

opencut_upload_retention 返回 upload_id、project_id、retained/released、reserved_bytes 和观察时 release_eligible。retained 表示配额尚保留，不承诺文件当前存在或完整。opencut_upload_quota 返回本项目保留量和公开的项目／工作区限制，不披露其他项目用量。两者只要求 read，纯查询不执行清理。

opencut_upload_release 要求 read + edit + run_jobs。仅 aborted 会话或 succeeded/failed/cancelled 的导入任务可释放；receiving、queued、running 一律拒绝。cancel_requested 不是终态。释放前及最终提交均检查当前凭据；重放也重新鉴权。释放不会改变工程 head、原任务终态、导入素材记录或已固定的 CAS。

原生清理获取与写块相同的独占 OS 锁；组装输入持共享锁，防止终态后的遗留读取与删除竞争。实际删除指定上传目录并同步父目录后，SQLite 短事务标记 released（公共 reserved_bytes 映射为 0，原声明容量保留供核对），同时写 upload.staging_released 事件和审计。该审计关联上传、项目以及已绑定的任务和原始接受请求。不能在 SQLite 写事务中等待文件锁／删除文件。

文件已删除但数据库／审计提交失败时保留预留，返回 UPLOAD_RELEASE_PENDING；inspect 后重试 release。提交后响应丢失时重试返回已释放状态，不重复事件。删除／提交之间崩溃只会保守占用额度，不会让仍保留的路径被当成已释放容量。正在运行的旧写块在获取锁后重新检查状态，无法在 abort/release 后重新确认或创建内容。

abort 保留原 UploadView 返回类型，先提交 aborted，再调用相同释放流程。清理未完成仍返回 UPLOAD_CLEANUP_PENDING；这是已取消但待清理，不能解释为 receiving。凭据被撤销则返回 NOT_AUTHORIZED，owner 可接手释放。

## 终态保留与失败处理

终态暂存默认保留到显式 release；当前没有后台 TTL 自动删除。成功素材已固定 CAS，可以释放暂存后继续绑定／预览。失败／取消任务保持原终态和诊断，不原地重跑或改写历史；修正输入后以新的创建幂等键重新上传／封存。原 key 永远指向原会话，即使暂存已释放。

如果以后增加“复用暂存重试”，必须先定义新的接受身份及与 release 的原子互斥；不得复活已经释放的会话或把 release_eligible 查询当授权凭据。当前没有承诺该功能。

## 发现与客户端

capabilities.uploads.staging 是可选增量字段，明确配额、四个工具名、terminal 前提、automatic_terminal_cleanup=false、measures_filesystem_usage=false。TS hostCapabilities()／Rust host_capabilities() 从 typed capabilities resource 获取增量能力；旧 v1 capabilities 工具 Schema 保持冻结。三入口／双 MCP 和 SDK 共用同一注册表。

错误 UPLOAD_QUOTA_EXCEEDED 为 HTTP 422／CLI 6；不自动取消或删除其他上传。UPLOAD_STATE_CONFLICT 为 409／5；UPLOAD_RELEASE_PENDING 与 UPLOAD_CLEANUP_PENDING 为 500／8，必须查询／重试，不能按错误状态猜测文件未改变。UI 仅约定保留容量、可释放状态和明确释放动作；网页／桌面实现仍暂缓。
