# 非 UI 功能交付台账

用户范围（2026-09-24）：完成 Agent 视频编辑框架的全部功能，Web 页面与 Desktop 界面可以暂缓。此项调整不免除 Rust 内核、媒体执行、预览后端、CLI／MCP／API／SDK、插件与工程恢复能力，也不把 `capability=false` 当作功能完成。

本文件记录当前执行范围，历史 R0–R7 审计仍可追溯。状态分为「动线已验」「实现待集成／验收」「未完成」，不以新增工具数量替代验收。收据中的 binary SHA 是实际被测版本，早先 release 包不能证明当前源码已经通过发行验收。

| 需求 | 当前非 UI 状态 | 剩余边界 |
|---|---|---|
| R0 真实动线、性能基线 | 已有本机导入／编辑／渲染／存储故障证据 | 指定硬件的完整长素材、VFR／混合采样率、冷／热 p50/p95、真实访谈人工质量评审仍未完整冻结／放行 |
| R1 核心可靠性 | 固定修订、精确时间、原子编辑与持久任务链已具备 | 全需求故障窗口与音频样本级同步仍需完整复核；页面状态问题暂缓 |
| R2 分层与统一契约 | contracts／core／composition／application／native／SQLite 边界检查通过；旧 22 个工具契约仍兼容 | 全工作区写入准入、多宿主、非 Unix、完整历史版本与新发行矩阵未整体放行 |
| R3 撤销／分支／接管 | schema2/3 的选择性撤销、重做、分支 create/edit/compare/merge、双父 DAG 及 owner 接管均有公共 CLI 收据 | 升级本身不能选择性撤销；协作 UI 暂缓 |
| R4 连续预览 | HTTP 固定修订单 FFmpeg fMP4 流、lease/generation、播放暂停 seek、独立 presentation、交接与重启终结动线已验 | 字幕像素、SIGTERM 清理、SIGKILL 重启回收与另一个活跃宿主保护已验；schema3 的 fMP4 预览、帧任务、整段导出在三个像素点比对通过；长时性能、完整故障矩阵、浏览器实际呈现／出声未放行；浏览器 UI 暂缓，不能宣称低延迟或已观看 |
| R5 素材分析 | 真实 Whisper 转写、三种 FFmpeg 分析、来源查询分页与取消重放动线已验 | 英语真实词级时间与固定模型 SHA 已验；统一 evidence query 已对转写、静音、镜头、波形、说话人边界做真实来源投影；分页游标绑定结果、查询与调用身份；JFK 单人样本得到 0 个边界，不能证明双人正例准确率；多语言质量与真实访谈闭环未完成 |
| R6-A 编辑语义 | 当前编辑操作、字幕人工修正与 SRT／ASS 交换后渲染已验 | Classic v31 非空工程转换、明确损失接受与独立渲染已验；schema3 的图形、调色、硬蒙版/羽化模糊、交叉淡化在真实 FFmpeg 帧／片段／预览路径通过像素验收；曲线动画与完整 keyframe 语义尚未完成；ASS 子集会拒绝无法表达的样式 |
| R6-B 插件 | 受限 WASM planner 与 Plugin V2 声明式模板／字幕样式的安装、授权、计划、应用、真实渲染和篡改／撤销拒绝已验 | 声明式模板／样式包在 schema3 工程中完成授权、计划、应用、真实渲染、篡改和撤销拒绝验收；效果／转场仍由内核图节点表达；分析器、生成器、provider、导入导出插件等类别与完整兼容治理未完成 |
| R6-C 工程包 | schema2/3 linked 与 self-contained 固定修订包均通过导入、源工作区删除、继续编辑和真实渲染 | 字体／插件／工具尚未全部打包锁定；当前包不是全历史可复现归档 |
| R7 发行与平台 | 已有前一 R2 release 包独立验收，新源码持续集成中 | 当前 schema3 候选二进制尚未封装为正式发行包；升级恢复、跨平台矩阵与最终整包验收未完成；Web/Desktop 页面开发暂缓 |

