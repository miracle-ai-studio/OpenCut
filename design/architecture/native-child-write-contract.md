# 原生子进程写入预留契约

Worker 已通过原生资源端口接入输出/字幕准入及 attempt 生命周期锁。SQLite schema 15 是新的宿主协议门槛；工具仍为 59 个、任务仍为 9 类，coverage 新增 media_output、media_caption。此文定义实现契约，发行与验收状态以 QA 报告为准。

## 已实现的原生机制

`storage-budget::Reservation::spawn_single_file(self, Command)` 消费既有预留和 Command，返回 `ReservedChild`。调用方承诺这是一个受控原生命令，且只会写一个受预留保护的 payload 文件；该机制不是任意命令的文件系统沙箱，也不把单文件限制当成整个进程的合计磁盘限制。

预留的字节数是该子进程的文件大小上限。父进程先复制生命周期锁描述符为大于等于 3 的 CLOEXEC 描述符，只有这个命令的 pre_exec 在子进程中清除 CLOEXEC。其他并发命令不获得可跨 exec 保留的预留锁。Command 被消费并在 spawn 返回后销毁，调用者不能保留安装了该 hook 的 Command 再次启动写入者。

子进程在 exec 前把 RLIMIT_FSIZE 的 soft/hard 限制分别降到既有限制和预留字节数的较小值；不放宽调用环境的更严格限制。RLIMIT_CORE 降到 0，防止大小限制信号额外产生未预留的 core 文件。hook 仅进行描述符和资源限制系统调用，不执行应用锁、日志或分配。

`ReservedChild::process()` 提供现有 Child 的管道、监控、取消及 wait 操作。正常析构会先停止并回收仍在运行的子进程，然后析构预留。父进程 SIGKILL 不运行析构，此时存活的 FFmpeg 自己持有继承的锁，恢复操作不能仅因父进程或任务租约消失就释放额度。

预留释放已调整为：取得工作区全局锁，关闭父进程自己的生命周期描述符，从另一个 open file description 非阻塞尝试取得同一锁，确认无人持有后才移除账本记录。绝不显式 unlock 共享的 flock，避免同时解除子进程的锁。若子进程还持锁，保留账本；它退出后由后续 reserve 或显式 recover 释放。恢复不删除实际 payload，孤儿 bytes 继续计入占用。

此版本未在公共账本加入 PID。PID/时间戳可复用，也不能作为回收依据。

## Worker 接入与公开契约

`media::OutputAdmission` 只接收原生命令、目标路径或确定的字节内容；不依赖工作区、数据库或任务 DTO。`job-runtime::TaskOutput` 提供原生适配，绑定预算、真实 job_id/attempt 与 canonical attempt 目录。普通嵌入 backend 仍可选择调用者自己的资源策略。纯媒体契约与合成层不引入原生依赖。

Owner Configure 可选 `media_output_limit_bytes`，规范正十进制字符串，默认 1073741824（1 GiB）。省略/null 保留当前值；旧账本缺省推导为 1 GiB。旧 Policy 的三个字段保持不变，新字段位于 Report/PolicyChange 顶层，旧历史回执不补造字段。新策略依然受原 revision 和幂等键控制。

每个 FFmpeg 文件输出先在同一预算锁下观察、恢复已死预留，然后取配置上限、逻辑 headroom、卷余量 headroom 的最小正值并持久预留；无一字节余量则启动前拒绝。实际 ceiling 可小于配置上限。公开 reservation 的 writer 为 `{job_id, attempt}`。实际文件达到 ceiling 或子进程因 SIGXFSZ 退出，任务失败为 `MEDIA_OUTPUT_LIMIT_EXCEEDED`，没有可下载的成功产物；达到上限的文件即使工具返回成功也保守拒绝。Agent 应请求 owner 检查上限/容量，调整后用新幂等键提交新任务，不能静默截断或自动扩大上限。

所有已接入 FFmpeg 输出命令只创建一个可增长文件。字幕 PNG 先在内存编码，再按准确大小预留，使用 create_new 写入并同步；一份输出预留不能被当成多个文件的合计限制。编码内存、日志管道不由磁盘准入治理。

`AttemptLease` 在真实 attempt 目录 inode 上持有共享 flock，输出与只读 ffprobe/解码子进程均继承。只读子进程使用 RLIMIT_FSIZE=0，不申请输出额度。发现工具是否安装的启动检查发生在私有输入准备之前，不属于持有输入的子进程。清理旧/终态 attempt 必须非阻塞获得独占锁，核对 device/inode 与公开产物引用；存活子进程占锁时跳过。Worker 持有适配器贯穿任务执行，父进程退出不解除子进程的锁。后续 Worker 清理可回收已无人使用的旧目录；预算 recover 只释放死亡预留，不删除 payload。

目录根、job 目录与 attempt 必须是真实目录，输出必须是当前 attempt 的直接子路径且不能已存在。此机制适用于合作的原生 Host/FFmpeg，不是防御同一 OS owner 恶意替换目录的文件系统沙箱。PID 或任务租约过期均不是释放依据。

固定工程修订、heartbeat、取消和 complete 的租约条件保持原协议。取消当前重试只能停止它自己管理的子进程，不能按 job_id 扫描杀死其他 attempt。旧子进程停止后才可释放其预留和清理其目录。升级前必须停止旧 Host/Worker；已打开的旧进程不能靠数据库 marker 追溯阻止写入。

当前 macOS 真实 FFmpeg 验证包括父进程 SIGKILL 后继续增长至 4 MiB 上限、独立只读子进程锁继承、正常/失败输出与字幕清理，以及 Worker 自然租约恢复。数据库/WAL 增长、真实磁盘满恢复、产物保留/下载租约和跨平台验证仍是独立未完成范围。

## 平台依据与当前实测范围

Apple 说明 fork/dup 描述符引用同一 flock，显式解锁会影响共享该锁的进程；本实现采用关闭自身引用后独立重检的方式。见 [Apple flock 文档](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/flock.2.html)。

RLIMIT_FSIZE 限制单个文件的扩展，超出时写入失败并产生信号。见 [Apple 资源限制文档](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/getrlimit.2.html)。子进程 hook 的限制依据 [Rust CommandExt 文档](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html)。这些文档不能替代目标平台实测；本轮只实测当前 macOS x86_64 与实际安装的 FFmpeg，不据此宣称 Linux/Windows 已验收。
