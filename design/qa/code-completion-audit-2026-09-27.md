# 2026-09-27 全项目代码完成度核验

**结论：当前不能验收为“全部代码设计完成”。** 本轮实际修改了编译器、应用端口、原生适配器、公共协议和验收代码，并执行了构建/行为测试。仍有架构迁移和产品功能只有目标设计，没有完整实现。

机器收据：[code-completion-audit-2026-09-27.json](evidence/code-completion-audit-2026-09-27.json)。被测候选为本机 macOS x86_64 debug CLI，SHA-256：`d2278bba33b57c1532ed415744940f481bb1a8691a84d1d020d1bbd28e501658`。它是本次验收的固定副本，不是已签名发行包。

## 本轮真实代码改动

| 改动 | 实际代码入口 | 验证方式与边界 |
|---|---|---|
| 编译时视觉覆盖校核 | [coverage_v3.rs](../../crates/composition/src/coverage_v3.rs)、[compile_v3](../../crates/composition/src/project_v3.rs) | 每次 V3 编译都与源快照核对 clip 绑定、shape、effect、mask、transition。丢失、注入、重复、参数改变、层序/索引错误立即失败；禁用轨明确记录排除。负向测试主动删节点和改参数。尚非音频/字幕/关键帧全语义覆盖，也未持久化完整执行回执 |
| 版本化复现核验协议 | [纯 DTO](../../crates/api-contracts/src/reproducibility.rs)、[应用端口](../../crates/editor-application/src/reproducibility.rs)、[原生观察器](../../crates/native-services/src/reproducibility.rs) | 新工具 `opencut_project_package_reproducibility_v1` 经同源 registry/SDK/OpenAPI/dispatcher 接入。应用层检查 owner 和清单身份；原生层只读核对 manifest/project 字节，观察当前可执行文件和原生工具。旧包缺少原引擎身份时严格复现结果为 false；未锁定工具是 not_verified，不伪装成 mismatch |
| 旧复现声明警告 | [Automation::call](../../crates/automation/src/lib.rs) | 旧导入/导出 receipt 的布尔值保留兼容；为 true 时 envelope 增加 `LEGACY_REPRODUCIBILITY_CLAIM_UNVERIFIED`。不篡改旧工程包或幂等回执，新核验接口单独给出证据状态 |
| 公共入口独立结果验收 | [verify-render-semantic-coverage.py](../../scripts/verify-render-semantic-coverage.py) | 用合成红/蓝视频经 CLI 提交、Worker 执行、HTTP 下载。按手算颜色检查帧/全片/区间的转场以及关键帧时钟；核验产物 SHA、固定 revision 和只读包检查。每次先冻结执行文件，防止并行构建替换被测 binary |
| 修复失效的 MCP 验收范围 | [verify-mcp.py](../../scripts/verify-mcp.py) | 原脚本只声明 9 类 Job，遇到已实现的分析和 V3 Job 时失败。补齐 17 类的显式必需字段断言，继续拒绝遗漏、重复或未知描述符；没有把精确检查放宽为任意超集 |
| CI 行为门槛 | [architecture.yml](../../.github/workflows/architecture.yml) | 除依赖政策外，增加 composition/application 的 Rust 行为测试 job。远程执行和 required check 配置尚未验收 |

该新核验接口没有完成“旧渲染器可恢复”“跨硬件逐像素/逐字节一致”。当前执行文件 hash 只是当前宿主的观察值，不补写为旧包的原始渲染器身份，也不把素材完整声明提升为实际素材验证。

## 实际验证

| 检查 | 结果 |
|---|---|
| 冻结源码后的非 UI Rust workspace 回归 | 379 项通过、0 失败；6 个 ignored 均为由父测试显式启动的子进程入口，实际崩溃/恢复父测试通过 |
| 最后增量的 composition/application/automation 复测 | 44 项通过 |
| 非 UI workspace Clippy；最后增量受影响 crate Clippy | 均通过，`--all-targets -- -D warnings` |
| TypeScript SDK | 新生成类型的构建及 13 项测试通过 |
| Web | 31 项测试及生产构建通过；不等于编辑页面产品动线验收 |
| 独立视觉 oracle | 6 个实际输出像素检查通过：转场前/中/后、全片中点、区间中点、clip-local 关键帧 |
| 工程包公共动线 | 16 项检查通过：旧包兼容、迁移后继续编辑、插件恢复计划/篡改拒绝、工具差异、缺字体诊断及真实渲染 |
| stdio MCP | 18 组检查通过，MCP 修改后再由 CLI 读取状态 |
| 历史 wire 契约 | 仓库冻结的 22 个工具通过；额外逐项对比上一 release 的全部 129 个工具 input/output schema 和 read_only 均不变；当前 130 工具 |
| 同源类型/契约生成 | 使用固定候选检查，无漂移 |
| 架构门禁 | 21 个 Rust 模块依赖检查、14 项门禁测试、既有纯契约/应用/合成传递边界检查通过；仍有 12 条登记的迁移边 |
| 格式与改动检查 | Rust fmt、git diff whitespace 检查通过 |
| Desktop | **未通过编译**：GPUI 构建 shader 时 `xcrun` 找不到 Apple `metal`；没有跳过该错误并标为成功 |
| 独立 Cloudflare `apps/api` | **构建未通过**：本地未安装 Wrangler。源码仍只有 `/`、`/health`、`/echo`；不能视为已接入 Rust 编辑应用服务 |

