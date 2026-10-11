# Classic 剩余迁移清单与依赖排期（规划）

本文件仅规划，不启动全面实施。依据 2026-10-07 已核实的 [Classic 审计](../qa/evidence/classic-parity-audit-2026-10-07.json)，未重做审计或改变原状态。
审计 SHA-256：`eeb1a9cfc4362b017268ed751ec58bb29be9db9394a64e3ceddd93a552c39dcf`。机器可读清单见 [同名 JSON](classic-remaining-migration-plan-2026-10-07.json)。

57组：16指定动线已验、23未迁移、14部分迁移、3已实现待等价验收、1原版占位。剩余实施/验收共40组；不计算完成百分比。
13页面：8指定动线已验、3已实现待外部验收、2部分迁移。页面、功能和接口是不同投影，不相加成另一套总数。
F编号严格对应原审计features数组顺序；F35是原版占位，排除缺失统计。本机外部Agent插件第一阶段已经单独交付，不放入此剩余清单。

## 六阶段与资源安排

阶段是交付门槛而非严格串行的六个日历时间盒。可以提前准备下一阶段，但只有依赖和验收通过才能标为交付。P0=数据/契约或主流程阻塞；P1=功能等价核心；P2=完成全量仍需做、但不挡前置主链。

|阶段|范围/唯一主责F组数|工程人日|完成前置|
|---|---|---|---|
|S1 选择、工程恢复与素材基础|11组：F05、F06、F08、F10、F12、F16、F17、F40、F41、F42、F44|26–40|既有能力/无新增前置|
|S2 自由文字、字体与字幕等价|5组：F19、F20、F21、F23、F47|22–34|S1|
|S3 参数化图形、复杂蒙版与合成|10组：F24、F25、F26、F28、F30、F31、F32、F33、F34、F51|36–58|S1、S2|
|S4 动画、变速及全元素交互收口|8组：F27、F29、F37、F38、F39、F43、F46、F50|28–44|S1、S2、S3|
|S5 导出格式与可靠交付|2组：F52、F53|10–16|S1、S2、S3、S4|
|S6 资源、公共页面与最终对照验收|4组：F18、F54、F55、F57|16–26|S1、S2、S3、S4、S5|

主链：S1选择/历史基础 → S2文字/字体 → S3合成扩展 → S4跨元素动画/交互收口 → S5全配置导出 → S6总验收。
可并行：S1期间准备外部provider、反馈接收端和公共数据实测；S2期间准备图形/基础蒙版；S3期间准备曲线求值、变速和WebM。S6公共页验收不必等所有编辑功能，但全量结项必须等S5。
S1不提前宣称所有新元素编辑完成：全元素裁剪/分割F39、完整轨道矩阵F43特意归S4，避免依赖后续文字/效果却先验收的循环。

**S1工作顺序：** 先完成 F40 选择/跨轨与坐标基础，再并行迁移恢复、媒体和音频操作；F12/F16/F41 等在 F40 后收口。
人员分工：前端负责选择/交互，全栈负责迁移/缓存，Rust工程师审查批次事务和音频语义。

**S2工作顺序：** 先冻结独立文字/排版契约；F19→F20→F21，随后 F47 原位编辑及 F23 字幕差距验收。
人员分工：Rust工程师主责文字渲染，全栈主责字体获取，前端主责文字与字幕界面。

**S3工作顺序：** F25 图形骨架与基础 F30 可在 S2 期间准备；文字蒙版在 S2 后完成；F31→F32；混合、效果栈和背景共用合成契约。
人员分工：Rust工程师主责IR/后端，前端主责几何/路径控件，全栈负责资源目录及独立渲染夹具。

**S4工作顺序：** F37 新版过冲曲线及 F29 音频变速可提前设计；F38/F39/F43/F46 待选择与新元素落地后统一验收。
人员分工：Rust工程师负责求值/PCM，前端负责关键帧/命中，全栈负责跨元素矩阵和性能。

**S5工作顺序：** F53 WebM/质量/音轨开关可提前实现和用旧元素验证；F52 全配置与取消发布窗口要在新增渲染元素稳定后收口。
人员分工：全栈主责导出配置和任务界面，Rust工程师负责媒体编码与发布围栏，QA执行恢复/取消矩阵。

**S6工作顺序：** 音效/公共上游访问与产品决策从 S1 即可并行准备；完整快捷动作依赖 S4；最后复核13页面、57组以及独立辅助项。
人员分工：全栈负责provider/公共页，前端负责快捷键与反馈，QA贯穿并主导全量对照。

## 工时估算与假设

这是基于现有代码复杂度的粗略规划估算，不是承诺日期。阶段开发及专项测试138–218工程人日；跨功能集成/全局回归另22–44人日；风险缓冲32–66人日；合计192–328人日，对外用约190–330人日。
估算团队为3名工程师（前端、Rust内核/渲染、全栈/媒体各1）+0.5名QA。有效容量按12–14工程人日/工作周，约14–28工作周；关键依赖与外部等待可能延长，不能简单以总量除以人数承诺日期。未给用户设定任何deadline。
专项测试已计入阶段，追加集成仅计跨阶段组合、跨入口兼容、13页回归和证据；风险缓冲约为实施+集成的20–25%，不与专项测试重复。

- 优先当前macOS本机服务+既有Web栈；目标浏览器与FFmpeg版本在S1冻结，移动端/全平台正式发行不在此预算。
- 沿用现有模块/事务/Worker/SDK，不重写架构或引入新微服务；以Classic原有能力为边界。
- 每阶段入口前先确认依赖契约、样本和独立oracle，阶段完成不是按文件数量打勾。
- 允许外部provider准备并行；同一渲染/IR改动不能按3人无冲突全并行计算。
- 云账户新增、购买字体/素材费用、新媒体provider商业接入、不可访问上游等待、额外历史工程格式和远程部署不包含；发生时重估。
- 14–28工作周只是在约12–14人日/周有效投入下的容量换算，还要满足依赖；不是从今天起算的交付日期，上游等待另记。

