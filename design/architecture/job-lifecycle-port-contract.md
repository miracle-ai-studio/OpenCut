# 任务生命周期与租约端口契约

2026-09-21。生命周期决策由 `editor-application::lifecycle` 的纯 transition 函数定义；JobService 编排调用，JobLifecycleStore 提供原子持久化，SQLite 实现在 `sqlite-store::sqlite_jobs`。Worker 已使用生产时钟入口 `live_jobs()`。

## 原子边界与时钟

JobLifecycleStore 提供 claim_atomic、update_atomic、recover_atomic。每次写入必须按以下顺序执行：

1. 获得存储串行化边界。SQLite 使用 IMMEDIATE 事务。
2. 读取当前任务，并在该边界内读取 JobClock；不能在等待写锁前缓存时间。
3. 调用共享 transition 函数，得到新状态与待发布事件。
4. 原子写入状态和事件。任何失败整体回滚。

生产 SystemClock 在 SQLite 获锁后读取 Unix 毫秒；同一转换仅取一个时间样本。旧显式 now_ms 方法保留为基于 FixedClock 的嵌入／测试兼容入口，不提供真实墙钟过期保证。原生 Worker 的领取、心跳、进度、完成、失败、取消确认和恢复均改用 live_jobs；正常时钟错误明确返回错误，不伪造零值或无限期限。

## 身份与状态转换

LeaseIdentity 绑定 job_id、worker、attempt。任务必须为 running、owner 与 attempt 一致，且 lease_until 严格大于事务内 observed_at，才允许 Worker 更新。等于截止时刻即过期。恢复后即使同一个 Worker 再次领取，也必须使用新 attempt。

| 动作 | 前置状态／条件 | 结果 | 事件 |
|---|---|---|---|
| claim | queued 且没有取消请求 | running，attempt 加一，设置 owner/deadline，清理旧 result/error/progress | running |
| heartbeat | 有效租约，正租期，无时间溢出 | 更新 deadline | 无，避免心跳事件噪声 |
| progress | 有效租约，合法阶段／比例／精确时间 | 更新 progress | progress |
| complete | 有效租约 | succeeded，保存 result，清除 error 和 lease | succeeded |
| fail | 有效租约 | failed，保存 error，清除 result 和 lease | failed |
| request cancel | queued | cancelled，cancel_requested=true | cancelled |
| request cancel | running 且首次请求 | 保持 running，cancel_requested=true | cancel_requested |
| request cancel | 已请求或终态 | 原状态不变 | 无 |
| finish cancelled | 有效租约，已有取消请求 | cancelled，清除 lease | cancelled |
| recover | running 且 deadline 已过期 | 已请求取消则 cancelled，否则 queued；清除 lease | recovered |

attempt 自增检查 u32 溢出；不允许回绕到可复用的旧租约身份。来源 kind/input 在生命周期转换中不可改变。恢复保留上一 attempt 的最后进度，下一次领取清空它。

取消是协作请求，不等同于任务已经 cancelled。保持 v1 语义：后台在观察取消前已经成功返回时，只要持有有效租约，成功提交仍可获胜，最终记录可能是 succeeded 且 cancel_requested=true。终态不能被后续取消或过期 attempt 覆盖。停止本地等待不发送取消请求。

SQLite 更新同时校验旧 state、attempt、owner 和 deadline；状态更新与事件写入在同一事务内。恢复一次选中的 expired batch 也在一个事务内提交：第二条事件失败时，第一条恢复不能单独保留。重复恢复不产生重复事件。

## Worker 与产物

Worker 的预读取状态用于选择后续动作，最终发布仍由事务内租约校验决定。租约丢失时停止媒体执行／拒绝发布，不能以之前的内存状态跳过校验。文件 rename 与数据库结果提交不是一个文件系统事务；Worker 重启后依据已提交结果及 attempt 清理未发布文件，保持现有内容校验下载契约。

## 验证与迁移范围

定序测试使用真实 SQLite busy handler 确认同一次调用正在等待写锁。在锁仍被占用时推进逻辑时钟到租约边界，释放后验证 heartbeat/progress/complete/fail/finish_cancelled 均返回 JOB_LEASE_LOST；状态和事件由独立连接回读。该验证不依赖操作系统恰好调度到某个睡眠窗口。

故障注入覆盖 running/progress/cancel_requested/succeeded/failed/cancelled 事件写入失败和恢复批次第二条事件失败，验证状态与事件共同回滚。公共验收另执行真实 Worker SIGKILL、自然等待租约过期、第二 attempt 完成、独立产物校验和私有目录清理。

后续已完成 [原生存储迁出和读端口](native-storage-boundary.md)，Application 包不再依赖 SQLite。预检／插件等剩余用例归位和后端无关合成尚未完成；本契约不宣称 R2 全部完成，也不替代其他 R 的产品验收。
