# 预览渲染吞吐与编辑、撤销后的播放恢复

2026-10-25。**本次播放切片限定通过，其余功能仍暂停。**复用原海南 QA 工程、媒体、共享 Rust 场景语义、Classic 执行器和设备时钟，没有新建工程副本。原七图 42 出口、整体迁移和正式导出性能不由本报告关闭。

实际服务的主界面与四个播放工作器已通过 [HTTP字节核对](/private/tmp/opencut-jianying-native-stages-20261024/final-served-assets.json)；交付证据校验和见 [final-evidence-sha256.json](/private/tmp/opencut-jianying-native-stages-20261024/final-evidence-sha256.json)。

当前候选为 [final-candidate.json](/private/tmp/opencut-jianying-native-stages-20261024/final-candidate.json)，验收摘要为 [final-playback-acceptance.json](/private/tmp/opencut-jianying-native-stages-20261024/final-playback-acceptance.json)。Native SHA `18cd5553d5b022c9e220e29b857420d4773d11914e1744b9ab0aeab9d74185d3`；最终 Web 为 `/private/tmp/opencut-jianying-native-stages-20261024/web-expired-cleanup`。清单绑定实际 Native、943 个 Native 源、4 个生成文件、134 个渲染执行源、759 个 Web 源及 285 个 Web 产物，收尾逐文件核验一致。HEAD 仍为 `abd2c969b4b48067a85f7020d84b04fbb48a2d38`，现有脏树保留，未把局部验收写成整仓通过。

## 为什么此前没有收敛

原默认 Play 等待持久 MP4 缓存片段，冷回执曾达 81.106 秒；仅从 21 窗改为当前加两窗，仍需 34.014 秒。后续逐段定位证明，GPU 指令编码约 1ms，但错误校验回执常需 40–100ms；准备和显示共用工作器，冷解码、字体重载又阻塞已就绪画面的显示。字体在无文字间隙被淘汰，使 6 秒边界文字合成约 308ms。另一次真实编辑后重播失败为 `PREVIEW_EXPIRED`：清理旧静帧 owner 的普通 Stop 要求活租约，阻止了新播放。首次失败及其 QA148/149 因果恢复证据原样保留。

## 复用和修复边界

|直接复用|执行适配|
|---|---|
|Rust 固定修订、场景采样 IR、解码许可、逐帧绑定和授权资源端口|默认连续预览执行现有 Scene GPU 路径，不再等待持久 MP4 Job；所有时间和层次仍来自 Native|
|Classic/Mediabunny 解码、文本与 GPU 算法|按固定元数据预留真实解码表面；最近一组字体穿过空白间隙继续计费，额度不足时淘汰后只重试同帧一次|
|原 SurfacePlayer、PreviewOutputClock、Worklet PCM 队列|准备端与设备显示端分工作器；准备端 6 帧、显示端 18 帧，合计最多 24 帧、约 0.96 秒，外部驻留仍受 64MiB 与原 Native 1 秒窗口限制|
|原始 GPU 校验与不可变帧所有权|原始渲染和可转移拷贝分别校验；成功后才转移 ImageBitmap；纹理、导出位图双份及中间画布均计费|
|同一设备回执、样本计数和 token|已验证连续图片额度直达 Worklet；输出位置直达原消费工作器，跨端口乱序不能上报未确认消费，较旧位置不得倒退|
|已有原始请求恢复／原子退役接口|普通 owner 清理也复用原子退役，保持原请求、权限、owner/lease 和新代次保护；不靠忽略过期错误继续播放|

无新 Rust 模块、依赖或第二套项目／时间模型；新增文件均是 Web 执行适配及其私有工作器协议。无内部 CLI shell 替代应用端口。固定版本、取消、旧响应退役及逐帧新授权保持。前置 Native 固定事实、字体、私有二进制回环和显式 Canvas 编码优化的合同仍由同一个 18cd 二进制生成并检查，186 工具／12 领域通过；已批准的 local-network-access 仅用于隔离渲染进程的私有回环通道。

## 实际验收

