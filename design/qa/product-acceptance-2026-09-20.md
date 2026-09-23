# 产品动线、Agent 契约与架构验收

日期：2026-09-20。结论：**整体验收不通过，不能标为全部完成或无已知 bug。**

范围基准是 `design/architecture/agent-friendly-editor-v1.md`，尤其产品能力表、Classic 迁移矩阵、完整访谈动线与第 22.2 节的十二项验收场景。本报告不把当前已实现的功能重新定义成完整产品。此次只做审计、临时复现与报告，没有修复生产代码。

## 1. 证据等级与验收方法

| 等级 | 本次执行 | 可支持的结论 |
|---|---|---|
| 公共产品链路 | 新建隔离工作区，真实 HTTP 服务、CLI、SQLite、上传、FFmpeg 导出、服务重启 | 本次明确检查的控制面、持久化、固定修订及媒体内容 |
| 真实组件执行 | 挂载实际 EditorWorkspace，Testing Library 点击实际按钮；jsdom、受控 fetch | 组件的目标选择和异步状态行为；不是浏览器网络／播放验收 |
| 代码动线走查 | UI handler → client → Automation → application → core／Worker → artifact | 明确分支与状态缺陷；标记尚未执行的竞态复现 |
| 隔离媒体实验 | 按后端实际表达式运行 FFmpeg filter，对比采样级参考 | 表达式的量化误差；不是完整公开 Job 输出验证 |
| 既有回归证据 | 上一阶段真实取消、SIGKILL／lease 恢复、NTSC、分割、字幕与其他记录 | 只覆盖原检查场景，不代表本次重新执行，也不证明完整产品 |

CUA 本次返回 `apps=[] / browsers=[]`。因此没有真实浏览器的拖拽、预览播放、音频输出、长时间使用、跨标签页或 GPUI 操作验收。仓库中未找到可用于完整访谈验收的真实视频素材；本次媒体为可独立判定预期内容的合成音视频，不冒充真实访谈剪辑验收。

当前运行 binary SHA-256：`8ff30b2a253cc22cdfd19f3937fe1457d5102a3ea07942fa747ec554a4aa5daa`。代码与 binary 指纹清单：`/tmp/opencut-product-qa-20260920/source-manifest.json`。工作区含大量未提交实现，结论绑定本次指纹与执行结果，不绑定一个已发布版本。

## 2. 实际走通的产品动线

独立执行脚本：`/tmp/opencut-product-qa-20260920/journey.py`。

证据目录：`/tmp/opencut-product-qa-20260920/journey-1789835851290517000`，含请求／响应 trace、数据库、服务日志、源媒体、preview.mp4、export.mp4 和 report.json。

1. 启动真实受认证 HTTP Host，读取当前能力与任务类型。
2. 创建工程，HTTP 上传包含前红后蓝画面及 600 Hz 音频的四秒视频。
3. 生成编辑计划，独立读回确认计划未改变 revision；提交绑定／轨道／片段事务，再用同键重试，得到原事务。
4. 分割片段；另一编辑者加入字幕；用旧 revision 提交 Agent 删除操作，收到 REVISION_CONFLICT，字幕保留。
5. 执行固定修订预检和 1–3 秒区间预览。独立解码验证时长 2 秒，前段 RGB=[253,0,0]、后段=[0,0,255]，PCM 非静音。
6. 修正第二段透明度为 0.5，然后继续编辑 head 删除第二段。请求历史修订导出，产物仍为四秒，第二段蓝色像素=[1,0,130]，保留音轨；HTTP 下载哈希匹配。
7. 真正停止并重启服务，HTTP 及独立 CLI 重新读取，最新 head 与一个片段的状态保持。
8. 恢复历史完整工程，得到新的 revision，并恢复两个可继续编辑的片段。

这证明基础编辑／预览／导出／保存链路可工作。它没有覆盖自然语言理解、转写、自动选段、完整 UI、人机选择性撤销、工程包或真实访谈成片质量。

## 3. 已确认缺陷与代码动线风险

P1：发布前必须解决的核心流程／契约问题。P2：明确条件下的正确性或可靠性问题。未发现 P0 不等于证明不存在 P0。

### QA-01 / P1：连接新服务失败后，编辑仍发送到旧服务

