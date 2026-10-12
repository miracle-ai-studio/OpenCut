# SQLite 与控制元数据写入准入契约（实施中）

状态：设计冻结前的实施契约。当前产品仍返回 `enforces_all_workspace_writes=false`、`enforces_total_storage_limit=false`；[当前交付台账](headless-completion-ledger.md)显示 payload 余量为零时 WAL 仍可增长 4,021,152 字节。不能通过给现有 payload `coverage` 列表增加 `sqlite` 字符串、限制 `PRAGMA max_page_count`、观察 `statvfs` 或事后 checkpoint 来宣称写入前准入。

2026-09-24 首个实施增量：`storage-budget` 的账本 v2 新增独立控制额度与持久预留，旧 v1 策略、回执、存活锁可迁移；payload 和控制预留共享卷余量但分开计逻辑额度。owner 的配置、检查、恢复和双 SDK 生成契约已经扩展；`control.write_interception_enabled=false`。真实 CLI 将控制限额设为 4096 字节后仍成功提交工程，数据库主文件达 339,968 字节，详见[当前交付台账](headless-completion-ledger.md)。因此本增量**没有**完成下述 VFS、修复预留和全工作区准入门槛。

2026-09-24 第二个实施增量：`sqlite-store` 注册具名 `opencut-control-v1` VFS，保留原生 VFS 的锁、同步和实际 I/O，在主库／WAL／主 journal 的 `xWrite`、增长型 `xTruncate`、`SQLITE_FCNTL_SIZE_HINT` 与 WAL-index `xShmMap` 执行前扩展持久控制预留。CLI、HTTP、MCP 和 Worker 的管理型数据库连接统一使用该入口；嵌入式 `EditorApplication::open` 仍为非管理入口。每个管理型工作区绑定一个数据库文件名，第二个不同数据库会在打开前被拒绝。`opencut_database_inspect.managed_vfs_enabled` 报告当前连接是否使用具名 VFS；总存储强制标志保持 false。真实 CLI 的固定 WAL 读者、额度拒绝、原键重试和完整性检查已通过，但临时文件、外部直接 SQLite 写入、修复预留及全工作区物理上限仍未放行；`control.write_interception_enabled` 因此仍保守为 false。旧二进制与新宿主不可同时写同一工作区。

2026-09-24 第三个实施增量：本机 OS owner 可使用 `opencut --database <path> storage recovery inspect` 查看原生账本，或以 `storage recovery configure --input <json-file-or->` 携带 expected revision／幂等键恢复预算策略。这个 CLI 分支在 SQLite 鉴权和打开之前执行；带 `--credential-file` 的调用直接拒绝。它使用现有持久账本回执记录 actor／request ID，不写 SQLite 审计表。该路径已在遗留 WAL 需要重建 SHM、普通工具因过高余量门槛无法打开时恢复访问；也支持在 64 MiB 独立卷初始化数据库前设置较低门槛。它**只解决策略锁死和初始化**，没有分配 repair 文件，不能保证盘满时取消／释放事务。见[当前交付台账](headless-completion-ledger.md)。

2026-09-24 第四个实施增量：管理型工作区首次写入时建立账本 v3、实际写入并同步默认 8 MiB `repair.reserve`，且核对文件的物理块数；普通写入不得使用该预留。本机 OS owner 通过 `storage recovery enter --input -` 以 expected revision／幂等键释放预留，随后只能经专用 `opencut-repair-v1` VFS 执行 `storage recovery cancel <job_id>` 或 `storage recovery release <upload_id>`；`storage recovery exit --input -` 先补足并同步预留，再恢复普通写入。进入／退出要求管理型数据库连接全部关闭；`connections.lock` 的共享／排他 OS 锁防止在状态切换时新连接抢入。真实 64 MiB HFS+ 卷在外部可用空间为 0 时完成排队 Job 取消、终态上传暂存释放、幂等回放、错误隔离、补足和 SQLite 完整性检查，详见[当前交付台账](headless-completion-ledger.md)。这仅证明有界修复，不满足全工作区写入准入、运行中 Worker 取消或跨平台保证；能力位继续为 false。旧二进制不得与 v3 账本并行写工作区。

