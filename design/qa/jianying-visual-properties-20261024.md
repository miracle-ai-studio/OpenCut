# 共同画面属性与单字段草稿

2026-10-24，原七图05/07属性交互的一项增量实现与sol6.1/high限定验收。证据目录`/private/tmp/opencut-jianying-visual-properties-20261024`。整体七图42出口仍开放，不以本轮数字字段替代其它交互或渲染验收。

## 问题与复用

原多选面板只提供共同音量，单选画面属性仍是整表保存，无法按设计单独确认位置、缩放或不透明度。新的共同画面区提供X/Y像素、X/Y缩放百分比、旋转角度及不透明度百分比；只在所有目标都有visual时显示，多选混合值为“—”。锁轨不静默跳过成员，动画字段明确使用原关键帧编辑，未编辑动画与精度保持。原单选基础值/速度等完整表保留在高级片段属性，纯音频速度表继续可见。

复用唯一Rust `opencut_element_properties_plan`、typed SDK、原固定修订、shared recovery和原子apply；逐目标严格核完整expected element和操作scope后才合为一个事务。只做UI单位换算与预期字段核对，不新增领域模型、公共契约、依赖或Native执行入口。

本轮没有新增不透明度滑杆：当前typed transform draft只接受明确变换手势，不能把opacity伪装成translate；其scale手势还具有显式动画清理语义，不能冒用为本轮保留channels的数字字段。真实原生opacity草稿预览仍需共享语义后续，未用CSS视觉替代。原Classic三个保留路径本轮已不存在，独立报告明确保留这一复核限制，不把恢复worktree旧迁移UI当原Classic。

## 草稿生命周期

Enter只确认当前字段、Esc只还原当前字段，保留其它字段原字符串。稳定cohort组件只在当前head等于自身已知回执revision且完整目标等于计划时采纳新baseline；外部head或自身提交后的他人后继head均冻结剩余草稿，不重基。原applyPlanned先释放owner再refresh，故已知保存完成后显式恢复其它字段的owner。

未知结果交给精确原request/key的durable recovery；durable阻塞期间让出本地owner，恢复ack后未提交字段重新获得选择/写保护但仍冻结，逐字段还原后才能跟随权威快照。输入错误及动画提示与字段关联，初始化独立显示能力加载，不误报目标变化。

## 已执行检查

六文件69项通过，包含新控件18项及相关旧属性、音量、Product、选择历史和草稿回归。覆盖混合值、未动精度、负缩放/不归一化旋转/百分比单位、双字段先后提交、逐字段Esc、第二目标错误计划原子拒绝、外部head、外部后继head、断开迟到、原请求未知恢复和恢复后owner，以及动画/锁定成员。初始相关回归中的一个测试包装错误保留，修复为保存绑定的真实方法后重跑受影响组，不当作产品缺陷。

初次Web构建含类型检查，297文件Web边界通过；既有大chunk提示保留。实际1280截图发现画面区被旧视频音频说明挤到下方，因此单选Inspector只调整一行顺序为画面优先，再做一次必要Web构建。初段零写入、无草稿后才改源码；原候选/产物/截图/捕获分别保留`candidate-initial.json`、`web-dist-initial`与`sol-browser-initial.json`，不把两段混为同一运行身份。最终`candidate.json`固定750源码/284产物，原bf8 Native未重编重启；5173实际新UI，59790仍保留前音效行生产Web。无模块/协议边界变化，不重复旧Rust、PCM或媒体输出验收。

最终排序变更后只复验受影响Product：12项直接通过，首项触发原5秒超时；同一用例保持门槛隔离复核3.21秒通过。超时保留，不强归因构建竞争。其余五文件56个对应行为的form/helper/test源码均未改变，按原检查范围复用，不相加成重复测试总数。

