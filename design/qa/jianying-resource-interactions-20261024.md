# 中文工作台资源与预览交互

状态：资源与预览修复限定通过；冷启动和七图整体未通过。独立检查为用户指定的 `gpt-6.1-sol / high`。全部 42 条出口见[验收矩阵](jianying-workbench-exit-matrix-20261024.md)，后续遵循[原实施计划](../plans/jianying-zh-workbench-plan.md)。

## 已验证运行候选

源码基线 `abd2c969b4b48067a85f7020d84b04fbb48a2d38`，包含既有未提交修改。资源交互与故障注入阶段 Native 为 `runtime/opencut`，SHA256 `9d8090717d157d066f5a034395779c9b6d46fb360521b6bbadf9b80a7b53ae06`；后续真实队列调度及正常暂停复测采用 `runtime-priority/opencut`，SHA256 `bb53820ad6465fec4c0e285f2a4d247b29581cba2ddeccae48db44f686b2dbe7`。两者各自同显式二进制 generated/canonical 186 操作、12 领域通过。Web 在 5173 验收，最新类型及生产构建保留于证据目录 `web-dist`；当前调度候选清单为 `candidate-priority.json`，不把早期资源实测改标成新候选。

## 本切片通过范围

- 素材、音频、模糊效果、交叉淡化采用本地拖放意图，复用原应用计划、固定修订、原子事务及恢复端口。落点检查类型、锁轨、碰撞和接缝；拖动零写入，Esc/失焦取消，重复 Drop 和修订变化拒绝。目录音频固定落点并在提交后选中、定位。
- 独立浏览器已检音频移动/增益/右边缘修剪/删除/Undo、效果附着与区间层编辑/删除/Undo、转场接缝拖入/同 ID 替换/时长 Enter/删除/Undo、目录音频拖新轨道并选中。媒体真实拖入/移动/修剪/删除与旧转场非法值/锁轨保持原限定证据，未改标为新候选全矩阵。
- 播放通知保持未变选择的引用；相同声音位置不重复发布。缓存准备只等待当前位置窗口，其余后台准备。声音按连续已解码画面范围放行，精确样本换算、单调与总量上界保留。新冷修订同一次播放跨 14 秒和 16 秒边界，声音/画面同位等待并自动恢复到 18 秒，窗口 lag p95/p99/max 均 40ms，missed/underflow 均 0、无误 EOF；不证明冷起播或全场持续实时性能。
- 缓冲暂停取消自己的帧等待，不等待共享后台 Job；明确暂停保留最后有效画面，并允许同一固定目标的已确认暂停重锚定更新代次。普通 Seek/修订替换/退役仍清理旧像素。精确延迟同修订 20–22 秒真实 artifact 下载，暂停立即及 2.5 秒后均保留 19.96 秒有效人物画面，工程 20 秒、无 error/pageerror；原字节放行后续播不再 closed/busy，恢复到画面20.4/声音20.58。此项为故障注入，不冒充新 Native cold 起播。
- 底栏曾有两个 summary 比普通按钮低 7px；最终 1280/1440/1920 每个窗口 10 个可见 button/summary 中心完全一致，最小命中 28px，无页面横溢出。
- SceneV1 缓存入口允许渲染器已支持的转场，窄能力继续拒绝。原生相关 8 项通过。复用 `crossfade-oracle-20261021` 固定修订，范围 Job `job_0f0fdb36b7804e8bbfcb4b17039dd406` 实际渲染 27 帧；独立黑白线性 RGB 预期最大误差 3.9≤6，产物 SHA256 `5a5a030c452519dd2176cf021996140a8e6610855ed44847cfb6303c5e063748`。缺执行器配置的前次失败保留。

证据根目录 `/private/tmp/opencut-jianying-transition-20261024`，核心浏览器证据为 `review/sol-review.md`、`sol-document-comparisons.json`、`sol-layout-fixed.json`、`sol-cold-playback-fixed.json`、`sol-pause-exact-final.json` 及对应截图。拖放组件13项、播放117项、冷窗口87项、取消/保帧111项、重锚定101项与会话10项分轮通过，存在重叠，不相加。类型、生产构建和281文件Web边界通过。

## 会话错误与服务日志

`A valid bearer token is required` 是鉴权拒绝。本地 HttpOnly 会话八小时过期或服务重启会失效；现停止旧连接轮询，显示明确中文提示和“重新连接”，重新认证后自动打开原工程，不重放编辑，不把显式 bearer 切为 owner Cookie。

