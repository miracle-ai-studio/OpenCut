# 连续预览实施计划

状态：**设计，尚未实现**。

2026-09-21 前置进展：共享纯合成描述已实现并接入现有离线 full/direct/range 链路，见 [合成契约](composition-contract.md)。本文的连续 Session/producer/telemetry 仍未实现；后续须消费同一描述，不能重新解释工程时间或以离线 Job 循环代替。

本文定义固定工程修订的连续音视频预览最小完整垂直切片。它不是现有
`preview.project_frame`、`preview.composited_frame` 或
`preview.project_range` Job 的循环调用，也不承诺低延迟、逐帧无损或跨平台逐字节一致。
已有 Job 继续承担可恢复的离线证据与短区间产物；连续预览使用独立的临时 Session。

## 目标与边界

首个切片必须同时满足：

- 固定 `project_id + revision + sequence_id`，素材内容由 CAS 清单锁定，工程 head
  后续变化不改变正在播放的 Session。
- 一个 generation 内只有一个持续的音视频 producer，不为每帧或每个短区间创建 Job。
- 音频与视频来自同一复用工程求值结果和同一 mux 流，避免浏览器与 Rust 分别解释
  clip 时间、关键帧和音量。
- 播放、暂停、seek、断开和超时有明确取消与资源回收规则。
- Web 和 Agent 观察同一 Session 状态，但区分“服务端已生成到哪里”和“浏览器实际呈现到哪里”。

首版建议采用软件编码的 H.264/AAC fragmented MP4。一个 play 或 seek generation
对应一个长期 FFmpeg 进程和一个有界流；客户端可通过 MSE 消费。该选择只定义首个
后端方向。MSE 在本仓库支持的真实浏览器中的可用性、启动行为、seek 行为和内存上限
尚未验证，因此能力在浏览器验收通过前必须保持未实现。

## Session 契约

以下是计划中的传输 DTO，名称与字段在实现前仍需进入共享 contracts 冻结流程。

```text
PreviewSessionRequest {
  project_id: String,
  revision: String,
  sequence_id: String,
  start_time: Time,
  alignment: exact | nearest | floor | ceil,
  viewport: { width: u32, height: u32 },
  audio_enabled: bool,
  quality: "encoded_stream"
}

PreviewSession {
  session_id: String,
  revision: String,
  sequence_id: String,
  generation: DecimalU64,
  state: preparing | ready | producing | paused | ended | failed | stopped,
  actual_start: Time,
  duration: Time,
  producer: ProducerStatus,
  presentation: Option<PresentationObservation>,
  owner: OwnerLease,
  asset_locking: media_content_locked,
  expires_at_ms: DecimalI64
}

OwnerLease {
  owner_id: String,
  lease_id: String,
  lease_until_ms: DecimalI64
}

ProducerStatus {
  generation: DecimalU64,
  state: starting | encoding | draining | exited,
  produced_through: Option<Time>,
  buffered_bytes: DecimalU64,
  server_observed_monotonic_ms: DecimalU64
}

PresentationTelemetryInput {
  owner_id: String,
  lease_id: String,
  generation: DecimalU64,
  sequence: DecimalU64,
  presented_time: Time,
  media_clock_time: Time,
  playing: bool,
  stalled: bool,
  clock_anchor: {
    sequence_time: Time,
    media_clock_time: Time,
    client_observed_at_ms: DecimalI64
  },
  client_observed_at_ms: DecimalI64
}

PresentationObservation {
  telemetry: PresentationTelemetryInput,
  server_received_monotonic_ms: DecimalU64,
  staleness_ms: DecimalU64,
  client_timestamp_status: unsynchronized | synchronized
}
```

`DecimalU64`／`DecimalI64` 在线上 JSON 中是十进制字符串，不使用无界 JavaScript
`number`。它适用于 generation、事件游标、telemetry sequence 和单调时钟采样。内部可以使用
Rust 整数；任何选择 JSON safe integer 的适配器都必须显式限制在 `2^53-1`，溢出前拒绝，
不能静默失精度。

`alignment` 沿用工程帧的精确有理帧格点语义：`exact` 拒绝非格点，`floor`／`ceil` 分别向前／
向后，`nearest` 等距时取较晚格点。请求时间和对齐结果都必须位于半开序列范围内。

