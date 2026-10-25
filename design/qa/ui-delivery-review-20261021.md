# UI 交付审查 · 2026-10-21

**结论：当前 UI 尚未达到本次要求的交付标准。** 四种语言/主题组合的主要入口可以打开，但浅色主题存在近乎不可读的输入值，中文翻译没有覆盖完整流程，导出、历史记录和属性编辑仍直接暴露内部数据模型。工程包失败恢复、历史整版恢复和模拟反馈也需要在交付前处理。这些是可用性与流程问题，不只是视觉润色。

本报告只审查，不修改产品代码。依据为当前源码与现有服务的实际浏览器观察；没有把源码接线、页面能打开或历史验收当成本次功能验收。没有重新验证连续播放，也不沿用早期审查中已被后续播放修复取代的失败结论。

## 范围与证据边界

- 已读 `AGENTS.md`、`design/architecture/architecture-renewal-plan.md`、`design/architecture/headless-completion-ledger.md`，参考既有 `editor-interaction-design-review-20261021.md` 与 `buffered-scene-playback-20261021.md` 确认当前边界。
- 使用 `emil-design-eng` 的组件、可读性、反馈与交互审查方法。没有套用营销落地页的视觉规则。
- 使用仓库已有 Playwright 和已有 Chromium，在**独立无头 QA 浏览器上下文**走查 `http://127.0.0.1:59790`。CUA 当前没有可用浏览器，未操作用户真实浏览器或桌面。
- 主检查项目为既有合成项目 `scene-preview-fc284e7a-6b31-436f-a444-2befa7059942`，版本 `rev_82c85f787aa592a336bebf90995826b4f14684503ca2c16a6ad9096f93cd277d`。
- 检查打开页面、面板和弹窗，切换仅在浏览器本地保存的语言/主题，搜索不存在的名称，查看历史版本后返回当前版本，展开撤销预览但不应用；未点击导出提交、恢复版本、删除、保存工程、新建工程、导入素材或提交反馈。
- **发现浏览项目列表会自动提交封面渲染任务。** 本次因此新增 8 个静帧任务和产物，已停止继续走这条路径；没有删除或回滚。工程内容未变与后台任务新增是两件不同的事实，完整记录见“浏览副作用”。

运行身份：Chromium `155.0.8059.39`；服务二进制 `/tmp/opencut-buffered-preview-20261021/runtime-final/opencut`，SHA-256 `d23a944a7a33b7ae6dce076083446e94e8e02f03e5a17c9e8dd28900f8bb9c20`；服务载入 `apps/web/dist`。本次源码/静态资源清单见 [source-identity.json](/tmp/opencut-ui-delivery-review-20261021/source-identity.json)，不能用正在并行修改的工作区代替这个运行候选身份。

证据目录：[ui-delivery-review-20261021](/tmp/opencut-ui-delivery-review-20261021)。[主走查](/tmp/opencut-ui-delivery-review-20261021/review.json)与[补充走查](/tmp/opencut-ui-delivery-review-20261021/supplement.json)共记录 132 次成功状态捕获，包含重复入口；不是 132 个独立功能通过。主走查 4 个选择器不匹配项在每个组合中均已补查；这是测试脚本定位问题，不计作产品故障。补查等待弹窗进入动画结束，视觉结论优先使用 `supplement-*` 截图。四组合均没有捕获到浏览器 `pageerror`。

## 覆盖矩阵

“实看”表示打开、读取和观察界面，不表示提交后的成功/失败流程通过。主视口为 1440 × 1000；关键帧、视觉效果编辑草稿、导出另在中文浅色/英文深色的 1280 × 720 下观察。

