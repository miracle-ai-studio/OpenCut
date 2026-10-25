# 开发与交付检查

本目录用于开发、构建和验收；仅 `distribution/` 中的环境诊断和配置生成器随宿主包发布。

| 范围 | 入口 |
|---|---|
| Rust 模块与依赖边界 | `check-architecture.py`、`check-contract-boundaries.py` |
| 唯一最终契约 | `check-canonical-contract.py --binary <同一已构建CLI>`，不隐式重编 |
| 门禁与打包负向测试 | `python3 -m unittest discover -s scripts/tests` |
| 验收执行器故障测试 | `python3 -m unittest discover -s scripts -p test_acceptance_runner.py` |
| 独立后台服务 | `verify-service.py --binary target/release/opencut`；CLI 启停/状态、脱离提交进程后的真实取帧、产物哈希与期望像素 |
| 本地交付包 | `package-local.py`、`verify-local-package.py` |
| 联动剪辑与托管 MCP Worker | `verify-editorial.py`；合成声画、三入口、权限、撤销重做及断线恢复 |
| 真实本地自动字幕 | `verify-auto-captions.py --model <本机模型> --web-root <构建或包内web> --artifacts-dir <隔离目录> --verify-browser`；合成语音、原生ASR、固定修订字幕、撤销重做、字幕像素及独立PCM，然后完整网页生成/取消与视口验收 |
| 统一工程包 | `verify-package-exchange.py --binary <同一CLI> --workspace <已有源工作区> --target-workspace <已有恢复工作区> --export-request <固定请求JSON> --target-project <目标ID> --import-key <原key> --report <报告>`；流式哈希和原独立图/证明检查，不生成媒体或运行渲染 |
| 各能力实际验收 | `verify-*.py`；部分脚本调用配套的 `.mjs` 客户端 |
| 项目事务／公开任务专项 | `verify-project-transactions.py --binary target/release/opencut`、`verify-public-job-views.py --binary target/release/opencut`；复用冻结候选，不触发 Cargo |

`verify-local-package.py` 在仓库外解包，验证可执行文件、SDK、真实媒体及调用结果。选定检查与完整验收分别出具结果，不互相替代。运行前通过脚本的 `--help` 查看参数和本机依赖。

`verify-control-vfs.py` 与 `verify-control-budget-policy.py` 承担数据库写入准入回归。一次性审计探针删除后由这些可复用用例承担验证。

测试素材应使用合成或获准的公开样本。临时日志/数据库/媒体放在仓库外，结束后把必要结论更新到 `design/qa/acceptance.json` 并清理现场。用户素材不进入源码或远程 CI。实施顺序和检查触发条件统一见[迁移执行计划](../design/plans/classic-migration-plan.md)。

生成契约的 Moon 入口统一为 `sdk-ts:contracts-check`，同时检查 SDK、OpenAPI、工具清单和 Web 类型桥。Web 不再注册第二个相同任务；`npm --prefix apps/web run contracts:check` 保留为手动入口。有冻结二进制时传 `-- --skip-build --binary /绝对路径/opencut`，避免隐式构建；未指定二进制的独立检查仍会构建默认 CLI，以免误验旧产物。


## Web 编辑器验收

`python3 scripts/verify-web.py --binary target/release/opencut` 在仓库外创建隔离服务和合成音视频，通过真实 Chromium 执行连接、新建、上传、编辑、连续预览、字幕、撤销/重做、刷新后导出及下载解码。首次先在 `apps/web` 执行 `npx playwright install chromium`；也可用 `OPENCUT_CHROME` 指定本机浏览器可执行文件。结果包含二进制与页面 hash、浏览器版本和实际检查。`--web-root <解包目录/web>` 复用已有构建；脚本结束停止自己启动的服务，成功后默认删除临时数据库、媒体、截图和日志。失败现场保留供定位；必要时用 `--keep-workspace` 显式保留成功现场，或 `--report <文件>` 只保存结果。

`--suite` 支持 `timeline-selection`、`media-fps`、`v4-media-fps`、`v4-file-import`、`v4-file-import-boundaries`、`project-covers`、`feedback`、`v4-editor`、`classic-audio`、`v4-performance`、`source-art`、`edit-recovery`，共用 `apps/web/scripts/verify-suite.mjs`；各功能只保留自己的用例模块。重复参数（如 `--suite media-fps --suite feedback`）共享一个隔离服务/浏览器工作区。`full` 和 `classic-recovery` 独立运行。`npm --prefix apps/web run verify:browser -- --suite media-fps` 使用同一 Python 入口，避免直接连接固定测试端口。完整浏览器组合也包含反馈专项。

`v4-file-import`只验文件放置纵切面：视频/PNG与双WAV批次、缩放滚动落点、Chromium trusted PNG粘贴、原事务/历史、整批拒绝、真实chunk取消及锁轨。视频探测需已有锁定packet运行时；源码候选在启动隔离服务前设置`OPENCUT_CLASSIC_AUDIO_PACKAGE`为`packages/classic-audio-renderer`绝对路径，并保证对应Node可用。运行`python3 scripts/verify-web.py --binary target/release/opencut --web-root apps/web/dist --suite v4-file-import`复用明确CLI与Web构建；失败使用`--reuse-workspace`原路径继续定位，不另复制素材。浏览器拖放事件不证明Finder来源，原生PNG粘贴不证明视频/音频跨应用文件剪贴板或系统IME。

