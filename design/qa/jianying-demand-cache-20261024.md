# 按需预览窗口与首声准备

2026-10-24。继续原 jianying-zh 七图42出口；本文不关闭冷播放性能或整体验收。证据目录 `/private/tmp/opencut-jianying-demand-window-20261024`，前置计时与已撤回试验在 `/private/tmp/opencut-jianying-frame-timing-20261024`。

## 已实现与候选

Rust 应用缓存端口只提交当前精确时间所在窗口及后两个窗口，复用原视频提交、固定修订、运行时指纹与幂等键，保留全场 cache_key、duration、CFR网格及末窗真实长度。只读探测必须有当前窗，缺失或未修复的可选前瞻不阻止使用当前窗，且零新提交；负数/EOF复用共享 INVALID_ARGUMENT。它限制单次提交为最多3窗，不保证快速多次seek遗留队列全局有界，也不取消其它消费者共享的Job。原两次前景领取后一次FIFO机会保持，不宣称ID顺序就是严格当前窗优先。

Web按精确起止窗口与真实job_id管理Blob和decoder，原授权scope内增量合并manifest；补窗不清空计划、关闭decoder或按数组位置串窗。新代际仍先重新授权；同窗替换使旧Entry失效。下载、retain、解码、warm后均重验原scope/Entry/consumer，晚bitmap关闭；保持64MiB压缩驻留限制与散列校验。进入下一窗口且前瞻不足时，每至少500ms才可重试一次补窗，同scope至多一请求；缺窗最多两次原会话授权刷新，不无限围绕旧consumer循环或误报EOF。后台授权、身份或网络失败保留原错误并退役该scope，热Blob不能继续绕过；已取消frame不得先续prepare。

Surface首次起声只同步填充已发布窗口的实际画面，不为凑满8帧等待下一冷窗口。后台继续补充，readyThrough只来自实际已解码连续帧；没有放宽PCM/device credit、音画同步、授权、取消或发布围栏。

当前Native SHA d48a3e237887b7e97feac86aa52887f89fd231ca161b2753ab2118fdf42d0274；源码HEAD仍abd2c969且有既有大范围dirty树。相对前候选，943 Native/Cargo源只变preview_cache.rs，752 Web源只变picture-cache/surface-player及各自测试；4生成文件不变，完整源码/渲染器/284 Web产物由candidate.json固定。Native/Web各构建一次；同一显式二进制生成检查current、规范检查186操作/12领域通过。130项受影响Web、7项应用缓存、14架构、22模块/0临时例外和边界检查通过。没有新增依赖、模块、业务操作或手抄DTO。其余渲染器/媒体旧证据保留原身份。

## 计时与撤回

先在原93a7 Native下执行一次同QA147/0–50帧/2秒/540p/无音频的数值计时任务 job_5cd37fd70c8c4921bc871d53d54181af：总观察23.296秒，50个不同帧的host内部总12.616秒；剩余10.680秒尚未细分启动、Native准备、编码/收尾，不能归为单一慢点。稳态中位host211.7ms、解码3.1ms、绑定3.5ms、合成41.3ms、GPU阶段157.4ms；GPU阶段包含flush128.8ms，flush包含CDP、Node输出及背压，不是纯GPU计算或纯Rust校验。临时只记录最多50帧有限数值，无计划/资源/令牌，完成后源码精确恢复原SHA987acfae。

再试16块桥的同范围 job_da79d834d1534f3ebe1af1b2a700a380，24.303秒，未见收益，产品与测试已撤回原4块；不把调参算优化。两份实际授权下载200/335515字节/SHA均e4d9802e7f97f82381e35a914ec9ad4e0f0ddbd5de4e89d0e381102f8525e6cd，含真实X-OpenCut-Request-Id。诊断只增加2成功Job，不改QA147/用户50完整工程，旧失败/取消历史保持。

## 部署与实际界面

