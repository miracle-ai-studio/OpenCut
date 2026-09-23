# R2 调用来源与原子审计验收

本轮增加可信 Host ExecutionContext、数据库 v4 来源记录、共享 `opencut_audit_query`（当前 30 工具），以及 CLI/HTTP/两种 MCP 的来源绑定。范围与未完成边界见 [契约](../architecture/actor-audit-contract.md)。这是可信本机主体的来源链，不是完整 principal/project 授权放行。

## 验证依据

最终 debug 候选 SHA-256：`14cab09a9fac7f7abd0840e51fcb6d1c7963a6c04cf8d34d630fe365a68b2c39`。

- `scripts/verify-audit.py` 六组真实动线通过：CLI 响应 ID 与创建记录绑定；HTTP 幂等重放保留首次 actor；伪造请求头不改来源，正文注入身份被拒绝；真实 stdio/网络 MCP 编辑分别记为 mcp_stdio/mcp_http；真实 FFmpeg frame 产物关联接受请求、Worker 和 attempt；逐页遍历与边界验证；真实 SQLite 审计插入故障回滚工程写入，修复后原 key 可成功提交。
- `scripts/verify-history-compatibility.py` 13 项、八格矩阵（v1–v4 × legacy/v2 identity）通过。历史来源保持未知；v3→v4 DDL 故障全量回滚，修复后可重试。另执行 20 轮四进程同时打开 DELETE journal 库并转 WAL，无启动锁错误，快照和 integrity_check 一致。
- SQLite 55 项通过，含审计故障对工程创建/编辑/任务接受/取消/完成的回滚，跨来源重试保留首次记录，租约恢复和旧 attempt 拒绝，超过 JS 安全整数范围的序号，以及 WAL 配置等待期限/原超时设置恢复。CLI 七项、Automation 一项及 TS SDK 五项通过；这些不能替代上面的真实协议和媒体证据。
- 旧 22 工具 input/output Schema 和 read_only 不变；契约生成、纯依赖边界及受影响 crate 的 Clippy `-D warnings` 通过。新增 typed AuditQuery 同源生成到 TS/Rust SDK。

原始证据索引为 `/tmp/opencut-audit-acceptance.json`、`/tmp/opencut-audit-history.json`、`/tmp/opencut-audit-{mcp-http,mcp-context,mcp,http,event-streams}.json`；各包含哈希或临时证据路径。临时文件不保证长期保留，交付包另附 acceptance 清单。

## 验收发现的实际问题

1. 旧迁移单测用当前库再降级构造夹具时，仍残留新增 audit_records，导致迁移失败。已修正单测的降级构造；真正兼容承诺由独立冻结 v1–v4 SQL 夹具验证，未修改 v1–v3 夹具来迎合实现。
2. 八格矩阵在两个真实 CLI 同时打开 v4/DELETE 库时复现 SQLITE_BUSY。WAL 转换现在只在 autocommit 配置阶段按原 busy_timeout 的剩余预算重试，结束后恢复配置；不重放业务提交。持续锁定仍按期限返回错误。SQLite 的 busy handler 可能因潜在死锁而不被调用，不能仅假设设置 busy_timeout 就必然等待，参见 [SQLite busy handler](https://www.sqlite.org/c3ref/busy_handler.html) 与 [WAL 配置](https://www.sqlite.org/wal.html)。
3. 并行 Cargo 检查时，包脚本将成功命令的 stderr 锁等待提示拼入 cargo metadata JSON，导致解析失败。现在 stdout/stderr 分离，失败保留两路诊断，成功仅解析 stdout。

## 本轮交付

新版 release 包正在独立验收。本节仅在实际解包后的来源查询、SDK、历史矩阵及媒体动线通过后更新，不将 debug 结果作为 release 通过证明。

## 剩余范围

本机 OS 调用者/共享 bearer 目前统一为 local-owner，Worker 为 local-runtime；并未识别具体人员、模型或 Agent。来源记录不限制另一个持有共享 token 的调用者访问项目。受限凭据、principal/project/action/resource 授权、订阅重新鉴权、插件/媒体维护的完整审计及拒绝日志仍需实现。R3 分支和选择性撤销、R4 持续预览、R5 分析、R6 完整编辑及工程包、R7 平台与整体验收仍未完成；Web/GPUI 实现继续暂缓。
