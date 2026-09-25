# 非 UI 功能交付台账

用户范围（2026-09-24）：完成 Agent 视频编辑框架的全部功能，Web 页面与 Desktop 界面可以暂缓。此项调整不免除 Rust 内核、媒体执行、预览后端、CLI／MCP／API／SDK、插件与工程恢复能力，也不把 `capability=false` 当作功能完成。

本文件记录当前执行范围，历史 R0–R7 审计仍可追溯。状态分为「动线已验」「实现待集成／验收」「未完成」，不以新增工具数量替代验收。收据中的 binary SHA 是实际被测版本，早先 release 包不能证明当前源码已经通过发行验收。

| 需求 | 当前非 UI 状态 | 剩余边界 |
|---|---|---|
| R0 真实动线、性能基线 | 已有本机导入／编辑／渲染／存储故障证据 | 指定硬件的完整长素材、VFR／混合采样率、冷／热 p50/p95、真实访谈人工质量评审仍未完整冻结／放行 |
| R1 核心可靠性 | 固定修订、精确时间、原子编辑、持久任务及发布窗口恢复有真实动线；QA-05/06 的媒体识别、截断拒绝、sidecar 检查修复及产物完整性通过公共 HTTP／CLI 验收（[媒体动线](../qa/evidence/r1-media-integrity-2026-09-25.json)、[媒体维护](../qa/evidence/r1-media-maintenance-2026-09-25.json)）；所有上传探测白名单容器族在本机 release CLI 的 25 项 MP4/未知 brand/MOV/3GP/3G2、Matroska/WebM、音视频和图像变体中通过错误扩展名判型、MIME 和字节回读（[格式矩阵](../qa/evidence/r1-media-format-matrix-2026-09-25.json)）；公共 render Job 的 44.1→48 kHz 常速、4/5 变速和变速后拆分脉冲均精确落点（[音频同步收据](../qa/evidence/r1-audio-sample-sync-2026-09-25.json)）；24 次、6 路并发的完整小型合成媒体旅程通过，描述性 p50/p95 见[并发成本收据](../qa/evidence/r1-concurrent-journey-cost-2026-09-25.json) | QA-01–03 的真实浏览器状态／回包门槛按用户要求暂缓；QA-05 尚缺其他 FFmpeg 版本／平台和更广编码参数矩阵；QA-06 已有单机 963,923,861 字节上传→30 秒 1080p render→94,498,818 字节下载样本（10.87 秒上传、49.07 秒渲染、0.87 秒下载，详见[大产物成本收据](../qa/evidence/r1-large-artifact-cost-2026-09-25.json)）；并发样本只覆盖单一小素材配置和隔离工作区，不是共享数据库、多尺寸或单 API 性能门槛；跨平台临时文件语义仍待验；Composition v1 未表达保音高 |
| R2 分层与统一契约 | contracts／core／composition／application／native／SQLite 边界检查通过；旧 22 个工具契约仍兼容；重建的 schema v1–v23 历史夹具在 46 个 legacy/v2 矩阵单元中全部迁移通过 | 全工作区写入准入、多宿主、非 Unix 与新发行矩阵未整体放行；历史覆盖是重建格式，不等于旧发行二进制的升级保证 |
| R3 撤销／分支／接管 | schema2/3 的选择性撤销、重做、分支 create/edit/compare/merge、双父 DAG 及 owner 接管均有公共 CLI 收据；schema2 历史编辑在 promotion 后仍可于 schema3 中撤销／重做，并保留后续独立视觉编辑 | promotion 是格式迁移而非用户内容编辑，仍不提供“撤销 promotion”；协作 UI 暂缓 |
| R4 连续预览 | HTTP 固定修订单 FFmpeg fMP4 流、lease/generation、播放暂停 seek、独立 presentation、交接、重启终结和客户端断开取消（PREVIEW_DISCONNECTED）动线已验；当前候选 60 秒和 300 秒合成 A/V 流分别完整解码为 60.02 秒和 300.02 秒 | 这些样本验证媒体时长与进程路径，不是实时持续播放或 p50/p95 基线；超过 300 秒素材性能、完整故障矩阵、浏览器实际呈现／出声未放行；浏览器 UI 暂缓，不能宣称低延迟或已观看 |
| R5 素材分析 | 真实 Whisper 转写、三种 FFmpeg 分析、来源查询分页与取消重放动线已验；固定英语 Apollo 13 正例经 release CLI/Worker 检出 4 个说话人边界，且 EOF 之外标记被过滤（[正例收据](../qa/evidence/r5-speaker-turns-positive-boundaries-2026-09-25.json)） | 英语词级时间与固定模型 SHA 已验；统一 evidence query 已对转写、静音、镜头、波形、说话人边界做真实来源投影；此处对照的是 whisper.cpp 上游示例时间而非独立人工标注，TinyDiarize 本身为实验特性；多说话人精确率／召回率、重叠语音、多语言质量与真实访谈闭环未完成 |
| R6-A 编辑语义 | 当前编辑操作、字幕人工修正与 SRT／ASS 交换后渲染已验；schema3 cubic Bezier v1 关键帧曲线已贯通计划／应用、校验、选择性撤销、剪辑拆分、持久化、composition、固定修订帧与完整导出；debug 与打包 release CLI 得到相同真实像素及工件哈希 | Classic v31 非空工程转换、明确损失接受与独立渲染已验；schema3 的图形、调色、硬蒙版/羽化模糊、交叉淡化及曲线在真实 FFmpeg 帧／片段路径通过像素验收；ASS 子集会拒绝无法表达的样式；本轮曲线端到端样本为 opacity 标量通道 |
| R6-B 插件 | 受限 WASM planner 与 Plugin V2 声明式模板／字幕样式的安装、授权、计划、应用、真实渲染和篡改／撤销拒绝已验 | 声明式模板／样式包在 schema3 工程中完成授权、计划、应用、真实渲染、篡改和撤销拒绝验收；效果／转场仍由内核图节点表达；分析器、生成器、provider、导入导出插件等类别与完整兼容治理未完成 |
| R6-C 工程包 | schema2/3 linked 与 self-contained 固定修订包均通过导入、源工作区删除、继续编辑和真实渲染；schema3 新清单锁定 FFmpeg／ffprobe 版本与二进制 SHA，导入回执标明本机匹配或差异，差异时仍能恢复编辑；旧 schema2 包及旧持久化回执显式兼容（[工具锁真实 CLI 验收](../qa/evidence/r6-v3-tool-locks-2026-09-25.json)） | 字体／插件依赖字节和版本仍未打包锁定；完整历史可复现归档与跨平台工具指纹验收仍未完成 |
| R7 发行与平台 | 修复 R5 speaker-turn EOF 标记后的当前源码 x86_64 macOS development 包在仓库外完整通过 40 项验收，归档、manifest 和 CLI SHA 一致（[验收收据](../qa/evidence/r7-local-package-x86_64-apple-darwin-2026-09-25.json)） | 包 manifest 标记为 development；其他平台矩阵、签名/升级交付链和正式发行验收仍缺；Web/Desktop 页面开发暂缓 |

