# 简洁音量、多选和属性草稿围栏

2026-10-24，范围是 jianying-zh 图05/07 的音量属性切片。原七图42项出口继续开放；本轮不新增渲染器、Rust公共合同、素材或导出。

## 实现和复用

单选和多选显示百分比/dB及静音。两种数值对应同一实际增益，保存时保留每个元素原单位；0 dB不会被解释成线性0。混合值显示“—”，滑杆不伪报0，静音复选框使用原生混合态。未修改的显示精度不提交，也不量化原始精度。

普通增益/静音复用 `opencut_element_properties_plan`；已识别的简单线性淡入淡出复用 `simpleFades` 和 `opencut_keyframe_plan`，只改峰值，保留底值、key ID/时间/segment、原base gain及其余字段。自定义曲线仍进入原关键帧编辑。不能将包络压平到floor；校验直接说明应使用静音，且关联字段ARIA。

每个多选目标先独立规划并核对完整预期对象、固定head、API/连接身份及完整选择集合；全部成功才将不重叠的element_put合成一个原子事务。任何目标错误、锁定、计划污染或迟到响应都不允许部分提交。最终授权、聚合验证、幂等和持久恢复仍归原Rust应用与共享写端口。

音量数字Enter确认/Esc还原，滑杆仅在松手提交一次。这个控件提供**视觉草稿**，没有调用Native临时音量试听端口，不能称为已试听临时音量或PCM等价验证。原轨道普通dB音量线保留自身既有Native路径。

新私有property owner接入controller.acquire及资源拖放/使用/轨道编辑入口。表单只能用自己的owner提交；旧cleanup只清自己的token。普通属性、淡入淡出也登记草稿；保存一个fade后保留另一个未确认字段。取消速度草稿同时复位RateInput的独立显示。音频元素隐藏旧重复增益控件后，速度和保持音调入口仍可访问。外部head已被刷新观察到时，普通属性表单保留本地输入并提前阻止规划；新的草稿保护提示直接显示中文确认/取消指令。

## 独立审查和修复

原GPT-6.1-sol/high子Agent只读检查发现5项：隐藏gain连带丢retime、未知结果被自己的dirty owner阻塞、旧闭包清掉保存焦点、轨道音量线绕过选择保护、校验/ARIA缺失。各项已修复并增加有行为意义的回归。

未知结果只有在确实已有**精确同一原请求**的持久恢复记录时才移交并退役本地owner；规划失败或未stage成功不清草稿。新增过宽的recoveryId acquire守卫曾使旧AudioGain原请求重试3项失败，已撤下；保留原EditRecovery.execute的严格context校验。真实WorkspaceController测试证明无关新key拒绝，原key重试成功，不用取消草稿绕行、不自动生成新事务。

保存焦点通过实时controller snapshot核API/完整scope/revision，不使用旧render闭包。轨道音量线UI与AudioGain控制层都拒绝未确认属性草稿。校验文字直接可见，invalid、mixed与custom状态分开。

## 检查与候选

证据目录 `/private/tmp/opencut-jianying-volume-20261024`；candidate.json固定746个Web源码/测试文件、284份生产文件及Native身份。5173实际开发界面消费该源码；59790仍服务上一音效行生产Web。本轮没有重启或重编Native，保留 bf8f16e3ab6111e7879610b059b636b37bd775435505ec44ac1f958327f8f596、PID95774/实例61b63401-6de4-4883-9002-61a9368ac2c8。

受影响12文件93项、相关15文件145项均通过（淡入淡出文件重复覆盖，不相加）；速度取消补充回归3项，以及最终提示/外部head两文件8项均通过（与前述有重复，不相加）。类型、Web生产构建、295文件Web边界及架构14项/22模块/0临时例外、合同边界通过。公共合同没有改变，不重编Native或重复SDK生成。保留旧初始失败、修复日志与构建大chunk提示。必要Web修复构建分别对应速度取消显示、草稿提示/外部head改动；没有为纯验收器断言修正重编Native。