重估检查点：S1迁移/选择样本、S2字体排版、S3复杂路径和效果真实渲染。风险主要来自文字度量一致性、复杂蒙版/混合alpha、旧契约曲线范围扩展、音频变速和上游限制。

## 13个页面状态与收口

|页面|原状态|阶段/工作引用|验收门槛|
|---|---|---|---|
|`/blog/[slug]`|已实现待外部验收|S6 / X01|真实上游成功+失败恢复，直接访问/刷新/分页与详情正确；按X01留证据。|
|`/blog`|已实现待外部验收|S6 / X01|真实上游成功+失败恢复，直接访问/刷新/分页与详情正确；按X01留证据。|
|`/brand`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/changelog/[version]`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/changelog`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/contributors`|已实现待外部验收|S6 / X01|真实上游成功+失败恢复，直接访问/刷新/分页与详情正确；按X01留证据。|
|`/editor/[project_id]`|部分迁移|S6 / F08、F10、F12、F16、F17、F18、F19、F20、F21、F23、F24、F25、F26、F27、F28、F29、F30、F31、F32、F33、F34、F37、F38、F39、F40、F41、F42、F43、F44、F46、F47、F50、F51、F52、F53、F54、F55、F57|S1–S5编辑器相关剩余组、S6资源/快捷/反馈通过；原指定预览/快照/外壳动线回归，直接链接与刷新恢复。|
|`/`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/privacy`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/projects`|部分迁移|S1 / F05、F06|完成工程封面与旧浏览器工程升级/恢复动线；回归F01–F04，不能将列表页已有能力重算为缺失。|
|`/roadmap`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/sponsors`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|
|`/terms`|指定动线已验|保留已验；S6回归 / 既有能力/无新增前置|保留指定动线已验状态，S6回归直接访问/刷新、中英明暗和页面原有交互；法律/历史内容仍按本机真实行为。|

## 57组完整分类总表

名称和状态原样保留；详细原始发现、Classic/current源码路径及审计行号全部保存在同名JSON。此表每个F仅一行。

|ID|领域|原始功能名称|审计状态|主阶段/处理|优先级|
|---|---|---|---|---|---|
|F01|项目库|新建、打开、服务持久保存|指定动线已验|保留已验；S6回归|—|
|F02|项目库|单个与批量复制|指定动线已验|保留已验；S6回归|—|
|F03|项目库|搜索、四种排序、网格/列表、信息|指定动线已验|保留已验；S6回归|—|
|F04|项目库|单个与批量删除|指定动线已验|保留已验；S6回归|—|
|F05|项目库|工程封面|未迁移|S1|P2|
|F06|项目库|Classic 工程迁移对话框|未迁移|S1|P0|
|F07|工程状态|时间线缩放/滚动/播放头恢复|指定动线已验|保留已验；S6回归|—|
|F08|工程状态|导入后自动提高工程帧率|未迁移|S1|P1|
|F09|场景|创建、重命名、切换、删除、主场景保护|指定动线已验|保留已验；S6回归|—|
|F10|场景|书签位置、颜色、注释及画布叠层|未迁移|S1|P2|
|F11|历史|连续撤销、重做、新编辑清空重做|指定动线已验|保留已验；S6回归|—|
|F12|历史|撤销删除后恢复活动场景和选择|未迁移|S1|P1|
|F13|素材|上传、源预览、插入、替换|指定动线已验|保留已验；S6回归|—|
|F14|素材|素材网格/列表、类型/时长/大小排序|指定动线已验|保留已验；S6回归|—|
|F15|素材|多选、范围选择与批量移除|指定动线已验|保留已验；S6回归|—|
|F16|素材|剪贴板粘贴文件与选定轨道拖放|部分迁移|S1|P1|
|F17|素材|缩略图和波形缓存|部分迁移|S1|P1|
|F18|音效|远程搜索、试听、分类、下载插入|未迁移|S6|P1|
|F19|文字|独立自由文字元素|未迁移|S2|P0|
|F20|文字|字体浏览与字体加载|未迁移|S2|P1|
|F21|文字|字重、斜体、装饰、字距、行距和背景参数|部分迁移|S2|P1|
|F22|字幕|字幕文件交换、文本编辑与定位|指定动线已验|保留已验；S6回归|—|
|F23|字幕|选择语言并自动生成转写字幕|部分迁移|S2|P1|
|F24|贴纸|图形、国旗、品牌分类与搜索|部分迁移|S3|P2|
|F25|图形|矩形/椭圆/多边形/星形参数化元素|部分迁移|S3|P1|
|F26|图形|描边、对齐、圆角、边数和图形关键帧|未迁移|S3|P1|
|F27|属性|基础变换、透明度、音量|已实现待等价验收|S4|P1|
|F28|属性|17 种混合模式|未迁移|S3|P1|
|F29|变速|速度属性和 Change pitch|部分迁移|S4|P1|
|F30|蒙版|矩形/椭圆与羽化|部分迁移|S3|P1|
|F31|蒙版|Split、电影黑边、心形、菱形、星形、文字、钢笔|未迁移|S3|P1|
|F32|蒙版|叠层、反转、点增删、手柄与动画|未迁移|S3|P1|
|F33|效果|模糊效果及参数|已实现待等价验收|S3|P1|
|F34|效果|效果重排、动态参数、独立效果轨道|未迁移|S3|P1|
|F35|转场/调整|原版两个工具标签|原版占位|不计缺失|—|
|F36|动画|标量关键帧和基本 Bézier 曲线|指定动线已验|保留已验；S6回归|—|
|F37|动画|Y 轴过冲、Pop 与自定义曲线预设|未迁移|S4|P0|
|F38|动画|关键帧拖动、多选及复制粘贴|未迁移|S4|P1|
|F39|时间线|单选裁剪、移动、分割、复制与删除|部分迁移|S4|P1|
|F40|时间线|多选、框选和跨轨拖动|未迁移|S1|P0|
|F41|时间线|Split left/right 与 ripple 模式|部分迁移|S1|P1|
|F42|时间线|源音频分离/恢复|未迁移|S1|P1|
|F43|时间线|轨道显示、静音、锁定与增删|已实现待等价验收|S4|P1|
|F44|时间线|直接拖动音量线|未迁移|S1|P2|
|F45|预览|连续播放、定帧、全屏与静音|指定动线已验|保留已验；S6回归|—|
|F46|预览|实时内容变换、精确命中与多选|部分迁移|S4|P1|
|F47|预览|文字画布内编辑|未迁移|S2|P1|
|F48|预览|视口平移/缩放、网格及平台安全区|指定动线已验|保留已验；S6回归|—|
|F49|预览|保存与复制快照|指定动线已验|保留已验；S6回归|—|
|F50|设置|画布尺寸与帧率|部分迁移|S4|P2|
|F51|设置|纯色与模糊工程背景|未迁移|S3|P1|
|F52|导出|持久导出、进度、下载及取消|部分迁移|S5|P0|
|F53|导出|WebM、四档质量、Include audio|未迁移|S5|P0|
|F54|快捷键|基本快捷键与帮助|部分迁移|S6|P1|
|F55|快捷键|自定义/重置/冲突和持久化|未迁移|S6|P2|
|F56|外壳|面板尺寸记忆、新手引导、变更通知|指定动线已验|保留已验；S6回归|—|
|F57|外壳|反馈提交|未迁移|S6|P2|