此前完整本机集成验收候选为 SQLite schema23、127 个叶工具，binary SHA-256 `7a404e8984318356e18e3cf71e5aa7d8ad86cfaad1afc0fde9af6cc7136ab59f`。同一二进制通过旧 22 个工具兼容、contracts/core/application/composition 边界检查、TypeScript SDK 13 项测试、契约生成检查和 schema3 作业／预览／工程包公共动线。源码级 `cargo test --workspace --exclude opencut-desktop --offline` 全量回归通过；受影响 media 与 project-render 的 Clippy `-D warnings`、`cargo fmt --all -- --check` 也通过。完整 workspace 命令会尝试编译 GPUI Desktop，在此机器因 Apple `metal` utility 缺失而停在 shader 构建；页面和 Desktop 当前按用户范围暂缓。该候选仍未封装正式发行包或通过跨平台与升级恢复验收。

本轮 R4 60／300 秒 HTTP 预览与断连清理收据使用当前 Rust 源码构建的 debug CLI SHA-256 `893c57d289da5afd86da7a1957d253abe0fc4075d2735b79ede08835a5ff472e`。它通过对应 R4 全动线和流媒体哈希／时长复核；这不替代上一段完整集成候选的全 workspace 回归，也不构成实时播放性能基线。

上一候选包 `opencut-0.1.0-dev-3ffa8f86d659-x86_64-apple-darwin.tar.gz` 的 archive SHA-256 为 `efcaab0a7c1aa9f26faed6508ede6fcf15e4f045e40e204bec10e00ed81312ae`，CLI SHA-256 为 `bfb1ebb7fec36b04f52565fd8b4bf559c21c5c462f5db9df7a057e6f4ab36c90`；它已被本轮当前源码包取代。

