# R2 任务生命周期、租约时钟与恢复验收

2026-09-21。本轮落地纯任务状态机、原子生命周期存储端口和 Worker 事务内时钟，修复等待写锁期间过期仍可能被旧时间接受的风险。R2 整体仍未完成；契约见 [任务生命周期端口](../architecture/job-lifecycle-port-contract.md)。

## 代码与动线结果

JobService 使用 JobLifecycleStore 执行领取、取消、心跳、进度、终态和恢复。SQLite 适配器持有 IMMEDIATE 事务后才取 JobClock 样本；共享 transition 函数校验 job/worker/attempt/deadline，生成状态与事件，适配器将两者原子提交。Job Runtime 的正式路径全部使用 live_jobs/SystemClock，显式 now_ms 的旧接口仅保留为确定性嵌入／测试兼容入口。

新增五项检查中，一项覆盖五种 Worker 更新：用 SQLite busy handler 确认同一操作正在等锁，锁内推进受控时钟至精确 expiry 边界；解锁后 heartbeat/progress/complete/fail/finish_cancelled 全部 JOB_LEASE_LOST，独立连接确认状态和事件不变。该测试证明取时顺序及判定，不把受控时钟描述成真实自然等待。

另验证六类生命周期事件写入故障整体回滚、恢复批次第二条事件失败时全批回滚、同 Worker/attempt 的跨 Job proof 拒绝，以及协作取消与不可变终态。取消请求后有效租约仍可能发布已成功完成的后台结果，保持原 v1 语义；没有通过修改取消语义取得通过。

## 最终版本证据

CLI binary SHA-256：`c67fccdcf5c1419ec79cb865dc823c1c6c04a92bb11a81b8df9e76688fca4e09`。

| 检查 | 实际结果 | 证据 |
|---|---|---|
| 真实区间 Worker 崩溃恢复 | 5 项通过：确认活动 FFmpeg 和私有输入后 SIGKILL，自然等待租约过期，attempt 2 从锁定素材成功，清理旧输入，独立哈希及红色像素验证 | `/tmp/opencut-lifecycle-recovery-range.json` |
| rename 后／结果提交前崩溃 | 5 项通过：已完成文件被观察并独立解码，DB 仍 running 且 result=null；停止 Worker 后 SIGKILL，自然租约恢复，attempt 2 发布并清理旧未发布产物 | `/tmp/opencut-lifecycle-recovery-rename.json` |
| 真实区间编辑／活动取消 | 10 项通过：固定修订、两秒区间、独立 RGB/PCM 验证、CLI 回读，在路径限定的真实 FFmpeg 活动期间取消，结束后没有 MP4 或私有 inputs | `/tmp/opencut-lifecycle-active-cancel.log` |
| 公共 Job 完整动线 | 11 项通过：八种实际 Worker 结果、六类产物下载和独立解码、失败及损坏拒绝、事件重启续接、外部安装 TS 公共辅助方法 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-job-views-t2t1pe8n/report.json` |
| MCP／HTTP | 各 18 项通过，含持久任务、取消、游标／SSE 和 CLI 回读 | `/tmp/opencut-lifecycle-mcp.json`、`/tmp/opencut-lifecycle-http.json` |
| Rust 相关检查 | Application 52、Job Runtime 7、Automation 2、CLI 5，共 66 项通过 | `/tmp/opencut-lifecycle-tests-final.log` |
| Clippy | 四个受影响包 all-targets `-D warnings` 通过 | `/tmp/opencut-lifecycle-clippy.log` |
| 契约／依赖 | 生成文件一致，旧 22 工具 Schema/read_only 不变，总数仍 27；纯契约／SDK／Application 原生执行依赖门禁通过，lifecycle 源码禁止直接原生 I/O | 生成、wire compatibility、contract boundaries 三个现有 gate |

两份真实崩溃报告和公共 Job 报告均含最终 binary hash；已逐一比对。区间取消记录保留独立 PCM 频率约 879.375／439.375 Hz、实际取消的 FFmpeg PID。自然过期期间沿用原运行进程／工具 handle，没有因等待超时重启验收。

迁移期间编译检查发现 Worker 在 if-let 临时表达式中持有过长可变借用，已改为先取得 owned claim result，并明确不捕获额外生命周期；随后完整编译、Clippy 和真实动线通过。没有隐藏失败日志或缩减原验收条件。

## 下一阶段边界

工程事务和任务生命周期已有端口，但 Application 包仍持有 SQLite／CAS，任务查询分页、工程历史／事件查询、数据库生命周期及完整存储适配层迁出继续推进。后端无关合成、其他用例归位、历史版本矩阵与发行仍待完成。R3–R7 目标及非 UI 验收范围保持不变；网页与桌面实现继续暂缓。