- 证据：**真实组件执行，主 Agent 独立复跑通过**；网络响应由 fetch mock 控制。
- 动线：连 A → 把地址改成不可达 B → Connect 报错 → 点击实际 Track 按钮。
- 实际捕获请求仍为 `http://127.0.0.1:8788/v1/tools/opencut_edit_apply`，project_id=A、expected_revision=rA、operation=track.create。
- `connect` 未清理旧 client/snapshot，catch 仅设置状态；下一 render 重新建立旧客户端的写入上下文。见 `apps/web/src/editor/editor-workspace.tsx:290`、`:174`。
- 同类静态确认：成功连接空 B 时 setClient(B)，但无 list[0] 因而保留 A 快照，形成 B client + A snapshot；编辑会向空 B 请求不存在的 A。不能把这个空 B 场景夸大为已经修改 B 的同 ID 工程。
- 复现：`apps/web/node_modules/.bin/vitest run --config /tmp/opencut-vitest.config.mjs`。临时 harness 和配置也备份于审计目录。不是完整浏览器验收。

### QA-02 / P1：历史／新建工程异步回包缺少上下文隔离

- 证据：实际 handler 调用链与 setter 走查；未执行真实浏览器竞态。
- A 加载历史修订期间切换 B，旧响应无条件 setHistorical，`historical ?? snap` 可以把 A 历史覆盖到 B 的界面。`viewRevision` 虽递增 generation，却未捕获并在 await 后校验。见 `editor-workspace.tsx:1117`。
- `createProject` await 后也无连接与请求代次检查；新建请求和项目选择交错时，旧创建结果可覆盖新选择。见 `editor-workspace.tsx:362`。
- 历史 Refresh 直接 then(setHistory)，存在同类过期回读问题。

### QA-03 / P1：Undo 后的新编辑没有失效 Redo，恢复失败也会更新 Redo

- 证据：组件真实 onClick 调用链与 setRedoRevision 全部写入点走查；未做 DOM／浏览器执行复现。
- Undo 通过整工程 restore 生成新 revision 并记录旧 revision；随后正常编辑只更新 snapshot/history，没有清除 redo。Redo 按钮仍可用，执行的是恢复旧完整工程，会覆盖分叉后编辑。
- `act` 捕获错误但不向调用者报告失败，`restoreRevision` 仍无条件退出历史视图并设置 redo。revision 冲突时 UI 会呈现没有真正发生的恢复意图。
- 代码：`editor-workspace.tsx:234`、`:1129`、`:1798`。
- 当前整工程 restore 本身语义清楚，问题在 UI 的 Undo/Redo 状态机；它也不能替代设计要求的选择性撤销。

### QA-04 / P1：等值有理时间不满足设计中的规范化幂等契约

- 证据：**真实公共 CLI 复现，主 Agent 独立回读／重试确认**。
- 首次同键提交字幕，start/duration 为 1/2；原请求重试 exit=0，返回原事务。只将二者重写成 2/4，同键重试 exit=5，返回 IDEMPOTENCY_KEY_REUSED。
- `request_hash` 对原始 serde JSON 哈希，未规范化时间。见 `crates/editor-application/src/lib.rs:1577`。
- 与设计时间契约及第 8.2 节规范化摘要不一致；跨 SDK 的等值时间重编码无法安全重试。
- 证据：`/tmp/opencut-product-qa-20260920/idempotency-report.json`。原请求目录由 `/tmp/opencut-idem-qa-path` 指向。
- 修复必须兼顾已有幂等记录迁移，不能直接改变 hash 算法后使旧成功请求失去可重试性。

### QA-05 / P2：上传依赖扩展名判型，重复上传可改坏共享媒体元数据

- 证据：**真实 HTTP 上传与 GET 回读**。
- 相同 MP4 字节以 source.mp4 上传得到 kind=video；以 same-video.png 上传得到 kind=image、mime=image/png；asset ID 相同。
- 重复上传写同一个 `<hash>.json`。原媒体 URL 再 GET，字节仍是 MP4，但 Content-Type 已为 image/png。
- 代码：`apps/cli/src/media_http.rs:96`、`:170`、`:180`。输入检查、内容识别和共享资源元数据身份需要统一。
- 证据：本次 journey 的 report.json、integrity-report.json。实际浏览器的坏图表现未验证；类型与 MIME 错误已确认。

### QA-06 / P2：已发布产物被本地损坏后，下载仍返回成功

- 证据：**真实 HTTP 复现，使用本次隔离 QA 工作区，实验后已恢复原文件**。
- 成功导出的 recorded size=70086。将该 artifact 改成 38 字节的损坏内容后，GET artifact 返回 200、video/mp4 和 38 字节，任务仍 succeeded、记录 hash 不变。
- 下载端点验证成功状态及路径归属，但不比对已记录 size/hash。见 `apps/cli/src/media_http.rs:239`。
- 这是本地损坏／误写下的完整性问题，不是已发现的越权或路径逃逸。
- 证据：journey 目录下 integrity-report.json；复现脚本 `/tmp/opencut-product-qa-20260920/integrity.py`。