| 页面或状态 | 中文浅色 | 中文深色 | 英文浅色 | 英文深色 | 本次边界 |
|---|---|---|---|---|---|
| 编辑工作区、片段属性、素材信息 | 实看 | 实看 | 实看 | 实看 | 不修改字段或时间线 |
| 素材音频、文字、字幕、形状、效果、转场面板及空态 | 实看 | 实看 | 实看 | 实看 | 不导入、不应用、不生成 |
| 视频导出、格式选择 | 实看 | 实看 | 实看 | 实看 | 未提交；完成、下载、取消链只查源码 |
| 音频导出、指定采样范围 | 实看 | 实看 | 实看 | 实看 | 未提交或下载 |
| 工程包导出、恢复入口 | 实看 | 实看 | 实看 | 实看 | 不检查外部路径、不写包、不恢复 |
| 画布/帧率设置 | 实看 | 实看 | 实看 | 实看 | 不保存 |
| 历史列表、旧版查看、返回当前版、撤销预览 | 实看 | 实看 | 实看 | 实看 | 未应用撤销或整版恢复 |
| 素材预览 | 实看 | 实看 | 实看 | 实看 | 不作为播放/音画同步验收 |
| 关键帧入口 | 实看 | 实看 | 实看 | 实看 | 中文浅色/英文深色另等待异步控件完成并检查详情；不写关键帧 |
| 任务面板现有状态 | 实看 | 实看 | 实看 | 实看 | 未专门制造失败、取消或新导出任务 |
| 偏好设置、快捷键、本地会话、反馈弹窗 | 实看 | 实看 | 实看 | 实看 | 未发送反馈或更改连接 |
| 项目列表、排序控件、无搜索结果、新建弹窗 | 实看 | 实看 | 实看 | 实看 | 未保存新建；项目列表引发自动封面任务 |
| 视觉效果编辑草稿（1280 × 720） | 实看 | — | — | 实看 | 打开后取消，无提交 |
| 成功导出/包恢复、失败重试、删除确认、并发冲突、断线恢复 | 源码 | 源码 | 源码 | 源码 | 未做状态变更，不宣称行为验收 |
| 完整键盘导航、读屏、Safari/Firefox、移动端、真实听音 | 未验收 | 未验收 | 未验收 | 未验收 | 基础关闭/焦点观察不能替代完整可访问性验收 |

## 优先发现

P1：阻止按当前用户要求交付，应先修复。P2：影响一致性、效率或资源使用，交付前应有明确处理决定。本次未发现可证实的永久数据丢失，因此没有标 P0。