控制命令为 `play`、`pause`、`seek { sequence_time }`、`stop`、`heartbeat` 和
`report_presentation`。除创建外，每个命令都携带 `session_id`、`generation`、
`owner_id` 和 `lease_id`。旧 generation 或非 owner 的改变状态命令返回冲突，不能静默
影响当前播放。

Session 事件至少包含 `ready`、`producer_started`、`producer_progress`、`playing`、
`paused`、`presentation`、`buffering`、`ended`、`failed`、`owner_changed` 和 `stopped`。
事件带十进制字符串单调游标、revision、sequence、generation 与服务端时间。事件历史有界；断线重连
超过保留窗口时返回明确的 cursor-expired 结果并要求读取当前 Session 快照。

## Producer 状态与呈现事实

服务端只知道编码器、输出管道和网络缓冲状态。`produced_through` 表示媒体已经生成到的
sequence time，不表示用户已经看到或听到该时刻。它不得写入 `presented_time`，也不能被
Agent 描述成真实 playhead。

真实呈现位置的观测只能来自当前 owner 的 `PresentationTelemetryInput`。客户端从媒体播放时钟
采样，只提交客户端事实和客户端时间戳；它不能填写 `server_received_monotonic_ms` 或
`staleness_ms`。宿主接受输入前检查 owner lease、generation 和 sequence；迟到、乱序、旧
generation 报告被丢弃。宿主在接收时记录自己的单调时钟，构造只读
`PresentationObservation`，并只用服务端单调接收时刻计算 freshness。

客户端和服务端时钟尚未完成显式 offset/uncertainty 同步时，`client_observed_at_ms` 标记为
`unsynchronized`，不得直接与服务端 wall clock 或 monotonic clock 相减来计算 age。即使报告
fresh，也只表示最近收到一份通过身份和顺序检查的客户端观测；它不能证明浏览器没有伪报、
媒体确实出声或像素已被用户看到。超过服务端 freshness 门槛后，Session 的 presentation
变为 stale，而不是根据 producer 状态外推。

有音频时，首版以 `HTMLMediaElement.currentTime` 为客户端媒体主时钟；无音频时仍使用同一
媒体管线时钟。`requestAnimationFrame` 只采样和绘制 UI，不累计播放时间。sequence time
由 `actual_start + media clock` 映射，seek 或 resume 后使用新 generation 的 anchor 重新建立。

Agent 可以读取 producer 状态、最新 presentation telemetry 及其 staleness，并可请求同一
revision 的 frame/range 证据。没有新鲜 owner telemetry 时，Agent只能报告“服务端正在生成”
或“最近一次客户端观测”，不能宣称用户正在观看某一帧。

## 单 owner 与多客户端观察

一个 Session 同时只有一个控制和 presentation owner。其他客户端可以订阅状态、事件和
只读流元数据，但不能发送 play、pause、seek 或 presentation telemetry。

owner 通过短 lease 保持所有权。切换 owner 必须由当前 owner 显式授权，或等待 lease 到期后
执行可审计的 acquire；不能以最后写入者自动抢占。授权切换会：

1. 终止当前 generation；
2. 保存旧 owner 最后一个经过确认且未过期的 presentation position；
3. 递增 generation 和 lease identity；
4. 由新 owner 从该位置显式 resume。

观察者读取的 telemetry 必须标识 owner。客户端本地预测位置不能覆盖共享 Session 快照。

## 播放、暂停、seek 与背压

`play` 从 Session 的确认位置启动新 generation。`seek` 分两阶段执行：先在不改变 Session
的纯校验阶段验证 owner/lease、调用方提供的当前 generation、请求时间、序列半开范围和
alignment；任何失败都返回错误并保持当前有效流不变。校验通过后，在同一个 Session 锁内
重新检查 owner/lease 与 generation，随后才原子失效旧 generation、记录新 anchor 并分配新
generation。锁外再启动新 producer；启动失败进入该新 generation 的 failed 状态，不能复活
旧流。任何旧流字节、旧事件或旧 telemetry 都不得进入新 generation。

首版 `pause` 不无限缓存，也不尝试让已阻塞 producer 长期存活。owner 先上报最后确认的
presentation position；服务端接受该位置后取消当前 generation，关闭传输并保存 pause
position。`resume` 从该位置创建新 generation。若 telemetry 缺失或过期，pause 返回其采用的
最后可信位置和不确定性，不能用 `produced_through` 代替。

