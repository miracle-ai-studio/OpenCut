# R2 已发布产物引用观察增量验收（2026-09-24）

状态：**Owner 的有界引用查询已通过源码、权限、真实媒体和仓库外交付包验收；完整回收未完成。** `opencut_artifact_references_inspect` 按 `artifact_id` 在同一 SQLite 读事务中扫描工程修订、Job 输入、其他 Job 结果、项目素材、插件发布及项目插件授权。生产该产物的 Job 结果是发布事实，不被当成阻止 release 的外部引用。返回保护根类别和身份、总数、是否截断、扫描行数和原生来源候选数，不返回原生路径。受限主体不能查询。损坏的权威文档和扫描超限报错，不解释为零引用。

公开操作由 63 增至 64，生成 OpenAPI 由 78 增至 79 条路径；schema 保持 16。领域发现和精确 schema 同源，TypeScript SDK 的生成类型同步更新。`root_count` 为扫描所得总数，最多展示 100 个根。当前仅按字符串观察；路径别名尚未按 inode 解析，扫描没有协调并发发布，因此**空结果不是删除许可**，也没有公开删除入口。契约见[产物保留与回收](../architecture/artifact-retention-contract.md)。

同一 macOS x86_64 **debug 开发包**为 [`opencut-0.1.0-dev-cef315cbbff1-x86_64-apple-darwin.tar.gz`](../../dist/local/opencut-0.1.0-dev-cef315cbbff1-x86_64-apple-darwin.tar.gz)，归档 SHA-256 `85fb4f22a1b9be38e050b9800c32196e27c2dd1f02ca6f3b29e8f50b36328fa3`，包内二进制 SHA-256 `10d22e0c68a4876a6ad0604da2783416d03d745a4b155a07b01211e25b1d580b`。[仓库外 35 组整包验收](evidence/r2-artifact-references-package-acceptance-2026-09-24.json)通过，含双 SDK、MCP／HTTP、v1–v16 两摘要历史矩阵、受限权限、独立盘满、Worker、导入恢复和最终重开。可选 WASM component 未随本包复验。

[包内真实媒体动线](evidence/r2-artifact-references-product-2026-09-24.json)完成 12 条检查：Owner 先查询产物默认保留，release 后在另一个真实工程修订中绑定已发布字节，并创建引用同一路径的排队 Job；再次查询能看到 `revision` 和 `job_input` 两个独立保护根，原产物仍可下载。动线还覆盖 HTTP／CLI 同键重放、审计一次提交、重启持久化、损坏／替换／别名文件拒绝、实际预览 RGB／PCM 和旧固定修订导出。相关 Rust 测试、受影响后端 Clippy `-D warnings`、TypeScript SDK 11 项测试、生成契约检查、领域入口与受限身份动线通过。

下一步须将原生路径候选与产物按文件身份安全匹配，在新引用发布门闩和下载租约下重检，再实现持久可审阅计划、精确哈希执行、删除回执和 SIGKILL／盘满恢复。完整 R2 还缺控制元数据严格物理保证、全并发故障矩阵和 release／跨平台验收；R0/R1、R3–R7 的独立产品门槛仍未放行。
