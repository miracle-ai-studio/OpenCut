# 历史选择保持与音量加载状态

2026-10-24，修复上一轮独立UI发现的两项问题。目标仍为原 jianying-zh 七图42出口；本切片不改变工程合同、渲染器或Native音频算法。

## 根因与适配

真实Undo后界面保留primary，内部多选集合却消失，首次Meta追加因此丢失原对象。源码根因在Session.planUndo/planRedo：规划前直接timelineEdit.invalidate()清选择；refresh本身已有目标过滤。Classic `core/managers/commands.ts:55–63`说明无显式选择意图的命令应保留当前UI选择，`update-elements.ts`无选择覆盖；`move-elements.ts`则返回实际目标轨道选择。这些规则复用到当前私有呈现状态，不新增持久选择模型。

历史规划改为invalidate(false)，仍取消旧规划并保留epoch/API/head围栏。刷新只在同一场景内按稳定element/caption ID从权威快照解析实际track owner，删除目标被剔除；主对象删除时回到幸存选择。跨场景/连接/历史导航继续清理。显式编辑请求仍走原严格track/id规范化，不接受错误轨道配对。原anchor因view/revision失效时，Shift范围以仍选中的primary为起点。

Undo/Redo在规划前、异步返回后都检查属性草稿；被拒绝时不规划/不提交，不切走Properties。工具栏草稿期间禁用历史写操作。普通冲突仍进入原历史审查，不改变Native Undo/Redo、幂等或恢复协议。

音量表单初次render时alive=false曾被reason当成目标变化。现在能力加载独立呈现中文“正在检查音量编辑能力…”，控件保持禁用；成功后正常启用，失败结束加载并显示原错误。真正保存与计划晚返回仍使用原严格reason，不放松目标/连接/修订检查。

## 源码与检查

证据目录 `/private/tmp/opencut-jianying-selection-20261024`，candidate.json固定747个Web源/测试文件、284份Web产物及原Native SHA。当前HEAD仍abd2c969，既有dirty树保留；没有重置其它改动。5173实际界面使用新源码，59790继续服务上一音效行生产Web。

受影响6文件65项与另5文件39项通过，合计11文件104项，无重复文件。覆盖保存/Undo/Redo后的cohort、首次toggle/Shift、删除primary与scene、跨轨移动/反向刷新重绑定、草稿前后守卫、迟到scene/connection、初始能力加载/失败及Properties留存。类型、一次Web生产构建、295文件Web边界、架构14项/22模块/0临时例外及合同边界通过。保留构建大chunk提示。公共合同未变，不重复生成SDK、不重编或重启Native。

原bf8f16e3 Native、PID95774/实例61b63401…不变。主代理基线UTC06:57:59：QA133/head013f1dd3/rawSHA768641c5…；用户49/head313459f0/rawSHA2c025848…；721终态Job/失败事件20957/最新拒绝96358，日志104511B/SHA42b874…全部保持。

## 实际增量验收

GPT-6.1-sol/high子Agent的 `selection-ui-review.md` 限定通过。三尺寸1280×720/1440×900/1920×1080均实际两选、ready、百分比/dB为“—”，初始化先正确显示能力加载且没有伪目标变化提示；关闭上一轮1280 ready混合值证据缺口。dirty期间Undo真实disabled，Esc后恢复，零规划/写入。

第二组历史操作写前硬断言两个准确音频ID、混合值/共同值、Properties和按钮可用。Redo后仍两选且共同200%，Undo后仍两选并恢复“—”，Properties持续可见。随后首次Meta正确移除music0，Shift恢复两项。原非末项primary配置和真实跨轨/字幕owner历史没有由这个路径构造，仍区分源码/fixture与真实UI范围。

**首组覆盖缺口保留**：第一次历史写前实际只剩music0，未验证脚本“普通点0已重置cohort”的假设；Redo成功后等待两选超时，马上Undo恢复。没有把它说成产品清空多选，也没有算作多选通过。为填补明确缺口，主代理授权第二组唯一Redo/Undo，先硬核前置条件再写，验证通过；四笔全部留存，无新音量规划/保存、媒体、Job或输出。

主代理 `main-journal.json` 独立检查四笔Native不可变修订：每次Redo精确引用前一个Undo、原生canonical目标仍为原multi forward txn_9bade5e25f4b47a1b4b345be0986d9f7，每次Undo base就是该次Redohead；只变四处key.value。Redo全文等于原已保存multi修订，Undo全文等于QA133基线。QA133→137最终head82ac5de6…/SHA768641c5d3d66c3d23a70dec21f6da797f6aaf1cb7ff37f6d770e2e3e67405a7精确恢复；用户49全文/head/事务不变，不用整版恢复。

## 日志、身份和清理

完整4891响应、4887个请求ID：4879×200、4×204、5×PREVIEW_CACHE_MISSING404、3×旧资源释放PREVIEW_STOPPED409。27个ERR_ABORTED保留，不逐条强行归因；零401/403/5xx、console warning/error及pageerror。尾窗8.005秒372响应：371×200+1缓存404、5个取消、0页面异常。没有故障route/gate。

`main-final.json` 和 `service-final.json` 确认同Native/PID/实例ready；747源码/284产物/Native SHA精确，721终态Job（621/67/33）及失败事件20957、拒绝记录均无新增；server.log仍104511B/SHA42b874dc2b5439a318adc2e250a10e10f452997dff97c0fdfe62af676d704485。只读status经沙盒外本机连接核对，没有重启服务。

原Chrome唯一QA页保留1200×959、无选择/时间0/声音停止，无playing/dialog/drag；runner92553退出0，监听清理。最终截图sol-final.png、独立源身份sol-final-identity.json和完整请求/错误均在证据目录。

本切片关闭已证的两选历史保持和初始化误提示；全七图42出口仍开放。非末项/非末次选择锚点跨历史、真实跨轨/字幕目标历史、更广多选和其它属性仍需按原设计核对；不能把本次Meta/Shift路径提升为全部选择语义通过。其它音量、音效行、字体、拖放、媒体、PCM和渲染性能复用各自候选证据，不重复或改标为本候选重新验收。
