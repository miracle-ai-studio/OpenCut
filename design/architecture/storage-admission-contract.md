# 持久写入准入与恢复契约

当前源码已统一为一个原生准入账本写入结构，不带开发版本计数；必要旧账本只在私有严格读取中校验并归一化。策略、活动预留、真实初始策略基线、完整审计回执及修复状态共同构成权威，不由文件代际决定功能。当前改造尚未统一构建或运行验收，不能据此声称全工作区硬配额已交付。公开能力与生成契约以同一个后续集成二进制为准。

## 分层与权威

`api-contracts::budget` 定义 DTO；纯 `editor-application::budget` 定义 owner 权限、规范字节校验、准入算式和 BudgetNative 端口；`storage-budget` 是独立的原生适配器，负责工作区目录锁、持久账本、文件观察和预留存活锁。`native-services::budget` 从可信宿主数据库位置选择工作区，Automation 只装配。公共请求不能提供文件路径、actor 或预留 ID 来释放其他写入者的额度。

预留不采用业务 SQLite 的写事务：文件写入必须在工程/任务提交之前获得额度，且不能持 SQLite 写锁执行长时间 IO。原生账本是该资源协议的权威，与业务提交不假装跨数据库原子。它记录准备写入的额度，实际字节是否仍占用由受控目录观察确认；业务失败产生的孤儿仍计入占用，另走显式回收。

账本位于工作区 `.storage-budget/ledger.json`，包含真实初始策略基线、当前策略、活动预留和策略变更回执。策略改变与幂等回执在一次文件替换中提交；暂存文件 sync 后 rename，再 sync 目录。不是覆盖原账本就视为提交成功。回执保存可信原始 request_id/actor，重试返回原回执；inspect 的 last_policy_change 可核对最近来源。当前文件的未知/重复字段、损坏内容或不明确的目录对象拒绝写入；旧文件先按其原始格式及历史默认值严格校验，没有失败后换解码器的回退。该目录计入 storage_inspect 的 database 控制存储类别，不计入 payload 字节。

## 当前覆盖与默认策略

生产宿主显式使用 managed 构造入口，已接入：

| coverage 值 | 执行边界 |
|---|---|
| cas_capture | 绑定/替换素材、任务固定内容及导入发布时的新 CAS 捕获 |
| upload_chunk | 项目上传新分块的临时写入、同步和重命名；精确重放不再次分配 |
| upload_assembly | Worker 拼接封存上传形成的私有输入 |
| private_asset_copy | Worker 从固定 CAS 清单复制任务私有输入 |
| media_upload | 全局 HTTP 上传每段写入前增长；同步、探测及发布完成后清理 |
| media_inspection | 元数据修复的私有检查副本，源增长时先增加额度 |
| media_metadata | 元数据临时文件；在登记发布意图之前预留完整大小 |
| plugin_package | 包快照逐文件受控复制；复制失败清理整个 staging |
| plugin_grants | 授权文件替换；在登记发布意图之前预留完整大小 |
| artifact_download | 匿名验证副本；精确大小预留持有到响应流结束或断开 |
| media_output | Worker 单文件 FFmpeg 输出；内核上限与继承的额度/attempt 锁 |
| media_caption | Worker 字幕 PNG；编码后按精确大小先预留再写入 |
| preview_caption | 连续预览生成目录内的字幕 PNG；按编码后精确字节数预留，再以独占创建、写入和同步完成 |
| whisper_model_snapshot | Worker 为转写任务复制固定 Whisper 模型快照；私有目标通过存储准入端口先预留源文件字节数 |


普通嵌入式 open/new 构造仍供独立使用者选择自己的资源策略；原生宿主的所有上述入口必须走 managed 构造。非 Unix 暂不启用此适配器，原有嵌入行为保留；管理工具明确返回不可用。内存数据库不提供公开策略管理。

缺少账本时，新工作区没有默认产品 payload 或媒体输出文件额度（规范整数记账上界仍为 i64::MAX），最小卷可用余量为64 MiB。现有持久策略和输出上限保持原值，不随新默认值自动放宽。第一次受控写入才创建控制目录；inspect 不创建任何控制状态。Owner 可以显式改变策略，没有环境变量暗中重置额度。

观察范围是 asset-store、media、uploads、artifacts、plugins 下唯一普通文件的长度，跨目录硬链接按 device/inode 去重，稀疏文件按逻辑长度计入。既有文件、失败留下的文件和未受控类别中的可见文件都参加观察。没有预先迁移一份可能失真的“已用量”。符号链接、特殊对象、跨设备、无法安全读取的目录、超过 100000 条目/64 层/5000ms 合作式遍历预算均拒绝准入；不是返回不完整结果后继续写。