## 40组剩余工作的依赖和可检验验收条件

以下是总表中40项的展开，不增加计数。每项验收还需满足后文G1–G7；当前状态不因列入计划改变。

### S1 选择、工程恢复与素材基础

**F05 项目库｜工程封面**（未迁移；P2）

- 前置：F49；固定场景/修订帧、封面失效规则、项目列表缓存。
- 验收：新建/修改/切换主场景后封面更新；失败有回退；刷新和复制后关联正确；删除原工程不破坏副本，封面无跨项目串用。
- 审计定位：`/features/4`，原JSON第145行。
- Classic：`tmp/opencut-classic/apps/web/src/core/managers/project-manager.ts`。
- 当前：`apps/web/src/editor/projects/browser.tsx`。

**F06 项目库｜Classic 工程迁移对话框**（未迁移；P0）

- 前置：F01、F11、F13；旧 IndexedDB 版本夹具、同源访问边界、现有 Classic 导入、进度/恢复状态机。
- 验收：合成旧版浏览器工程可发现并显示升级进度；成功后文档和素材可打开/渲染；损坏/中断可重试且旧库不被销毁；跨源明确提供旧源导出桥接，不能以单独 JSON 导入冒充自动升级。
- 审计定位：`/features/5`，原JSON第157行。
- Classic：`tmp/opencut-classic/apps/web/src/project/components/migration-dialog.tsx`。
- 当前：`apps/web/src/editor/projects/browser.tsx`。

**F08 工程状态｜导入后自动提高工程帧率**（未迁移；P1）

- 前置：F07、F13；素材帧率元数据、精确时间模型、原版提高帧率决策规则。
- 验收：混合低/高/分数帧率导入逐例对照原版；只在应提高时提高；现有片段、播放头、关键帧的实际时间不漂移；刷新、撤销和导出帧率正确。
- 审计定位：`/features/7`，原JSON第185行。
- Classic：`tmp/opencut-classic/apps/web/src/core/managers/project-manager.ts`。
- 当前：`apps/web/src/editor/assets/panel.tsx`。

**F10 场景｜书签位置、颜色、注释及画布叠层**（未迁移；P2）

- 前置：F07、F09、F11；场景标记持久化、视口坐标、颜色/注释协议。
- 验收：位置/颜色/注释可新增修改删除并持久恢复；切场景隔离；时间线和画布叠层与标记一致；撤销恢复；辅助叠层不混入导出。
- 审计定位：`/features/9`，原JSON第209行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/scene/toggle-bookmark.ts`、`tmp/opencut-classic/apps/web/src/commands/scene/update-bookmark.ts`。
- 当前：`apps/web/src/editor/projects/scenes.tsx`。

**F12 历史｜撤销删除后恢复活动场景和选择**（未迁移；P1）

- 前置：F09、F11、F40；多选状态和历史视图快照、活动场景有效性。
- 验收：删除所选片段或活动场景再撤销，恢复有效场景与原选择；重做、新编辑和刷新无悬空 ID；视图恢复不额外修改项目文档。
- 审计定位：`/features/11`，原JSON第234行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/preview-tracker.ts`。
- 当前：`apps/web/src/editor/workspace/controller.ts`。

**F16 素材｜剪贴板粘贴文件与选定轨道拖放**（部分迁移；P1）