### QA-07 / P2：音频片段起点被量化到整数毫秒

- 证据：源码确认 + 同构 FFmpeg filter 隔离实验，**不是公开 render.project 的 PCM 实测**。
- `backend.rs:1540` 将 timeline_start 秒乘 1000 后 round，再传给 adelay。
- 1/3000 秒在 48 kHz 应延迟 16 samples，当前表达式为 0ms；隔离 filter 实验提前 16 samples（0.333ms）。这是采样精度问题，不宣称肉眼可见的音画失步。
- 证据：`/tmp/opencut-audio-delay-repro.vna7QB`。完整产品输出受 AAC 延迟／前振铃影响，尚未完成对应隔离测量。

### QA-08 / P2：部分废弃 attempt 的不可达产物不清理

- 证据：源码与已有清理测试的明确保留断言；本轮未做临界点 SIGKILL。
- `clean_abandoned_inputs` 只为 composited_frame/project_range 删除整个旧 attempt；其他类型只清 inputs。编码成功后、数据库发布前崩溃或失 lease，旧输出可长期不可达。
- 代码：`crates/job-runtime/src/lib.rs:254`、`:1055`。应按任务发布状态区分公共产物与废弃输出，而非只为两种类型做完整清理。

### 其他审查项：尚需专门复现，不计入公共链路实测缺陷

- HTTP 未穷尽错误类别，未匹配 code 全部映射 500；与 CLI 的分类表分散，可能诱导错误重试。`apps/cli/src/http.rs:392`、`crates/automation/src/lib.rs:405`。
- `analysis.read` 是可授权插件权限，但没有相应 analysis Host ABI；能力和权限描述应区分已实现与预留。
- 插件重复 operation 名称未被 manifest 校验拒绝，invoke 使用第一个匹配项；没有明确可选择的版本身份。
- 插件输出无法反序列化为 Operation 时被归类 INVALID_ARGUMENT，错误归因容易误导 Agent 修改用户输入。以上见 `crates/plugin-host/src/lib.rs`。
- Web 只订阅 Job 事件，没有工程提交事件的同步消费；Agent 在外部编辑时，UI 工程内容不能及时同步，通常要下一次本地操作冲突后刷新。见 `editor-workspace.tsx:641`。

## 4. 产品能力与需求动线矩阵

| 需求 | 当前状态 | 验收判断 |
|---|---|---|
| 创建、保存、重开本地工程 | 已实现，真实 HTTP/CLI 重启回读 | 本次基础路径通过 |
| 素材导入与版本化 | 已实现 CAS、固定 revision | 部分通过，QA-05 阻止完整导入验收 |
| 剪辑、分割、裁剪、移动、常速映射、七项 clip 参数 Hold/Linear | 已有实现与真实媒体样例 | 支持范围内部分通过，非完整 Classic 编辑器 |
| 帧／音视频短区间预览 | 已实现八类任务中的相关路径 | 部分通过；不能替代连续工程播放 |
| 连续播放、低延迟 seek、预览缓存 | Session 仍为设计文档 | 未完成 |
| 完整文字、效果、遮罩、转场、复杂曲线与变速 | 当前模型与渲染未覆盖完整范围 | 未完成 |
| 转写、词级字幕、说话人、镜头、静音、内容检索 | 当前任务注册表无对应任务 | 未完成，完整访谈动线在理解阶段中断 |
| API/MCP/CLI 的事务、固定修订与任务 | 已实现，能力可发现 | 基础良好；规范化幂等、完整查询与 SDK 仍缺 |
| Agent 与人工协作、操作可见、选择性撤销 | 乐观冲突可用；UI 无工程同步、无选择性撤销 | 未完成，另有 QA-01～04 |
| 插件 | 原生 WASM planner、空 WASI、配额与计划校验 | 部分实现；效果／分析／Provider／UI 扩展未完成 |
| 工程包、离线携带媒体重开、来源与依赖锁 | 原生库可重开，媒体内容锁已有 | 完整工程交换包、字体／插件／工具锁及 GC 未完成 |
| GPUI 桌面 | 只有 UI 壳，preview 显示占位文字 | 未完成 |
| 浏览器独立运行、移动端、脚本面板 | 未有对应完整接入 | 未完成 |
| 独立 SDK、OpenAPI、历史协议兼容门禁 | 当前生成 TS DTO 与本地 client | 未完成 |
| 性能与真实使用质量 | 无完整硬件／长素材 benchmark 验收 | 未验证 |