同一 QA 场景 42 秒、25fps、540p，48kHz 双声道；每轮完整输出 2,016,000 个音频帧。原门槛保持：首正设备回执不超过 10 秒、硬件等待比例不超过 1%、显示新鲜度 125ms／Native 150ms。表中首播时间是点击到首正 device_timestamp 回执的到达时间，不是物理扬声器首响；未清理 OS 磁盘缓存，也未伪造热片段命中。

|实际动线|首设备回执|显示帧／丢帧|硬件等待|最大画面落后|墙钟最大画面间隔|
|---|---:|---:|---:|---:|---:|
|新会话首播|5.507s|1050／0|160ms，0.380%|40ms|176ms|
|opacity 100%→80% 后同页播放|4.841s|1050／0|85.3ms，0.203%|40ms|94.1ms|
|因果 Undo、片尾闲置超过 57 秒后同页重播|4.468s|1050／0|2.7ms，0.0064%|40ms|53.6ms|

三轮均自然到达 42 秒、零 FIFO 欠载、无播放／画面错误。不是“零停顿”声明：墙钟间隔单独留证，首播曾为 176ms；当前最终候选的编辑和 Undo 两轮均低于 125ms。首播候选与最终清理修复候选的四个播放工作器产物完全同 SHA；清理修复后两轮实播覆盖最终主界面代码。原始逐秒数据和完整最终统计见摘要中的三个 `samples`／`final` 路径，没有只截取顺畅片段。

最终编辑仅变 `/sequences/0/tracks/0/elements/0/visual/opacity`：1.0→0.8。QA150 实际保存，QA151 因果 Undo 后完整文档精确恢复，文档 SHA `768641c5d3d66c3d23a70dec21f6da797f6aaf1cb7ff37f6d770e2e3e67405a7`。保留首次失败的两事务，整体 QA147→151 共四笔事务；未隐去失败。原用户工程 head、完整文档和 50 笔事务始终不变。最终三段 UI 验收未新增 Job，终态数仍为 695 成功／74 历史失败／33 取消。

## 检查与证据

- [真实跨工作器像素结果](/private/tmp/opencut-jianying-native-stages-20261024/gpu-transfer-worker-oracle/result.json)：手写独立 RGBA 预期，经两个 GPU 设备及 ImageBitmap 转移，16 个通道完全一致，最大误差 0。仅证明新增像素交接，不冒充整场所有场景和导出的独立像素证明。
- [受影响 Web](/private/tmp/opencut-jianying-native-stages-20261024/final-affected-web-tests.log)：19 文件、199 项通过；[渲染器](/private/tmp/opencut-jianying-native-stages-20261024/dedicated-surface-visual-tests.log)：203 项通过。类型检查、生产构建、14 项架构测试、22 模块零临时例外、契约边界均通过。原始输出在同一证据目录，不相加重叠子套件。
- [同显式 Native 的生成和规范检查](/private/tmp/opencut-jianying-native-stages-20261024/single-contract-check.log)通过；后续仅改 Web，没有为验收器或 Web 变更重编 Rust。
- [最终服务状态](/private/tmp/opencut-jianying-native-stages-20261024/final-runtime-status.json)为 ready。保留可继续编辑的 QA 工作台及静帧 owner；临时像素页面和 59812 监听已关闭，未强杀用户其它工程进程。
- [恢复后的实际工作台](/private/tmp/opencut-jianying-native-stages-20261024/ui-final-restored-workbench.png)与三份完整数据库快照、候选清单共同保留；后者通过摘要引用，不复制媒体或重复列全部文件哈希。

## 保留限制

本次实际编辑恢复覆盖数值 opacity、因果 Undo 与片尾闲置重播，未推广为所有字段、播放中任意编辑、快速 seek 或完整故障矩阵。Native Canvas 试验的有损 MP4 不等于原 FFmpeg 字节／严格源像素，正式导出吞吐仍开放。旧严格导出 SSIM、混合采样率驻留、长片、跨设备和物理 DAC 验收继续保留。`sqlite-tests.log` 仍为 258 通过／3 失败／3 忽略，未证明这些失败为既有问题，本轮不宣称全 Rust 工作区绿或完整迁移完成。其余功能继续暂停。