`enforces_all_workspace_writes=false`保持明确：管理型CLI/HTTP/MCP/Worker的主库、WAL、journal、SHM和匿名磁盘临时文件经过既有具名VFS及独立控制预留；嵌入式非管理连接、外部写者、文件系统/控制元数据和完整保留回收不属于全工作区硬物理保证。`managed_vfs_enabled`报告连接入口，`control.write_interception_enabled`和总强制位继续按实际范围保守报告。控制额度、已用量、预留及真实repair状态单独呈现，不占用payload逻辑额度；两者共同参与卷余量判断。详见[控制写入契约](database-control-admission.md)。

`.storage-budget/ledger.tmp`在创建前核对活动预留与可用物理空间，普通准入另保留最小余量；owner恢复可以越过过高的旧最小余量配置，但不能在没有暂存空间时报告策略已持久化。repair退出先检查补足8 MiB实分配储备后能否安全保存账本。当前源码改造并未重跑原盘满/恢复证据，APFS写时复制、外部写入及文件系统元数据仍在严格物理保证之外。

## 准入与保守计费

在同一个工作区目录锁内读取策略与全部活动预留、确认可恢复的旧预留，再观察当前文件与 statvfs 余量，最后提交新预留。目录锁最多等待 5 秒，超时返回 STORAGE_BUDGET_BUSY；遍历时间预算不能抢占单个文件系统调用。

两个必要条件：

- observed_payload_bytes + 全部活动完整预留 + 新增申请 ≤ payload_limit_bytes。
- 全部活动完整预留 + 新增申请 + minimum_available_bytes ≤ 本次观察到的 volume_available_bytes。

活动写入已经落盘的部分可能同时出现在“观察占用”和“完整预留”中。这是有意保守计费，不报告为实际独占磁盘空间；高并发时可能提前拒绝。物理块舍入、APFS clone/快照、外部进程和文件系统自身开销使其不能成为整个卷的硬物理保证。实际 ENOSPC 仍由写入错误路径处理。

已知大小先预留完整大小；源文件增长时，在每次超出当前预留的写入之前增加额度，扩额失败就停止，不先写后补。正常结束不需要猜测“实际使用”并扣减另一份计数器：移除预留后，下次扫描仍计入已发布或未清理的文件。上传声明不再带额外的固定素材总量额度；封存清单分页读取，实际分块/拼接/CAS 写入仍逐次按卷空间和显式策略准入，创建会话不预先保证后续分配成功。

降低策略不杀死已获准的写入。它可使 headroom 为零；已有预留可写到原上限，新增申请或扩额按新策略检查。不会为了满足新上限自动删除历史或重试失败任务。

## 生存期与崩溃恢复

每条预留持有单独的 OS 排他锁，锁文件名为宿主生成的 UUID；全局目录锁仅覆盖短账本操作及有界观察，不跨实际素材复制。进程时间戳、PID 或 Job 租约过期都不能证明写入已停止。受控复制在持锁进程内执行；Worker FFmpeg/ffprobe 子进程继承必要的存活描述符，不能仅因监督进程退出而释放。详见 [子进程契约](native-child-write-contract.md)。

恢复先尝试获取各预留的存活锁，确定已停止的写入者，再扫描实际字节；顺序不能颠倒，否则会漏掉扫描结束前仍在写的内容。仍持锁的预留保持完整额度。释放持久记录失败时保留保守状态，进程退出后可重试恢复。回收只删除无主控制锁文件，不删除 payload；孤儿捕获另用 CAS plan/execute，上传暂存另用 release。

活动预留最多 4096 条，策略变更最多 1024 次，账本最多 16 MiB。达到上限明确拒绝，不静默淘汰幂等键。记录缺失的锁文件、未知对象类型或账本损坏不能当成“写入已死”直接释放。后续元数据历史治理需独立契约。

## API 与 Agent 动线

三个 owner-only 工具：

1. `opencut_storage_budget_inspect {}`：读取 policy、last_policy_change、观察使用量、完整预留、保守 headroom、活动条目和真实 coverage；只读，不恢复。
2. `opencut_storage_budget_configure`：expected_revision、稳定 idempotency_key、规范十进制字符串 payload_limit_bytes/minimum_available_bytes，以及可选的正数字符串 media_output_limit_bytes（省略/null 保留当前值，新工作区无产品默认额度）。返回 policy/request_id/actor 与本次媒体上限的持久回执。Policy 的原三个字段不变，媒体上限位于 Report/PolicyChange 顶层；旧回执保持原样。同键同请求重放原结果；同键换参数拒绝；真实策略revision不匹配须重新审阅后显式提交。
3. `opencut_storage_budget_recover {}`：持锁确认死亡写入并重新核对实际占用，返回释放条数与新观察；这是写操作，不能由 SDK 自动触发。