- 前置：F13、F15、F40；统一导入入口、目标轨道命中、平台剪贴板权限/焦点。
- 验收：真实系统剪贴板与拖放覆盖视频/音频/图片、空/已有兼容轨道、多文件、取消及失败；不兼容轨道显式拒绝；并发替换上传不被粘贴改靶；落点和撤销正确。
- 审计定位：`/features/15`，原JSON第287行。
- Classic：`tmp/opencut-classic/apps/web/src/media/use-paste-media.ts`。
- 当前：`apps/web/src/editor/assets/panel.tsx`。

**F17 素材｜缩略图和波形缓存**（部分迁移；P1）

- 前置：F13、F45；内容哈希缓存键、缩放密度分级、内存/磁盘预算和淘汰。
- 验收：长短素材、密集时间线和快速缩放下图/波形无错位旧帧；刷新可复用缓存；预算内淘汰后可重建；100片段/5轨/5分钟合成夹具记录冷/热成本与峰值，按阶段冻结预算验收。
- 审计定位：`/features/16`，原JSON第300行。
- Classic：`tmp/opencut-classic/apps/web/src/media/thumbnail.ts`、`tmp/opencut-classic/apps/web/src/media/waveform-summary.ts`。
- 当前：`apps/web/src/editor/workspace/media-preview.ts`。

**F40 时间线｜多选、框选和跨轨拖动**（未迁移；P0）

- 前置：F07、F11、F15；选择集合、跨轨目标规划、坐标/吸附和原子批次编辑。
- 验收：单击修饰键/范围/框选以及多片段跨轨拖动有入口；相对时间/轨距和锁定轨保护正确；一次拖动一个原子事务，取消不写入、修订冲突不部分提交，撤销恢复。
- 审计定位：`/features/39`，原JSON第585行。
- Classic：`tmp/opencut-classic/apps/web/src/timeline/components/selection-hit-testing.ts`。
- 当前：`apps/web/src/editor/timeline/panel.tsx`。

**F41 时间线｜Split left/right 与 ripple 模式**（部分迁移；P1）

- 前置：F40、F11；既有 timeline plan、动作路由、常驻 ripple 模式状态。
- 验收：Split left/right 在边界和中间点准确；ripple开/关下后续片段位移、锁轨/多轨关联符合原版；快捷/菜单入口共用规划；模式可辨识，撤销无残留。
- 审计定位：`/features/40`，原JSON第597行。
- Classic：`tmp/opencut-classic/apps/web/src/actions/definitions.ts`。
- 当前：`apps/web/src/editor/timeline/commands.tsx`。

**F42 时间线｜源音频分离/恢复**（未迁移；P1）

