# R2 Worker 输出准入与恢复验收

本批实现 Worker 的媒体单文件限额、字幕精确预留、子进程锁继承与旧 attempt 清理保护。R2 未整体完成，R3–R7 仍保留原范围；网页和桌面不在本批实现范围。

## 实现与契约

- `media::OutputAdmission` 为原生资源端口；`job-runtime::TaskOutput` 绑定预算与 job/attempt。纯 contracts/composition/application 不引入子进程、文件锁或 SQLite 实现依赖。
- 默认媒体输出上限 1 GiB，owner 可通过已有 configure 工具的可选字段修改。旧 Policy 三字段结构不变；旧请求省略新字段保留当前上限，旧历史回执按原样重放。
- 未知大小输出先原子预留配置上限与当前 headroom 的较小值，再用 RLIMIT_FSIZE 执行。碰顶返回 `MEDIA_OUTPUT_LIMIT_EXCEEDED`，不发布部分产物；字幕先编码、按精确字节预留后写入。
- 输出预留公开真实 job_id/attempt。只读 FFmpeg/ffprobe 也继承目录锁，文件扩展上限为 0。父进程 SIGKILL 不释放仍存活子进程的锁；清理必须持有目录独占锁并核对 inode。
- schema 15 阻止旧 schema-14 宿主重开工作区；原生账本仍 version 1，新字段可读取旧格式。coverage 从十类增至十二类，`enforces_all_workspace_writes=false`。

完整边界见 [子进程契约](../architecture/native-child-write-contract.md) 与 [写入准入](../architecture/storage-admission-contract.md)。

## 真实产品与原生证据

`scripts/verify-worker-admission.py` 使用实际 CLI/Worker、FFmpeg、SQLite、已交付 schema-14 二进制，不用 mock 编码器。测试 wrapper 只增加实时读取节奏并 exec 真实 FFmpeg。

1. 实际旧宿主拒绝 schema 15 且 SQL dump 不变；旧 ledger 升级保留原 policy 回执和幂等重放。
2. 64 字节上限下真实抽帧明确失败且无公开产物；调整上限、新建任务成功，独立解码检查红色像素。
3. 字幕合成失败/成功均清理 PNG 中间文件；工程帧、直接合成帧、区间预览、工程导出都执行真实编码并独立解码。
4. Worker SIGKILL 后停止的孤儿 FFmpeg 仍保留预留；等待自然租约到期，新 attempt 不删除仍在使用的旧目录。取消新 attempt 只停止新子进程；恢复旧进程后等待其实际退出，额度释放，后续 Worker 清理旧目录。
5. Artifact 根目录别名清理诊断/输出拒绝检查纳入交付包验收，确认不清理外部目标。

源码首轮完整 Worker 动线通过的 debug SHA：`c40e79b99d4a57dfb485debe218ebfc24d2b83cf6eaa956a7668fb41bebc1610`；证据目录：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-worker-admission-80qqvkta`。该轮尚未包含随后加入的目录别名脚本项与并发清理微调，最终以交付包重新验收结果为准。

原生子进程实测：父进程 SIGKILL 时输出 786432 字节，子进程继续增长至 4194304 字节内核上限后退出；存活时预留 4194304，结束后 0。独立只读子进程也阻止其目录清理；无关 sleep 仍存活，不会泄漏锁。日志 `/tmp/opencut-worker-attempt-tests.log`，后续并发目录清理与额度争用补测见 `/tmp/opencut-worker-final-tests.log` 和 `/tmp/opencut-worker-ceiling-tests.log`。

## 修正与验证边界

代码复查修正了并发清理窗口：目录已经由另一个清理者删除时，跳过该条目；目录 inode 已替换时不得据旧锁删除新目录。新增验证包含共享锁阻止删除、删除后正常跳过、替换 inode 不匹配、符号链接拒绝。

脚本调试曾因音频枚举写成 mute（正确为 drop）、错误地从公开 JobView 读取内部 attempt、将 60 秒实时输入的孤儿退出等待设为 20 秒以及误将非致命清理诊断当作进程退出码而失败。均修正验收脚本后重跑，不计为产品通过证据，也不将超时假定成锁泄漏。

首轮相关 Rust 共 128 项通过；清理与争用新增测试随后独立通过，最终 storage-budget/job-runtime 相关 17 项通过；Clippy warnings-as-errors 通过。TypeScript SDK 8 项通过；生成一致性、22 个旧工具 wire 兼容、分层边界通过。历史迁移 30 格/24 组通过，包含 schema 15 协议门槛失败回滚，证据 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-gqnorncx/report.json`。

仍未覆盖完整 SQLite/WAL/控制元数据增长、真实磁盘满恢复、产物保留及下载/预览租约、跨平台验证。普通探测输出捕获仍有独立的内存/取消边界待治理。单文件内核上限不是任意子进程的合计写入沙箱。本报告不证明无 bug 或完整 R0–R7 交付。

旧导入恢复脚本让只读 probe 子进程写 readiness 文件，受到新 RLIMIT_FSIZE=0 限制而失败；已改用本机 UDP 通知测试边界，不放宽生产限制。另将内部超限消息修正为预算或继承系统限制，避免系统既有限制更小时误报实际碰到的字节上限。最终包包含此修正。

## 交付包

最终候选 dev-443c76cb7b64 正在仓库外验收。最近已正式验收包仍为 dev-4398ef6e2a9e；新包验收前不替换该结论。