前一源码候选 `opencut-0.1.0-dev-c6e0ea2b0f5e-x86_64-apple-darwin.tar.gz`（archive SHA-256 `bb33e064bec277cb330d766a9b42000aaa74e3289c353b737b0e3b81b7fbafa4`，CLI SHA-256 `e1476a950c70fe6fc477456f5164a8f146cb84b6e1a89a77b006d668b52c5d7c`）已在仓库外解包通过 40 项验收。随后代码复审将 HTTP chunk 缓冲改为按需分配和批次复用，当前源码包 `opencut-0.1.0-dev-545a0ee9980b-x86_64-apple-darwin.tar.gz` 的 archive SHA-256 为 `3d9f8aac74a8c009c178a667f8ae452f92b270315f145af70e4bc6fb30b778ec`，CLI SHA-256 为 `dbd042e79d4c1f6ae3e2389b092671b6a3ec9f9add205dc9debc93b5fa1b8d1b`；仓库外 40 项验收全部通过，报告 SHA-256 为 `dce1eb0f86c52a71bac4cedc2e1a3d083222637e8e8f4be7b157993c86da69f4`。所有本机包的清单与验收范围标记为 `development`，不代替其他平台、签名／升级或正式发行验收。

随后 R5 复核发现 TinyDiarize 将最后一段的终端标记错误发布为媒体内边界，已修复并新增正例基准。当前源码包 `opencut-0.1.0-dev-6eecf43c516c-x86_64-apple-darwin.tar.gz` 的 archive SHA-256 为 `c73246f411f17b8619cc12502b26d2d636649a3a7edd201e62c4c3c44fb8ad78`，CLI SHA-256 为 `978d9657b32db170fb9f047e24aa7ea3cd8c5a13637f13601ed1e3809c0f3429`；仓库外 40 项验收全部通过，报告 SHA-256 为 `45f706c397074a5b14cb9cccc1b9a58d1fe452a673ee1cdc89afe6ae01ff9ce6`。全部本机验收仍限定于 development 包。

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
- [跨 schema promotion 的历史编辑撤销与重做](../qa/evidence/r3-history-after-schema3-promotion-2026-09-25.json)
- [Schema 3 固定修订任务](../qa/evidence/r6-v3-jobs-2026-09-25.json)
- [Schema 3 连续预览与帧导出一致性](../qa/evidence/r4-v3-preview-2026-09-25.json)
- [Schema 3 插件模板](../qa/evidence/r6-plugin-v3-2026-09-25.json)
- [Schema 3 工程包恢复、编辑与渲染](../qa/evidence/r6-v3-package-2026-09-25.json)
- [修复后 schema3 固定修订作业与真实渲染](../qa/evidence/r6-v3-jobs-audiofix-2026-09-25.json)
- [修复后 schema3 连续预览像素对比](../qa/evidence/r4-v3-preview-audiofix-2026-09-25.json)
- [修复后当前候选 schema2 连续预览与进程清理](../qa/evidence/r4-continuous-preview-audiofix-2026-09-25.json)
- [60 秒合成音视频连续预览时长与传输验收](../qa/evidence/r4-preview-60s-audiofix-2026-09-25.json)
- [300 秒合成音视频连续预览时长与传输验收](../qa/evidence/r4-preview-300s-2026-09-25.json)
- [修复后 schema3 工程包导入与渲染](../qa/evidence/r6-v3-package-audiofix-2026-09-25.json)
- [Schema3 Bezier 关键帧曲线端到端计划、持久化、帧与导出](../qa/evidence/r6-keyframe-curves-2026-09-25.json)
- [打包 release CLI 的 Bezier 关键帧曲线端到端验收](../qa/evidence/r6-keyframe-curves-release-x86_64-2026-09-25.json)
- [SQLite schema v1–v23 历史迁移矩阵与回滚验收](../qa/evidence/r2-history-v1-v23-2026-09-25.json)
- [R1 媒体识别、截断输入与产物完整性 HTTP 验收](../qa/evidence/r1-media-integrity-2026-09-25.json)
- [R1 媒体元数据 sidecar 并发修复与发布 journal 验收](../qa/evidence/r1-media-maintenance-2026-09-25.json)
- [R1 全部上传探测容器族 25 项媒体格式矩阵](../qa/evidence/r1-media-format-matrix-2026-09-25.json)
- [R1 大产物上传、渲染、下载单机成本样本](../qa/evidence/r1-large-artifact-cost-2026-09-25.json)
- [当前源码 macOS x86_64 本机开发包 40 项包外验收](../qa/evidence/r7-local-package-x86_64-apple-darwin-2026-09-25.json)
- [Schema 3 FFmpeg／ffprobe 精确工具锁、迁移兼容与真实渲染验收](../qa/evidence/r6-v3-tool-locks-2026-09-25.json)

执行原则：先关闭真实功能和执行路径缺口，再冻结二进制、生成契约并做整包发行验收；已验证的专项收据均记录二进制 SHA 与原始报告。页面端和 Desktop 暂缓，不降低内核、协议、执行、恢复与 SDK 的要求。