- 前置：F13、F11；音视频同源绑定、独立音频轨道、来源与恢复关联。
- 验收：分离音频后不重复出声且时间对齐，单独编辑和撤销/恢复可靠；无音轨素材显式拒绝；裁剪后分离的已知脉冲位置和 PCM 与预期一致。
- 审计定位：`/features/41`，原JSON第609行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/timeline/element/toggle-source-audio-separation.ts`。
- 当前：`apps/web/src/editor/timeline/panel.tsx`。

**F44 时间线｜直接拖动音量线**（未迁移；P2）

- 前置：F36、F42；音量参数与可视几何映射、pointer会话提交。
- 验收：音频和带音频视频的音量线拖动可预览，松手一次原子写入；取消不写；静音、上下限和缩放下命中正确；既有音量曲线不丢，PCM增益对照通过。
- 审计定位：`/features/43`，原JSON第633行。
- Classic：`tmp/opencut-classic/apps/web/src/timeline/components/audio-volume-line.tsx`。
- 当前：`apps/web/src/editor/timeline/panel.tsx`。

### S2 自由文字、字体与字幕等价

**F19 文字｜独立自由文字元素**（未迁移；P0）

- 前置：F11；独立文字领域元素、排版测量、版本化编辑/IR/原生渲染。
- 验收：创建的自由文字不是 caption 替身；位置/尺寸/持续时间及多行排版可编辑，复制/拆分/撤销/持久恢复正确；固定字体下独立字形布局/像素夹具验证预览和导出。
- 审计定位：`/features/18`，原JSON第325行。
- Classic：`tmp/opencut-classic/apps/web/src/text/components/assets-view.tsx`。
- 当前：`apps/web/src/editor/assets/titles.tsx`。

**F20 文字｜字体浏览与字体加载**（未迁移；P1）

- 前置：F19；系统/可下载字体目录、宿主解析与渲染字体身份、来源许可策略 D2。
- 验收：可浏览/搜索/预览并加载字体；浏览器和 Worker 使用同一可验证字体；离线、缺字和缺字体明确回退/拒绝；中文/拉丁字体夹具可稳定重开和导出。
- 审计定位：`/features/19`，原JSON第337行。
- Classic：`tmp/opencut-classic/apps/web/src/fonts/google-fonts.ts`、`tmp/opencut-classic/apps/web/src/fonts/system-fonts.ts`。
- 当前：`apps/web/src/editor/assets/titles.tsx`。

**F21 文字｜字重、斜体、装饰、字距、行距和背景参数**（部分迁移；P1）

- 前置：F19、F20；文字样式版本契约、字体度量、背景几何。
- 验收：逐项覆盖字重/斜体/装饰/字距/行距、背景圆角/边距/偏移；组合渲染有独立预期，不能只验证控件值；撤销/复制/导入与旧字幕兼容。
- 审计定位：`/features/20`，原JSON第350行。
- Classic：`tmp/opencut-classic/apps/web/src/params/registry.ts`。
- 当前：`apps/web/src/editor/inspector/panel.tsx`。

**F23 字幕｜选择语言并自动生成转写字幕**（部分迁移；P1）

- 前置：F19、F20、F21、F22；既有本机 ASR/任务与字幕原子生成、文字表示兼容、语言质量夹具。
- 验收：保留已有混音/取消/修订锁与撤销链；补齐原版字体/编辑/诊断入口；中英固定人工标注语音检查语言选择与文字准确率阈值，明确估算词时序；其他语言逐一标记待验，不能泛称已验。
- 审计定位：`/features/22`，原JSON第374行。
- Classic：`tmp/opencut-classic/apps/web/src/subtitles/components/assets-view.tsx`。
- 当前：`apps/web/src/editor/assets/automatic-captions.tsx`、`crates/editor-application/src/caption_generation.rs`、`crates/sqlite-store/src/sqlite_caption_generation.rs`、`crates/host-runtime/src/worker/project_v3.rs`。

**F47 预览｜文字画布内编辑**（未迁移；P1）

- 前置：F19、F20、F21、F40；文字叠层、光标/IME、排版与画布坐标共用。
- 验收：双击画布原位编辑，中文输入法/换行/选区可用；缩放旋转后的编辑位置正确；Esc取消/提交/撤销一致，时间线快捷键不抢文本输入；最终原生渲染匹配排版。
- 审计定位：`/features/46`，原JSON第669行。
- Classic：`tmp/opencut-classic/apps/web/src/preview/components/text-edit-overlay.tsx`。
- 当前：`apps/web/src/editor/preview/transform-overlay.tsx`。

### S3 参数化图形、复杂蒙版与合成

**F24 贴纸｜图形、国旗、品牌分类与搜索**（部分迁移；P2）

- 前置：F13；分类索引与可替换旗帜/品牌资源 provider、资产许可策略 D2。
- 验收：三类均有可追溯资源和搜索结果；正确插入、缩放、透明显示及导出；离线/缺项有状态，不能用几枚本地图形替代全部分类。
- 审计定位：`/features/23`，原JSON第390行。
- Classic：`tmp/opencut-classic/apps/web/src/stickers/providers/index.ts`。
- 当前：`apps/web/src/editor/assets/titles.tsx`。

**F25 图形｜矩形/椭圆/多边形/星形参数化元素**（部分迁移；P1）

- 前置：F11、F36；参数化图形领域/IR、几何生成和抗锯齿。
- 验收：矩形/椭圆/多边形/星形均为可编辑参数节点；参数变更、复制、撤销和保存可恢复；不同边数/星角的轮廓与独立几何预期匹配，SVG 图片不计等价。
- 审计定位：`/features/24`，原JSON第402行。
- Classic：`tmp/opencut-classic/apps/web/src/graphics/definitions/index.ts`。
- 当前：`apps/web/src/editor/assets/titles.tsx`。

**F26 图形｜描边、对齐、圆角、边数和图形关键帧**（未迁移；P1）

- 前置：F25、F36；图形参数类型/范围、描边对齐、可动画参数声明。
- 验收：描边内/中/外对齐、圆角、边数和允许参数关键帧可编辑；开始/中间/末帧几何正确；超界拒绝、撤销精确，非连续参数的插值规则明确。
- 审计定位：`/features/25`，原JSON第414行。
- Classic：`tmp/opencut-classic/apps/web/src/graphics/components/graphic-tab.tsx`。
- 当前：`apps/web/src/editor/inspector/visual-properties.tsx`。

**F28 属性｜17 种混合模式**（未迁移；P1）

- 前置：F19、F25；混合模式显式契约、色彩/alpha规则、后端支持矩阵。
- 验收：按原版注册表逐一覆盖17种模式，含半透明/黑白/彩色边界；用独立公式/参考像素验收顺序和alpha，不支持不能静默降级；预览/导出与历史保持一致。
- 审计定位：`/features/27`，原JSON第438行。
- Classic：`tmp/opencut-classic/apps/web/src/params/registry.ts`。
- 当前：`apps/web/src/editor/inspector/panel.tsx`。

**F30 蒙版｜矩形/椭圆与羽化**（部分迁移；P1）

- 前置：F36、F40；蒙版局部/画布坐标、尺寸/位置/旋转与羽化单位。
- 验收：矩形/椭圆位置尺寸旋转羽化可在属性和画布操作；变换/裁剪后坐标正确；边缘/内外像素有独立预期；保存和撤销恢复。
- 审计定位：`/features/29`，原JSON第462行。
- Classic：`tmp/opencut-classic/apps/web/src/masks/components/masks-tab.tsx`。
- 当前：`apps/web/src/editor/inspector/visual-properties.tsx`。

**F31 蒙版｜Split、电影黑边、心形、菱形、星形、文字、钢笔**（未迁移；P1）

- 前置：F19、F20、F25、F30；复杂形状和路径契约、文字轮廓、路径填充规则。
- 验收：Split/电影黑边/心形/菱形/星形/文字/钢笔逐一可创建编辑；复杂路径与孔洞/闭合规则显式；独立几何和透明度夹具覆盖形状边界及实际渲染。
- 审计定位：`/features/30`，原JSON第474行。
- Classic：`tmp/opencut-classic/apps/web/src/masks/builtin/definitions/index.ts`。
- 当前：`apps/web/src/editor/inspector/visual-properties.tsx`。

**F32 蒙版｜叠层、反转、点增删、手柄与动画**（未迁移；P1）

- 前置：F30、F31、F36、F40；蒙版栈次序、控制点/切线和命中、基础动画通道。
- 验收：多层次序和反转正确；钢笔点增删/拖动/手柄可撤销；动画起中末帧与独立预期一致；对象变换框不能代替蒙版手柄；快速操作不残留旧修订。
- 审计定位：`/features/31`，原JSON第486行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/timeline/element/masks/index.ts`、`tmp/opencut-classic/apps/web/src/preview/components/mask-handles.tsx`。
- 当前：`apps/web/src/editor/preview/transform-overlay.tsx`。

