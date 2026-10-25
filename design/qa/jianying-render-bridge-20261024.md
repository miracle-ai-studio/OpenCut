# 冷渲染采样与有界RGBA批桥

2026-10-24，保留原七图42出口。本文是Native视频数据传输的限定修复和诊断，不是冷播放/UI性能通过。原81秒首设备输出失败见[加载诊断](jianying-preview-reload-20261024.md)。证据目录 `/private/tmp/opencut-jianying-cold-cache-20261024`。

## 当前实现与身份

只改 `classic-visual-renderer/host/video-browser.mjs` 的视频Browser→Node私有桥：最多4个原64KiB RGBA块一次跨CDP，Node仍顺序await原单块handler、绑定检查、hex/块长限制和PacketWriter，Rust继续逐原包验证offset/frameIndex、授权、租约、取消和发布。每块在GPU复用其Uint8Array前转为独立不可变hex；不足一批仅成功渲染后flush，flush完成才返回成功帧。批内每块前后都检查原frame、取消与park状态，没有Promise.all并发或整帧拼接。

4块的256KiB原字节会产生hex字符串和CDP副本，约1MiB级额外有界JS驻留；不能称为零内存开销。原进程树RSS、协议队列、输出和GPU额度均未放宽。真实取消/极限RSS故障未新验。

Native仍93a7e8ac9f570174558eae14045e714229191c576c2594ade5b95f47613ddd57；752 Web源码、943 Native/Cargo源码、4生成文件和284 Web产物全部精确不变，未重编Native/Web/SDK或重启服务。最终视频桥源码SHA987acfaeddca6ab980107f86e2cf7e363648746cdbf54fb3a8c085717ffb348a，78份实际renderer源码另由 `candidate.json` 固定。新任务确实加载新host模块，不能把旧缓存作为新运行时证据；Native/Chromium/shader/GPU身份保持，module_sha实际变化。

## 检查与真实范围任务

完整visual模块183项通过；新增4项覆盖原绑定、批数/单块合法性、跨块取消/退休、同buffer9块按[4,4,1]逐字节输出、背压与尾批失败不返回成功。原caption/graphic/blend消费测试一起迁移，未删除原拒绝断言。14项架构、22模块/0临时例外和contract-boundaries通过；没有新crate、依赖、public DTO或业务入口，故未重生成不变契约。既有owned-browser七项仅为原路径证据，不算新增直连能力。

复用QA147固定revision、同C01 0–50帧/2秒、960×540/25fps/high/无音频，只通过公共render端口执行必要的范围任务，不改工程：

|任务|总观察时间|作用|
|---|---:|---|
|job_f1cd72caaefc4abeb39bbc985623cff6|28.077秒|原桥，Native/Node/Chromium三进程各5秒栈采样|
|job_2ba9ca54fc6f41e49b360a440f0be07c|23.417秒|4块批桥，同样采样|
|job_eeef5d9d29f94b0bb5f90518f7c27fe6|23.428秒|冗余ownedLocal开关对照；该开关已移除，不能归为直连优化|

审查发现base browserHooks原来已有ownedLocal:true、video通过spread继承，且共享运行时realpath确实指向同源码。新增显式开关是行为无效的假设，已仅撤回该新增代码；当前最终源码精确对应第二个bridge任务。不要把第三次对照的结果当另一项性能修复。

三份产物实际授权下载均200、335515字节，独立全文SHA均e4d9802e7f97f82381e35a914ec9ad4e0f0ddbd5de4e89d0e381102f8525e6cd，完整frame_receipts摘要亦保持。保留一份 `native-bridge-frame0-50.mp4`，独立ffprobe为H.264/960×540/25fps/2秒/50帧，FFmpeg全50帧解码无技术错误。传输另有独立构造的逐字节RGBA预期；同MP4不外推其它合成组合的独立像素oracle。下载request_id读取为空，不声称已与服务审计逐ID配对。

**冷性能仍失败。** 两次只有单次观测，5秒采样及系统负载会干扰，不能严格归因4.66秒为固定净提升。编码稳态约3fps，含启动/验证的总吞吐约1.78→2.14fps；这不是UI帧率。Native/Node/Chromium的等待栈只说明采样窗口内存在通信等待，尚未证明完整耗时分布。没有重新做完整冷UI Play，原81秒失败、21窗提交、跨窗/seek/新修订门槛不关闭。

## 保存与收尾

三笔是明确的诊断范围视频任务，非产品自动缓存；只增加3成功Job，当前766终态为666成功/67历史失败/33取消。不删历史/产物，不改目的字段掩盖实验。`profile_after_all.json`在12:32:06Z确认QA147及用户实时50的完整文档、head和事务数不变，失败事件20957、拒绝96368和日志105183字节保持。16个已采样的自有Node/Chromium进程均已退出；保留原静帧owner和用户Chrome，没有发送进程信号。

保留sol6.1/high的 `sol-bridge-final-review.md` 是只读源码/日志/真实任务证据审查，未冒充UI操作。另由同一Agent执行一次明确reload的静态收尾：reload前ready且无可见源图错误；reload后QA147/C01原100%/time0无草稿通过，最终1200×959/0选择/原生WebGPU ready、声音停止/HTML无播放/无dialog。489响应为486×200、2×204、1次cache缺失404；捕获无认证/5xx/console/pageerror，6取消、478 JSON/11个204或非JSON分支缺bodyRead、pending0限制保留。8.188秒尾窗85×200/0取消。没有Play/工程写/Job，SQL前后完全保持；Node56074退出0，监听卸载、Chrome保留。见 `sol-ui-final-review.md`，不计作冷播放通过。

最后显式服务仍PID31538/实例d8d5705a-795a-4c3b-b915-9cf0c99cf303/93a7 Native ready，日志全文105183字节/SHA94f52d4067163cc69f0af97f7c560094a843b0c9acaee5dcadec94356f778c12保持。未用重启、隐藏错误或热缓存宣称原81秒已修复。

## 后续可审阅切片

先补原生逐帧阶段计时，区分准备、授权检查、源解码、绑定、GPU/回传和编码，避免仅凭等待栈继续猜测。随后将Rust一次提交全场的行为改为授权consumer当前窗口及有限前瞻，保留全场内容/运行时身份、精确时间网格、完整duration及公平领取；Web按稳定segment身份合并窗口，旧代际不能污染新窗口，缺远窗不误EOF。Surface首声只依赖真实已解码连续范围，不能因补满8帧跨到下一冷窗而额外阻塞，更不能放开未完成窗口的设备credit。按需窗口审查保留在 `/private/tmp/opencut-jianying-preview-demand-20261024/sol-design-review.md`；这些仍是待实施，不是本轮已交付能力。