最新本机集成验收候选为 SQLite schema23、127 个叶工具，binary SHA-256 `7a404e8984318356e18e3cf71e5aa7d8ad86cfaad1afc0fde9af6cc7136ab59f`。同一二进制通过旧 22 个工具兼容、contracts/core/application/composition 边界检查、TypeScript SDK 13 项测试、契约生成检查和 schema3 作业／预览／工程包公共动线。源码级 `cargo test --workspace --exclude opencut-desktop --offline` 全量回归通过；受影响 media 与 project-render 的 Clippy `-D warnings`、`cargo fmt --all -- --check` 也通过。完整 workspace 命令会尝试编译 GPUI Desktop，在此机器因 Apple `metal` utility 缺失而停在 shader 构建；页面和 Desktop 当前按用户范围暂缓。该候选仍未封装正式发行包或通过跨平台与升级恢复验收。

全仓回归发现并修复了一个真实音频渲染停滞：多轨混音在 `atrim` 前使用无界 `apad`，本机 FFmpeg 8.1.2 的简单一秒渲染无法输出首个媒体包；改成按目标绝对结束时间做有限补齐后，同一动线 1.25 秒通过，真实项目渲染测试 7/7 通过。详见 [媒体与全 workspace 回归](../qa/evidence/r0-r7-backend-regression-audiofix-2026-09-25.json)。

预算 VFS 已将逐页同步改为有界提前预留，并在同步边界释放余量；额度拒绝／回滚／恢复测试通过。110 万字节 track name 在该 debug 构建的一次真实 CLI 样本中耗时 3.50 秒，原受影响 MCP 全动线已通过。此样本不是长媒体 p95 指标。

当前真实动线收据：

- [R3 协作与持久接管](../qa/evidence/r3-collaboration-2026-09-24.json)
- [英语词级时间与模型来源](../qa/evidence/r5-word-timestamps-2026-09-24.json)
- [预览崩溃恢复与多宿主保护](../qa/evidence/r4-preview-crash-recovery-2026-09-24.json)
- [Classic 工程迁移](../qa/evidence/r6-classic-import-2026-09-24.json)
- [Plugin V2](../qa/evidence/r6-plugin-v2-2026-09-24.json)
- [预算写入性能样本](../qa/evidence/r2-control-admission-performance-2026-09-24.json)
- [连续预览](../qa/evidence/r4-continuous-preview-2026-09-24.json)
- [静音、镜头、波形](../qa/evidence/r5-analysis-jobs-2026-09-24.json)
- [跨分析 evidence 分页及身份游标](../qa/evidence/r5-analysis-evidence-2026-09-25.json)
- [真实中文转写与统一 evidence query](../qa/evidence/r5-transcription-evidence-2026-09-25.json)
- [说话人边界与统一 evidence query](../qa/evidence/r5-speaker-turns-evidence-2026-09-25.json)
- [转写证据分页与搜索](../qa/evidence/r5-analysis-query-2026-09-24.json)
- [独立工作区工程包](../qa/evidence/r6-project-package-2026-09-24.json)
- [字幕交换并解码验证](../qa/evidence/r6-subtitle-exchange-2026-09-24.json)
- [Schema 3 协作与撤销](../qa/evidence/r3-v3-collaboration-2026-09-25.json)
- [Schema 3 固定修订任务](../qa/evidence/r6-v3-jobs-2026-09-25.json)
- [Schema 3 连续预览与帧导出一致性](../qa/evidence/r4-v3-preview-2026-09-25.json)
- [Schema 3 插件模板](../qa/evidence/r6-plugin-v3-2026-09-25.json)
- [Schema 3 工程包恢复、编辑与渲染](../qa/evidence/r6-v3-package-2026-09-25.json)
- [修复后 schema3 固定修订作业与真实渲染](../qa/evidence/r6-v3-jobs-audiofix-2026-09-25.json)
- [修复后 schema3 连续预览像素对比](../qa/evidence/r4-v3-preview-audiofix-2026-09-25.json)
- [修复后 schema3 工程包导入与渲染](../qa/evidence/r6-v3-package-audiofix-2026-09-25.json)

执行原则：先关闭真实功能和执行路径缺口，再冻结二进制、生成契约并做整包发行验收；已验证的专项收据均记录二进制 SHA 与原始报告。页面端和 Desktop 暂缓，不降低内核、协议、执行、恢复与 SDK 的要求。
