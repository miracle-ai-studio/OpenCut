# R2 历史格式兼容与本地交付验收

2026-09-21。本轮补充冻结历史格式验收，并交付经过仓库外解包验证的 macOS x86_64 本地开发包。不是完整 R0–R7 放行，也未发布公共 registry、签名/公证或其他平台包。

## 交付物

- 安装包：`dist/local/opencut-0.1.0-dev-64424c1d8807-x86_64-apple-darwin.tar.gz`，约 13 MiB。
- archive SHA-256：`f95b03cb8974212d97dfc8c2859520678cac9a4cb6a908e65ce46cb55d5551bd`。
- release CLI SHA-256：`40227333301c3fe03e8e86c222487e0917077c67ecc9a1880155aacd6ba825f4`。
- 同目录 `.sha256`、`.json` 和 `.acceptance.json` 分别保存校验、打包清单和实际验收报告；打包清单的 `packaged_not_yet_accepted` 记录打包时状态，是否验收通过以绑定同一 archive hash 的 `.acceptance.json` 为准。
- 包内有优化构建的 CLI/HTTP/MCP 宿主、TS npm tarball、Rust SDK 与四个纯契约 crate 的源码包、工具 Schema/OpenAPI、文件清单、依赖版本清单、诊断与升级/回退说明。

通过 `package-local.py --skip-build` 重复组装，archive 路径和 SHA-256 相同；这是同一已构建二进制和源文件的确定性打包，不宣称跨机器重编译 Rust 二进制逐字节相同。

源码清单摘要 `3bea87d4ade1840795cbc41d7a3ceba29ba8734dec110d1bcaeb9718add54379`，覆盖 144 个 Cargo/crates/CLI/scripts/packages 文件；清单位于 `/tmp/opencut-delivery-source.json`。实际平台 macOS 26.5.1 x86_64，Rust 1.97.1、Node 24.19.0、FFmpeg/ffprobe 8.1.2。

## 兼容范围与实际证据

`packages/contract-fixtures/history` 是从仓库保留的历史格式定义重建并冻结的夹具，不是从旧发布二进制导出的数据库。数据库版本和后期引入的任务种类做交叉组合，验证当前承诺的格式兼容，不虚构历史发布时间表。

| 验收 | 结果 | 证据 |
|---|---|---|
| Debug 历史格式 | 10 项/6 格通过：数据库 v1/v2/v3 × legacy/v2 摘要；两个真实进程并发打开、回执重放、历史表不变、升级后继续编辑、8 类结果和旧排队输入 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-6c3vbm0s/report.json` |
| 迁移失败/未知格式 | DDL 错误完整回滚，移除测试故障后可重试；未来/损坏/其他应用数据库拒绝且主文件字节不变 | 同上，原始请求/响应在相邻 `trace.json` |
| 仓库外实际交付 | 10 项通过；archive 与包内文件校验、损坏文件诊断、TS 独立安装、Rust 源码编译/纯依赖检查、真实服务、跨 SDK 重放、CLI 回读、服务停止后重开 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-xn56csks/report.json`；已复制为 archive 旁 `.acceptance.json` |
| Release 历史格式 | 同一 10 项/6 格通过，使用从实际交付包解出的 binary | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-5ksuy6rz/report.json` |
| Release 真实媒体 | 12 项通过：上传/重复内容、预检、固定修订预览/导出、修复/并发、损坏产物拒绝、独立 RGB/PCM 及音频样本偏移 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-xn56csks/media-journey/journey-1789934311599959000/report.json` |
| Release MCP | 18 项通过，使用与交付包字节一致的优化 binary | `/tmp/opencut-release-mcp.json` |
| Release WASM 插件 | 14 项通过；实际组件、权限/Schema/语义失败、无工程副作用、历史修订与重试 | `/tmp/opencut-release-plugin.json`，记录相同 binary hash |
| Release 契约 | 解包程序现场输出的工具 Schema 与包内生成文件完全一致 | 仓库外验收 `commands.json` |

Rust SDK 消费项目仅从解包目录解析 OpenCut 源码，编译缓存单独复用 target 目录；没有通过仓库源代码路径偷渡成功。第三方依赖来自当前 Cargo 缓存，SDK 包附 lockfile，但不包含 registry vendor，也未验证断网空缓存首次安装。

首次包验收在已编译 SDK 后因 `/var` 与 `/private/var` 的等价路径比较失败；已统一真实路径后重新执行完整动线通过。失败记录保留在早先 acceptance 临时目录。没有改变仓库外来源要求、降级路径检查或跳过 Rust 消费。

## 放行边界与后续

本轮交付证明当前平台的本地开发包可解包、诊断、消费 SDK 和执行已覆盖动线；不代表签名分发、全新 OS、ARM64/Windows/Linux、任意历史发行版降级或完整产品验收。

完整 R2 核对另发现此前“只剩矩阵与发行”的描述遗漏了 CLI NDJSON 和 MCP resources/prompts；actor/资源授权也跨 R2/R3/R7 未完成。已记录于 [R2 放行核对](../architecture/r2-completion-audit.md)。下一步先补这两类 Agent 接口，再推进协作与后续 R3–R7；网页/桌面实现及真实 UI 保持暂缓。
