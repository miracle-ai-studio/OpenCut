# R2 数据库控制额度账本增量验收 — 2026-09-24

本轮实现控制与 payload 分账的必要基础，不是 SQLite 写前拦截或 R2 放行。纯策略在 `editor-application::budget`；`storage-budget` 的账本 v2 记录独立控制限额、固定数据库文件名、活动控制预留与 OS 存活锁。默认控制限额 4 GiB；owner 可以用 `opencut_storage_budget_configure.control_limit_bytes` 改动，省略字段保留现值。旧 v1 请求与回执维持原序列化；首次设置控制限额或申请控制预留时升级账本 v2，旧宿主须停机后升级，不可与新进程共同写工作区。

`opencut_storage_budget_inspect.control` 分开报告控制限额、主库/WAL/SHM/journal 与账本目录的观察字节、活动预留、余量和数据库文件名绑定。原 `reserved_bytes` 仍只代表 payload，物理余量同时扣两类活动预留。控制文件路径只允许工作区中的单个普通文件名；符号链接或特殊文件观察失败即拒绝。当前观察不覆盖 SQLite 临时文件、其他外部写入和全部潜在 sidecar，且不具有原子文件系统快照语义。

数据库文件名尚未绑定时，`control.admission_headroom_bytes=null`，不能把只观察到账本目录的字节误读成数据库可用额度。

真实测试和证据：

- `cargo test -p opencut-storage-budget`：原有多进程 payload、FFmpeg 子进程和 SIGKILL 恢复仍通过；新增测试验证控制预留扩展、payload 额度隔离、进程被杀后的持久控制预留恢复、歧义 sidecar 拒绝、owner 幂等配置和旧格式配置不重置控制额度。
- `python3 scripts/verify-control-budget-policy.py --output design/qa/evidence/r2-control-ledger-policy-2026-09-24.json`：当前 debug CLI SHA-256 `565e3b1244fd49b635ebe24a15d8186a3eb58fa9a6de16bb55d58bebc3843c20`，公共工具读取、配置、精确重放与 v2 持久状态通过。[机器可读证据](evidence/r2-control-ledger-policy-2026-09-24.json)。
- 同一 CLI 将控制限额设为 4096 字节后，工程创建仍成功，主库为 339,968 字节。这是写前拦截**尚未接入**的明确反例；公开字段 `write_interception_enabled=false`、`enforces_all_workspace_writes=false` 与行为一致。
- `cargo test -p opencut-sqlite-store` 的 86 项单元及 6 项实际崩溃/并发测试通过；TypeScript SDK 的 11 项测试、Application 测试及依赖边界检查通过。它们只验证本次契约/账本没有破坏现有链路，不证明数据库容量准入已生效。
- 新 macOS x86_64 debug 开发包 `dist/local/opencut-0.1.0-dev-bc781d37f372-x86_64-apple-darwin.tar.gz`（SHA-256 `5b010611b95597895b574e083b3b23b0096e7d8a12053391af7e0932260dee1c`）在仓库外解包后通过 36 项验收，[报告副本](evidence/r2-control-ledger-package-acceptance-2026-09-24.json)已固定。包内附有本控制写入契约；这仍是开发包，不是 release／跨平台或完整 R0–R7 验收。

下一门槛是把所有管理型 SQLite 连接接到受控 VFS，在主库、WAL、journal 和 SHM 实际增长前扩展控制预留，再用固定读者、独立 64 MiB 卷、并发 Host、SIGKILL 和迁移验证无部分提交。真实分配的修复预留、repair 状态、完整控制文件观察和跨平台能力边界仍需随后完成。此前不得把 `write_interception_enabled` 或全工作区写入保障设为 true。