随后源码复核发现干净控件在history规划失败、只变viewGeneration而head不变时会持续禁用；新增最小回归先在旧实现失败，保留`pristine-view-regression-before.log`。仅对无草稿且非in-flight的baseline同步补generation/viewGeneration观察；save-time固定视图检查不放宽。最终组件19项通过，总计70个唯一用例覆盖，按各源/阶段保留，不重复相加。为该实际缺陷再做一次必要Web构建；之前已通过四笔写的候选/源码/产物另存`candidate-before-view-fix.json`及`web-dist-before-view-fix`。最终组件与该写候选的变化仅为pristine同步与对应回归，不重新执行相同写动作。

## UI、事务与日志

保留sol6.1/high为唯一浏览器验收者，复用原QA139与既有媒体，限定通过，见`sol-visual-ui-review.md`。三尺寸1280×720/1440×900/1920×1080，单选六字段置顶、右栏scrollTop0和命中通过；多选Y为“—”、动画不透明度禁用且有对应提示。真实高级表可打开、纯音频速度仍可见。

单选C01输入X32/Y72，Enter X只发一次计划/保存；自身修订后焦点回X，Y72草稿保留且Undo禁用。静帧3.185秒ready，无reload；Esc Y还原0，零额外计划/写入；因果Undo后静帧3.506秒ready。多选C01与travel标题，横向缩放125%通过两份严格计划、一次原子保存；Y0/350保持混合，原动画保留，焦点回缩放；静帧2.790秒ready，Undo后4.053秒ready。这是同次观察的DOM/运行时就绪与交互证据，不是独立像素oracle或通用冷性能通过。

主代理`main-journal.json`独立确认四笔事务：single只改C01.position_x_px一处，multi只改两目标scale_x两处；每个Undo的目标、expected/base及完整文档均准确。QA139→143，原全文SHA精确恢复；用户49的head/全文/事务保持，721终态Job/失败20957/拒绝96358及104511字节日志SHA保持，同实例ready。见`main-final.json`与`service-final.json`。

最终候选另做零写增量：仅一次QA浏览器精确undo_plan本地模拟503，不转发Native，不拦apply/recovery。真实Undo失败后，Properties/C01/head保持，六字段782毫秒内恢复可编辑，无reload或重选。撤除route后Y72草稿使Undo禁用，Esc还原0并恢复Undo；零property plan/apply/undo apply/Job。不能把这个模拟503当作真实服务端故障。

三个捕获阶段按候选分别保存；四写阶段5400响应头含7个404、3个旧释放409，24个取消，零认证/5xx/页面异常。其53个body parse_failed含二进制与导航后CDP不可取，4个timed_out为HTTP200；所有非成功错误体和四笔写回执已收取并有独立SQL证明，pending处理链为0不等于所有body完整。完整初段/四写段/最终零写段统计及最终清理以独立报告与`main-browser-summary.json`逐段记录，不伪造全body收齐。

初段2673响应头中157个body超时和3个解析失败保留。最终零写段2550响应头为2543×200、4×204、2缓存404和唯一浏览器模拟503，17个取消；唯一控制台错误对应该503，零页面异常/认证/其它5xx。18个二进制body明确未按JSON解析；8.334秒尾窗379×200+1缓存404、5取消。三段统计不混作同一产品身份。

最终原QA恢复1200×959、0选、时间0、声音stopped、无播放媒体/对话框/拖放；`sol-final.png`与final snapshot明确为WebGPU画面ready。失败规划截图当时仍preparing，只证明控件恢复；不把它当ready截图。route及监听已撤销，runner52474已退出，原Chrome保留。主代理再次核对750源码/284最终Web/原Native SHA精确，并确认服务同实例ready与日志全文SHA保持。

完整文字/图形/蒙版/源范围等草稿、Native数字滑动预览、真实跨轨与原七图其它出口不因本轮数字字段而关闭。

本轮真实保存只覆盖X及横向缩放；Y、纵向缩放、旋转、不透明度的其它单位/写入分支仍按协议fixture与原Native端口的既有范围区分，不将两组实际写外推为六字段逐一保存。特别是数字opacity的transform=null分支应在后续必要增量实际保存/Undo核对，再进入新的共享草稿预览工作；不重做已通过的X/scaleX事务。
