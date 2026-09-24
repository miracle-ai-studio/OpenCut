# R2 产物回收审阅计划增量验收（2026-09-24）

状态：**Owner 可创建、找回和分页查询持久审阅计划；尚无删除执行入口，R2 仍部分完成。** `opencut_artifact_gc_plan` 将精确产物 ID、幂等键、完整保护根集、私有原生路径及文件身份和公共审阅快照保存到 schema 17。计划与 `artifact.gc.planned` 审计在同一 SQLite 事务提交；相同请求重放得到同一计划，参数变更报冲突。公共计划最多展示 100 个保护根，不暴露原生路径或 inode，且明确返回 `execution_available=false`。`opencut_artifact_gc_inspect` 和 `opencut_artifact_gc_query` 提供重启后的找回与有界分页。受限主体不能调用这三个工具。

原生观察会按文件身份识别普通路径、符号链接和百分号编码的 `file://` URI 别名；损坏权威文档、无法解析的文件 URI 和扫描超限均报错，不当成零引用。计划仅是某一时刻的审阅快照：SQLite 读取与文件身份观察不是跨存储原子快照，不能据此删除字节。未来执行必须让所有引用发布者进入同一门闩，在排他尝试锁内重新检查保留状态、引用及文件身份，并持久记录删除结果与失败恢复。

本轮公开注册表为 67 个叶操作、9 个领域入口，生成 OpenAPI 为 82 条路径。冻结的 v17 历史夹具、旧版碰撞回滚及 v1–v17 两摘要共 34 格历史矩阵通过。受影响 Rust crate 测试、Clippy `-D warnings`、TypeScript SDK 11 项测试、生成契约检查、领域映射与受限身份验收通过。

同一 macOS x86_64 **debug 开发包**为 [`opencut-0.1.0-dev-f3b26f2747fe-x86_64-apple-darwin.tar.gz`](../../dist/local/opencut-0.1.0-dev-f3b26f2747fe-x86_64-apple-darwin.tar.gz)，归档 SHA-256 `825f6057dcb1ae8e1446a4f5dcc18c746302fd8718fdcc8f39cd5b7df90b8f54`，包内二进制 SHA-256 `f818129209b0f8e74d279cbdaeddf079552594c8a3045a241fc2c58350fd4ce6`。[仓库外 35 项整包验收](evidence/r2-artifact-plan-package-acceptance-2026-09-24.json)通过，包括双 SDK、MCP／HTTP、历史兼容、独立盘满、Worker、导入恢复和最终重开。[包内真实媒体动线](evidence/r2-artifact-plan-product-2026-09-24.json)通过 13 项检查：审计写入失败时计划回滚；release 后的工程修订、排队 Job 和 URI 别名被识别；同键重放、分页、重启找回成立，原产物仍可下载。可选 WASM component、release 构建与跨平台包未随本轮复验。

下一步先实现引用发布门闩，再接排他锁内的执行前重查、持久 `removing`／结果回执，以及 SIGKILL、盘满、别名替换和并发发布矩阵。控制元数据的严格物理保证、全工作区准入及 R0/R1、R3–R7 各自的产品门槛仍未放行。
