# R2 项目上传基础验收 — 2026-09-21

## 结论与范围

已完成并验证上传持久化基础，**尚未完成项目素材导入端到端交付**。本轮没有发布新的开发包，最近通过仓库外验收的包仍为 `dev-0046d31b1081`（数据库 v7）。当前源码／debug 二进制为数据库 v8，36 个公开工具、8 种公开任务保持原状。Web/GPUI 实现继续暂缓。

本次源码层动线：创建项目 → 创建上传会话 → 写入分块 → 查询确认偏移 → 精确续传／冲突拒绝 → 进程重开；同时检查中止、权限变更、存储失败和迁移。没有把未接通的 REST/MCP/CLI 上传入口、FFprobe 或导入 Worker 算作通过。

## 设计与实现

- `api-contracts/uploads.rs`：纯控制 DTO，字节不进 JSON；size/offset 为规范十进制字符串，SHA-256 为小写 64 位。
- `editor-application/uploads.rs`：验证规则与 UploadStore 端口，不依赖 SQLite、文件系统或编解码器。总大小 1 GiB，单块 4 MiB，最多 4096 个确认块；每项目最多 8／工作区 128 个 receiving 会话。
- `sqlite-store/sqlite_uploads.rs`：v8 会话、块及项目素材表。创建与块确认的事件／审计在业务写事务内提交；读快照鉴权，写事务再次检查当前凭据。授权先于幂等回放。上传操作不创建修订或任务，不移动 head。
- `asset-store/uploads.rs`：先验证块字节，按 upload_id 获取跨进程 OS 锁，再授权、持久化文件及目录、提交确认偏移。文件落盘但数据库未提交时，可以留下私有块；重试必须验证完整长度／指纹。已确认块损坏或丢失返回完整性错误，不静默覆盖。
- 创建／写块／中止要求 read + edit + run_jobs，查询要求 read，归属只取权威会话记录。跨项目和猜测 ID 被拒绝；同项目授权编辑者可以续传。

实现中修正：移除原生诊断里的路径暴露；补齐相对存储根路径的父目录同步；对小块数量设上限，避免仅有字节配额却没有元数据上限；更新制造旧库的单测清理范围。打包和历史验收脚本改为读取被测二进制实际建立的数据库版本，避免仅凭工具名称推断 v7。

## 实际验证

`cargo test -p opencut-editor-application -p opencut-asset-store -p opencut-sqlite-store`：86 项通过，包含 Application 5、AssetStore 7、SQLite 70、媒体发布真实进程 1、上传真实进程 3。两个 ignored 条目是专供父测试启动的子进程入口，父测试已实际启动并验证它们。

新增存储验收 8 项，覆盖精确回放／冲突区间、错误长度／hash、磁盘失败、审计／事件失败原子回滚、重开、跨项目与权限组合、撤权后回执拒绝、中止幂等／配额释放、4096 块上限、参数逃逸和 v7→v8 DDL 冲突回滚。故障期间逐表计数和快照读回验证没有部分业务写入。

`tests/upload_crash.rs` 使用独立操作系统进程和真实 SQLite／文件系统：

1. 字节同步后、数据库 ACK 前 SIGKILL；偏移仍为 0，另一进程持锁等待，原进程死亡后重验已落盘字节并确认一次。
2. ACK 后、响应前 SIGKILL；偏移保持 3，精确重试不再增加块或确认事件。重试通过相对路径访问同一绝对路径工作区。
3. 两个进程向相同偏移写不同字节：只有一个成功，另一个冲突，持久化字节与记录一致。
4. 字节落盘后暂停，再由另一连接撤销凭据或中止会话：恢复执行不能确认，偏移保持 0。

暂停点仅在测试的 UploadStore 适配器里，生产代码没有故障注入开关。SIGKILL 校验实际退出 signal=9，未用修改数据库状态模拟崩溃。

历史验收：`python3 scripts/verify-history-compatibility.py`，17 组／16 格（v1–v8 × legacy/v2）通过；包括独立冻结 SQL 夹具、多进程迁移、旧回执重放、真实 WAV Worker、80 次并发打开、v7 升级冲突回滚。旧 v1–v7 夹具未改写。报告：

`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-xuaxutb6/report.json`

契约／工程门禁：Clippy `--all-targets -- -D warnings`、fmt、纯依赖／分层检查、生成契约一致性、旧 22 工具＋两个媒体发布工具兼容均通过。当前公开工具仍为 36，没有通过新增发现项宣告未实现的上传动线。打包的版本探测命令分别实际执行于本轮 debug 和上一已验收 release，正确读取 v8／v7；本轮未执行新包发行验收。

Debug binary SHA-256：`f0855cfba5d5b2fb8f6c03a56e023a8f94bf2fd4a8e5cc35779e8f54a690bfcb`。

本机记录：`/tmp/opencut-upload-final-tests.log`、`/tmp/opencut-upload-final-crash.log`、`/tmp/opencut-upload-final-clippy.log`、`/tmp/opencut-upload-build.log`、`/tmp/opencut-upload-history.log`。这些是本轮记录，不代表以后修改后的源码也已通过。

## 后续闭环门槛

1. 上传封存与 media.import 接受／工程归属原子提交；Worker 校验全量字节与真实媒体，固定 CAS。
2. 素材登记与合法 attempt 的任务成功原子提交，取消／过期 attempt／故障不发布资源。
3. 项目素材引用查询、绑定／替换；拒绝伪造 URI、版本、跨项目 media_id 和已知全局 hash 越权。
4. 公开控制工具、HTTP 字节路由、CLI 本地流式助手、双 SDK、MCP 数据面交接与能力声明。
5. 终态／孤儿块清理、磁盘总配额、真实视频／音频导入后导出与独立 RGB／PCM、断网／撤权、仓库外新包验收。

上述项完成前，不将 R2 或“项目导入”标记完成，也不把本轮过程验证表述为无 bug 的完整产品验收。
