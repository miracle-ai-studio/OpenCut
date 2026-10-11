# 独立服务与客户端开发顺序

2026-09-29。范围：本地独立服务生命周期、外部 Agent 调用方式，以及 Web/Desktop 的开发优先级；随后按用户要求执行 Web 迁移，验收见[迁移审查](../qa/web-migration-review.md)。

## 决策

先完善 **本地 Rust Server + 浏览器 Web 界面**，再按需求加入桌面安装与系统集成。网页负责交互，Rust 服务负责工程权威、任务、媒体和渲染。网页关闭不决定任务生命周期。不要求素材上传到云端，也不把 Rust 合成器迁移到浏览器。

这是基于当前源码和交付成本的判断，不是 Web/GPUI 性能对比测试的结论。`apps/web/src/editor/editor-workspace.tsx` 已有工程、素材、时间线、预览、任务、历史和编辑操作；`apps/web/src/lib/editor-sdk.ts` 已接共享 SDK。`apps/desktop/src/shell.rs` 目前装配 GPUI 面板，未接入同等编辑动线。Web 的本机 Chromium 编辑主流程已接入浏览器验收；大规模时间线和跨浏览器性能仍需单独测量。

## 独立服务

`opencut server start/status/stop` 管理后台 HTTP/MCP 宿主及同进程 Worker。原 `serve` 保留前台方式，供 launchd/systemd 等外部进程管理器使用。无编辑用例的 Cloudflare 示例已移除。`--web-root` 可同源托管静态编辑器。

```sh
# 在仓库根目录构建并启动；工程位置被解析成绝对路径。
cargo build --locked --release -p opencut-cli
target/release/opencut --database .opencut/workspace.sqlite --json server start
# 状态默认查 ~/.opencut/server，与调用时的工作目录无关。
target/release/opencut --json server status
# 输出 ready=true 且退出码 0 才可继续调用。
printf '{}\n' | target/release/opencut \
  --endpoint http://127.0.0.1:8788 \
  --credential-file "$HOME/.opencut/server/api-token" \
  --json call opencut_capabilities --input -
# 按需停止；不要求每次 Agent 退出时停止。
target/release/opencut --json server stop
```

- `server --directory PATH` 选择独立运行目录；多实例应使用不同目录、端口和数据库。默认数据库仍沿用全局 `--database`，即当前目录下 `.opencut/workspace.sqlite`；跨目录再次 start 请传原绝对数据库路径。
- `start --listen 127.0.0.1:8788 --allow-origin http://localhost:3000` 配置端口和允许的浏览器来源。来源必须匹配实际 Web 开发地址；不默认开放任意来源。返回真实 endpoint、PID、instance、数据库、凭证文件及日志路径，不输出 token。
- 启动等待认证的能力查询和有界数据库事件读取；重复启动同配置返回已有实例，修改配置需先停止。`status` 在停止、启动中、停止中或探测失败时 `ready=false`，退出 1；操作错误通过既有错误 envelope 非零退出。
- CLI 管理命令用命令锁串行化，服务进程持有运行锁；PID 仅作诊断，既不作为健康证据，也不用于停止信号。停止文件包含实例 UUID，旧停止请求不影响重启后的新进程。
- `stop` 复用既有 HTTP/MCP/预览/Worker 的优雅退出。超时报告错误并保留停止请求，不自动强杀正在执行的媒体工作。任务恢复仍按既有租约及发布 fence 规则处理。
- 当前后台启动不安装开机启动、系统守护或崩溃自动拉起，也不提供远程公网服务。已实现的后台进程能力与跨平台发行验证分开记录。日志追加到 `server.log`，自动轮转尚未实现。

边界理由：新增 `apps/cli/src/server.rs` 只负责本机进程管理、锁、就绪探测和诊断。没有新增 crate、依赖或依赖政策例外。编辑仍走共享 HTTP/SDK/应用用例；后台进程复用现有 `http::serve` 与 host-runtime。启动自身可执行文件是 Agent 请求的进程生命周期动作，不是应用模块通过 CLI shell 绕过应用端口。未增加或修改公共 HTTP/MCP 工具 schema；服务控制文件仅为 owner 私有运行状态。

## 方便性、性能与投入