设计第 21 节“访谈选三个重点、做一分钟竖屏、中文字幕、背景音乐、修正、导出、打包”不能完成：analysis.transcribe、analysis.shots、media.search、project.package 等能力缺失。手工给定全部剪辑点与字幕后成功导出，不能作为这个需求的替代验收。

## 5. 第 22.2 节十二项完成门槛逐项核对

| # | 要求 | 当前证据与结论 |
|---|---|---|
| 1 | 同事务两次只出现一次修改 | 本次精确原请求通过；等值时间重编码失败，QA-04 |
| 2 | 第 N 条失败无部分提交 | 原阶段原子事务证据与当前代码事务路径；本轮未重新执行失败批次 |
| 3 | 人改 head 后 Agent 旧计划冲突 | 本次真实 HTTP 通过，字幕保留 |
| 4 | Agent 撤销自己修改不覆盖人的后续字幕 | 无选择性撤销；整工程 restore 不能满足，未完成 |
| 5 | 导出旧 revision 与活动 head 隔离 | 本次历史导出通过，独立像素／时长确认 |
| 6 | Worker 被杀恢复且旧 attempt 不覆盖 | 上一阶段真实 SIGKILL、自然租约恢复通过；仍有 QA-08 清理缺口 |
| 7 | UI/MCP 关闭后任务可 CLI 查询 | 持久化机制及此前公共验证；本次服务重启回读通过，不承诺所有进程关闭时仍计算 |
| 8 | 插件死循环／超额分配失败且工程可继续读写 | 有 fuel/memory/deadline 代码；本轮没有故障插件动线验收，不判完全通过 |
| 9 | 缺效果插件保留工程数据，正式导出明确失败 | 当前尚无完整效果节点／依赖模型，未完成 |
| 10 | VFR／混合采样率／分割变速同步 | 部分真实回归已有；完整混合矩阵未完成，存在 QA-07 精度缺口 |
| 11 | CLI/MCP/UI 同计划生成相同语义工程 | CLI/HTTP/MCP 基础契约与既有验证；真实 UI 全链等价未验收 |
| 12 | 真实访谈分析到成片再重开编辑 | 核心分析和工程包缺失，未完成；本次合成片不能替代 |

## 6. 分层与封装评价

**方向成立，封装尚未达到完整设计目标。**

保留：contracts/time/editor-core 的纯计算边界；显式 Operation；原子修订与固定快照；版本化素材与私有输入；lease/attempt fencing；MCP/CLI/HTTP 共享用例；插件返回计划经宿主验证。这些确实有利于 Agent 稳定调用。

需要继续治理：

1. Automation 同时负责协议注册／JSON 分派和部分业务编排，EditorApplication 同时负责应用服务与 SQLite 实现。共同用例存在，但存储端口与领域／适配边界没有完全分开。
2. job-contracts 依赖 native media/asset-store，公共 DTO 含 PathBuf；prepared 执行输入与外部协议尚未彻底隔离，不利于独立 SDK 和后端替换。
3. project-render 已统一部分视觉／字幕编译，但仍输出 media Timeline DTO，FFmpeg-specific 求值与执行边界尚未转为真正后端无关的合成描述。
4. Web 大组件内分别管理连接、工程、历史、预览、任务与 busy generation，缺少统一会话状态机；QA-01～03 是实际后果，不能以文件名分层替代状态所有权设计。
5. 错误分类、资源身份、协议兼容与能力范围仍需要单一注册来源；插件 planner 的沙箱不等于独立插件进程或完整插件生态。
6. 桌面未调用共享应用层；浏览器独立存储／WASM 尚未接入。因此跨平台统一架构还未被全部实际宿主验证。

## 7. 放行决定与下一步

- **完整产品交付：不放行。** 明确缺失多个承诺模块，完整需求动线不能完成。
- **无 bug 声明：不成立。** 有公共入口和组件执行复现的已知问题；也未做完整平台、真实浏览器、长视频与性能验收。
- **Agent 友好：基础控制面部分达标。** 事务、版本、任务与内容锁值得保留，但能力理解、查询、证据闭环、跨语言重试、人工协作和工程交付尚不完整。
- **架构：可继续演进，不能称全部合理且已完成。** 应在现有模块基础上收拢用例与状态权威，保持媒体／协议／UI 各自边界。

修复优先级：先阻止错宿主编辑与历史／Redo 状态错配；再处理幂等规范化的兼容迁移、上传内容判型和产物完整性；补真实浏览器人机协作验收；随后按原完整范围推进连续播放、分析／检索、完整编辑、插件类别和工程包。不得以只修本报告缺陷替代剩余产品功能。
