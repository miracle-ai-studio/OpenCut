# SQLite 与控制元数据写入准入契约（待实现）

状态：设计冻结前的实施契约。当前产品仍返回 `enforces_all_workspace_writes=false`、`enforces_total_storage_limit=false`；[实进程缺口证据](../qa/r2-database-control-gap-2026-09-24.md)显示 payload 余量为零时 WAL 仍可增长 4,021,152 字节。不能通过给现有 payload `coverage` 列表增加 `sqlite` 字符串、限制 `PRAGMA max_page_count`、观察 `statvfs` 或事后 checkpoint 来宣称写入前准入。

技术边界依据 SQLite 官方的[VFS 结构](https://www.sqlite.org/vfs.html)、[文件 I/O 方法](https://www.sqlite.org/c3ref/io_methods.html)和[WAL 共享内存要求](https://www.sqlite.org/wal.html)；本仓库使用的 `rusqlite` 提供具名 VFS 打开入口。这里定义目标协议，尚未声称已注册或使用新的 VFS。

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
5. 账本升级须保留 v1 payload 策略、活动预留、幂等回执和原始 actor；新格式与旧宿主不能同时写同一工作区。每次策略/repair 变更仍使用 expected revision、幂等键和持久回执。未知版本或损坏状态拒绝受控写入，不自行重建空账本。

## 公共契约与 Agent 恢复路径

`opencut_storage_budget_inspect` 保留旧字段，增加独立 `control` 对象：策略修订、控制限额、已观察字节、活动预留、修复预留实际状态与本次观察完整性。配置工具通过可选控制字段升级，旧请求不重置控制策略。`opencut_database_inspect` 保留现有数据库/WAL观测，并报告受控 VFS 是否启用；只有在所有管理型写入入口和控制账本均生效、独立实测通过后才把总准入能力改为 true。

Agent 收到 `DATABASE_CAPACITY_EXHAUSTED` 时先按原 request ID／幂等键检查工程或任务是否已提交，再由 owner 查看控制报告；若修复预留可用，显式进入 repair、完成取消／释放与 checkpoint，最后补足预留并重试尚未提交的原意图。只读查询、权限拒绝和无法判断结果的传输失败保留各自错误类别。

## 放行门槛

- 固定读者下连续提交大修订，控制额度在 WAL 达上限前拒绝下一写入；拒绝后工程 head、事务、审计、事件和上传／任务状态无部分变更。读者释放及显式 checkpoint 后恢复容量，原幂等键可安全重试。
- 在独立 64 MiB 卷上把外部可用空间压到零：普通编辑被拒绝，已实际预留的 repair 容量仍允许指定取消／释放事务；恢复、补足预留、重开与 `integrity_check` 通过。
- 两个 Host 并发写同一工作区、旧 attempt 和长 FFmpeg 子进程存活、SQLite 事务中及账本替换前后 SIGKILL、额度收紧／重放、迁移失败与旧数据库升级分别验收。锁顺序必须实测无 SQLite 写锁与工作区账本锁互等。
- 公共 CLI/HTTP/MCP 和独立 SDK 观察相同策略与错误；非 Unix、浏览器独立存储和旧宿主只报告实际能力，不以禁用功能抵扣完整 R2。