| 维度 | 本地 Web + Server | 桌面端 | 当前判断 |
|---|---|---|---|
| Agent 使用 | CLI 启动、查询；HTTP/MCP 调同一服务 | 同样可连接服务；不应要求打开窗口 | 独立服务先完成，两端收益相同 |
| 开发与迭代 | 延续现有 React、SDK 与组件；浏览器易调试 | GPUI 需接入命令、状态、播放器和原生交互 | Web 的当前增量成本更低 |
| 用户安装 | 仍需本机 Server；浏览器可直接打开界面 | 安装包可捆绑 Server，提供双击启动和升级 | 最终桌面打包更方便；初期 Web 更快验证工作流 |
| 导出/分析速度 | Rust/FFmpeg 在本机执行 | 使用同一宿主与后端时执行路径相同 | UI 技术本身不能证明导出更快 |
| 时间线交互 | 大列表、波形、缩略图需虚拟化和后台准备 | 原生绘制有优化空间，也需同样的数据与调度设计 | 先测拖拽/缩放/seek，再决定原生投入 |
| 播放与内存 | 浏览器解码、缓存和媒体传输可能增加开销 | 可做更直接的本地解码/显示及系统集成，但当前尚未实现 | 连续预览、代理、预取和缓存是两端都需要的工作 |
| 素材导入 | 浏览器选择文件后经本地 HTTP 导入；仍有读取、摘要、暂存成本 | 可通过受控本机导入能力减少额外传输、提供目录监听 | 本地路径权限必须由宿主授权，不能让客户端直写数据库 |
| 跨平台 | 界面可复用，仍需逐平台验证媒体后端 | 安装、签名、更新、图形驱动和窗口交互分别验收 | 初期避免同时维护两套完整编辑界面 |
| 离线/隐私 | 本地提供页面及服务即可离线；需验证字体、资源无云依赖 | 可离线打包，但仍需验证外部 provider 策略 | 是否联网由实际部署和 provider 决定，不由 Web/Desktop 名称决定 |

浏览器原生文件选择/目录权限能力存在兼容限制，不能将 File System Access API 当作所有浏览器可用的本机文件系统；见 [MDN showOpenFilePicker](https://developer.mozilla.org/en-US/docs/Web/API/Window/showOpenFilePicker)。公共网站连接 localhost 还涉及浏览器本地网络授权，Chrome 已明确该权限边界；见 [Chrome Local Network Access](https://developer.chrome.com/blog/local-network-access)。因此首期应优先本地提供页面，减少公共网站跨来源连接本机服务的接入成本；同源静态托管现已实现，生产页面不依赖外部 CDN。

## 下一阶段顺序与验收

1. Web 完成连接、项目/素材、时间线编辑、预览、任务/导出、断连恢复的闭环，复用共享 SDK；补浏览器真实拖拽、播放出声、冲突/重试与关闭页面后任务继续执行验收。
2. 独立测 UI 与媒体性能：1,000/10,000 片段时间线的交互延迟、掉帧率和峰值内存；1080p/4K 代理首帧、seek p50/p95、连续播放掉帧/音画同步；本地大素材导入吞吐、暂存与峰值内存；导出耗时及 CPU/GPU 使用。测试机器、编码、帧率、代理和缓存状态固定并记录，未测前不承诺实时 4K。
3. 桌面阶段优先解决安装、自动连接、文件/目录授权、窗口/快捷键和系统通知。如果复用 Web 的桌面容器能满足这些需要，可减少 UI 重写；若实测浏览器播放/交互上限阻塞核心场景，再投入 GPUI 原生播放器和时间线。该技术选型尚未实施。

不把“换成桌面”作为现有连续预览、缓存、媒体编解码和任务调度性能问题的通用修复。也不把本轮服务就绪探测耗时当作时间线/渲染性能基线。

## 本轮本机观测

当前 release 的合成媒体验收中，后台启动至认证就绪为 1.01 秒；10 次独立 `server status` CLI 调用中位数 75.61 ms、最大 86.66 ms，包含进程启动和两次 HTTP 检查。数据来自同机小样本，无跨机器或 UI/导出性能推论；详细收据见[本机服务验收](../qa/evidence/local-service-2026-09-29.json)。
