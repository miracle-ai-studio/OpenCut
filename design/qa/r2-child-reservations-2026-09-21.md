# R2 子进程预留机制验证

本轮推进原生子进程写入的必要机制，尚未把 Worker 输出接入生产准入。公开工具仍 59 个，SQLite schema 14，coverage 仍十类，enforces_all_workspace_writes=false。最后正式验收开发包仍 dev-4398ef6e2a9e；本轮源码增量尚未重新打包。

## 变更与权威边界

- Reservation 新增消费式 spawn_single_file，复制 CLOEXEC 存活锁，只为目标子进程保留跨 exec 引用，同时下调 RLIMIT_FSIZE soft/hard 和禁止 core 文件。ReservedChild 正常析构先停止并回收进程，再释放额度。
- Held 释放改为关掉父进程自身描述符后，用独立 open file description 非阻塞重检；子进程仍持锁就保留持久记录。父进程 SIGKILL 不影响子进程自己的预留引用。
- 这是受控单输出命令的机制，不是任意进程的合计磁盘沙箱。未扩展 Worker、公开 Job 失败契约、字幕写入或旧 attempt 清理。
- 后续接入要求与平台依据见 [子进程写入契约](../architecture/native-child-write-contract.md)。

## 实际进程验证

使用实际 FFmpeg 8.1.2，macOS x86_64；没有用模拟编码器替代。

测试创建真实监督进程和 FFmpeg，FFmpeg 设置 60 秒 rawvideo/NUT 输出、4 MiB 预留。产生实际文件后暂停 FFmpeg，SIGKILL 监督进程，再执行恢复：仍保留 4194304 字节，恢复数量为零。恢复 FFmpeg 运行后，文件从 786432 字节继续增长，最终止于 4194304 字节，随后预留可恢复为零。与此同时，无关的 sleep 子进程仍存活，证明它没有泄漏持有预留。

另验证 1024 字节上限下真实 FFmpeg 失败且文件不超过上限、不存在的执行文件启动失败不泄漏预留，以及直接丢弃活动 ReservedChild 会停止/回收子进程并释放预留。

原生子进程两项和原有持久账本三项测试通过；ignored 条目是由测试主动启动的子进程入口，不是跳过验收场景。最终日志 /tmp/opencut-child-admission-child-final.log；包含原有账本的全套日志 /tmp/opencut-child-admission-final-tests.log。

首轮测试曾读取到尚未写完的 children.json，属于验证程序的就绪通知竞态；已改为临时文件写完后原子 rename，清理遗留测试子进程后复验通过。相关失败未记为通过。Clippy 的跨平台整数转换及子进程所有权提示已处理，最终 --all-targets -D warnings 通过。

## 原有真实产品动线回归

调试二进制 SHA-256：13a8d688a4ea123099b5225a90e6bd2a5e83e445bcef36362b4fe761b0aa1b7d。

- 持久准入七组：/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-budget-6bgje4fb/report.json。覆盖实际 CAS/私有输入、上传/导入、策略回执、CLI SIGKILL、owner/受限权限以及官方 MCP stdio/HTTP 消费。
- 扩展写入七组：/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-budget-writers-sd07pqus/report.json。覆盖 WAV 上传/修复、WASM 安装/授权/执行、目录别名拒绝、约 18 MiB PNG 下载、慢消费者、并发容量拒绝、断开及宿主 SIGKILL。
- 旧 22 工具和 2 个媒体发布工具 Schema 不变，当前 59。纯 Application/SDK 依赖边界通过。

基线脚本原检查名称硬编码 schema-13；实际工作区经独立 SQLite 查询确认是 schema 14。脚本已改为报告实际 version；这只修正证据标签，不改变旧宿主拒绝断言。

关键计数、实际大小、源码哈希、平台版本和两个回归报告已保留到 [结构化证据](evidence/child-admission-2026-09-21.json)。

## 剩余交付

Worker/媒体执行资源端口、可配置输出上限及明确失败错误、job/attempt 绑定、字幕/中间文件准入、子进程持有的 attempt 清理锁、真实任务恢复/取消/碰顶验证和新包交付均未完成。不能用本轮原生进程验证宣称生产渲染已经实施额度控制。R2–R7 完整范围保持。
