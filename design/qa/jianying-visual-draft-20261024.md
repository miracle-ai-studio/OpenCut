# Native 单字段视觉草稿与不透明度滑块

2026-10-24，静态opacity滑块限定通过；其它属性与冷播放性能仍开放，后者见[诊断报告](jianying-preview-reload-20261024.md)。证据目录 `/private/tmp/opencut-jianying-visual-draft-20261024`，完整七图42项保持。

## 实现边界

原transform草稿公共入口统一为唯一 `opencut_visual_preview_edit_draft_frame`，显式typed intent区分原手势与单字段静态属性；SDK、工具声明、OpenAPI、Web canvas/worker消费者一并迁移，无并存后备入口。新纯契约与应用模块留在原crate，无新增依赖或allowlist项。

属性分支复用原公共事务规划、固定head与授权；只允许同一场景既有活动可视元素的指定字段变化，保留完整其它字段、channels、source与时间。请求字段已有非空动画时拒绝，原手势scale清理通道语义不变。Native沿用原暂停consumer、owner/lease/generation、单草稿门闩、快照→IR→后端及最终head围栏；临时图不写journal或新增Job。

首个UI只接不透明度range，pointerdown获取原表单owner，串行执行一个候选并保留最新意图；松手等待精确最终Native候选后只提交一次原prepared transaction/key。Esc、捕获丢失、blur/hidden及上下文改变取消零写，并等待真实原固定帧重新呈现后释放恢复态。保留其它字段草稿。没有CSS假预览，没有音频临时试听；旧宿主不支持该操作时不展示无效滑块。

静帧会话通过 `StillPreview.currentPaused` 提供原暂停owner；不伪造声音状态。只在声音paused且session非空时使用声音owner，停止后paused/null可使用实际Still owner。旧恢复排队与新草稿并发时序75→100恢复→40已有回归，旧原图结果会丢弃，新意图仍串行执行。

## 已运行检查

首Native候选a6102f857eb9a9780dd2c07eba18eb61fe24b45a51ca2afe940ad4c0b93aa815只集成构建一次（10分20秒）。16项应用transform/visual-draft回归通过；最初一项错误空通道夹具失败保留，改为保留合法非空无关动画并拒绝无效空通道，不放宽产品规则。同显式二进制生成与check、canonical 186工具/12领域、SDK构建、架构14项/22模块/0例外及契约边界通过。

Web最终集成六文件115项通过；新增paused/null回归在旧实现先失败，修复后Still/Visual与worker协议复核通过。Product/Player原17项中的超时按失败范围复验，最终history在原5秒期限内3.47秒通过；保留初始超时，不称一次全绿。Web最终tsc/Vite构建通过，源码752/Web产物284份由candidate.json记录。其初始状态是部署前记录，不能据该字段当实际UI已通过。

## 原a610真实UI失败

保留sol6.1/high在QA145、C01/time0/100%原Native图ready后，实际按住滑块从max向75移动。首95%候选被Native400拒绝，request ID `6b4b2fca-65bd-4386-8f43-5705c95374f4`，INVALID_ARGUMENT：`Visual edit draft requires the original paused scene and exact consumer time`。自动恢复原画面；0 apply、0事务、0新增Job，用户实时50保持。

同一Still owner的frame.time为0/1、consumer.presentation_time为0/48000。Host的两处比较用了RationalTime结构相等，其构造不会约分，因此等值时间被误拒绝。resolution省略默认Source并非该次错误来源。失败捕获和截图见 `sol-initial-ui-failure.md`、`sol-browser-initial.json`、`sol-held-failure.png`。3712响应、pageerror/console均0，body处理队列已排空；不把未读body写作完整。runner23515退出0，监听已卸，Chrome原现场保留。

修复将准入与发布两处围栏共用精确 `cmp_exact` 比较，非法输入关闭，仍拒绝一采样点偏移。定向host回归1项通过（另90项未运行），同显式新二进制generated check/canonical 186工具/12领域通过。Native增量构建22.31秒，未重建不变Web/SDK；候选 `candidate-time-fence.json` 为93a7e8ac9f570174558eae14045e714229191c576c2594ade5b95f47613ddd57，部署PID31538/实例d8d5705a-795a-4c3b-b915-9cf0c99cf303真实ready。旧a610二进制与首失败证据原样保留，部署前所有763Job均终态。

## 修复候选的实际增量

保留子Agent独占原Chrome：精确75%按住取得真实Native ready与1920×1080 PNG，Esc明确restoring后364ms原图/控件恢复（画面约65ms、控件idle约335ms），0 apply/工程写。随后只做一次75% Release及立即因果Undo，保存3128ms、Undo3632ms真实静帧ready，无reload或Play；文档ID、timeOrigin、NavigationTiming及canvas/transport/timeline三个DOM引用在取消/保存/Undo均保持，不能泛化为所有编辑动线已验证。

主代理独立SQL确认只有 `txn_f5d568570d854381baf06d516a540c71` 将C01.opacity 1→0.75，`txn_13f35c792fbb4b3991d865ca5c7802ce` 因果撤销0.75→1，均无其它字段变化；QA145→147后全文SHA768641c5…恢复，用户实时50全文/head/事务数保持。1920×1080持有及保存75%与独立原图RGB×0.75预期最大误差1、三通道MAE0.152/0.157/0.199；Esc和Undo RGBA与基线完全相同，见 `main-pixels-final.json`。

全部752 Web源码、943 Native/Cargo源码、4生成文件、284 Web产物及93a7 Native哈希再次核对一致。Jobs仍663成功/67历史失败/33取消、失败事件20957不变，没有新增缓存Job；服务升级后11:41:30新增3条authentication_rejected（96366–96368），早于11:42:42新浏览器捕获，不能称全期零认证错误或冒充已匹配请求。这些ID及服务日志SHA见 `main-final-sql.json`；`main_after_ui.json`在11:58:19Z再次确认工程、事务、Jobs、失败、拒绝高水位和日志长度均保持。最终显式服务状态同PID31538/实例ready。

三尺寸1280×720/1440×900/1920×1080 range均28px高、五点命中、页面无溢出。捕获5373响应（5366×200、2×204、3次cache404、2次旧owner释放409），无捕获期401/403/5xx、console或pageerror；22取消独立保留，5346 JSON解析/27个204或非JSON分支缺bodyRead标记，pending0不代表全部body检查完成。8.25秒尾窗81×200/0取消。最终1200×959/0选择/time0真实WebGPU ready，UI声音“已暂停”、HTML无播放；Node31591退出0，监听及MutationObserver卸载、Chrome保留。子Agent短报告 `sol-slider-ui-review.md` 与事务证明 `sol-fixed-transaction-proof.json` 为原始依据。

只关闭静态C01 opacity的按住/主动Esc/唯一保存与Undo、三尺寸几何；75→100恢复pending→40及其它失焦/捕获取消等分支只有相应源码回归而未实际UI验收，不外推其它字段、全产品或冷起播性能。
