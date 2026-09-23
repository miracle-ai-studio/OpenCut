# 领域工具与恢复源码验收（2026-09-23）

范围是当前未提交工作树构建的本地二进制和恢复后的契约，不是全产品发行验收。被测二进制：`/private/tmp/opencut-recovery-target/debug/opencut`，SHA-256 `4990ed34888ceefa86aa52f2c6f2fd71523cc5f2f428374660b0aa272f48d9a8`。各验收脚本会创建独立临时工作区；下列报告路径仅供本机追查。

| 检查 | 当前结果 | 实际边界 |
|---|---:|---|
| `python3 scripts/verify-domain-surface.py --binary <opencut>` | 通过；默认 9 个 MCP 领域工具，61 个动作 | CLI／stdio MCP／HTTP 领域发现、精确 schema、计划与提交、旧操作兼容、受限身份拒绝 owner 操作 |
| `node scripts/verify-sdk.mjs --binary <opencut>` | 通过；13 项 | SDK tarball 在仓库外安装，TypeScript 正误类型检查，实际 HTTP／CLI／Worker、断连后显式同键重放、领域调用与公开 Job 等待；生成 OpenAPI 的 72 条叶操作／领域路径及 1355 个本地引用与当前二进制核对。报告：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-sdk-CXzslc/report.json` |
| `python3 scripts/verify-rust-sdk.py --binary <opencut>` | 通过；12 项 | Rust SDK 仓库外源码消费、依赖闭包、真实 HTTP／CLI／Worker、传输丢失后精确重放及独立 TS SDK 跨语言幂等。报告：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-rust-sdk-__q03khi/report.json` |
| `node scripts/verify-mcp-client.mjs --binary <opencut> --sdk-root <外部 npm prefix>` | 通过；官方 SDK 1.26.0，5 项 | 会话协商、领域动作、丢失响应后 SSE 回放、资源通知和 DELETE。报告：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-mcp-interop-oGA74i/report.json` |
| `python3 scripts/verify-history-compatibility.py --binary <opencut>` | 通过；22 项、30 格 | 冻结合成 v1–v15 数据库 × legacy/v2 摘要；真实进程并发升级、旧回执／结果读取、后续编辑和失败回滚 |
| `python3 scripts/verify-product-journey.py --binary <opencut> --artifacts-dir <临时目录>` | 部分通过；10 项真实合成音视频动线 | HTTP 导入／编辑／冲突、CLI 等价重放、分段预览音视频独立解码、固定旧修订导出、重启重开、整工程恢复、重命名上传和损坏产物拒绝；报告：`/private/tmp/opencut-domain-product-journey-20260923/journey-1790174225505345000` |
| `npm --prefix packages/sdk-ts run contracts:check -- --binary <opencut> --skip-build`、`cargo fmt --all --check` | 通过 | 生成契约无漂移；Rust 格式一致 |
| `npm --prefix packages/sdk-ts test`、`python3 scripts/check-wire-compatibility.py --binary <opencut>`、`python3 scripts/check-contract-boundaries.py` | 通过；SDK 10 项、冻结旧工具 22 个 | SDK 传输与公开任务行为；旧 v1 schema 不变，公开契约与 Application／原生依赖方向符合门禁 |
| `package-local.py --binary <opencut> --output <临时目录>` 后 `verify-local-package.py <archive> --mcp-sdk-root <外部 npm prefix>` | 通过；仓库外 15 项 | 解包文件哈希与篡改检测、实际 schema／61 操作／9 领域／72 路径、双 SDK、官方 MCP、30 格历史、项目导入四入口与 Worker 强杀自然租约恢复、真实媒体与重启回读。包：`/private/tmp/opencut-packages-current/opencut-0.1.0-dev-266a75ab2813-x86_64-apple-darwin.tar.gz`；SHA-256 `bcc89928f01718763ecbd3aea3e71347c25cc90727d1d2f1808df3d9cb0bdf28`；报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-kivbf0s1/report.json` |
| `python3 scripts/verify-worker-admission.py --binary <解包的 opencut> --artifacts-dir <临时目录>` | 通过；4 项 | 真实 FFmpeg 输出碰顶、字幕与多种预览/导出、父 Worker SIGKILL 后孤儿子进程锁与 attempt 2、目录别名拒绝；报告：`/private/tmp/opencut-package-worker-admission-20260923/report.json`。本地缺旧 schema-14 归档，因此脚本跳过旧二进制拒绝新库那一项，不能把它计入本轮通过。 |

领域调用只压缩发现表面：`{action,input}` 会在共享 Automation 中解析回已注册叶操作，再由原有权限、幂等、审计与应用用例处理。旧 61 个操作仍可直接调用；旧 v1 冻结工具 schema 和独立 SDK 调用继续验收。精确动作输入／输出 schema 在 `opencut://schemas/domains/{domain}` 和 CLI／HTTP 领域发现中提供。默认工具列表不展开全部动作输入 schema，Agent 应先发现领域，再读取目标领域契约，最后调用动作。

当前不能据此宣布 R0–R7 完成。新验收包采用当前 debug 二进制，只证明 macOS x86_64 本地开发交付动线，未完成 release 优化版或跨平台发行。恢复回放仍有 20 次补丁与 61 次 Python 写入未逐项核销；历史夹具是从旧格式定义重建的合成数据，尚未覆盖真实旧用户库。R0 的真实素材／浏览器基线仍缺；R1 的真实 UI 联动验收未完成；R2 仍有全工作区写入准入与保留治理，`enforces_all_workspace_writes=false`；R3 选择性撤销／分支协作、R4 连续预览、R5 素材理解、R6 完整编辑／插件／工程包、R7 各宿主与发行整体验收仍待实现或验证。旧的 2026-09-21 开发包证据不自动继承到当前未提交源码。
