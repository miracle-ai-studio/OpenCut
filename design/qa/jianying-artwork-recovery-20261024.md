# 源缩略图、波形恢复与会话日志复查

当前候选 `candidate-latest.json`，证据根目录 `/private/tmp/opencut-jianying-artwork-recovery-20261024`。原失败经修复后，独立sol6.1/high对实际恢复、动作隔离、源预览和三尺寸卡片限定复验通过，见 `review/sol-repaired-review.md`。1280完整音乐故障行的命中、极窄提示完整发现性仍未验，七图42条出口及Native预算根因继续开放，本文不代替全工作台、冷播放或渲染质量验收。

## 原因与实现范围

`A valid bearer token is required` 表示请求未通过鉴权，不能解释成工程损坏。原本地 HttpOnly grant 固定八小时到期，服务重启后原内存 grant 也失效。最近六条拒绝为审计96338–96343、UTC23:08:51–53，时间与已记录的服务重启吻合；这只能归因已保留的旧会话段，不能证明任何今后同文案错误必然由重启引起。原会话修复会停止旧连接请求，显示中文失效提示和重新连接入口，显式连接后恢复原路由，不自动重放编辑，也不把显式 bearer 降级成 owner Cookie。本轮 controller/错误提示/Native鉴权源码身份与原验收一致；顶栏虽另有改动，其错误与重新连接块逐字符一致，见 `auth-reconnect-proof.json`。

前轮另有5个真实 `SOURCE_ART_BUDGET_EXCEEDED`422，属于源图读取预算拒绝，不能混为鉴权失败。已核对Native有工作内存、缓存条目、并发准入等同码路径，具体触发预算仍未证明，未扩大预算或并发、重启抹状态。原始事件继续保留在导出UI证据中。本轮补的是界面恢复和生命周期，不声称解决了该原生预算根因。

源图读取错误原先压成字符串，只显示不可用。现在保留错误码、HTTP状态、服务request_id和原始message；会话失效、权限拒绝、预算及其它读取失败分别给中文提示。授权失败不提供解码重试、立即隐藏旧图；一般失败提供手动重试，失败后不自动无限重试。重试仅对原固定工程/修订/素材版本/query申请新读取ID，复用原lease、release、cache与取消确认信用，不刷新整个工作台、不清全局缓存、不提交媒体或渲染Job。重复点击及旧API/修订/尝试的迟到事件不能启动新读取，旧响应不能显示到新目标。

资源卡重试与片段选择按钮同级，使用原生button、局部pointer-events和已有no-gesture路径。错误态隐藏整图预览覆盖层，在同一错误动作行提供原setPreview回调的眼睛入口；文案限定两行、动作28px，总高68px。卡片捕获真实pointer起点，来自错误动作行时即使原生dragstart转向父article也拒绝，不只依赖子节点draggable=false。其它区域仍保留原资源拖动。

时间线错误层按排除固定轨道栏后的真正可见区定位，取当前轨道header实际right，不硬编码宽度，独立于Native音频overscan投影。28px错误子层允许显示完整控件，但保持原source-art测量ref高度和原query，避免因高度变化自动重新读取。按钮距两侧8px，视频可见宽至少60px、音频88px才放重试/技术动作，保留窄片段修剪和音频操作空间；更窄时通过可hover/Tab的ⓘ显示中文放大提示及原错误码/ID。授权拒绝不附加放大重试提示。实际命中、裁切和拖拽以独立UI结果为准。

首轮实际UI失败完整保留在 `review/sol-review.md`：横滚后按钮被固定轨道栏遮住、卡片按钮被整图预览层盖住且略移触发父卡拖动、28px按钮被24px艺术区裁切、1920卡内错误88.6px超过72px、19px窄可见区的中文建议不能从可hover入口发现。首轮真实键盘缩略图和鼠标C02缩略图恢复已通过，不重复新建媒体或渲染；本轮只修复这些失败及补波形手动恢复。原一次模拟422与17条取消请求保留，不能把修复后的结果追认给原候选。