2026-09-24 第五个实施增量：VFS 改为每个工作区／数据库／模式注册独立实例，使 SQLite 的匿名 `TEMP_DB`、`TEMP_JOURNAL`、`TRANSIENT_DB`、`SUBJOURNAL` 可归属当前工作区；临时文件放在 `sqlite-temp/`，由同一控制预留和写入闸门保护。SQLite 会在打开后 unlink `DELETEONCLOSE` 临时文件，活动字节只能由文件句柄的预留表示；可见残留仍计入控制目录观察。原生回归已验证 `PRAGMA temp_store=FILE` 的 4 MiB 临时表在紧额度下写前返回 `SQLITE_FULL`、放宽额度后成功、关闭连接释放预留，以及目录符号链接拒绝。此增量不改变对具名未知 SQLite 文件、外部写者、控制元数据和全工作区物理额度的保守能力报告。

2026-09-24 第六个实施增量：原生账本每次原子替换 `ledger.json` 前，按文件系统块向上取整新 `ledger.tmp` 的序列化字节并留 64 KiB 目录／同步余量，与活动 payload／control 预留共同核对真实卷空闲；普通准入同时保留配置的最小空余。自动计算的 Worker 单文件输出上限还要留出本次账本和锁文件的写入空间，不能把卷余量全部承诺给 FFmpeg。owner 策略恢复不受原先可能过高的最小空余门槛锁死，但仍须有实际暂存空间。退出 repair 前先构造下一版完整账本并核对补足预留后的暂存空间，避免先补足文件却无法持久化 Ready 状态。真实 64 MiB 盘满卷已验证策略写入提前返回 `STORAGE_CAPACITY_LOW`、修订不变；释放 repair 预留后用同一请求持久重试，再完成取消、暂存释放、补足和完整性检查。该预检不构成严格物理保证：外部写者、文件系统写时复制、目录分配和 fsync 竞态仍可能使实际写入失败，能力位继续为 false。

VFS 将真实控制额度／卷余量拒绝映射为 SQLite `SQLITE_FULL`，由公开层返回 `DATABASE_CAPACITY_EXHAUSTED`；工作区观察失败、别名目录及账本错误映射为 SQLite I/O 错误，避免把结构损坏误导为“释放磁盘即可恢复”。Worker heartbeat 失败保留本地错误诊断。完整公共错误保真与修复路径仍需在 R2 放行前审查。