**F33 效果｜模糊效果及参数**（已实现待等价验收；P1）

- 前置：F19、F25；既有模糊效果、统一参数/对象支持矩阵。
- 验收：按原版全部参数、对象类别、启停和叠加组合补等价验收；已知边缘扩散/alpha夹具核验实际像素；失败报告对象/参数，不能只断言任务成功。
- 审计定位：`/features/32`，原JSON第499行。
- Classic：`tmp/opencut-classic/apps/web/src/effects/definitions/blur.ts`。
- 当前：`apps/web/src/editor/effects/panel.tsx`。

**F34 效果｜效果重排、动态参数、独立效果轨道**（未迁移；P1）

- 前置：F19、F25、F36；独立效果元素/轨道、作用范围、效果栈顺序和动态参数。
- 验收：可重排效果、创建独立效果轨道并对支持参数设关键帧；用不可交换的效果组合证明顺序有效；范围/叠层/静音或禁用语义明确，撤销和工程恢复正确。
- 审计定位：`/features/33`，原JSON第511行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/timeline/element/effects/reorder-effect.ts`、`tmp/opencut-classic/apps/web/src/effects/components/effects-tab.tsx`。
- 当前：`apps/web/src/editor/effects/panel.tsx`。

**F51 设置｜纯色与模糊工程背景**（未迁移；P1）

- 前置：F30；场景背景契约、内容适配和模糊采样规则。
- 验收：纯色和模糊背景在横竖/方形、多片段、透明内容下可设置；独立颜色/模糊参考样本验证真实画面；保存/撤销一致；不额外增加原版没有的渐变。
- 审计定位：`/features/50`，原JSON第724行。
- Classic：`tmp/opencut-classic/apps/web/src/components/editor/panels/assets/views/settings/background.tsx`。
- 当前：`apps/web/src/editor/inspector/sequence-settings.tsx`。

### S4 动画、变速及全元素交互收口

**F27 属性｜基础变换、透明度、音量**（已实现待等价验收；P1）

- 前置：F19、F25、F34、F39；基础属性既有应用语义、新元素能力矩阵与统一单位。
- 验收：对视频/音频/图片/文字/字幕/图形/效果适用属性逐格验收位置/尺寸/旋转/透明度/音量；数值输入与画布操作结果一致，边界值/撤销/持久恢复正确；独立像素及PCM验证属性确实生效。
- 审计定位：`/features/26`，原JSON第426行。
- Classic：`tmp/opencut-classic/apps/web/src/components/editor/panels/properties/registry.tsx`。
- 当前：`apps/web/src/editor/inspector/panel.tsx`。

**F29 变速｜速度属性和 Change pitch**（部分迁移；P1）

- 前置：F08、F42；有理数速度映射、源时间范围、音频重采样/保音高策略。
- 验收：速度比率控件和 Change pitch 两种语义可用；1/2倍、2倍、分数比率后时长/切点/字幕/曲线同步；已知频率与脉冲 PCM 验证保音高和变调，不能只改变尾点。
- 审计定位：`/features/28`，原JSON第450行。
- Classic：`tmp/opencut-classic/apps/web/src/speed/components/speed-tab.tsx`。
- 当前：`apps/web/src/editor/inspector/panel.tsx`。

**F37 动画｜Y 轴过冲、Pop 与自定义曲线预设**（未迁移；P0）

- 前置：F36；新版本曲线契约、求值/限制策略、自定义预设存储。
- 验收：原版Y=-0.5..1.5、Pop和自定义预设保存/载入可用；旧0..1契约保持不变；独立数值oracle检查过冲样本，位置可过冲、受限参数按明确规则处理，渲染不能静默裁成原曲线。
- 审计定位：`/features/36`，原JSON第548行。
- Classic：`tmp/opencut-classic/apps/web/src/timeline/components/graph-editor/easing-presets.ts`、`tmp/opencut-classic/apps/web/src/timeline/components/graph-editor/custom-presets-store.ts`。
- 当前：`apps/web/src/editor/inspector/bezier-graph.tsx`。

**F38 动画｜关键帧拖动、多选及复制粘贴**（未迁移；P1）

- 前置：F36、F37、F40；关键帧选择集、坐标/吸附、时间空间和剪贴板格式。
- 验收：拖动/框选/多选/复制粘贴、跨片段时间偏移和边界拒绝可用；批次原子提交，撤销恢复；重复时间点规则明确，曲线切线不丢失。
- 审计定位：`/features/37`，原JSON第561行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/timeline/clipboard/paste-keyframes.ts`。
- 当前：`apps/web/src/editor/inspector/keyframes.tsx`。

**F39 时间线｜单选裁剪、移动、分割、复制与删除**（部分迁移；P1）

- 前置：F19、F25、F34、F40；通用元素编辑能力矩阵、新文字/图形/效果节点。
- 验收：视频/音频/图片/文字/字幕/图形/效果各类支持的裁剪、移动、分割、复制、删除逐格验收；不适用操作禁用并说明；源范围、曲线/蒙版/字幕关联及撤销保持正确。
- 审计定位：`/features/38`，原JSON第573行。
- Classic：`tmp/opencut-classic/apps/web/src/timeline/components/index.tsx`。
- 当前：`apps/web/src/editor/timeline/panel.tsx`。

**F43 时间线｜轨道显示、静音、锁定与增删**（已实现待等价验收；P1）