新 Agent 实际失效后静置17.596秒零新请求，显式重连成功。预期拒绝 request `34fa7bad-def5-489b-8aa3-6ccaea90044b` 与服务审计序号96307精确配对；重连工程读取 `b09cd6d0-a722-41d1-8ebb-11b06596b6e4` 成功且没有拒绝记录。服务日志为 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-web-bsfcr_6o/service/server.log`；鉴权拒绝持久保存在同工作区SQLite的 `denial_records`，按request_id配对，不记录凭据值。

故障注入结束后有一次 `Preview cache preparation timed out`，见 `sol-final-runtime-note.json`；gate持有超过stillFrame的10秒预算，失败保留。调度候选无注入复测：16秒缓冲时正常暂停，2.5秒后静帧16.0、工程16.00且无错误；续播实际跨18秒到20秒。再正常暂停后静帧20.0，观察至少12秒始终ready，没有重现超时或closed/busy，pageErrors为0。该限定出口通过，不代表冷播放性能通过。

北京时间04:42再次核对当前8b服务：ready=true，最后鉴权拒绝仍为审计96337、UTC `2026-10-23 19:31:23`（北京时间03:31:23），与服务替换时刻吻合。sol6.1/high复用唯一QA页普通重载，原路由及18元素恢复；工程读取200/request `3c685b87-8107-4239-8c58-687d51064265`、`e2bce238-c007-452b-9f58-726a60ab6714`，资源、访问与声音恢复状态读取也均200。主Agent逐个request_id查拒绝表为零，审计最大序号未增长；最新failed Job事件仍20957、UTC19:31:50，Job状态计数不变。当前会话生命周期7项单测通过，QA121/用户46的head、全文档SHA256和事务数精确不变。证据根目录 `/private/tmp/opencut-auth-recheck-20261024`，见 `server-before.json`、`server-after.json`、`project-and-job-baseline.json` 与 `review/browser.json`。普通重载中有取消请求，不能混称服务器错误；本次没有强制失效或重启服务，不把正常重载当作重新覆盖前轮失效注入验收。

这轮另有一个500：只读缓存复用探测返回 `PREVIEW_CACHE_MISSING`，request `2ad78bd8-ad66-4a4c-99f8-2e075e2de66b`。图片Worker按该明确代码回退正常渲染；原因是共享CLI/HTTP错误策略未登记“缓存不存在”，落到未知内部错误500。现补映射404/退出码4，`PREVIEW_CACHE_FAILED`等真正失败仍保留500。该修复已构建部署，Native `8b195d83d925f53ff40bdd981566e508cbb6743c555c209a92af58234f951728`，清单 `candidate-cache-policy.json`；共享策略1项及同显式二进制generated/canonical186操作/12领域通过。只读复用原交叉淡化oracle固定修订，真实HTTP返回404、代码不变，request `bde40e2e-ed64-4c1d-b9b9-f1a84472aa37`；临时会话create/pause/stop均200，最终stopped，未播放。Job637、事务987、修订1035前后相等；四个request_id在拒绝表均无记录，不能用已达保留上限的拒绝表行数代替请求级核对。见 `cache-missing-http-proof.json`。该候选只复核错误分类，前轮实际像素/冷播放仍绑定bb53820，不重标为8b候选全量验收。

主Agent同时查了实际Job错误：旧候选24–26秒缓存 `job_616901b441f2470cb5e87abcf8290b80` 在UTC19:29:12出现 `AUDIO_HOST_FAILED: Audio runner input closed`，底层退出原因尚未证实，保留为开放问题。切Native后七个旧指纹queued缓存以 `PREVIEW_RUNTIME_CHANGED` 被原发布围栏拒绝；不移除该围栏，不把这些记录混称鉴权错误。原记录见 `service-job-errors-after-policy.json`。

04:42只读复查的末尾12002ms共119个响应，全部200；整段记录无401/403/500、console或pageerror。唯一404为预期缓存未命中 `PREVIEW_CACHE_MISSING`，body request `d51c531b-e13c-4783-8b93-aad298e149c2`，拒绝表零记录。重载期间九个 `ERR_ABORTED` 仅表明浏览器取消，未逐一证明取消来源，不计为服务端错误也不宣称已定位。记录同时显示source-art状态查询频繁，不能用本次鉴权通过证明轮询成本或整体性能通过。汇总 `browser-server-summary.json`；本轮未Play，最终声音状态为stopped，原未归因pageerror没有重现，仍不能视为根因已修复。

新8b候选的独立UI定向复测通过：同一Chrome/唯一QA页重新连接204，QA读取200/request `57d0336a-5bd9-48a2-8093-a7b272718991`；只读缓存复用探测404/request `5ac30edf-b720-4991-a756-ad1391014060`后实际静帧正常ready，未Play、未提交渲染Job。再观察12001ms，无500/401/403或页面异常，最终素材/时间线缩略图及静帧可见。主Agent查看真实截图并复核两个request_id无拒绝记录；最后旧Cookie拒绝停在服务替换时UTC19:31:23/审计96337，重连后未继续增长。证据 `runtime-cache-policy/sol-policy-ui.json`、`sol-policy-static-final.png`。`review/sol_policy_before.json`与`sol_policy_after.json`中QA121/用户46完整快照逐字段相等，主Agent独立复核。

## 数据保留

全部代理写入只在原QA工程；因果Undo最终121事务、修订 `rev_2309e85078b2d70c09b3d3ca42543225ad18d9f553f2fb676d2ccfe5c42ee9e1`，全文档 SHA256 `768641c5d3d66c3d23a70dec21f6da797f6aaf1cb7ff37f6d770e2e3e67405a7` 精确恢复基线。调度正常复测前后，用户海南42事务、修订 `rev_774bec40db48b63b9d15c66d4ef834345fda96a60a700cb39b18976e14cd612a`、全文档 SHA256 `84414738bba086a7a8f1838fba79a8770b1aece0f85a2fcbc55d688a23af8e16` 精确不变。之后错误分类复测前出现外部四个编辑，当前用户工程46事务/head `rev_c951f0e687a491f4d6ca560ed388d98a0bd8ceabccb31615a3dd3d9a6e149973`、SHA256 `2c52e4f15ed1b31ad36d13525682d7e10af096022ff431f96d5cff1649cf7018`，作为新只读基线保留，没有覆盖恢复。主Agent独立快照为 `review/cache_policy_after_http.json`；未复制工程或重导入用户媒体。

## 冷启动调度切片与剩余出口

冷起播约204秒仍失败：新固定输入排在旧修订后台缓存任务之后，且逐窗口生成仍有明显等待。不能用后续同步修复掩盖这个缺陷。

调度候选已完成一次release构建并切服务。应用 `preview_cache::preferred_jobs` 按精确原生时间派生当前与后两个窗口；`PreviewClaimBudget` 每两次优先领取给一次FIFO机会。Host从现有已授权playing会话取得原清单，应用生命周期端口接收有界任务ID偏好，SQLite仍在原事务中做queued/cancel/lease/attempt/event检查。暂停/过期不提升，不改Job时间，不取消共享缓存；没有新模块、依赖、DB schema、公共DTO或UI优先级入口。HTTP Worker与会话注册表确认为同进程；独立外部Worker保留原领取语义。应用5项、存储生命周期8项、Host预览19项、架构14项/22模块/0例外及契约边界通过。新候选独立27帧黑白交叉淡化RGB最大误差3.9≤6通过，Job `job_1ea4d18b3134409ca262fbc311a41221`、产物SHA与先前相同；这只覆盖该固定输入。

实际保留QA前置三个普通8秒后台Job后，从12秒单次Play，无注入。已运行的后台0–8秒Job原租约完成后，服务事件确认12–14、14–16秒两个前台窗口先于更早排队的后台8–16、16–24领取；其后给后台一次FIFO机会，再领取16–18与18–20。`priority-server-observation.json`保留原事件顺序及范围。优先领取和公平机会按设计生效，但没有抢占运行中任务，2秒窗口本身仍需约21–25秒生成。首次设备消费/连续画面采样69.778秒，随后窗口仍长时间同位缓冲，冷播放性能明确失败。无注入正常暂停/续播已单独通过。浏览器原始数据为 `runtime-priority/sol-ui-playback.json`。

七图其余失败/未验出口继续按矩阵关闭，包括中央源上下文与I/O范围插入、54px历史及准确摘要、导出记录、关联语义、获取/隐藏边界、参数/字幕/图形等。后续[时间线定位](jianying-timeline-navigation-20261024.md)和[资源区分类与卡片](jianying-resource-layout-20261024.md)已有各自限定证据，不改标本文旧候选。物理音频/手势、完整渲染质量和整体迁移均未计通过。
