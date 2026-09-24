# 已发布产物的保留与回收契约

状态：**Unix 本机的单产物显式回收执行已接通；完整资源治理和跨平台回收尚未完成。** Owner 须先 release、审阅零保护根计划，再以准确计划 ID 与哈希执行。计划或 release 本身均不删除字节。

## 权威与身份

成功 Job 的 `job_id + attempt + result.artifact` 是发布事实；公开 `artifact_id` 由 `job_id + content_hash` 派生。文件路径只是 SQLite 中的原生执行数据，Agent 不能提交路径要求删除。Job 历史与审计不因字节释放而删除；`availability: unchecked` 也不是字节存在证明。工程修订、其他 Job 固定输入、项目素材、插件、未来 Session 和工程包若引用产物，均属于保护根。未知格式／损坏的权威引用必须让本次回收失败，不能被解释为无引用。

保留状态须独立于历史 Job 成功状态持久化。默认成功产物保留；显式 release 只撤销该默认保留，不立刻删除，也不能绕过其他保护根。释放与实际删除分开审计，重复请求按请求身份和精确参数幂等回放。客户端必须可查询 `retained / released / removing / removed / changed` 等状态；历史成功不暗示当前可下载。

## 执行路径

1. Owner 查询产物保留和引用，按 `artifact_id` 显式 release；受限主体不得推断全局保留图。全量可释放产物发现仍需独立入口。
2. Owner 生成持久、可审阅的回收计划，计划记录 Job／attempt、文件身份、大小与当时保护原因；计划不删除文件。执行须提供精确计划哈希。
3. 执行在与新引用发布协调的门闩下重新扫描**全部**权威引用；逐项取得尝试目录独占 inode 锁，与下载共享锁、Worker/FFmpeg 继承锁互斥。锁内再次核对引用、目录与文件身份、普通文件及单链接条件。忙、被保护、替换均不删除。
4. 删除后同步目录，再提交逐项回执。崩溃在删除与回执之间留下 `removing`；重试必须重新检查引用及真实文件，区分已经不存在、身份变化和仍需删除。不能凭超时或逻辑大小推断物理空间已回收。

`ArtifactSource` 负责安全读取，`ArtifactCandidate::observe/remove_if` 提供第 3 步的原生围栏与持久目录项删除顺序。Owner 可经共享 Automation 的 HTTP／CLI／MCP 入口查询已发布产物的保留状态，以精确 `artifact_id` 和幂等键 release；SQLite 将状态和审计一次提交，失败回滚，同键同对象重放。release 仅撤销成功 Job 的默认保留根，删除成功前仍可下载原字节。

Owner 还可调用 `opencut_artifact_references_inspect`：在一次 SQLite 读事务中扫描工程修订、任务输入、其他任务结果、项目素材、插件发布及项目插件授权，返回有界保护根身份和原生来源候选数，不公开原生路径。单个文档、总行数、总字节数和耗时均有限额；损坏文档或超限直接报错，不解释成“没有引用”。生产该产物的任务结果是发布事实而非外部依赖，审计和幂等回执也不作为活引用。结果中的 `root_count` 是精确扫描所得总数，`roots` 最多展示 100 个，`roots_truncated` 指示截断。该查询只做字符串级观察，尚未按 inode 解析路径别名，也没有与新引用发布协调；**零个根不代表可删除**。

schema 17 的 `artifact_gc_plans` 保存 `opencut_artifact_gc_plan` 的幂等参数、完整根集、私有原生路径／文件身份和公共审阅快照；计划与 `artifact.gc.planned` 审计在同一 SQLite 事务提交。Owner 以 `opencut_artifact_gc_inspect` 按 ID 重读，或用 `opencut_artifact_gc_query` 有界分页找回。计划哈希绑定公共快照与私有观察。计划时会把原生路径候选按文件身份与发布文件比对，识别普通路径、符号链接和百分号编码的 `file://` URI；无法解析的文件 URI、损坏权威文档及扫描超限会报错。公共计划最多展示 100 个保护根，不泄露路径或 inode。SQLite 读快照和文件身份观察不是跨存储原子快照，之后仍须在执行门闩内重检。具备持久目录身份的新 Unix 计划标为 `execution_available=true`；旧计划保持不可执行，须重新计划。

`opencut_artifact_gc_execute` 要求 `plan_id` 和 `expected_plan_hash`。受保护计划与哈希不符均拒绝；执行取得尝试目录独占锁，在 SQLite 即时扫描保护根和 inode 别名、确认 release，并将 `removing` 与审计原子提交，然后仅删除计划绑定的目录中身份不变的单链接普通文件。`opencut_artifact_gc_execution_inspect` 读取持久进度。`removed`、`absent`、`changed` 为终态回执；`busy`、`retained` 为本次非持久结果。删除后若回执审计失败，持久状态保持 `removing`，同一计划哈希重试会在重查后写入 `absent`。

schema 18 的持久发布围栏在 `removing` 存续时阻止工程修订、Job 输入／结果、项目素材、插件发布及项目插件授权的新增或引用字段改写，返回 `ARTIFACT_COLLECTION_RECOVERY_REQUIRED`；普通 Job 状态更新可继续。schema 19 开放上述执行入口，并防止旧宿主打开已升级的工作区。执行者先持有 SQLite `BEGIN IMMEDIATE` 写事务，完成即时引用重查并原子写入 `removing`，再释放事务进行原生删除；终态回执落盘后才解除围栏。围栏不约束直接在数据库之外篡改文件系统的进程；本能力只为遵守工作区锁协议的 Unix 本机宿主提供保证。

## 验收门槛

已在真实 HTTP／CLI 媒体动线验证受保护计划拒绝、哈希不符、删除意图审计失败不丢字节、删除后回执审计失败保留围栏、重启同计划恢复、正常删除与历史 Job 保留。正式放行仍须在同一交付包覆盖下载中执行返回忙、文件与目录替换、符号链接／硬链接、执行各窗口 SIGKILL、盘满时回执失败、权限撤销及跨进程并发。非 Unix 宿主需要等价的句柄与锁协议后才能广告回收能力。
