# 存储治理与只读盘点契约

存储观察接口：`opencut_storage_inspect`，只读、仅 LocalOwner。输入为 `max_entries`（默认 100000，范围 1–100000），不接受路径、project ID 或 actor。CLI 通用 call、REST tools、stdio MCP 和 Streamable HTTP 使用同一注册定义，双 SDK 使用同源 DTO。该观察接口本身不需要数据库迁移；后续显式 CAS 回收使用 schema 12，见下文。

## 已实现：有界物理占用观察

固定类别顺序：数据库（当前数据库及 -wal/-shm/-journal，加 .storage-budget 原生控制目录）、asset-store、media、uploads、artifacts、plugins。不扫描宿主指定工作区中的其他文件，不包含外部源素材、其他数据库、交付包或文件系统元数据。`complete` 只表示上述范围在本次预算内没有遗漏，不表示整个磁盘已统计，更不表示可安全回收。

`logical_bytes` 是唯一普通文件的长度；`allocated_bytes` 是唯一普通文件和目录的 `st_blocks * 512`。二者均为十进制字符串。按 `(device,inode)` 全局去重，跨类别硬链接计入固定顺序中最先遇到的类别；同类别的遍历顺序不承诺稳定。APFS clone、压缩、快照、共享块及外部硬链接使 allocated 不等于独占或可回收字节。volume 字段来自工作区根目录句柄的 statvfs，代表整个卷而非 OpenCut 配额；观察失败时为 null、volume_observed=false。卷观察与类别 complete 独立。

扫描是 `live_non_atomic`：文件可能在观测过程中增长、替换、删除，返回值不是数据库与文件系统的一致快照，不用于写入准入。进程内最多两个扫描；超限返回 `STORAGE_SCAN_BUSY`（HTTP 429，CLI 7）。100000 次条目检查、64 层目录深度、5000ms 合作式时间预算限制扫描；单个文件系统调用无法被该时间预算抢占。条目预算包括固定根及不存在的数据库 sidecar 检查。输入可降低预算。

达到全局预算时返回 stop_reason，并把当前及未扫描类别标记为 incomplete；present=false 必须结合 complete 阅读。深度、符号链接、跨设备挂载、特殊文件、IO 错误有分类遗漏计数，都会使相应类别 incomplete。不存在的固定根为正常空类别；扫描途中消失的条目为 IO 遗漏。数据库 sidecar 的正常缺失不报错。

Unix 原生适配通过根目录句柄、statat/no-follow 和 openat/no-follow 遍历，目录打开后比较 device/inode，不跟随发现的符号链接，不枚举链接外的内容，不打开普通文件数据。原始路径、文件名、inode、OS 错误文本不出现在公共返回中。宿主配置的工作区路径及祖先路径是可信输入；发现的子路径不可信。扫描不创建目录、不修改工程或业务记录、不删除/修复文件；目录访问可能影响 OS atime。非 Unix 和内存数据库明确返回 CAPABILITY_UNAVAILABLE；其他平台实际验收待补。

Application 定义权限、预算和原生观察端口；native-services 实现文件系统观察；Automation 只负责装配，DTO 保持纯契约。受限 principal 无论项目 grants 如何都不能读取全局占用，拒绝沿现有审计记录。不会把全局空间统计伪装成项目配额。

## 写入准入接入进度

显式 CAS 捕获、上传分块/拼接、任务私有输入复制现已接入独立持久预留，见 [写入准入契约](storage-admission-contract.md)。默认逻辑 payload 准入上限 64 GiB、卷观察余量 64 MiB；策略、活动预留和可信变更来源可通过 owner 工具读取。enforces_all_workspace_writes 仍为 false，不能以四类接入抵扣下列完整治理范围。

## 后续写入准入与回收要求

2026-09-21 增量：schema 12 的显式 CAS 孤儿/捕获临时文件回收已接通，详情见 [CAS 回收契约](cas-gc-contract.md)。storage.garbage_collection 表示当前平台具备这个有限范围的显式能力；不是自动 GC，也不是全部类别的回收。下列统一治理范围仍未完成。

1. 统一管理 CAS、媒体副本、上传、attempt 临时输入/输出、保留产物、插件、数据库增长。分别定义逻辑额度、物理余量和无法预知输出量的处理，不能把文件长度或本次观察冒充硬物理配额。
2. 写入前原子预留，执行期间有界增量预留，发布后结算；预留必须绑定 job/attempt/lease 并可在进程崩溃后恢复。已过期执行者不能结算新 attempt。
3. 建立可审阅引用图：全部历史修订/分支、project_media、已接受及运行任务的固定输入、下载和预览会话、保留产物、工程包、插件精确版本。不能仅用当前 head 标记活对象。
4. GC 使用 plan → 显式执行 → 重新验证引用和租约 → 可恢复隔离/删除 → 审计结果；计划不是删除授权，检查与删除之间必须防止新引用竞态。外部路径永不删除。
5. 磁盘满、并发导入/导出、下载中回收、锁内撤权、删除/结算窗口 SIGKILL、历史恢复必须走公共入口验收。未完成这些条件，R2 物理治理和 R6 CAS GC 保持未完成。
