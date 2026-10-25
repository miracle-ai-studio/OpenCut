# 编辑后加载与冷起播诊断

2026-10-24，本报告只记录实际诊断，冷性能仍失败。证据目录为 `/private/tmp/opencut-jianying-reload-diagnosis-20261024`；原七图42项不缩减。Native 为旧 bf8f16e3，后续 visual-edit-draft 的 a610 候选不是本次性能测量对象。

## 实际等待在哪里

保留 sol6.1/high 独占原 QA145 浏览器，只主动普通 reload 一次，然后只点击一次 Play，无编辑、Undo、导入或导出。初始静帧 Native total 3356.5ms。Play 后 documentId、NavigationTiming 与 canvas/transport/timeline DOM 引用保持，没有新增主框架导航；不能据此证明每次编辑或所有 React 子树都不重挂。

Play 后首 produced 回执在78.248秒、首 consumed 在80.699秒、首 device_timestamp output 在81.106秒；后者是回执到达时间，不是精确硬件首响测量。实际图片从2.4秒/61帧推进至3.96秒/100帧后再次等待，不能称为流畅持续播放。原请求分别为 b9668586-03da-4418-8309-46d641e2fea7、d19ba0d0-002f-43a0-9067-c25296080ee3、23c5e28a-2b7d-4ed9-87c4-776a194093b3。

`cache_prepare` 自身响应约7.877秒（9ca68bfd-547a-4b3b-b886-af0cba4e1c62），产品为42秒时间线自动提交21个两秒片段 Job。首 Job `job_e4acf619252244d795c73bbda7995301` 在10:51:58Z排队、10:52:12Z领取、10:53:12Z完成：约14秒队列和60秒执行，覆盖了本次大部分等待。最终产物为960×540、25fps、50帧，不含音频。不是人为导出，也不是单个HTTP请求持续81秒。

源码中 `editor-application/src/preview_cache.rs::prepare` 将完整固定快照、输出配置和运行时共同作为缓存身份，一次提交全部窗口；当前/后续三个窗口有有限优先权，但全部片段仍入队。Web `AudioPreview.create` 等首图片缓存准备后才启动PCM流。修订改变会换固定会话并失去原整快照缓存命中；这解释了重复加载机制，但尚未逐一实测所有编辑、Undo、滑杆动线。不能直接删除revision/runtime身份、复用未重新授权的旧资源，或只把loading改成ready。

## 保留与服务端核对

QA145及用户实时50的head、全文SHA与事务数均未变；不能恢复用户较旧49。Job从742增至763，增量21与产品缓存吻合；基线已有11个活动Job，来源未推断。原失败事件20957、拒绝96358及日志104511字节/SHA42b874dc…在诊断期保持。完整响应9394条，未出现401/403/5xx；取消及缺body在原始报告保留。

原验收清理合成pagehide后出现409 PREVIEW_STOPPED与410 PREVIEW_EXPIRED，属于清理失败，不能归因原Play。之后仅一次显式清理reload恢复到time0真实ready；尾窗80次200、无错误，无Play/写入/新Job。`sol-recovery-review.md` 与 `main-after-cleanup.json` 分别固定恢复证据和SQL结果。

同时清理了两个身份已核实的旧验收Node进程76423和80898，不动Chrome、Vite或服务；新诊断runner自行退出。旧runner在原测量时有CPU/CDP负担，无法由这一轮量化产品净性能或断言清理后已修好81秒。相关PID与检查写在 `main-orphan-cleanup.json`。

## 下一切片

先关闭已暴露的滑杆Native时间准入错误，再以有界分段计时定位单帧准备、授权资源、解码、GPU、回传和编码的消耗。缓存提交应随当前窗口及有限前向需求推进，保留固定身份、租约、取消、公平领取和共享消费者；跨修订复用须证明实际渲染输入等价并重新授权。每个改动分别验证，不把缩短首片段或热命中当持续吞吐通过。冷启动、编辑/撤销后的可播放恢复仍开放。