- 前置：F34、F40、F42；轨道类型矩阵、效果轨道、跨轨选择和编辑保护。
- 验收：各轨道类型的新增/删除/显示/静音/锁定逐格验收；锁定拒绝批次写入，隐藏/静音在真实像素和 PCM中生效；删轨关联清理和撤销精确。
- 审计定位：`/features/42`，原JSON第621行。
- Classic：`tmp/opencut-classic/apps/web/src/commands/timeline/track/index.ts`。
- 当前：`apps/web/src/editor/timeline/panel.tsx`。

**F46 预览｜实时内容变换、精确命中与多选**（部分迁移；P1）

- 前置：F19、F25、F30、F32、F40；统一命中/坐标、可取消本地交互预览与权威修订回读。
- 验收：拖动期间显示实际内容变换而非仅辅助框；透明区域/旋转对象命中准确，多选保持相对变换；撤销/取消/冲突可回退，旧异步帧不覆盖新姿态；松手后的服务渲染与交互结果一致。
- 审计定位：`/features/45`，原JSON第657行。
- Classic：`tmp/opencut-classic/apps/web/src/preview/components/preview-interaction-overlay.tsx`。
- 当前：`apps/web/src/editor/preview/transform-overlay.tsx`。

**F50 设置｜画布尺寸与帧率**（部分迁移；P2）

- 前置：F08、F13；工程/视图偏好分离、自定义尺寸和来源元数据。
- 验收：自定义尺寸记忆刷新保持、原始尺寸回退有准确来源；跨项目隔离；尺寸/帧率修改可撤销并正确导出，视图偏好不意外成为内容事务。
- 审计定位：`/features/49`，原JSON第712行。
- Classic：`tmp/opencut-classic/apps/web/src/components/editor/panels/assets/views/settings/index.tsx`。
- 当前：`apps/web/src/editor/inspector/sequence-settings.tsx`。

### S5 导出格式与可靠交付

**F52 导出｜持久导出、进度、下载及取消**（部分迁移；P0）

- 前置：F19、F25、F28、F29、F34、F37、F51、F53；固定修订任务、发布围栏/取消、下载和终态恢复。
- 验收：队列/渲染中/产物发布前后取消逐窗口验收；进度/断连重连/刷新任务恢复准确；失败无成功下载、取消无迟到发布；完整新增元素组合输出实际解码/声画基准通过。
- 审计定位：`/features/51`，原JSON第736行。
- Classic：`tmp/opencut-classic/apps/web/src/components/editor/export-button.tsx`。
- 当前：`apps/web/src/editor/tasks/export-dialog.tsx`。

**F53 导出｜WebM、四档质量、Include audio**（未迁移；P0）

- 前置：F29、F51；WebM容器/编码能力、四档质量映射、音频include契约。
- 验收：WebM真实封装与兼容编码可播放；四档质量配置实际传递至后端且非四个同值；Include audio开/关分别有/无音轨；MP4回归、错误组合拒绝、metadata/像素/PCM验证。
- 审计定位：`/features/52`，原JSON第748行。
- Classic：`tmp/opencut-classic/apps/web/src/export/index.ts`。
- 当前：`apps/web/src/editor/tasks/export-dialog.tsx`。

### S6 资源、公共页面与最终对照验收

**F18 音效｜远程搜索、试听、分类、下载插入**（未迁移；P1）

- 前置：F13；可替换音效 provider、授权来源/署名、搜索分页、下载导入。
- 验收：真实获准上游完成搜索/分类/试听/下载并插入剪辑导出；404/429/超时/取消和重复下载可恢复；保留来源/署名；本地音频过滤不能算完成。
- 审计定位：`/features/17`，原JSON第313行。
- Classic：`tmp/opencut-classic/apps/web/src/sounds/components/assets-view.tsx`。
- 当前：`apps/web/src/editor/assets/panel.tsx`。

**F54 快捷键｜基本快捷键与帮助**（部分迁移；P1）

- 前置：F10、F12、F38、F39、F40、F41、F42、F43、F47；原版30个注册动作/53个命令源文件映射、焦点路由。
- 验收：逐项登记动作入口/菜单/帮助/快捷键；文本输入、弹窗和时间线焦点互不抢键；macOS修饰键、禁用状态、按住重复和撤销行为实测；不按命令文件数新增功能计数。
- 审计定位：`/features/53`，原JSON第760行。
- Classic：`tmp/opencut-classic/apps/web/src/actions/components/shortcuts-dialog.tsx`。
- 当前：`apps/web/src/editor/shared/shortcuts.tsx`。

**F55 快捷键｜自定义/重置/冲突和持久化**（未迁移；P2）

- 前置：F54；快捷键配置schema、冲突检测和偏好存储。
- 验收：可改键/清除/重置，冲突实时提示且不静默覆盖；刷新恢复、导入坏配置回退；系统保留键给出说明；不向内容历史添加偏好事务。
- 审计定位：`/features/54`，原JSON第772行。
- Classic：`tmp/opencut-classic/apps/web/src/actions/keybindings-store.ts`。
- 当前：`apps/web/src/editor/shared/shortcuts.tsx`。

**F57 外壳｜反馈提交**（未迁移；P2）

- 前置：既有能力/无新增前置；反馈接收方/隐私范围 D3、表单/API、超时及错误反馈。
- 验收：用户显式提交后获可验证接收回执；校验/超时/重复/服务失败可辨识；不会自动附带素材/凭据；无接收方时明确未配置，不能将本地草稿当提交成功。
- 审计定位：`/features/56`，原JSON第799行。
- Classic：`tmp/opencut-classic/apps/web/src/components/editor/editor-header.tsx`。
- 当前：`apps/web/src/editor/editor-workspace.tsx`。

## 公共数据、辅助接口与不重复计数

**X01 博客/文章/贡献者真实外部数据验收**（S6；P1；57组之外）