技术边界依据 SQLite 官方的[VFS 结构](https://www.sqlite.org/vfs.html)、[文件 I/O 方法](https://www.sqlite.org/c3ref/io_methods.html)和[WAL 共享内存要求](https://www.sqlite.org/wal.html)；本仓库通过 `rusqlite` 的具名 VFS 打开入口安装包装层。下文是完整目标协议，已实施与待完成项以上述增量和 QA 证据区分。

## 权威、范围和分账

- 纯策略仍在 `editor-application::budget`；原生账本与容量判断由 `storage-budget` 执行；SQLite 文件操作由 `sqlite-store` 的受控原生适配执行。HTTP、CLI、MCP 和两个 SDK 不选择目录、预留 ID 或绕过策略。
- 现有 `payload_limit_bytes` 只治理素材／任务等 payload。新增独立 `control_limit_bytes` 与 `repair_reserve_bytes`，统计当前主数据库、`-wal`、`-shm`、rollback journal 和 `.storage-budget` 控制文件。两类在同一工作区锁下核对活动预留与卷余量，但不把数据库增长扣到素材逻辑额度上。
- 文件扫描按设备/inode 去重，拒绝不明确的符号链接、特殊文件和不完整观察；已存在的文件、迁移文件、失败留下的 sidecar 都计入控制使用量。APFS clone、外部硬链接和外部进程写入不能转化为“独占物理字节”保证。
- 管理型 Host 的全部 SQLite 连接，包括迁移、任务租约、权限/审计、上传状态和工程事务，必须统一走受控打开入口。嵌入式非管理构造与具有数据库文件写权限的外部 OS 进程不在强制边界内，能力报告须明确。

## 写入协议

1. 原生 SQLite VFS 包装默认 VFS，在主库、WAL、rollback journal、临时文件的 `xWrite`／`xTruncate` 及 WAL-index `xShmMap` 增长前，按文件身份和目标偏移申请或扩展持久预留。预留持有 OS 存活锁，绑定打开的文件句柄；不能用事务时间戳推断旧写入已经结束。
2. 账本记录控制与 payload 的不同计费类别，先锁、读取策略、确认死亡写入者、观察现有文件与卷余量，再提交新预留；文件写入不得先发生后补记。保留现有 payload 算式，新控制算式比较 `observed_control + active_control + requested` 与控制额度，同时把两类活动预留计入物理余量。
3. SQLite 返回明确的容量错误使正在进行的事务回滚；应用错误保持 `DATABASE_CAPACITY_EXHAUSTED` 及 request ID。不能因收到错误自动换幂等键重试，也不能把可能已提交的响应丢失解释成未提交。缩小／关闭文件后，在安全重观察前不提前释放额度。
4. 修复预留是已实际分配并同步的工作区文件，而不是 `statvfs` 的口头余量。常规写入不能消耗它；当卷已满时，owner 可进入有审计的 repair 状态，释放该文件并只允许有上限的取消、上传释放和恢复操作。容量恢复后显式补足预留才能回到常规状态。默认预留大小须经 64 MiB 盘满、最坏取消事务与并发基线定标；未分配成功不得报告有修复保证。

   当前反例：若 owner 把共享 `minimum_available_bytes` 设到超过实际空余，下次受控 SQLite 打开就可能因 SHM／WAL 增长失败，使预算检查和重新配置也不可达。独立 repair 通道必须能在这一状态下读取持久策略、完成受审计的有限恢复并补足预留；仅在配置时限制数值不能解决后来被外部写满的卷。
5. 账本升级须保留 v1 payload 策略、活动预留、幂等回执和原始 actor；新格式与旧宿主不能同时写同一工作区。每次策略/repair 变更仍使用 expected revision、幂等键和持久回执。未知版本或损坏状态拒绝受控写入，不自行重建空账本。

## 公共契约与 Agent 恢复路径

`opencut_storage_budget_inspect` 保留旧字段，增加独立 `control` 对象：策略修订、控制限额、已观察字节、活动预留、修复预留实际状态与本次观察完整性。配置工具通过可选控制字段升级，旧请求不重置控制策略。`opencut_database_inspect` 保留现有数据库/WAL观测，并报告受控 VFS 是否启用；只有在所有管理型写入入口和控制账本均生效、独立实测通过后才把总准入能力改为 true。

Agent 收到 `DATABASE_CAPACITY_EXHAUSTED` 时先按原 request ID／幂等键检查工程或任务是否已提交，再由 owner 查看控制报告；若修复预留可用，显式进入 repair、完成取消／释放与 checkpoint，最后补足预留并重试尚未提交的原意图。只读查询、权限拒绝和无法判断结果的传输失败保留各自错误类别。

## 放行门槛

- 固定读者下连续提交大修订，控制额度在 WAL 达上限前拒绝下一写入；拒绝后工程 head、事务、审计、事件和上传／任务状态无部分变更。读者释放及显式 checkpoint 后恢复容量，原幂等键可安全重试。
- 在独立 64 MiB 卷上把外部可用空间压到零：普通编辑被拒绝，已实际预留的 repair 容量仍允许指定取消／释放事务；恢复、补足预留、重开与 `integrity_check` 通过。
- 两个 Host 并发写同一工作区、旧 attempt 和长 FFmpeg 子进程存活、SQLite 事务中及账本替换前后 SIGKILL、额度收紧／重放、迁移失败与旧数据库升级分别验收。锁顺序必须实测无 SQLite 写锁与工作区账本锁互等。
- 公共 CLI/HTTP/MCP 和独立 SDK 观察相同策略与错误；非 Unix、浏览器独立存储和旧宿主只报告实际能力，不以禁用功能抵扣完整 R2。