## 新捕获的生命周期异常

初轮只读普通reload、尚无故障注入时，子Agent在UTC00:47:00.598捕获真实pageerror `Open an editor project first.`，完整栈指向 `Media.active → refreshAvailable → media-panel passiveEffect/reconnectPassiveEffects`，见 `review/sol-initial-review.md` 和 `sol-initial-browser.json`。该段存在HMR更新，不能只凭时间证明触发来源，也不能将它混为bearer拒绝或说全段零错误。

源码确认只读refreshAvailable在try外调用严格active()；旧render的effect可能在host已卸载时抛出未处理Promise拒绝。已将被动读取限定到原API/工程：无host或目标不匹配直接退出，panel传捕获的目标；真正已发起读取失败仍通过当前media.error呈现，后续响应保留原请求次序/代次围栏。修改不放宽任何写操作的active检查。新增断连/错API/错工程/当前有效读取测试，不用吞全局pageerror代替修复。

## 检查与候选身份

4文件38项通过 `final-tests.log`，覆盖原取消确认信用/授权撤销与旧响应，以及手动重试去重、原固定请求、新请求ID、failed probe恢复、可见窗移动与窄片段、断连读取围栏。最初技术详情误加role=img影响原图像数量断言，已删除该角色、保留原断言，原失败日志保留。窄片段提示限制到retryable后，权限用例定向1通过/11跳过，见 `permission-hint-test.log`。实际UI暴露的遮挡/裁切/父拖动/提示不可发现问题修复后，3源图文件27项通过 `repaired-art-tests.log`，保留原12项Media读取证据，分轮有重叠，不相加。289文件Web边界通过。随后只改CSS：将波形样式收窄到直接投影SVG，避免错误眼睛图标染色；错误动作行预留右侧原Add空间，避免ⓘ覆盖添加。全部非CSS源码哈希相同，行为证据沿用，三尺寸样式及命中另行实际补验。最终类型/生产构建 `web-build-accepted.log` 通过；一次错误工作目录npm未执行构建的日志另留，不作为产品失败。打包大chunk提示保留，不是构建失败。

Native仍 b3f2a78629fb37fccbc53579f35eefc28ad506c2bebdbd90ed128490d172a7f8、实例bd7891f2，未重建/重启，无公共DTO/依赖变化。5173源UI实际验收，284个生产产物留在 `web-dist-accepted`，未安装到59790。14源码与当前树精确匹配，见 `main-candidate-proof.json`。沙箱status首次因回环EPERM显示unhealthy，原结果保留；授权本地只读复查 `service-status-live.json` 和收尾 `service-status-final.json` 均显示实际ready=true，不能将工具隔离当服务故障。

## 实际交互与服务端复查

北京时间09:50–09:53再次只读复查会话错误，证据根目录 `/private/tmp/opencut-jianying-auth-recheck-20261024`。原Native b3f2a786/同实例实际ready=true，14项已验源码哈希一致，未改产品代码、构建或重启；原会话生命周期检查沿用相同源码证据。本次不制造会话过期，只复核当前正常QA页的一次普通reload：791响应（789×200、2×204）、776个唯一request_id逐一查询拒绝表均零匹配，零pageerror/console，8个取消请求另留，不能称所有请求均成功；尾5秒333响应、4个取消。原QA路由保留、播放器stopped、无连接alert或spinner。主代理前后SQL证明QA125/用户49全文/head/事务及721终态Job精确不变；拒绝仍96343/UTC23:08:53、失败事件仍20957，server.log仍104175字节/SHA1fa5eb0c全文不变。历史三处SERVICE_ERROR原文为Operation not permitted(os error1)，没有新增，不能仅凭它们判当前服务异常。首个沙箱status因回环EPERM显示unhealthy的原回执保留，授权只读status确认实际ready。正常reload不替代失效/重启故障验收，也不证明冷起播、字体加载或整体七图通过。独立sol6.1/high报告`review/sol-review.md`限定普通reload通过，runner81219已退出、保留原Chrome停止态。最终截图无可见字体加载提示；全DOM字体aria-busy不作为播放器故障证据，可见性祖先未采样。见 `main-final-audit.json`、`main-candidate-identity.json`、`service-status-live.json` 与 `review/sol-browser.json`。

