# 历史操作后的范围选择锚点

2026-10-24，原七图42项出口的一项增量修复。证据目录：`/private/tmp/opencut-jianying-anchor-20261024`。整体工作台验收仍开放。

## 实际缺陷与修复

先点击文档顺序靠后的b，再用Meta加入靠前的a，当前cohort按文档顺序为[a,b]，范围锚点为最后点击的a，而当前primary仍为b。原锚点身份包含head revision和view generation，保存、Undo或Redo后身份失效，下一次Shift点击c错误地使用b，得到[b,c]并漏选a。原有显式`selectMany(..., "a")`测试不能覆盖这一实际点击顺序。

新增三个回归先在旧实现上分别证明save、Undo、Redo均失败，保留`regression-before.log`。修复只调整UI范围锚点：同API/连接generation/工程/场景下保留意图，历史只读快照仍绑定其revision；使用当前cohort的稳定元素ID解析真实轨道。锚点删除、选择清空后不能复活；不放宽命令的strict track/id、固定head/view、原子写入、授权或未知结果恢复规则。实际导航仍主动清理锚点。

## 已执行检查

- 四个受影响Web测试文件48项通过，覆盖三个原失败路径、跨轨后的锚点owner、锚点删除及空选/书签/切场景/退出历史清理；既有迟到计划、草稿及连接围栏继续通过。
- 一次最终Web构建包含TypeScript检查，295文件Web边界通过；既有大chunk提示保留。
- 私有Web选择逻辑变更，无公共契约、依赖或模块边界变化；不重编/重启Native、不重跑旧PCM或媒体输出验收。
- `candidate.json`固定747源码和284份构建产物，Native SHA仍`bf8f16e3ab6111e7879610b059b636b37bd775435505ec44ac1f958327f8f596`。实际UI为5173，59790继续保留前音效行生产Web。

## UI与服务核对

独立sol6.1/high限定通过，见`sol-anchor-ui-review.md`与`sol-anchor-static-review.md`。原Chrome9223唯一QA页、1440×900下，使用真实鼠标b-only→Meta a，逐步硬断言两选前置；一次Redo后Shift c得到[a,b,c]。通过现有时间线焦点与Escape清空，再实际重建b-only→Meta a；唯一Undo后Shift c仍为[a,b,c]。前置两选字段ready；三选截图在能力初始化中，不据此声称三选音量ready。没有私有选择注入或强制穿透点击。

两次验收器坐标保护分别发现长片段被sticky header挡住、滚动后布局尚未稳定；错误与截图保留。改为读取可见交集和实际hit-test、有界等待命中后点击，没有重放已成功的Redo，不归为产品锚点失败。

主代理独立`main-journal.json`确认恰好两笔原链事务，每笔只改4处gain key.value，canonical目标/expected/base均准确；Redo全文等于原保存版本、Undo全文等于本轮基线。QA137→139，原工程全文SHA精确恢复；用户49的head/全文/事务保持。原生服务同实例ready，721终态Job、失败事件20957、拒绝96358及104511字节日志SHA均保持，零新Job。

捕获5327响应头：5317×200、4×204、4×缓存缺失404及2×旧资源释放409；54个ERR_ABORTED完整保留，不统一归因。已记录响应无401/403/5xx、控制台或页面异常；8.081秒尾窗363×200+1缓存404、3取消。最终序列化时2条body处理链尚未计数回落，无法精确定位或证明最终完成；所有错误体、选定历史回执均在，实际两笔修改还由显式读取回执和SQL交叉证明。保留原捕获，见`sol-pending-bodies-note.json`，不宣称所有body均收齐，也不为此重跑已过交互。

最终原QA页恢复1200×959、无选择、时间0、声音stopped、无播放媒体/对话框/拖放；DOM及网络监听已移除，runner20397已退出、原Chrome保留。主代理另验证747源码/284产物/Native哈希精确，见`main-final.json`、`main-browser-summary.json`与`service-final.json`。

## 后续范围

真实跨轨/字幕owner历史和其它选择配置仍须按实际覆盖区别记录。下一项回到原步骤二：按能力提供共同画面位置、缩放、不透明度等属性，Enter只确认当前字段、Esc还原，多选混合值为“—”；保留每个目标未编辑字段的完整精度、动画、source与时间语义，逐目标严格验证现有属性计划后一次原子apply。原七图其它出口不因本次选择修复而关闭。