独立实际UI结果见 `sol-volume-review.md`、`sol-browser.json` 和 `sol-transaction-proof.json`。三尺寸数字30px/滑杆28px/静音label28px（checkbox图形14px），未修改确认零写，四种非法数值具体中文及ARIA，Volume/淡入淡出/普通属性草稿阻止选择、Add和起拖，取消后恢复可用。实际速度2→Cancel→1，无写入。

单选真实150%保存后显示3.522dB，焦点回原百分比框；多选拖动多次期间零规划/写入，松手后两个target规划、一次apply，显示200%/6.021dB并回原焦点。两组保存各做精确因果Undo，不做整版覆盖或Redo；首组因验收器定位等待延后，第二组forward响应到Undo响应约4秒。主代理 `main-journal.json` 独立读Native不可变修订：单选只变两个峰值key.value，多选只变四个key.value，两个Undo的target均指前一个forward并恢复其前全文。QA129→133后全文SHA仍768641c5d3d66c3d23a70dec21f6da797f6aaf1cb7ff37f6d770e2e3e67405a7；用户工程49的head/文档/事务完全不变。

**真实未关闭问题**：多选表单初始化时，`reason()`曾在alive=false阶段短暂显示“声音目标已变化，请重新选择片段”。1280/1440混合值截图属于该能力/生命周期准备期，不能算ready可交互证据；1920混合态ready和1440实际批量保存已验，1280ready混合态仍未验，暂态时长未测。三尺寸单选ready布局证据保持。

Undo/refresh清空内部timeline selection cohort，却保留primary selectedId和视觉选中态。因此Undo后首次Meta追加只得到新单选；多选Undo后属性退为primary单选，未保持两选混合值。这不能归为验收器错误。本轮用先普通选择再Meta明确两目标完成音量原子保存，但没有修复选择一致性；完整七图和完整多选交互未通过，下一切片先修该入口并复用本轮已通过的增益/拖放/媒体证据。

初始接入空白图是正式workbench准备前的瞬态；验收器CSS[type=text]误查默认text RateInput，以及用“撤销”误查实际“撤销上一步”的等待均保留。多选前提/Undo后混合值的等待则保留为上述真实选择缺口，二者不混写。没有为这些验收器等待重提保存。

最后8.078秒尾窗387响应：386×200、1×PREVIEW_CACHE_MISSING404，3个取消及零页面异常。完整捕获27417响应：27402×200、6×204、6×缓存未命中404、3×旧visual resource释放的PREVIEW_STOPPED409；零401/403/5xx、console/pageerror。39个请求取消原样保留，未逐项强行归因，也不写为HTTP全绿。末态唯一QA页1200×959、素材tab、选择0/时间0/声音停止，无dialog/drag；自有runner退出0，监听/路由/gate/计时器已清理。

## 认证和服务日志

本轮初始只读审计为UTC05:37:46（北京时间13:37:46）。最新拒绝仍96358，8条96351–96358均在UTC04:32:41–42上轮管理服务重启窗口；之后没有新认证拒绝。服务日志仍104511字节，SHA42b874dc2b5439a318adc2e250a10e10f452997dff97c0fdfe62af676d704485。721个Job全部终态：621成功/67历史失败/33取消；最新失败事件仍20957。QA129/用户49的head、文档SHA和事务数保持。

主代理最终 `main-final.json` 和 `service-final.json` 再核对：四笔必要QA事务后文档恢复、用户49完整保持；721终态Job/失败事件20957、拒绝记录和日志全文SHA均不变，同Native/PID/实例ready，746源码/284产物/Native哈希精确。

沙盒内status因loopback EPERM报告unhealthy，这是检查本身受限；越沙盒只读status确认同PID/实例ready，没有因此重启服务或重复提交操作。

`A valid bearer token is required`是认证拒绝，文案也用于失效本机Cookie，不代表工程损坏。本机grant内存保存、8小时固定到期，重启使旧Cookie失效；用户原粘贴缺请求ID，不能精确归到某条日志。原认证修复与一次真实重连证据见[会话报告](jianying-auth-recovery-20261024.md)，本轮不重复造失效。完整冷播放/加载性能、全属性草稿、多选视觉参数及原七图其余出口没有由本切片关闭。