无活动Job时优雅替换原服务，保留原数据库/媒体。新实例PID77518/b77c0469-70b7-4c90-b447-c4ec8d2959a2于59790 ready，生产Web为本候选web-dist，5173为同源码。部署前后QA147/用户50的完整文档、head和事务及768终态Job（668成功/67历史失败/33取消）完全相同。切换瞬间旧Cookie产生6条认证拒绝96369–96374，原记录保留，不能写成零拒绝；日志只增加正常停止/启动336字节。子Agent在旧页实际见“本机会话已过期或服务已重启”，一次明确reload正常bootstrap后time0原生画面ready，部署重载不算编辑自动刷新。

sol6.1/high已完成真实UI限定验收，见证据目录 `sol-ui-review.md` 与 `sol-ui-summary.json`。唯一冷Play首次只产生0/50/100三个窗口Job，完整duration仍42秒；随后按播放位置滚动，累计仅10个自动缓存Job（0–20秒），全部成功。最终778终态为678成功/67历史失败/33取消，没有人为输出任务、工程写入或重提原Job。QA147和用户50的全文、head及事务逐字保持。

**冷性能仍失败。** Play动作前13:18:42.693Z到首个Native确认的device_timestamp输出128frames，13:19:16.707Z/request7873f45d-f2ac-4d69-8918-885a8619e812，精确34.014秒；早期约33秒简报已纠正。旧81.106秒属于原候选和原负载，不能据两个单次观测归因固定提升。实际跨0/2/4/6等窗口，画面5.96秒与设备6秒同位缓冲，不误EOF；消费时钟上的帧间距统计不证明壁钟无长卡顿。窗末首声不等待下一冷窗仅有本轮单测，实际UI未额外seek或故障注入，保持未验。

Pause在8秒保留7.96秒有效画面，随后原Native8秒静帧替换；至少2.5秒保帧检查通过，不冒称两张截图逐字相同或连续无黑白帧。原音频session续播约2.17秒确认实际8.48秒/设备输出23966，随后推进16秒；热续播不算冷性能通过。Play/Pause/Resume期间69个采样的document.timeOrigin均1792847843440.6，没有观察到整页重载，未注入DOM身份标记，不据此声称全部canvas节点身份已证明。

最终真实返回项目再Back只作cleanup，未再次reload或模拟pagehide。原声音owner已停止；主代理额外通过只读公共接口确认原ebf3698d会话stopped/generation5，新静帧c5c64fd3会话paused/generation1，恢复槽为空，见 `cleanup-inspection.json`。原成功dispatch正文漏存的捕获限制保留，不能反写成当时已采集；SQL原恢复行已ack清除也保留空结果。最终1200×959/time0/0选择/Native WebGPU ready/HTML无播放/无dialog，主代理实际查看最终截图。runner77644退出0、监听全部卸载，无route/DOM observer，Chrome保留。

捕获12242响应：12225×200、2×204、12×206、2次缓存缺失404和1次旧resource_release代际409；无本capture认证、5xx、console warning/error或pageerror，2条Vite debug单列。41个取消不统一归因；12162 JSON/77非JSON未采body/2空204/1份导航后不可取的source-art正文限制保留，pending0不代表所有body完整。尾13.283秒134×200/0取消。服务端拒绝高水位96374、失败事件20957及日志105519字节/SHA保持，reload之后无新增认证拒绝、任务失败或日志错误；部署时6拒绝不抹除。最后服务仍原d48a实例ready。

离线审查确认已有Ended→showStill在Classic全局时间片尾减去一个frame_step，再走Native静帧；不能据generic缓存EOF拒绝猜测可达UI缺陷。未额外修改或重建候选；实际全42秒片尾UI仍未验。此次只关闭按需提交、稳定滚动映射和正常Pause/Resume的限定分支，原七图42出口、滑杆完整取消矩阵与持续冷性能继续开放。
