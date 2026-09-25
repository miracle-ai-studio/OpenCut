# 连续预览后端契约

当前实现为进程内 Session，控制可通过 HTTP、Streamable HTTP MCP、CLI `--endpoint` 或嵌入宿主调用。媒体通过 `POST /v1/preview/stream` 返回原始 `video/mp4`；stdio MCP 与一次性本地 CLI 不创建无法消费的 Session。Web/Desktop 播放器不在本轮交付范围。

执行路径：共享 Automation → 项目 read/run_jobs 授权 → 固定 snapshot 与 CAS 内容捕获／私有复制 → composition/project-render 共享编译 → 一个 generation 的持续 FFmpeg producer → 有界 HTTP 字节流。创建在文件准备后复查授权。Session 不进入持久 Job 队列，重启终结 Session。

`preview.create` 绑定 project/revision/sequence、start_time、alignment、audio_enabled 与进程内幂等键，返回 owner principal、随机 lease、十进制字符串 generation。每个控制请求携带这三者；身份来源始终是宿主认证 principal。读取 Session 不赋予其控制权限。

`preview.control` 支持 play、pause、seek、stop、heartbeat、report_presentation、release_owner。seek 先校验半开序列范围和有理帧格；失败保留原 generation。成功才失效旧流并递增 generation。停止和暂停取消 producer；暂停从最近且新鲜的 presentation 保存 resume_position，缺失时保留旧 anchor 并标记不确定。编码进度从不代替呈现证据。

presentation 的序号必须递增，`presented_time = actual_start + media_clock_time` 使用精确有理时间比较。客户端时间戳仅存储并标为 unsynchronized；freshness 使用服务器单调接收时间（3 秒）。只读 producer 状态报告生成进度和有界缓冲字节，不能证明客户端播放或出声。

owner lease 为 30 秒，需显式 heartbeat；Session TTL 一小时。当前 owner 可以 release；另一个有项目执行权限的 principal 仅在 release／lease 到期后通过 `preview.acquire` 取得新 lease/generation。显式 stop 不可恢复。最近 256 条事件有界保留，过期 cursor 返回明确错误。结束的 Session 墓碑最多再保留 60 秒；幂等回执不是跨重启的持久保证。

每进程最多 32 个 Session，当前画布上限 1920×1080。每 generation 默认总字节上限 512 MiB、producer 缓冲 4 MiB、背压等待 10 秒。HTTP 另有两项有界发送队列并持续重查授权；断线、背压、取消或过期关闭流与 producer。caption 写入有宿主预算保留，释放跟随 generation 资源生命周期。

公开 API 与 SDK 的控制面只含 DTO；Path、进程、临时文件与队列留在 native adapter。TS SDK 提供 `openPreviewStream` 返回流式 Response，调用者应取消旧 Response 后处理新 generation，定期 heartbeat，并仅从真实媒体时钟报告 presentation。CLI 可使用 `opencut --endpoint http://127.0.0.1:8788 --credential-file <host-token> call ...`；不会自动刷新 revision、重试控制写入或伪造呈现上报。

[真实 HTTP/FFmpeg 动线收据](../qa/evidence/r4-continuous-preview-2026-09-24.json)证明一条 generation 流包含可解码音视频及控制隔离。它不证明浏览器播放、音频设备出声、长播放性能或所有硬件上的低延迟。