| ID / 优先级 | Before：当前现象 | After：具体建议 | Why：用户影响与证据 |
|---|---|---|---|
| UI-01 / P1 | 浅色属性输入使用深色固定背景，文字仍随浅色主题变深；实际测得文字 `rgb(41,44,51)`、背景 `rgb(41,41,45)`，对比约 **1.04∶1**。 | 输入、选择、边框、禁用态统一使用主题语义变量；逐控件检查文字/背景/选中/焦点，不仅翻转页面背景。 | 数值基本看不清。位置、缩放、旋转等核心编辑受影响。[截图](/tmp/opencut-ui-delivery-review-20261021/en-light-resources-captions.png)；`workbench/workbench.css:910–917` 写死 `#29292d`。 |
| UI-02 / P1 | 中文画布弹窗、工程包选项、字幕指导、关键帧说明混入大量英文；历史动作名也直接显示英文。 | 完成以用户流程为单位的翻译；建立页面文案枚举与缺失检查；业务事件名在展示层本地化，保留用户输入的素材/项目名称原文。 | 语言切换只完成一部分，用户无法稳定理解完整操作。[画布](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-canvas-settings.png)、[工程包](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-project-package.png)、[关键帧](/tmp/opencut-ui-delivery-review-20261021/detail-zh-light-keyframes-1280.png)。 |
| UI-03 / P1 | 速度/帧率要求分子和分母，关键帧出现 `Scalar property`、`Color component (linear RGB)`、`position_x_px`、`font_size_classic_height90` 等内部名称；音量含 `Linear amplitude`。 | 主界面使用“速度 1×”“帧率 25 fps”“位置/缩放/旋转/不透明度”等；精确有理数、单位转换仍在模型内保留。少数专业选项放入有解释的高级设置。 | 用户被迫理解存储和实现结构才能剪辑；英文模式同样不友好。`property-edit/editor.tsx`、`property-edit/settings.tsx:238–252`、`keyframes/editor.tsx:575–723`；[英文关键帧](/tmp/opencut-ui-delivery-review-20261021/detail-en-dark-keyframes-1280.png)。 |
| UI-04 / P1 | 视频导出首先展示完整版本哈希且溢出弹窗；只泛称使用画布和帧率，没有直接列出本次实际尺寸/帧率/时长。音频范围是从 0 开始的样本编号和帧数。导出提交后需要另去任务页找结果。 | 导出显示文件名、时长、尺寸、帧率、格式、预计用途；范围用时间或“整个视频/选中范围”。提交后的同一位置显示进度与取消，完成后直接提供下载/打开位置；版本和技术信息折叠。 | 普通用户难以确认“我会得到什么、保存到哪里、完成后去哪找”。[视频](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-light-video-export.png)、[音频范围](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-audio-export-range.png)；`video-export-dialog.tsx:23–44`、`audio-wav-dialog.tsx:65–105`。提交后问题是源码确认，未发起导出。 |
| UI-05 / P1 | 历史记录展示 `Place media`、`Import media`、`local-owner · http`；撤销预览主要说明“轨道”等大类，缺少素材名和可理解的前后变化。 | 使用“将 sample.mp4 加入视频轨道”“调整片段音量”等事件摘要；区分“你 / Agent 名称”；默认隐藏协议、内部身份和版本哈希。预览写清受影响对象及结果。 | 记录不能可靠帮助用户回答“谁改了什么、我该撤销哪一步”。[历史](/tmp/opencut-ui-delivery-review-20261021/zh-dark-history.png)、[撤销预览](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-undo-plan.png)；`history.tsx:85–120`。 |
| UI-06 / P1 | “恢复此修订”按钮直接调用整版恢复；普通撤销却要求先生成计划再应用，两种操作的确认强度与影响范围不一致。 | 普通可逆撤销直接执行并给出反馈；整版恢复先展示目标日期、作者、主要变化及“当前内容会恢复为此版本，可再撤销”，确认后执行。保留原有原子提交、冲突和幂等语义。 | 误点整版恢复比撤销一步影响更大，但界面没有清楚解释。`history.tsx:131–133` → `session.ts:828–860`；**仅查源码，未点击恢复，未证明数据丢失**。 |
| UI-07 / P1 | 工程包要求手填运行 OpenCut 的电脑上的绝对路径，且文件夹不能存在。首次有效路径请求后锁住全部输入；失败也只剩“检查目标”，无法在原弹窗更正目的地或再次导出。 | 提供可理解的选择位置/文件名流程，区分浏览器下载与服务端目录；未确认提交继续查原请求，明确未写入的失败允许修改位置并重试，避免重复写入。恢复入口也应支持选择工程包。 | 路径错误与权限错误容易使用户陷入无出口状态。入口实看：[工程包](/tmp/opencut-ui-delivery-review-20261021/supplement-en-light-project-package.png)、[恢复](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-light-restore-package.png)。失败分支仅源码确认：`package.tsx:25–53`，`setAttempt` 在请求前发生且 catch 不清除。 |
| UI-08 / P1 | 画布设置使用裸 `label/input` 拼接，标签与输入在弹窗内挤在同一行；项目库选择器和工作区选择器使用不同外观路径；素材预览使用浏览器原生音视频控制条。颜色字段仍调用原生颜色选择器。 | 统一表单行、间距、字段宽度、选择器、复选框、颜色面板、预览控制条及焦点态。保留语义化 input/select/dialog 和可访问能力，消除的是浏览器默认视觉，不是 HTML 元素本身。 | “所有组件统一风格”尚未达到；中文标签更容易把布局挤坏。[画布](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-canvas-settings.png)、[素材预览](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-source-preview.png)。`property-edit/settings.tsx:149–268`、`workbench/controls.css:102`、`resource-preview.tsx:113–125`。颜色弹出面板及跨浏览器选择器只查源码，未实际打开。 |
| UI-09 / P1 | 反馈入口默认接入模拟接收器，显示“提交模拟反馈”等，地址为 `feedback@example.invalid`。 | 接入真实可用的反馈流程并明确隐私范围、提交状态与失败重试；尚未接入时不作为成品入口展示。 | 用户会将产品内部演示当成真实反馈功能。[反馈](/tmp/opencut-ui-delivery-review-20261021/supplement-zh-dark-feedback.png)；`feedback/dialog.tsx:41`、`feedback/mock-receiver.ts:2,117`。未提交。 |
| UI-10 / P1 | 通用错误直接展示 `error.message`，任务展示后端 failure/result_unavailable 文本；恢复面板说明请求、幂等键、回执、恢复指针。英文转录能力提示暴露环境变量名 `OPENCUT_WHISPER_MODEL`。 | 为已知失败提供“发生了什么 + 可以怎么做 + 一个主要操作”；诊断信息折叠并可复制。内部恢复状态由系统管理，不能要求普通用户推断请求/回执的关系。 | 遇到问题时无法自助恢复；语言修补不能替代错误模型到用户操作的映射。`workspace/controller.ts:172–178`、`tasks/panel.tsx:63–73`、`scene/edit-recovery-panel.tsx:28–33`；英文字幕面板能力提示实看，其余失败路径仅源码。 |
| UI-11 / P2 | 多种可逆编辑使用“计划 → 应用 → 完成”界面；历史撤销也需要两步。快捷键/帮助仍提及 Classic、IME、scope 等实现背景。 | 可逆常规编辑自动应用并可撤销；只对冲突、批量影响或整版恢复提供有意义的预览。帮助按用户动作组织，技术术语仅放诊断说明。 | 高频操作成本高，额外确认没有提供等量的信息价值。撤销预览实看；`timeline-edit/review.tsx:44–88`、`timeline-edit/panel.tsx:516–522` 仅静态审查，不宣称提交链已验证。 |
| UI-12 / P2 | 可见项目卡片会提交完整分辨率静帧任务，下载后才缩成封面；缓存仅在当前 EditorApi 的内存中。相同工程/版本在新上下文中再次浏览会重复创建任务。 | 封面使用可持久复用的身份键和轻量缩略图流程，避免每次浏览创建普通渲染任务；统一并发、失效与错误回退，不阻塞列表交互。 | 浏览列表产生用户未主动请求的资源消耗和任务记录。此次实测新增 8 个任务、17,386,239 字节 PNG；链路及完整记录见下节。 |

