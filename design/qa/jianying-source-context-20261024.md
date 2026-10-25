# 中央源素材、选段与播放上下文

状态：sol6.1/high独立限定验收通过，覆盖中央上下文、源I/O、正常播放互斥及新增生命周期问题的定向修复。完整42条出口继续开放。证据根目录 `/private/tmp/opencut-jianying-source-context-20261024`；修复前`review/review.md`保留原异常，修复后见[定向复测](</private/tmp/opencut-jianying-source-context-20261024/review/retirement-review.md>)。未重做原Add的同一精确异步交错，故障下播放取消另有单测，不提升为全部故障UI验收。

## 实现与复用

- 源素材、特效和转场示例显示在中央播放器，标题区明确当前上下文并提供返回。时间线画布保持挂载和尺寸，隐藏时不参与焦点与指针交互；返回保留工程播放头、选择与画布视图。示例继续明确标识为样片，不冒充工程渲染。
- 源视频与音频复用原媒体播放器，拥有独立源游标；I/O标记、精确秒数输入、Enter确认和Esc还原只改变本地选段。区间须满足 `0 ≤ In < Out ≤ 源素材时长`，未确认草稿、未知时长、历史或恢复状态阻止添加。音频波形消费原授权SourceArt端口。
- 添加时明确目标轨道和工程起点。选段沿用精确有理时间与原 `media.place → source_range → edit_apply`，保持固定修订、原子提交和原请求恢复；碰撞按选段时长判断。没有新增工程/时间模型、公开DTO、SDK或Rust实现。
- 中央源播放和目录试听先确认原时间线声音暂停，之后才允许HTML源播放；不同源互斥，取消或换场景拒绝迟到启动。暂停结果为unconfirmed/failed时拒绝新播放，不把未确认状态当成暂停成功。全局播放键按中央上下文路由，时间线操作会退出源上下文。

## 候选与检查

源码基线 `abd2c969b4b48067a85f7020d84b04fbb48a2d38` 加既有未提交修改。原`candidate.json`/`web-dist`保留修复前身份；生命周期修复后`candidate-final.json`记录20个源/测试文件和284个`web-final`生产文件SHA256。UI在5173当前源码，构建没有安装到59790的生产Web。Native沿用 `8b195d83d925f53ff40bdd981566e508cbb6743c555c209a92af58234f951728`，本轮只读status确认ready=true、instance `9c3fcc54-6232-411b-ba60-1bcf045fb17f`，没有重新构建或重启Native，共享renderer生产源码哈希也保持不变。

|检查|证据与实际范围|
|---|---|
|源范围、共享播放器、所有权|`source-final-tests.log`：3文件9项通过；之后新增暂停未确认拒绝检查，`session-ownership-tests.log`的所有权4项通过，与前轮重叠|
|会话生命周期|`session-ownership-tests.log`：会话7项通过；覆盖拒绝后停连接、显式重连、不重放写入及退出/启动竞态，保持原Bearer权限边界|
|工作台集成|`product-final-tests.log`：完整11项通过，包含中央源与画布保留。原首测试全DOM查询超过5秒，修正为对应tablist内查询后全文件通过，未放宽时间或删除断言|
|类型与生产构建|最终`build-final.log`通过，含tsc与Vite；修复前`build.log`保留，两个构建均保留大chunk警告，不以构建推断播放性能|
|Web边界|最终`boundaries-retirement.log`：284文件通过；原`boundaries-final.log`保持原检查身份|
|资源关闭修复|真实Mediabunny复现`RESOURCE_SOURCE_CLOSED`未处理拒绝（`retirement-red.log`同页面栈）；Web单帧改用既有prefetch=false，保留512KiB与两行readAhead/预算，不拦截全局错误。`retirement-green-verified.log`4项验证64KiB/512KiB按需精确字节、无额外后台读取、前台关闭拒绝及活跃鉴权/哈希错误仍向调用者传播；`retirement-web-tests.log`23项及`build-final.log`通过|
|中央源与三尺寸|独立`review/context.json`及`source-*.png`：1280×720、1440×900、1920×1080无页面溢出，I/O和Add可见；原14秒海湾视频源scrub不改工程0秒，真实I/O键得到1.4→12.6|
|返回与样例|同一editor-canvas DOM；按钮/Esc后C01、工程0秒、75%zoom及pan(40,25)保持；特效/转场主体进入中央明确样例且没有Modal，返回视图不变|
|非法范围与草稿|独立`review/ranges.json`：负入点、14秒源出点15、反序、零长度均拒绝且Add禁用；8秒源出点10也拒绝。字段Esc仅还原当前字段，保留中央源和C01；主Agent确认首个成功事务前没有这些操作产生的写入|
|唯一真实插入|`txn_b9263b047aaf494cade7cf35a6b45822`，request `161d08f5-5b43-4be0-9275-0a3e01f0fe32`；新轨一个元素，timeline0/8秒、trim_start2/trim_end4/source_duration14。主Agent`applied-transaction-proof.json`移除唯一新轨后全文档等于原基线；单测15秒协议fixture不冒充本次14秒真实素材|
|因果Undo|`txn_42f18d528f9540dd90315c930c047004`的原选择性Undo请求精确引用上述添加；主Agent`undo-independent-proof.json`确认完整工程哈希恢复，没有整版覆盖旧head|
|Native与源声音互斥|`review/audio.json`：Native实际playing/设备时钟后开42秒音频源，暂停API确认先于HTML play；源Space推进0→0.143973→0.251755时Native保持paused，返回原工程transform31.5733px保持且旧源paused/unmounted。退出源后恢复audio-only，再Space暂停Native，证明键盘路由回时间线|
|目录试听与Native互斥|click1试听真实时长0.096735秒，0.014828秒时切Native；HTML在0.068804秒暂停且ended=false、connected=true，Native进入playing后仍保持该位置，不把自然结束或卸载当互斥证明|
|音频检查的范围|首次audio-only自然结束42秒，设备2016000帧/underflow0；脚本准备晚而错过playing切换窗口，随后正常重播补互斥覆盖，非产品失败。音频源是隐藏video节点，读取实际paused/currentTime，不用可见性断言。未测试物理听感、全声画或冷性能|
|生命周期真实复验|新分段`retirement-browser.json`：实际历史124→当前125、两轮0↔6.4秒、源打开/返回均ready；9次实际帧截图，相同位置SHA稳定，不外推为全渲染oracle。原写事务不重复；QA125/用户46不变。最终stopped，尾5.001秒61响应全成功，无新增pageerror/console/401/403/500，客户端退出且原Chrome/QA页保留|

