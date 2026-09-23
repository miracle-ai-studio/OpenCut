# R2 原生能力端口与真实动线验收

2026-09-21。本轮完成预检、媒体目录维护与 planner 插件用例的应用层归位。架构与契约见 [原生能力端口](../architecture/native-capability-ports.md)。R2 整体和 R3–R7 尚未完成，Web/GPUI 实现与真实 UI 验收继续暂缓。

## 变更与走查发现

- Application 持有指定修订读取、媒体内容分类、插件候选操作验证及计划生成。Native Services 装配 FFmpeg、CAS 与 Wasmtime；Automation 不再直接依赖媒体实现、AssetStore、ProjectRender 或 PluginHost。
- 走查发现媒体 inspect 和插件 list/inspect/plan 的构造器会创建目录，已分出只读打开方式；通过公共 CLI 查询后独立比较目录和完整数据库内容。
- 插件 JSON/Operation 结构合法但引用了不存在对象时，原来表现为普通编辑失败。现在 Core 验证失败返回 INVALID_PLUGIN，结构化 details 保留插件身份与原始 cause。真实 WASM 故障模式验证该错误及无事务、修订、回执、事件副作用。
- 路径转换拒绝非 UTF-8，避免静默替换字符。插件安装在发布前检查返回路径可表示性；Unix 原生非法字节路径测试验证失败时没有创建目录。
- 正常插件计划仍需调用方显式 apply；head 更新后再次请求旧 revision 得到原计划，直接提交过期计划继续返回 REVISION_CONFLICT。

## 最终构建与证据

平台 macOS 26.5.1 x86_64；Rust/Cargo 1.97.1，FFmpeg/ffprobe 8.1.2，Node v24.19.0。

CLI SHA-256：`87fb60463212fb23a576a09fa7d1f2a1db0920e35237c36c2a91e4773b14a7de`。

源码清单摘要：`9da429f8da446bb0040f72cecca02f55983e1595741ca357467f79426e07c12f`。清单覆盖 Cargo 文件及 crates、apps/cli、scripts 下 110 个非忽略文件；逐文件 SHA-256 与工具版本存于 `/tmp/opencut-services-final-fingerprint.json`，不包含文档或前端。

| 检查 | 实际结果 | 证据 |
|---|---|---|
| 只读 CLI 能力 | 7 项通过：空插件目录、缺失插件查询/计划、缺失元数据、非法身份、固定修订预检、数据库不变、没有媒体/插件/CAS 目录 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-read-only-7zr38n3d/report.json` 与 `trace.json` |
| 真实 WASM 插件 | 14 项通过；包括 JSON/结构/语义三个错误模式、数据库五张业务表独立比较、授权/Schema、实际执行、计划/提交/重试/冲突与历史修订 | `/tmp/opencut-services-final-plugin.json`，包含请求响应 trace、组件和 CLI 摘要 |
| HTTP/CLI 媒体产品动线 | 12 项通过：真实上传、并发内容修复、固定修订预检/预览/导出、服务重启、冲突与人工字幕保留、损坏下载拒绝、RGB/PCM 解码及 48 kHz 下 16 样本偏移 | `/private/tmp/opencut-services-final-journey/journey-1789932410794539000/report.json` |
| HTTP 与 MCP | 各 18 项通过；真实传输、工具发现、共享编辑、历史、Job、事件/SSE、游标和独立 CLI 回读 | `/tmp/opencut-services-final-http.json`、`/tmp/opencut-services-final-mcp.json` |
| 受影响 Rust 检查 | 27 项通过：Application 3、AssetStore 7、Automation 1、CLI 5、PluginHost 9、Native Services 2 | `/tmp/opencut-services-tests.log`；最终 Native Services 两项见 `/tmp/opencut-services-final-tests.log` |
| 构建及 Clippy | CLI 构建成功；六个受影响包 all-targets `-D warnings` 通过；最后新增路径检查后再次检查 Native Services 通过 | `/tmp/opencut-services-clippy.log`、`/tmp/opencut-services-final-clippy.log`、`/tmp/opencut-services-final-build.log` |
| 公共契约 | 生成无漂移；旧 22 工具 Schema/read_only 不变，当前 27 工具 | `/tmp/opencut-services-generate.log`、`/tmp/opencut-services-wire.json` |
| 边界门禁 | Application 102 节点闭包仍无存储/执行实现；新增 Native Services 与 Automation 直接依赖限制通过 | `/tmp/opencut-services-boundaries.json` |

最后的路径检查改变了 CLI 构建，因此最终构建重新执行了上表五条公共动线；只读、插件、媒体报告中的 binary hash 均与最终文件匹配。早期同轮 `c814...` 构建结果仅作调试记录，不替代最终证据。

媒体报告状态保留 `passed_with_remaining_scope`：合成音视频适合精确像素、时间、身份和并发判断；不代表真实访谈理解、中文转写质量或浏览器观看已经验收。当前仍缺 analysis.shots、analysis.transcribe 和 project.package 等后续任务。

## 可重复执行与未证明范围

先构建 `cargo build -p opencut-cli`，以及 planner-guest 的 `wasm32-wasip2 --release`；然后运行 `verify-read-only-capabilities.py`、`verify-plugin.py`、`verify-product-journey.py --artifacts-dir <目录>`、`verify-http.py` 和 `verify-mcp.py`。这些脚本均位于 scripts。只读验收从已初始化 workspace 开始；CLI 首次打开仍允许初始化 SQLite。

本轮没有重跑与本次能力迁移无关的所有 Worker SIGKILL 场景、SDK 包安装或所有渲染参数组合，其先前证据保留在对应阶段报告。未执行远程 CI、跨 OS 或浏览器适配构建；目录/端口分层完成也不等于所有功能无 bug。

后续顺序：后端无关的合成求值描述及 full/direct/range 统一 → 完整历史版本兼容矩阵与可安装交付 → R3 协作/选择性撤销、R4 服务端连续预览、R5 分析、R6 完整编辑与工程包、R7 发布验收。
