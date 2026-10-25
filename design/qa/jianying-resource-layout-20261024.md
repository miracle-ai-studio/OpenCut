# 中文资源区分类与卡片

状态：用户指定的sol6.1/high独立UI限定验收通过；保存晚回提示分支仍未验。七图全部出口仍由[42条矩阵](jianying-workbench-exit-matrix-20261024.md)维护，不把本切片视为整体通过。[独立报告与原始证据](</private/tmp/opencut-jianying-resources-20261024/review/review.md>)。

## 实现与边界

- 主类按“素材、音频、文本、图形、特效、转场、字幕”排列；保留原内部ID及箭头、Home/End的焦点规则。字体仍归文本，子分类保留在左侧。
- 特效和转场卡采用自适应列数、方形样片、独占一行的名称与完整名称提示。特效加号28px；特效、转场、字体卡内动作在鼠标悬停或键盘焦点时出现，触控入口保留。实际检查发现禁用按钮的全局opacity覆盖隐藏样式，已修正选择器优先级，独立普通重载后确认idle为0、悬停/焦点为0.4的禁用状态。
- 特效主体点击/Enter只打开原画与模糊样片对比，Esc关闭；它是明确标注的示例，不是工程画面，不代表中央播放器上下文已交付。Shift+Enter或卡内加号复用现有SceneStack事务，添加到明确所选且可编辑的画面片段；无兼容目标、锁定及原有播放/恢复围栏继续拒绝。区间层仍复用原精确时间与track_put。
- 保存中的同步引用阻止重复提交；目标改变后不把旧成功提示显示为新对象的结果。后者有单测证据，独立浏览器的延迟响应检查遇到验收器竞态，不能计为UI通过。
- 目录搜索按音效、国旗、形状、品牌隔离；音效/国旗已展开数量与滚动位置分别保留。品牌组件自己的分页与异步内容滚动恢复未覆盖，不能由这两类结果外推。不增加工程、时间、资源授权或获取语义；“仅获取不写工程”及全分类键盘矩阵继续开放。
- 实际滚轮检查还发现目录继承通用Panel固定高度与hidden溢出，长列表被裁切，外层无法滚动。现让目录按内容高度排版并由外层资源视图单独滚动；原失败保留为`review/catalog-scroll-failure.json`及截图，修复后真实滚轮与“显示更多”可达已复查通过。

## 候选与检查

证据根目录 `/private/tmp/opencut-jianying-resources-20261024`。源码基线仍为`abd2c969b4b48067a85f7020d84b04fbb48a2d38`及既有未提交修改；`candidate.json`记录本切片源文件和284个最终生产文件的SHA256。UI检查在5173当前源码，最终生产构建保留为`web-final`，没有替换59790生产Web。Native沿用`8b195d83d925f53ff40bdd981566e508cbb6743c555c209a92af58234f951728`；没有Rust/SDK/公开契约改动或新依赖。

|检查|实际结果与证据|
|---|---|
|特效、转场与目录行为|3文件23项通过，`resource-tests.log`；新增覆盖重复键、保存中重复提交、目标改变、预览关闭与分类搜索保留|
|目录测试最终类型修正|6项重测通过，`catalog-final-tests.log`，与23项重叠，不能相加|
|类型与生产构建|`typecheck-final.log`、`build-final.log`通过；初次目录测试断言的tuple类型错误已修正；两轮UI发现后的CSS修复各自重新构建，原构建与大chunk警告保留|
|Web边界|281文件通过，`boundaries.log`；此后仅CSS可见性与目录滚动修正，不涉及边界规则消费的TS源码|
|三种实际窗口|1280×720、1440×900、1920×1080分类/卡片检查与截图在`review/resources.json`及`layout-*.png`；最终`css-final.json`、`font-final.json`与`catalog-layout-final.json`补充可见性、字体单行/方形及长目录无页面溢出|
|特效预览与目标|三个预设各点击/Enter共六次预览，Esc关闭；选择与精确播放头保持，无目标/音频目标Shift+Enter零写；`review/readonly_after.json`全文档相等|
|真实键盘应用|唯一新增事务`txn_08fc324713b4426fa676c656a87695ee`在C02新增一个强度5的模糊，主Agent直接核对事务及前后完整文档，除目标effects外完全相等，见`applied-transaction.json`|
|验收器异常|延迟响应释放与unroute竞争导致`Route is already handled`、检查客户端退出；原Chrome及页面保留。未重复apply，原事务实际已保存；通过历史计划因果Undo恢复。见`review/tool-failure.md`，不算产品错误、未知保存恢复或晚回提示通过|
|分类搜索/展开/真实滚动|四类查询互不串词；音效40→51、国旗40→80的“显示更多”均先用真实滚轮移入可视区再点击，切换后数量和scrollTop3691.5/3603.5精确恢复。`catalog-long.json`保留首次音频wheel早于加载的无效检查，并补ready后的真实动作；过滤click6项/191.5与清空国旗40项/450也精确恢复，见`catalog-filtered-scroll.json`|
|特效新卡拖动取消|`hover-final.json`证明附着到海湾林带0–6秒，Esc后ghost/HUD清理，轨道无位移；enabled操作idle0/hover1/focus1，尺寸28×28px。未Drop，无新增写入|

## 工程保留与日志

主Agent独立SQL快照`main-before.json`与`main-after.json`：QA121→123，仅添加和因果Undo两事务；新head为`rev_6fdbcd0e27ad21b371ee91ea3c64242e185c66121c8a1cc8afcc7807ba3838c7`，全文档SHA256精确恢复`768641c5d3d66c3d23a70dec21f6da797f6aaf1cb7ff37f6d770e2e3e67405a7`。保留正常前进的修订历史，没有重写回旧head。用户工程46事务、head及全文档SHA256精确不变，没有外部编辑被覆盖。

同一观察范围的Job状态仍为558成功、67失败、33取消，零新增Job；鉴权拒绝最大序号仍96337，最近failed Job事件仍20957。浏览器采集分段，不声称全程连续覆盖：最终同一Chrome/唯一原QA页5秒有界观察的37响应无401/403/500、pageerror或console错误。`review/final-readonly.json`与`final-stopped.png`确认1440×900、声音stopped、C01选中、无dialog/ghost/HUD，验收客户端退出且无活动route/gate/drag。

重连后普通reload前曾读到旧提示`Audio preview request does not match the current session`，缺请求ID/堆栈，reload后消失，保留未归因，不归为鉴权错误。旧音频runner失败与此前无堆栈pageerror继续保留原未归因状态，不以本次正常检查替代根因证明。冷起播、中央源/I/O、历史、导出、共享语义和其余完整矩阵继续开放。