## 工程与日志

实时独立基线见`before-ui-summary.json`：QA123事务、用户46事务。唯一添加和Undo后QA125事务，新head `rev_fa7cf8ccce61389b945f0ece68a6f5afc9899b8c78f685e31d85ffe06674178d`，全文档SHA256精确回到 `768641c5d3d66c3d23a70dec21f6da797f6aaf1cb7ff37f6d770e2e3e67405a7`，保留正常前进的历史。用户46事务、head `rev_c951f0e687a491f4d6ca560ed388d98a0bd8ceabccb31615a3dd3d9a6e149973` 和完整文档SHA256 `2c52e4f15ed1b31ad36d13525682d7e10af096022ff431f96d5cff1649cf7018` 精确不变；没有导入媒体或新建工程。

修复后的日志复查同时读取 `service/server.log`、`denial_records` 和 `job_events`。鉴权拒绝最大序号仍96337，最后UTC `2026-10-23 19:31:23`（北京时间03:31:23），与服务替换吻合；最新failed事件仍20957/UTC19:31:50，Job状态558成功/67失败/33取消。`A valid bearer token is required`表示未通过鉴权，不表示工程损坏；本地HttpOnly grant八小时过期或服务重启会失效。现有恢复停止旧连接请求，中文提示显式重连并恢复原路由，不自动重放编辑。

原13961响应记录没有401/403/500，但保存后UTC22:07:37.464有一条`Verified source capability closed`的pageerror；这是已完成单帧仍有Mediabunny推测预读，在旧源关闭时产生无人消费的拒绝。修复仅调整Web按需读取，没有修改共享renderer、Native或全局异常处理。原`main-after.json`/`review/browser.json`失败保持，真实读取器复现及修复检查分别见`retirement-red.log`/`retirement-green-verified.log`。新分段3088响应没有pageerror/console/401/403/500，当前0秒海湾与6.4秒人物真实画面可见。主Agent目视截图并独立复核`retirement-main-after.json`：工程完整恢复、0新增Job/拒绝/失败事件、当前20源哈希无漂移；`service-status-final.json`确认相同Native实例ready=true。

日志中的404 `PREVIEW_CACHE_MISSING`仍表示缓存未命中。旧`visual_preview_resource_release`另有404 `NOT_FOUND`及409 `PREVIEW_STOPPED`/`PREVIEW_GENERATION_CONFLICT`：主Agent核对Native的session存在、固定scope和generation检查，分别对应已移除/停止/代次变化的旧资源清理，被围栏拒绝而不操作新会话；不是当前读取或鉴权失败。原状态码、body及request_id均保留。成功项目/资源读取和释放错误逐ID查询拒绝表均为零；没有以保留行数代替审计序号。鉴权修复六个相关源码哈希与前轮一致，见`auth-source-identity.json`。

旧 `job_616901b441f2470cb5e87abcf8290b80` 的 `AUDIO_HOST_FAILED: Audio runner input closed` 与鉴权不同：原事件在UTC19:29:12，源码在音频子进程输入write/flush失败时生成该错误，现有持久记录没有底层I/O或退出原因，根因继续开放。`service-error-audit.json`保留该事件链，不把普通重载正常当作音频故障已修复。已有冷起播性能、晚回提示浏览器分支及其余矩阵出口也不由本切片关闭。