- 依赖：固定公开上游可访问、限流策略和授权；仅记录真实结果，不绕过403/429。
- 验收：在允许环境取得真实列表/详情/分页成功响应并在浏览器呈现，记录上游URL/时间/响应状态及内容摘要；同时保留429/403/空列表/超时/重试夹具通过证据；上游不可达仍标待验。

**X02 RSS、sitemap与服务端分享元数据**（S6；P2；57组之外）

- 依赖：公共域名/规范URL、公共内容映射、服务端静态/动态输出方式；最终真实内容复核依赖X01。
- 验收：RSS/XML和sitemap解析有效，条目/规范URL/时间与内容一致；文章首个HTML响应具备正确标题/摘要/分享图而非只靠客户端脚本；无内容及未知slug有正确响应；不暴露本机工程ID或路径。

**X03 原账户系统与本机身份替代的范围决策**（S6；P0；57组之外）

- 依赖：产品决定D1，现有HttpOnly本机会话/Agent凭证；不是默认新增云账户系统。
- 验收：默认保留本机身份、明确记录与Classic账户系统不同；本机会话隔离/退出/过期/拒绝按既有契约验证。若坚持账户逐项等价，另行定义注册/登录/恢复/迁移及服务依赖并重新估算，不能声明账户已等价。

X01/X02工作量已计S6；X03默认只有范围记录与既有身份回归，不包含云账户开发预算。

|辅助入口|审计状态|工作归属/计数|
|---|---|---|
|`/api/auth/[...all]`|行为替换，原版账户等价未验|X03；范围决策，不重复添加F组|
|`/api/health`|服务健康入口替代|既有能力/无新增前置；已有健康入口替代，S6兼容回归，不新增开发组|
|`/api/feedback`|未迁移|F57；已计F57，不重复|
|`/api/sounds/search`|未迁移|F18；已计F18，不重复|
|`/rss.xml、sitemap`|未迁移|X02；57组外独立项X02|

## 少量产品范围决策及默认方案

以下可先按默认方案准备工作，不因未答复阻塞独立工作；默认方案不是用户已经批准范围缩减。

**D1 是否要求Classic云账户/登录逐项等价？**

- 默认：维持本机服务身份替代，公开注明差异；不默认建设云账户系统。
- 影响：不阻塞编辑迁移；不批准范围替代时，账户等价保持未决，云账户工作需另估，不在当前人日中。 关联：X03。

**D2 在线字体、旗帜/品牌贴纸和音效采用哪些许可与来源？**

- 默认：优先原版可继续使用且许可允许的来源；provider可替换，下载到本机并保留来源/署名，字体由宿主验证；离线保留已缓存资源。
- 影响：资源许可/接口不可用可能增加适配工作和等待；不把旧provider的商标或版权状态假定为授权。 关联：F18、F20、F24。

**D3 反馈发往哪个接收端，允许附带哪些信息？**

- 默认：自有可配置接收端，仅用户明确输入的文本和经确认的诊断字段；不自动附带素材、账号或凭据。
- 影响：接收端未配置时F57不得验收为可提交；外部上线/费用需单独授权，不因本规划自动执行。 关联：F57。

**D4 旧Classic浏览器工程来自哪些源站与版本？**

- 默认：保留同源旧库自动检测/升级；跨源采用用户在旧源发起的导出桥接并显示进度/恢复，不尝试跨源直读。
- 影响：按原审计固定旧版格式实现；额外历史版本/无法访问的旧源需补样本和估算。 关联：F06。

## 共同完成门槛

- G1：每项同时具备等价用户入口、共享应用语义和对应证据；代码存在/构建成功/预览等于导出均不能单独判完成。
- G2：新能力必须从源文档→版本契约→IR→后端→Web/CLI/HTTP/MCP/SDK贯通；不把域规则放进Web，旧wire schema冻结，新不兼容字段/范围走新版本并生成类型。
- G3：每次内容修改验证expected_revision、原子性、幂等重放、冲突拒绝和完整文档撤销/重做；临时交互、选择与偏好不误写内容历史。
- G4：视觉使用独立几何/颜色/alpha/字体度量参考；音频使用已知频率/增益/时间脉冲PCM参考；保留固定修订、执行身份、产物哈希和解码证据。
- G5：架构变更运行architecture负向测试与两项边界门禁；公共协议变更执行生成契约/旧wire兼容/SDK门禁；运行受影响行为测试及独立临时数据库浏览器旅程。
- G6：S6逐行核对57组和13页面及X01–X03，不重复加总页面/接口与功能；53命令源文件、30注册动作是覆盖基准，不再扩充功能总数；全量回归后更新实际完成台账。
- G7：外部请求仅在获准环境验收，不绕过权限/限流；素材保持本机、合成夹具进入测试。部署、真实数据迁移和持续凭证变更需要独立执行范围。

## 规划文件核对结果与限制

本轮只运行清单一致性校验：57组原始字段和顺序一致；40组唯一归属；13页无遗漏；依赖引用有效、无环且无逆向阶段阻塞；人日加总正确。没有重跑产品测试，因此这里没有新增“功能已验”声明。

16组已验仅保持审计指定动线；其组合回归包含在最终集成工作，不重新报为功能缺失。F35原版占位、原版没有的渐变背景、无原版证据的分组，以及远程Agent/正式发行不夹带入本迁移。
最终全量声明必须同时说明X03账户范围决定；若采用本机替代，应写“约定本机范围的Classic功能等价”，不能声称原账户系统也已逐项迁移。
本轮没有业务代码、数据库、部署、真实工程或全局Agent配置变更；计划不授权这些后续执行动作。