### 复现入口与源码定位

以下路径以 `apps/web/src/editor/` 为根。重复观察时复用现有合成项目；不要为验证界面重新创建工程或提交渲染。

| ID | 最短复现步骤 | 主要源码 |
|---|---|---|
| UI-01 | 偏好设置切浅色 → 打开合成工程 → 选中 sample.mp4 → 查看右侧位置/缩放/速度输入值。 | `workspace/scene/workbench/workbench.css:910` |
| UI-02、03 | 切中文 → 画布设置；再打开工程包、关键帧、字幕面板。英文也检查关键帧选项是否为普通编辑词汇。 | `workspace/scene/property-edit/settings.tsx:149`；`workspace/scene/keyframes/editor.tsx:575`；`workspace/scene/captions/workspace.tsx:1157` |
| UI-04 | 打开视频导出，观察版本行和输出摘要；关闭 → 音频导出 → 选择范围导出，观察输入单位；不提交。 | `workspace/scene/video-export-dialog.tsx:23`；`workspace/scene/audio-wav-dialog.tsx:65` |
| UI-05、06 | 历史记录 → 查看动作/作者 → 查看旧版并返回当前版 → 生成一次撤销预览后关闭；整版恢复路径只读源码。 | `workspace/scene/history.tsx:85`；`workspace/scene/session.ts:828` |
| UI-07 | 打开导出工程包，观察路径要求；失败锁死按源码控制流复现于后续专用可写验收，不在此次共享工程试写。 | `workspace/scene/package.tsx:25` |
| UI-08 | 画布设置观察布局 → 素材预览观察控制条；项目库排序/新建与工作区选择器对比。 | `workspace/scene/property-edit/settings.tsx:149`；`workspace/scene/workbench/controls.css:102`；`workspace/scene/resource-preview.tsx:113` |
| UI-09 | 打开反馈弹窗，读取说明和按钮；不提交。 | `feedback/dialog.tsx:41`；`feedback/mock-receiver.ts:2` |
| UI-10、11 | 查看英文字幕能力提示、快捷键帮助；打开撤销计划；失败/编辑提交只审查代码。 | `workspace/controller.ts:172`；`tasks/panel.tsx:63`；`workspace/scene/edit-recovery-panel.tsx:28`；`workspace/scene/timeline-edit/review.tsx:44` |
| UI-12 | 已有证据足够，无需再触发：项目列表可见卡片 → IntersectionObserver → 请求封面 → 提交固定版本静帧 → 下载完整 PNG → 浏览器缩小。 | `projects/cover.tsx:37–67` → `workspace/project-covers.ts:14` → `workspace/api.ts:73–88`；缩小逻辑 `workspace/project-covers.ts:160` |

静态文案扫描另发现 173 个不同的一参数 `t("...")` 字符串未出现在当前字典中（195 处候选引用），见 [strings.json](/tmp/opencut-ui-delivery-review-20261021/strings.json)。这是排查清单，不是“173 个已在当前界面复现的缺陷”；可能包含未到达路径。源码中的 input/select/dialog 标签数量也不等于原生默认外观的数量，共享 `Modal` 已有自定义外观，不能笼统说所有弹窗都是原生样式。