MEDIA_OUTPUT_LIMIT_EXCEEDED、STORAGE_BUDGET_EXCEEDED 与 STORAGE_CAPACITY_LOW 为 HTTP 422 / CLI 6；STORAGE_POLICY_CONFLICT 为 409 / 5；STORAGE_BUDGET_BUSY 为 429 / 7；账本/观察错误为 500 / 8。受限主体无权读取全局策略；其上传和公开任务结果仍返回经过清理的准确容量错误，让 Agent 请求 owner 处理，而不是把所有失败折叠为未知存储错误。

失败上传先读取已确认偏移，额度恢复后可重试同一未确认分块。终态失败 Job 不原地复活：检查失败原因，保留/释放原暂存；用新的上传或任务幂等键发起新的明确尝试。网页和桌面应显示观察范围、当前额度、未完成预留及错误恢复入口，不能将 headroom 显示成保证可用的物理容量。

## HTTP 文件与取消生命周期

`native-services::budget::AdmittedFile` 只承担同步文件与预留的共同所有权：先关文件，再删除命名 staging，最后释放预留。CLI 的 `BlockingOwned` 将创建、每段写入/读取和析构移到阻塞执行器；每个阻塞操作自身持有文件，取消异步等待不能提前释放仍在执行的写入。不会在 async reactor 等待额度目录锁或 fsync。

上传流每段在实际写入前调用 ensure，探测和发布继续由同一文件所有权保护。普通错误、连接中断清理 staging；SIGKILL 遗留的命名文件仍计入使用量，恢复只释放死预留，不擅自删除文件。下载先按发布的 size/hash 完整验证，私有副本立即 unlink，响应只读这一副本。由于目录不可见，必须保留完整预留直到文件真正关闭；慢消费者、权限撤销或连接中断均不能提前释放。校验期间取消逐段检查；流读取的阻塞操作也持有文件。普通文件已被观察和完整预留并存时仍按既有保守算式重复计入，不承诺精确物理硬配额。

容量拒绝发生在媒体元数据或插件授权 publication prepare 之前，直接返回明确的 STORAGE_BUDGET_EXCEEDED / STORAGE_CAPACITY_LOW；不能把尚未发生的发布误报为 PUBLICATION_OUTCOME_UNKNOWN。文件系统发布仍使用既有 Journal；业务发布与额度回执不构造虚假的跨存储事务。

## 下一批必接边界

- SQLite WAL/检查点及原生控制元数据：独立增长策略和必要操作的保留容量，避免盘满时连取消、回执和修复都无法提交。当前实进程缺口与实施契约见[数据库控制写入准入](database-control-admission.md)。
- 产物保留与下载/持续预览租约、工程包/插件版本和历史裁剪：先冻结引用/租约协议，再显式可恢复删除与释放。

验收必须继续覆盖真实磁盘满、并发完整导入/导出、下载中回收、租约丢失与旧 attempt 结算、每个发布/删除窗口 SIGKILL，不能用当前十四类受控写入的测试宣称全局准入已经完成。

## 当前账本及长音频输出接线

当前写入器用`baseline`保存真实初始payload策略和媒体输出上限。新工作区基线来自当前默认值；原账本在私有读取时绑定原64 GiB/1 GiB历史默认值，原政策变更回执、request_id/actor、幂等键及预留不改。策略校验从该实际基线重放，避免拿新的`Policy::default()`拒绝无变更记录的旧工作区。原格式的结构限制仍单独验证，拒绝混合当前/历史根字段；当前写入没有`version`字段。控制/repair能力按其真实状态判断，策略revision及repair revision是实际并发/审计时钟，继续保留。

原生Float32立体声WAVE输出共用纯长度规划：小输出保持原58字节RIFF头，超过RIFF长度时使用94字节RF64头和`ds64`的64位长度/样本数。准入、Native写入和Application结果校验使用同一容器选择与实际长度；PCM块仍逐声道严格有序、有限值、最多4096采样，最终长度、同步、真实进程结束及发布围栏保持。RF64格式依据[EBU Tech3306](https://tech.ebu.ch/docs/tech/tech3306v1_1.pdf)，固定Mediabunny已有对应读取器；这不证明当前长音频已通过。

当前仍需完成长SoundTouch准备与整段Web Audio压缩器的有界执行。不能通过逐块重建压缩器而丢失连续DSP状态，也不能把文件/RF64准入改造当成长场景PCM驻留已解决。未知长度原生FFmpeg输出仍按实际可用空间及显式策略选择并持有一个保守内核上限，其并发预留成本保留为集中验收项。没有修改已保留工作区的账本或数据库，也未运行当前产品测试。
