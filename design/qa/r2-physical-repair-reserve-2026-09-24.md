# R2 物理修复预留与盘满有限操作验收（2026-09-24）

状态：有界增量通过，R2 未放行。管理型工作区的原生预算账本升级到 v3，首次受控写入前创建、写满并同步 8 MiB `repair.reserve`，同时检查实际分配的文件块。普通写入要求预留完整；本机 OS owner 可以携带 expected revision 和幂等键进入／退出 repair。进入前必须关闭所有管理型 SQLite 连接；专用 repair VFS 只在 Active 状态允许打开当前 schema 的现有数据库。恢复 CLI 目前只开放排队 Job 取消和终态上传暂存释放，复用 Application 鉴权／状态事务与原生清理用例。凭证模式不能进入该本机 owner 路径。

真实 [64 MiB HFS+ 盘满报告](evidence/r2-physical-repair-reserve-2026-09-24.json)：外部可用空间为 0 字节时普通编辑返回 `DATABASE_CAPACITY_EXHAUSTED` 且工程 head 不变；进入 repair 后预留文件变为 0 字节，卷获得 8,388,608 字节。经独立 CLI 进程取消已排队 Job，并释放一个导入失败后保留的上传暂存；两者重复执行结果一致。外部填充仍在时退出返回 `STORAGE_CAPACITY_LOW`；删除填充后退出成功，预留重新实际分配，普通编辑用原幂等键提交一次，SQLite `integrity_check=ok` 且取消审计只有一条。报告绑定最终交付二进制 SHA-256，原始命令轨迹在仓库外整包验收目录的 `verify-repair-reserve/trace.json`。

`cargo test -p opencut-storage-budget -p opencut-sqlite-store -p opencut-cli` 通过：包括跨进程竞争、SIGKILL 存活预留、缺失账本拒绝重置、repair 状态回放和连接静止化等检查。生成的 SDK／OpenAPI／Web 兼容契约已经与同一二进制重新生成并检查一致；既有控制 VFS 和本机策略恢复动线继续通过。

首次整包回归在 Worker 工程帧任务中发现瞬时 SQLite `database is locked` 导致心跳直接取消，最终误报 `JOB_FAILED`。现将 SQLite BUSY／LOCKED 明确映射为 `DATABASE_BUSY`，心跳在半个租约期限内有界重试，进度样本遇忙可丢弃，最终发布仍重新检查租约。扩展后的 [Worker 动线](evidence/r2-worker-busy-2026-09-24.json)用真实 7 秒 SQLite 写锁超过单次 busy timeout，证明任务持续运行、租约续期，然后显式取消；原有输出上限、真实解码、孤儿 FFmpeg 与别名拒绝也通过。首次失败包不计作交付通过。

随后整包导入回归又暴露同类错误在内嵌 Worker 的空闲 `recover/claim` 阶段会终止整个 HTTP 宿主。现在仅对 `DATABASE_BUSY` 退避并重试，其他存储错误仍让 Worker 报错；[项目导入验收](evidence/r2-import-busy-2026-09-24.json)已增加启动后 7 秒真实 SQLite 写锁，再完成多分块上传、授权撤销、导入、预览和校验。第二个失败包同样不计作交付通过。

最终 macOS x86_64 debug 开发包 [opencut-0.1.0-dev-ef0b81be210b-x86_64-apple-darwin.tar.gz](../../dist/local/opencut-0.1.0-dev-ef0b81be210b-x86_64-apple-darwin.tar.gz) 的 SHA-256 为 `0ca2392d78137952c4d220492c470631facbd8ef98a54b21069b63941c428166`，二进制 SHA-256 为 `bc5e5221d0eac151cba7c9310858dd1e70d241f11f66d6e3bb71c4f1dbe9fac6`。仓库外解包后 [34 项整包验收](evidence/r2-physical-repair-package-acceptance-2026-09-24.json)全部通过，包含新增 repair、两处 Worker 忙锁真实动线、30 格历史、独立双 SDK、网络／导入／媒体和停服务重开。可选 WASM component 未随本包验收；这不是 release 构建、跨平台发行或完整 R0–R7 放行。

当前能力标记继续为 `enforces_all_workspace_writes=false`、`control.write_interception_enabled=false`。8 MiB 只由本次排队任务取消／终态暂存释放动线证实，未对运行中 Worker、FFmpeg 子进程、所有 SQLite 临时文件、外部写入者、控制元数据的严格物理占用以及最坏事务体积定标。repair 进入要求管理型连接静止；运行服务不能在不停机的情况下直接进入。中途崩溃可能留下不完整预留文件，正常写入会拒绝，但完整的自动补偿和跨平台保证尚未验收。完整 R2 仍需覆盖这些边界、统一产物／历史引用保留治理，并通过 release 与跨平台发行验证。