每个 generation 设定输出字节、未确认媒体时长和写阻塞上限。达到任一上限时取消 producer，
产生明确的 backpressure failure 或 paused 状态，不允许管道、MSE 队列或内存无限增长。

## 资源生命周期

Session create 在 SQLite 写事务外解析固定 snapshot 的必需素材并建立 CAS manifest；提交
Session identity 时再次检查请求身份、固定 revision 的存在／归属和 owner；工程 head 前进不使显式指定的历史 revision 失效。Session registry 首版可以是宿主进程内
状态，但必须显式声明服务重启会终止 Session，不能伪装成可恢复 Job。

Session RAII owner 持有：

- 固定 revision、素材锁引用和 generation 私有 inputs；
- FFmpeg 进程组、stdout/stderr/progress 管道和流 transport；
- 字幕与其他临时图层；
- owner lease、观察者连接、事件缓冲和 TTL；
- producer 的输出预算与取消 token。

seek、pause、stop、owner 切换、lease/TTL 到期、最后连接断开、传输错误和 unwind 使用同一
清理路径：先使 generation 失效并关闭下游，再向进程组发送终止信号，有界等待后强制结束，
最后删除该 generation 的 inputs 和所有未发布临时文件。stop 幂等。新 generation 不复用旧
generation 未重新验证的 pipe 或私有文件。

## 共享求值层先行

连续预览不能直接把工程模型永久绑定到 FFmpeg filtergraph，也不能移植 Classic 的 TS
时间语义成为第二套实现。实现 Session 前先从 `crates/project-render/src/lib.rs` 抽出后端无关
的已求值描述，建议边界为：

```text
EvaluatedComposition {
  revision, sequence_id, duration, frame_rate,
  visual_layers, audio_layers, captions
}

evaluate_frame(sequence_time) -> EvaluatedFrame
evaluate_range(sequence_range) -> EvaluatedRange
```

准备步骤先统一当前 full/direct/range 的视觉与字幕编译 helper，减少已有三条执行路径的语义分叉。这个去重复步骤仍产出 media 的 Timeline DTO，不属于后端无关 IR 已完成；后续必须将求值数据与 FFmpeg 文件／编码配置拆开，再接入 Session。

描述必须保留素材版本、source/timeline 时间映射、图层顺序、transform、opacity、关键帧采样、
caption layout input、音频 gain/retime 与轨道 mute 语义。FFmpeg export、project range、direct
frame 和未来 GPU/Web renderer 都消费同一结果。

Classic `frame-descriptor.ts` 的 layer/item、texture identity、rendered texture content hash、
transform、effect 和 mask 分层可以作为显示适配器参考；其中 `CanvasImageSource`、draw callback
和浏览器对象不能进入 Rust 或网络 contract。Classic `audio-manager.ts` 的 session id 失效、
lookahead、迟到 buffer 丢弃和 dispose 模式可以复用；其自行收集 clip、retime 和 gain 的逻辑
必须改为消费 Rust evaluated audio schedule。Classic `playback-manager.ts` 的墙钟 RAF 不能与
AudioContext 或 media element 同时成为主时钟。

## Single-owner transport

推荐传输分为控制面和媒体面：

- 控制面：HTTP create/read/commands，加 SSE 或 WebSocket 事件。所有状态改变经过 owner lease。
- 媒体面：generation-scoped、不可复用的授权 URL，传输 fMP4 init segment 和 media fragments。
- presentation telemetry：控制连接上的有序客户端报告；它不是媒体 producer progress。

媒体 URL 必须绑定 session、generation、短期 token 和 owner/observer 权限。旧 generation token
立即失效。一个媒体 producer 只有一个负责背压的 transport owner；多个观察者不能各自直接
拖慢同一 FFmpeg stdout。若允许多个观察者观看，必须通过有界广播缓冲或独立 relay，慢观察者
被断开而不是阻塞 producer。首个切片可以只允许 owner 消费媒体，其他客户端仅观察状态。

## 文件分工建议