`v4-file-import-boundaries`复用上述导入/Review/历史宿主，只补六项边界：48480/48000秒PCM精确源时长与24fps落点调整、批内及已有片段碰撞拒绝、第二文件上传准入失败后的已完成媒体恢复、临时规划失败原意图重试、空主轨起点调整。错误回复以版本化envelope注入，其他上传/探测/规划/SQLite是真实链路；不是所有原生存储故障证明。拖放坐标显式取整，并在zoom5用整数像素表达1.26秒请求；不把浏览器坐标截断当原生时间缺陷。使用`--suite v4-file-import-boundaries --web-root apps/web/dist`，只在代码影响旧范围时重跑旧专项。

此脚本只生成合成素材，不读取用户素材或凭证。浏览器文件保存接口在测试中接入受控字节接收器，随后独立解码；不表示已测试所有操作系统的原生保存对话框。实时播放性能、Safari/Firefox、4K 和大型工程另行验收。

`OPENCUT_BINARY=<binary> OPENCUT_WEB_ROOT=<web> node apps/web/scripts/verify-upload.mjs` 使用本机合成的 64 MiB WAV 验证单次读取、有界顺序分块、摘要冲突和独立 worker 完整性拒绝；每轮报告首块等待、传输与入库时间。可用 `OPENCUT_BASELINE_BINARY` / `OPENCUT_BASELINE_WEB` 指定旧版，连续三轮比较。性能比较应在其他构建/媒体验收结束后运行；仍是单机小样本，不代表 4K 或所有文件类型。

上传专项在确认所属服务和浏览器退出后，成功默认删除自有媒体/CAS/日志现场并向 stdout 输出结果；失败保留现场，`OPENCUT_KEEP_WORKSPACE=1` 可显式保留成功现场。源图专项仅保留去重、有界的必要回执，达到记录预算会明确失败，不能用截断证据冒充验收成功。


## 本机外部 Agent 插件

- `distribution/agent-plugin.py`：生成独立 Codex / Claude Code 插件包，复用 `agent-config.py`；不安装、不打开数据库。
- `distribution/agent-plugin/SKILL.md`：共享读取、计划/原子写入、预览、撤销与导出工作流。
- `verify-agent-plugin.py`：复用已拥有工作区、CAS和原成功渲染请求，串行从两份交付配置运行V4查询/原子编辑/历史及原Job重放；不新建素材或重复渲染。旧V3媒体动线由`verify-editorial.py`保留。
- `verify-agent-long-gain-v4.py`：原五分钟源上的元素局部/轨道场景变化音量；从既有整源PCM独立计算增益参考，只执行必要的新音频区间。浏览器采样由`apps/web/scripts/verify-agent-long-preview.mjs`复用。
- `verify-agent-v4-atomicity.py`：现有V4工程三操作的第二条失败、CLI/MCP同请求拒绝和完整工程/历史不变，带只读私有工作图正控；不新建媒体或Job。
- `verify-agent-long-mono-v4.py`：唯一300秒48k单声道PCM格式源与原VFR视频组合，原请求重放、独立PCM/单声道复制、实际30帧H264/AAC及声画事件；复用已导入媒体和成功输出。
- `verify-agent-worker-recovery-v4.py`：独占原验收队列，真实SIGKILL、子进程退出和自然租约恢复；`--retained-request`可复用原`render.v4.submit`音频/视频区间请求，服务须先停，禁止改库/时钟或超时重建任务。
- `verify-agent-plugin-clients.py`：原生 Codex 目录发现和 Claude 严格校验，无持久安装或模型请求。

用法及限制见 `design/architecture/local-agent-plugins.md`；实际验收范围统一记录在 `design/qa/acceptance.json`。

`verify-wasm-planning.py` uses an explicit integrated CLI and one existing owned
workspace/project. Requests reference already installed/granted components. It
checks the sole project planner through CLI,typed SDK over HTTP and MCP,and
applies/replays/restores one known rename. Optional scope covers the second fixed
dependency ABI,historical-payload rejection/bounded execution,changed-runtime
faults and grant revocation. Expensive proofs are retained inside coarse stages.
Delivered configurations are read only from an existing final `--agent-bundle`;
this verifier never creates packages,installs components,copies projects or runs
media. See `--help` for required request,credential and service paths. This source
is prepared for the integrated candidate and has not been executed.

统一包 UI 使用 `--suite package --reuse-workspace <上述恢复工作区>`，设置 `OPENCUT_PACKAGE_CHECKPOINT` 指向源工作区的 `package-exchange-checkpoint.json`；只有准备好的同一候选可运行。`OPENCUT_PACKAGE_HISTORY_CYCLE=1` 将导入历史的撤销/重做合进一次动线。旧包/历史输出脚本保留其原候选事实，不作为当前包协议入口；当前静态阶段没有运行这些检查。说明见[工程包契约](../design/architecture/project-package-contract.md)。