## 浏览副作用与数据完整性

浏览前后通过 SQLite 只读连接对全表 `SELECT * ... ORDER BY 1` 的行集合做一致的 SHA-256 摘要。结果如下，原始摘要见 [data-before.json](/tmp/opencut-ui-delivery-review-20261021/data-before.json)、[data-after.json](/tmp/opencut-ui-delivery-review-20261021/data-after.json)：

| 数据 | 浏览前 | 浏览后 | 结论 |
|---|---:|---:|---|
| projects | 48 | 48 | 全表摘要一致 |
| revisions | 912 | 912 | 全表摘要一致 |
| editor_transactions | 864 | 864 | 全表摘要一致 |
| jobs | 262 | 270 | 新增 8 个自动封面任务；原先 262 个任务的摘要仍一致 |

这支持“项目记录、版本和编辑事务没有变化”，**不支持“此次走查没有任何写入”**。记录范围内新增任务全部成功，为 `render.project_scene_frame.v4`，时间为数据库原样值 `2026-10-21 14:14:06–14:14:26`，不推断时区。没有显式点击渲染/导出提交，触发源是自动封面链。

| Job | 项目 | 新 PNG 字节数 |
|---|---|---:|
| `job_536ce087a0e941a1be1df3516dfb31c6` | `hainan-workbench-qa-20261021` | 8,321,175 |
| `job_942475e3152548b691e6652869e74d8b` | `agent-complex-masks-v4-acceptance` | 231,377 |
| `job_42c0afb97729401d8af1298c4a28bca7` | `crossfade-oracle-20261021` | 16,586 |
| `job_80ddcee2be7b48869310f6133d859776` | `agent-sound-catalog-v4-acceptance` | 231,377 |
| `job_0c49519435954275b3247d94e37f574e` | `fb92adbb-b999-414e-b351-35f012b8e7b8` | 231,377 |
| `job_d993fdb825fe423aad7a8e4f74bbf3d2` | `crossfade-oracle-20261021` | 16,586 |
| `job_37dd02b8764c4c80b3379c869e75cb0b` | `crossfade-oracle-20261021` | 16,586 |
| `job_8adeab161c534ed399270e2f7cbef2e1` | `hainan-workbench-qa-20261021` | 8,321,175 |

八个 PNG 合计 **17,386,239 字节**，实际文件大小与发布记录一致，逐一 SHA-256 校验通过。它们留在原服务的 `artifacts/<job>/attempt-1/frame.png`，没有删除、移动、覆盖或加入仓库。[任务、版本与发布结果](/tmp/opencut-ui-delivery-review-20261021/automatic-cover-jobs.json)、[文件大小与哈希验证](/tmp/opencut-ui-delivery-review-20261021/automatic-cover-artifacts-verified.json)、[原有任务未变证明](/tmp/opencut-ui-delivery-review-20261021/old-jobs-preserved.json)保留准确路径和身份。相同 crossfade 版本重复三次、Hainan QA 版本重复两次，证据显示跨浏览器上下文没有持久复用封面结果；不据此推断单页面内缓存失效。

## 修复与复查顺序

1. 先修主题令牌、共享表单/选择/颜色/播放器组件，以及画布弹窗布局；这些基础修复覆盖多处界面。
2. 将导出、工程包、历史记录做成完整用户流程：说明结果、保留上下文、成功可获得产物、失败可恢复、恢复整版有影响说明；保留底层固定版本、原子提交和幂等保护。
3. 完成中英文词汇和错误映射，隐藏内部标识、公式参数、协议/回执/环境变量等实现信息；真实接入反馈或收起未完成入口。
4. 改善普通可逆编辑的步骤数，消除项目库自动全尺寸重复渲染；不另建一套业务语义。
5. 修复后只复查受影响的四组合界面，再在明确隔离的合成工作区串行补成功/失败/取消/重试/冲突/下载/整版恢复验收。补全键盘与读屏、Safari/Firefox 及较小窗口覆盖后，才能对“所有交互友好”作交付判断。

可保留的基础包括：已有共享 Modal/Field/Button、语言和主题切换、部分工作区自定义选择器、历史查看与返回当前版本、项目搜索空态以及任务列表。它们降低修复成本，但现有证据不足以标记 UI 全面交付。