| 范围 | 责任 |
|---|---|
| `crates/contracts` 或新的无后端共享 contracts crate | Session 控制、状态、事件和 telemetry DTO；不得依赖 FFmpeg/Web 类型 |
| `crates/project-render` | 抽取 `EvaluatedComposition`、frame/range 求值和统一 visual/audio 语义 |
| `crates/media` | 持续 fMP4 producer、结构化 FFmpeg 参数、进程取消、输出预算和流片段 |
| `crates/editor-application` | 若需要，仅保存可审计的 Session 创建/终止摘要；不把高频 telemetry 写入工程事务/outbox |
| 新的 preview-session application/runtime 模块 | registry、owner lease、generation、TTL、资源 RAII、事件游标和 transport 协调 |
| `crates/automation` | typed create/read/control/observe 入口；Agent 输出明确区分 producer/presentation |
| `apps/cli` | HTTP/SSE/WebSocket 路由、鉴权、shutdown 和连接清理 |
| `apps/web` | MSE/media element、owner telemetry、stale UI、generation 隔离；不重新求值工程语义 |

不要将 Session 生命周期塞入现有 durable Job worker。Job 的 attempt、重试和成功 artifact
语义与交互 Session 的 owner、generation 和连接生命周期不同。

## 分阶段验收

### A. 共享求值层

- 用现有真实 frame/range/export fixture 比较重构前后时间映射、split 边界、图层顺序、关键帧、
  caption 和音频 gain。
- 证明同一 snapshot/time 的 evaluated descriptor 稳定，且 FFmpeg adapter 不再自行解释工程操作。
- 此阶段不公开连续预览 capability。

### B. Session authority，无媒体

- 覆盖 create、owner lease、显式 handoff、观察者只读、heartbeat、TTL 和幂等 stop。
- 并发 seek 只保留最高 generation；旧命令、事件和 telemetry 被拒绝；非法 seek 不终止当前流。
- 验证 producer status 与 presentation telemetry 独立，stale telemetry 不被外推。

### C. 单 owner 持续 A/V transport

- 一个 generation 只启动一个 FFmpeg producer；验证 init/media fragments、非零 start、duration、
  A/V 内容和输出预算。
- 慢消费者达到上限后终止，不死锁、不无限内存；断开和 shutdown 无子进程或私有文件残留。
- 固定 revision、CAS 缺失拒绝、不回退原 URI；工程后续编辑不改变 Session。

### D. 浏览器 owner

- 在受支持的真实浏览器验证 MSE codec 支持、autoplay约束、启动、pause/resume、seek、ended、
  tab 后台、网络中断和长播放内存。
- telemetry 取自真实 media clock，包含 anchor、sequence 与 staleness；服务端 producer progress
  不得改变 presented position。
- pause 必须取消 generation并从确认位置 resume；旧 fragment 不得进入 SourceBuffer。

当前自动化环境没有已确认可用的真实浏览器媒体工具链。Rust 单测、FFmpeg 流解析或 HTTP
测试不能替代 MSE/SourceBuffer、实际音频输出和浏览器 media clock 验收。完成 C 之前不能声称
transport 可用；完成 D 之前不能将 Web capability 标为 continuous playback。

### E. Agent 与 Web 一致观察

- Web 与 Agent读取同一 Session snapshot/event cursor；多观察者看到同一 owner/generation。
- Agent在 telemetry 新鲜时可报告 observed presented time，并同时返回 staleness；过期时降级为
  producer/session 状态。
- 同 revision 的独立 frame/range 证据与 presentation anchor 在帧格点和音频容差内一致。

### F. 后续低延迟路径

持久 decoder、WebCodecs/GPU compositor、texture cache 和 AudioContext schedule 属于后续优化。
只有在真实环境测得并公开 startup latency、seek latency、帧丢失、A/V sync、CPU/GPU 与内存
上限后，才能增加低延迟或 realtime capability。首个 encoded stream 切片不能提前使用这些名称。

## 兼容与发布门槛

- 现有八类 Job 的 JSON、幂等 hash、结果 quality 和 artifact 行为不得改变。
- Session DTO 使用独立版本；新增字段遵守向前读取规则，owner/generation/lease 等安全字段不得
  通过缺省值把旧请求解释成有效控制命令。
- `Time`、帧格点、半开范围、素材版本和锁验证继续使用现有 Rust 权威实现。
- capability 初始为 false 或不出现。只有 Session authority、资源清理、持续流和真实浏览器阶段
  分别留下执行证据后，才公开准确的 transport、quality、owner 和平台支持字段。
- 实现状态和测试证据写入现有 implementation status；本文保持设计文档，不以文件存在作为实现。