首轮仅一次精确QA125/固定asset/thumbnail模拟422，立即撤route；Native成功回执、媒体和预算未mock。已有owner失效传播到多个资源卡及轨道，如实保留。首轮资源卡Tab/Enter真实缩略图恢复、C02真实鼠标恢复成功；初轮2661响应中仅该模拟422，零pageerror，但遮挡等实际失败使整个切片当时不通过。修复复验又仅一次同范围模拟，原已通过的无关源I/O、导出和播放不重复。

修复候选实际音乐0鼠标重试为error→loading→真实Classic波形，request_id `dd48b69c-956c-48fa-80b5-a98fbfde6aa3`，1679bars，Native原css_height24不变，仅新增一次waveform_display读取。约58ms是有缓存条件的恢复观察，不能当冷性能。1440/1920音乐故障按钮避header.right145、left153留8px，中心和四边中点可命中。1280该故障行未完全可见，后来真实订阅恢复，没有再造错误，因此完整行五点仍未验。

真实按住retry略移，pointer确实命中retry，原生dragstart转向ARTICLE后下一task读defaultPrevented=true且无资源HUD；普通卡标题区仍能拖出HUD并Esc取消，无Drop。错误眼睛按钮真实打开原中央源播放器并返回，选择/播放头/画布/横纵滚动/缩放均保持。19px可见部分不硬塞按钮，ⓘ有中文放大提示及原代码/ID且Tab可达；但Tab自动横滚，不能把之后的截图作为19px条件鼠标发现性证明，整个authored20px片段也未验。

最终1280×720、1440×900、1920×1080：错误内容68px在72px图层内，灰底/padding0，原整图覆盖层隐藏；retry/eye均28×28、中心与四边中点全部命中。保留的原Add24×24中心和ⓘ中心均命中自身且不再遮挡，未点击Add，不增加事务。源SVG波形仍保留原绿色呈现，错误深层眼睛SVG不再误触该样式。页面无横纵溢出。截图 `review/sol-finalstyled-errors-*.png`，收尾为 `sol-repaired-final-normal.png`。

最终capture UTC01:06:41–01:30:25含9596完成响应（9589×200、4×204、2×206、1×明确模拟422），零真实HTTP错误、pageerror、401/403/500；唯一console资源错误与模拟422对应。23个ERR_ABORTED路径和时间保留，未逐条证明原因，不混称服务500。尾5秒313响应无HTTP错误/pageerror，另4个取消保留。原始初轮真实Media.active异常仍保留；本轮普通reload未复现，不证明所有HMR/卸载条件。

主代理分别对初轮1744、失败UI轮2631、修复轮9564个request_id查询拒绝表，均零匹配，见 `main-*-request-audit*.json`。修复capture结束字段为captureStoppedAt，收尾审计补读该字段见 `main-repaired-request-audit-complete.json`，无须重发请求。服务日志全文SHA及104175字节均与本轮前相等；鉴权拒绝最大仍96343/UTC23:08:53，失败事件仍20957。721 Job全部终态（621成功/67失败/33取消）未增。主代理 `main_before.json` / `main_final.json` 与子Agent完整SQL分别证明QA125/head fa7cf8cc/全文SHA768641c5、用户49/head313459f0/全文SHA2c025848的全文/head/事务精确不变，未改写用户工程，见 `main-final-comparison.json`。

原唯一QA最终1440×900、声音stopped、播放头0，真实缩略图和波形可见，无源播放器/HTML媒体/dialog/拖动HUD/route。全部临时监听移除，客户端退出，原Chrome与QA页保留。当前证据不关闭原5次真实预算拒绝的具体触发路径、授权轮询成本、冷起播及[七图矩阵其它出口](jianying-workbench-exit-matrix-20261024.md)。

