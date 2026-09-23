# OpenCut 文件恢复记录（2026-09-23）

主工作树已从 `miracle-ai-studio/OpenCut` 的 `400f097becba5db0fbc305d5a65348cb81c20356` 克隆，并用本机 Codex 历史会话中的实际文件变更与成功执行的写入命令恢复源码。`tmp/opencut-classic` 是 `OpenCut-app/opencut-classic` 的独立对照副本，未混入主源码。本次恢复源码、契约与 QA 证据由本地 Git 检查点封存；`tmp/` 现场与 `dist/` 开发包保留在工作区，不进入主源码提交。

这次回放处理 1556 个历史事件，成功恢复 132 次文件新增、983 次文件更新、149 次覆盖写入、47 次追加写入，以及多次迁移脚本和格式化。随后根据原始命令记录补回运行时路径转换、项目权限 SQL 迁移、一个原生模块字段名，并从保留的 QA 脚本恢复产品动线脚本。回放统计和脚本保存在 `tmp/recovery-evidence-20260923/`。旧的未完全回放副本保存在 `tmp/opencut-recovered-from-records-v6-20260923/`。

当前验证：

- `cargo check -p opencut-cli --offline` 与 `cargo build -p opencut-cli --offline` 通过；`cargo fmt --all --check` 通过。
- `opencut-editor-application` 的 5 项测试通过；`opencut-sqlite-store` 的 86 项单元测试及 6 项集成测试通过（另有 2 项忽略）。
- 新构建的 CLI 完成真实 HTTP/CLI 工程创建、事务编辑、冲突处理、预览、导出、服务重启和媒体完整性动线；受限凭证验收与项目上传/导入验收通过。报告保存在 `tmp/recovery-evidence-20260923/`。
- 补回冻结的 v1–v3 数据库 SQL、历史工程与结果夹具；当前二进制的 v1–v15 × legacy/v2 历史矩阵共 30 格、22 项检查通过。这些夹具是从历史格式定义重建的合成数据，不是旧发布包的真实用户库。
- TypeScript SDK 已重新生成类型与 OpenAPI，仓库外安装的独立消费项目通过 13 项真实 HTTP/CLI/Worker 检查；Rust SDK 仓库外消费与跨语言重放通过 12 项。官方 MCP SDK 1.26.0 通过领域调用、断线续接和资源通知。领域入口默认发现 9 个工具，覆盖 61 个旧操作，旧操作仍可调用。详见 [本轮 QA](design/qa/domain-surface-and-recovery-2026-09-23.md)。
- 当前 debug 二进制修复了盘满时公共 Job 入口把 `DATABASE_CAPACITY_EXHAUSTED` 折叠成 `JOB_FAILED` 的错误。生成 OpenAPI 又补入真实原始上传、产物字节下载和公共 SSE，现为 75 条路径，事件记录 Schema 与 Rust 二进制同源。重新打包的 macOS x86_64 本地开发包在仓库外通过 21 组验收，包含真实 64 MiB 盘满／WAL、权限与管理审计、存储观察、写入准入、受限网络流与撤权；双 SDK、官方 MCP、30 格历史、项目导入强杀恢复及真实媒体动线继续通过。清单从实际二进制读取 schema 15、61 个操作和 9 个领域；相同 SHA 的 Worker 输出准入另通过 4 项。新包、哈希和验收范围见 [数据面 QA](design/qa/r2-data-plane-openapi-2026-09-23.md)。这不是 release 优化构建或跨平台发行。

仍不能宣称历史工作树完整恢复：回放报告中仍有 20 次文件补丁与 61 次 Python 写入未自动应用，部分属于历史过程中被后续变更取代的尝试，尚未逐项核销。Web 编辑台的 [源码测试、类型、生产构建和统一契约检查](design/qa/r7-web-source-check-2026-09-23.md) 已在当前环境通过，但真实浏览器操作、桌面、全部插件和所有 R 阶段的整体完成状态尚未重新验收。

`tmp/opencut-accepted-package-20260923/` 保留了先前验收包的可执行文件、合同文件和 SDK 发布包，可用于对照，但它不证明当前源码已完整恢复。
