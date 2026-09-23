# R2 共享合成描述验收

2026-09-21。本轮完成后端无关合成描述与 full/direct/range 编译归一；R2 的完整历史兼容矩阵和发行仍未完成。设计见 [合成契约](../architecture/composition-contract.md)。

## 实现

- 新纯 `opencut-composition` 从固定 ProjectSnapshot 编译带 `opencut.composition.v1` 标签的描述；素材节点引用 ID/版本，没有文件路径或编码器参数。
- 泛化的 Timeline/clip/curve/caption 原语迁至 `media-contracts::composition`。原生 media 保留 PathBuf 类型别名，未反向依赖工程模型。
- ProjectRenderAdapter 的完整计划与直接帧计划调用同一编译器，区间计划继续共享完整计划；帧与区间精确选择函数也归入纯编译包。原生适配负责资源绑定和输出选项，FFmpeg 继续使用共享图构建器。
- 保留 Visual 对音频依赖的隔离：纯音频轨贡献时长，缺失音频不能阻塞单帧；完整音视频任务仍要求相应素材。
- 走查发现两条素材解析路径使用有损 UTF-8 转换，均改为明确错误并保留 unresolved 状态，不回退原 URI。

## 实际验证

最终 CLI SHA-256：`a5cf8f1eb5fd943424aaf5c9ceb873b08ef537a83b9a88de47fbf67ce7e8cdfe`。

源码清单摘要：`d9c6bca8ae338061e935324933e06985e1110d61e4fc0115f2c7eb8c2757456e`。117 个 Cargo/crates/CLI/scripts 文件及 17 个保留产物/报告的逐文件摘要位于 `/tmp/opencut-composition-fingerprint.json`。平台为 macOS 26.5.1 x86_64；本轮使用现有 Rust 1.97.1 与 FFmpeg/ffprobe 8.1.2。

| 验收 | 结果 | 证据 |
|---|---|---|
| Rust 编译与行为 | 56 项通过：Composition 4、Worker 7、Media 20、ProjectRender 23、编译失败契约 2 | `/tmp/opencut-composition-final-tests.log` |
| 无宿主资源编译 | URI 换成任意另一宿主仍产生同一描述；保留版本、源起点、常速映射、曲线、图层、字幕与音频尾段；可绑定整数资源句柄；未知 format 拒绝 | `crates/composition/tests/portable.rs`，包含在上述 56 项中 |
| 编码工程帧 | 16 项通过：动画位置、分割前后像素、子帧 clip 相位、NTSC、半开终点、预览/导出对照，以及有实际编码进度后的取消/清理 | `/tmp/opencut-composition-preview/report.json` |
| 直接 PNG 工程帧 | 11 项通过：固定修订、PNG/时间/hash、无中间 MP4、缺音频隔离、音频尾段黑场与活动合成进程取消 | `/tmp/opencut-composition-frame.json` |
| 区间音视频 | 10 项通过：非零区间、完整素材锁、后续编辑隔离、真实 RGB/PCM、下载/hash 与活动 FFmpeg 取消清理 | `/tmp/opencut-composition-range.json` |
| 固定修订完整导出 | 15 项通过：上传、真实任务/导出/下载、后续编辑隔离、中英字幕、字体/回退信息、独立字形像素与缺字体拒绝 | `/tmp/opencut-composition-render.json` |
| Clippy | 五个受影响包 all-targets `-D warnings` 通过 | `/tmp/opencut-composition-final-clippy.log` |
| 公共契约 | 生成无漂移；旧 22 工具 Schema/read_only 不变，当前仍为 27 工具 | `/tmp/opencut-composition-schema.log`、`/tmp/opencut-composition-wire.json` |
| 纯依赖与源码 | Composition 的 85 节点闭包无 Application、SQLite、CAS、FFmpeg 或 Wasmtime；既有契约/SDK/Application 门禁保持通过 | `/tmp/opencut-composition-boundaries.json` |

区间预览独立解码为两秒，两个采样窗口的音频频率约为 879.375 Hz、439.375 Hz，符合夹具的 880 Hz→440 Hz；画面验证为绝对源区间的绿→蓝。固定红色夹具的直接帧与编码帧最大通道差为 0，这仅支持该夹具，不推导任意有损视频逐像素相等。

主 Agent 另查看了保留的起始黑场、后段红色块和音频尾段黑场 PNG。该观察补充脚本的像素/时间判定，不属于浏览器观看验收。本轮公共链路实际使用 FFmpeg 和服务进程，没有用“缺工具则跳过”代替验收。

首次新增可移植夹具用了 schema v1 却包含 AssetVersion，Core 正确拒绝；夹具改为 v2 后通过。Clippy 指出新绑定函数放在测试模块之后，已调整位置后通过。失败记录分别保留于 `/tmp/opencut-composition-portable.log` 和 `/tmp/opencut-composition-clippy.log`，没有放宽生产验证或测试断言。

## 边界和后续

本轮没有实现第二个 GPU/Web 渲染后端，也没有把编译成功当成素材、字体、编解码器就绪。复杂效果、Bezier、完整变速、持续预览 Session、真实分析 Provider、工程包和后续发行仍按 R3–R7 计划推进；Web/GPUI 实现与实际 UI 继续暂缓。

本轮的真实公共检查限定在合成相关链路；SDK 外部消费、插件、数据库故障/迁移与全部崩溃窗口的先前证据不能冒充本轮重跑。下一步完成 R2 历史数据库/请求/成功结果兼容矩阵及可安装交付，再进入后续协作与预览能力。
