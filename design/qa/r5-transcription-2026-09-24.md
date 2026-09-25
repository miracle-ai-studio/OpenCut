# R5 本地转写交付与验收（2026-09-24）

本轮由三个 GPT-6 Sol 分别完成原生转写、持久任务接入和验收执行器，主代理负责接口审查、SDK、集成验证与状态收口。仅完成这一条产品子集，不放行全部 R5 或 R0–R7。

## 实现与契约

`opencut_analysis_transcribe`／`media.analysis.transcribe` → Application 固定修订和素材 → CAS 内容锁 → 既有持久 Job／Worker → FFmpeg + whisper.cpp → 有来源的公开 `extended_result`。不增加数据库迁移，不修改旧 job.start 枚举，不建立第二套队列。Rust／TS SDK 和 MCP prompt 已同步；完整口径见[转写契约](../architecture/transcription-contract.md)。

当前 schema 19、70 个叶操作、9 个领域、85 条 OpenAPI 路径。宿主通过 `OPENCUT_WHISPER_MODEL` 配置模型，客户端只传工程、修订、素材与语言。保留 read/run_jobs 权限、幂等、取消、私有路径过滤和现有输出预算。

## 实测证据

最终 debug 二进制 SHA-256：`a319617da05d70be06a0bfb907ee4818bd81a598f82216a1adc62ab40dc07a29`。

- [真实产品动线 6 组](evidence/r5-transcription-journey-2026-09-24.json)：项目上传与导入、固定素材转写、四段时间码、只读／注入拒绝、同键重放／异义冲突、字幕计划与显式提交、人工修正、真实导出和 CLI 重开。ffprobe、PCM 和字幕像素独立解码，主代理另查看导出帧，确认可见中文及人工修正文案。
- [客户端动线 7 组](evidence/r5-transcription-clients-2026-09-24.json)：最终二进制读取先前实际转写回执，TS SDK 与官方 MCP stdio／HTTP 发现 schema、重放相同 Job、拒绝只读执行；排队取消；未配置模型的新任务明确失败。收据同时记录生成旧回执和本次消费的二进制哈希，不把读取旧回执当作重新推理。
- 22 个旧工具 schema 完全兼容；生成契约检查、Rust 分层检查、Clippy `-D warnings`、格式检查通过。TS SDK 12 项、Rust SDK 2 项和域发现 1 项测试通过；应用与媒体的定向提交、投影和解析测试通过。
- [验收执行器真实专项](evidence/acceptance-runner-selected-2026-09-24.json)：旧 R2 交付包按名执行一个专项，约 4.7 秒，显示阶段与耗时，产生独立专项收据。5 项 runner 行为测试通过。默认全量逻辑保留；无跨版本自动缓存复用。

音频由 macOS Tingting 合成，约九秒；使用真实 multilingual tiny 模型，而非模拟模型结果。模型来自 [whisper.cpp 模型仓库](https://huggingface.co/ggerganov/whisper.cpp)，模型和音频 SHA-256 已写入收据。模型存在识别错误，人工字幕修正验证了编辑路径；不据此声称真人中文访谈准确率达标。

## 尚未放行

词级对齐、说话人、静音／镜头分析、证据检索、转写 UI、长视频和跨平台模型性能尚未完成。结果的 model 只是安全短名，不是模型内容版本锁。当前工作是源码与专项集成交付，没有构建包含本轮 R5 的新发行包；先前 R2 的 41 项整包验收属于旧二进制。

附加旧 MCP context 回归在创建 1.1 MB 轨道名称夹具时触发 debug CLI 的 20 秒超时；本轮不把该套完整回归标为通过。转写专用 MCP 验收独立通过。[独立复核](evidence/r5-large-context-limit-2026-09-24.json)同一提交最终成功但耗时 38.49 秒，证明此 debug 压力夹具仍超出原 20 秒界限；不增加原超时、不宣称性能达标。它不是正常视频素材基准，仍需单独剖析。

## 复验入口

```sh
OPENCUT_WHISPER_MODEL=/path/to/ggml-tiny.bin python3 scripts/verify-transcription-journey.py \
  --binary target/debug/opencut --audio /path/to/chinese.wav --artifacts-dir /tmp/opencut-transcription-new
node scripts/verify-transcription-clients.mjs --binary target/debug/opencut \
  --artifacts-dir /tmp/opencut-transcription-new --mcp-sdk-root /path/to/mcp-sdk-prefix
python3 scripts/verify-local-package.py --list-checks
```

产品动线脚本需新证据目录；客户端脚本复用该目录的已成功真实回执，输出单独 `clients-report.json`，不会重新做模型推理。
