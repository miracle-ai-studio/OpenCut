# jianying-zh 字体资源交互限定验收

2026-10-24，落实 [工作台实施计划](../plans/jianying-zh-workbench-plan.md) 第一项。该项完成字体卡片预览/应用分离及独立交互检查；其余转场、资源获取、布局、时间线和历史等切片仍按计划推进，不代表7张设计全部实现。

## 行为与实现

- 字体卡主体打开只读样片，独立“应用”按钮和 Shift+Enter 修改明确选中的可编辑文字/字幕片段；默认文本继续新增文字。资源预览不改变时间线选择。
- 应用复用 `opencut_text_plan` 的 edit 和原 `applyPlanned`，只选择字体绑定、字重和样式；保留内容、原始精确时间、位置、效果和动画。异步规划后再次核对固定修订和目标，重复点击不产生第二次提交。
- 加载期间不显示回退样片，失败有错误和重试；字体按连接、工程和修订授权，退出时释放。同修订返回相同元数据不重复加载。字体合成样式沿用既有证明和授权来源。
- 无兼容选择、多选、锁轨、历史版本或未完成文字编辑时禁用应用。错误和进行中提示区分普通保存与原字体导入；中文卡内动作、焦点和至少28px命中区已接通。
- 独立检查暴露 Vite 拒绝加载仓库外字体校验 Worker：默认文字已保存但预览报 `Font hash worker failed`。开发服务仅新增该 Worker 文件的读取许可；正式渲染和生产构建逻辑不变，刷新后已复查恢复。

主要落点：[字体卡](../../apps/web/src/editor/workspace/scene/text/font-card.tsx)、[文字应用](../../apps/web/src/editor/workspace/scene/text/workspace.tsx)、[样式](../../apps/web/src/editor/workspace/scene/workbench/workbench.css)、[中文文案](../../apps/web/src/editor/shared/messages.ts)、[开发服务配置](../../apps/web/vite.config.ts)。没有新增Rust契约、依赖或业务写入口。

## 验证与候选

|证据|结果与范围|
|---|---|
|受影响回归|5个测试文件、29项通过。覆盖默认文字、字体应用精确输入和原请求、双击、目标变化、锁轨、加载失败/重试、同修订刷新和已有画布/字体生命周期。新增加载失败属于组件故障注入，不是实际提供方失败验收。|
|类型与构建|Web类型检查、一次生产构建通过；构建仅保留原有大chunk提示。Web边界274文件通过。开发服务配置修正后无Rust重建，也没有重跑无关全量测试。|
|子Agent实际浏览器|单击/Enter预览、Esc退出保留选择；无选择/视频选择拒绝应用；Shift+Enter和独立按钮应用；锁轨拒绝；默认文本新增及Undo；1440×900和1280×720下卡片、长名称提示、焦点和模态框检查通过。窄窗口应用按钮42×28px。|
|真实文档比较|合成工程两种应用入口的变化均只有目标 `content.text.font.font_id`；Undo后完整文档恢复。默认文字新增为5秒且可撤销；Vite Worker修正后实际画面显示文字且无alert。|
|数据保留|复用 `agent-object-blur-v4-acceptance`，累计必要交互/撤销使事务6→16，最终文档SHA与初始一致。海南工程修订、文档和34条事务完全未变。没有另建工程副本或重新导入/ASR/成片导出。|

Native复用 `f744f89ee4efd89831823802c18df117502e7dd8f18b7f88d5bb90de986e4d75`，服务为 `http://127.0.0.1:59790`。本次交互检查使用 `http://127.0.0.1:5173` 的当前源码开发界面；生产Web构建另存在本机证据目录，不冒充已替换原59790界面的部署。

本机 [candidate.json](</private/tmp/opencut-jianying-font-20261024/candidate.json>) 保存源码、Native和生产Web文件身份；[verification.json](</private/tmp/opencut-jianying-font-20261024/verification.json>) 保存检查摘要；[子Agent检查报告](</private/tmp/opencut-jianying-font-20261024/review/review.md>) 保存截图、逐步原始快照与恢复比较。生产构建与检查对应同一字体产品源码；其后的Vite修正仅改变开发服务文件准入。已有候选及截图不覆盖。

![1280×720字体卡与键盘焦点](</private/tmp/opencut-jianying-font-20261024/review/final-fonts-1280-focused.jpg>)

## 保留范围

真实触控、读屏、系统IME和物理设备矩阵未验；实际字体断网/损坏重试、完整缺字与合成字体组合没有新增浏览器验收。字体卡当前使用独立预览弹层，中央预览上下文、字体拖放、多选共同属性按后续切片实施。没有改动渲染器，不将本项UI与事务证据写为新的独立像素、PCM、成片导出或持续播放验收；原严格质量、长素材和整迁移缺口保持。