独立像素验收中，转场中点在帧、完整导出、区间输出均为 `[125,0,122]`，排除了“相同引擎两端都退化成硬切”的假通过；关键帧蓝通道实测 96，在手算 102 的有损编码容差内，明显区分错误 handle 时钟约 138 的结果。色彩容差与确定的源色/时间点绑定，不用于推断所有真实素材的观感。

验证过程中保留了三个负面发现：最初验收夹具复用了素材/clip ID，被核心正确拒绝，已修正夹具；并行构建曾让运行中验收的文件 hash 改变，已改为固定候选；旧 MCP 验收脚本任务清单过时，已补齐断言后重跑。这些失败没有计入通过收据。

## 逐层完成度与仍需完成的代码

| 范围 | 已有实现与证据 | 尚不能验收的部分 |
|---|---|---|
| contracts/time/core/application | 纯契约、精确时间、编辑计划、原子提交、幂等、权限与租约；全量和故障回归 | 高层 ripple/roll/slip/slide/sync-group 意图规划及全部关联对象规则尚未形成完整产品接口 |
| composition/project-render/media | 固定修订编译、绑定、基础视觉/音频、转场、曲线和本轮覆盖校核；独立像素与原生测试 | 全节点 Compile/ExecutionReceipt、完整参数语义能力协商、跨后端独立 oracle、完整色彩/音频处理 |
| automation/CLI/native-services | 公共 DTO 与统一分派已工作；新增检查通过应用端口，原生 I/O 留在适配器 | `Automation` 仍持有 SQLite facade 和目录，并构造原生能力；CLI 仍直接导入具体实现；D01/D02 的 7 条边未移除，host-runtime 未抽出 |
| job-runtime/job-execution | 固定任务输入、取消/恢复/发布围栏和真实 Worker 动线 | 通用 ExecutorPort 未抽出；D03 的 5 条原生依赖仍在调度包内 |
| sqlite-store/asset-store/storage-budget | CAS、原子存储、额度、journal、崩溃恢复有运行验证 | 全工作区写入准入、长素材单文件限制、全平台资源治理仍未整体放行 |
| preview | 帧、区间和连续预览后端已有实现；本轮核验帧和区间 | 代理/缓存架构、低延迟冷/热 seek 指标、完整浏览器播放/出声未验收 |
| analysis | 转写、静音、镜头差异、波形、说话人边界和 evidence 查询已有实现 | 人物覆盖、相机运动/抖动、稳定区间、重复场景、节拍/乐句尚未形成统一正式能力闭环 |
| motion/effects/masks | 基础变换、曲线、交叉淡化、矩形/椭圆遮罩和羽化 | 稳像、速度曲线/重采样、Bezier 遮罩、对象追踪与失败恢复尚未完成 |
| plugin-host/providers | WASM planner、V2 声明式 preset、权限/恢复/篡改拒绝已验 | 分析/生成/效果/导入导出等插件类别和统一 Provider 执行协议尚未完整实现 |
| package/reproducibility | 工程包恢复、精确 FFmpeg/插件锁、新只读核验和旧声明警告 | 原始 OpenCut 执行身份入包/入 Job、字体文件锁、全局恢复事务和严格复现证据仍缺 |
| SDK/UI/服务宿主 | 双 SDK、原生 HTTP/MCP/CLI；Web 测试和构建 | TS 全部导入边界和传输实现收敛、Desktop 编译/接入、独立云 API 接入及实际 UI 完整动线 |
| release | 本机固定候选的专项验收 | 跨平台、签名、升级/回滚、整包发行矩阵未完成 |

源码中没有 `todo!()` 不能证明这些设计已实现；现有测试通过也不能证明没有测试和实现的目标已完成。后续实施仍按 [W1–W6](../architecture/architecture-renewal-plan.md) 推进；本报告不关闭这些缺口。
