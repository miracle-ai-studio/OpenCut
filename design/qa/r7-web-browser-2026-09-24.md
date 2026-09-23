# R7 Web 编辑器真实浏览器动线（2026-09-24）

这是当前源码在本机的补充验收，不改变 [R0–R7 总审计](r0-r7-current-audit-2026-09-23.md) 中对完整系统仍未放行的结论。环境为 macOS x86_64、Chrome 153.0.8010.53（headless）、本地 Vite `127.0.0.1:5173`、隔离的 OpenCut API `127.0.0.1:8788`。API 使用 `5e7b28c0` 对应的本地 debug 二进制，SHA-256 为 `cccee081dc81b714be5e24f6bea4718941e432798b987036014a3233aca5d6ee`；数据库、令牌和 48 kHz／1 秒合成 WAV 均在 `/private/tmp/opencut-web-qa-20260924`，没有写入用户工程。

Chrome 实际打开 `/` 后进入 `/editor`；连接 API，创建工程，上传 WAV 并在时间线生成音轨片段；插入 `OpenCut Web QA` 字幕，执行整工程恢复式撤销和重做；对当前修订预检；创建合成帧与 1 秒带音频区间预览；提交完整导出。各步 UI 显示成功，页面异常、控制台错误和 HTTP 4xx/5xx 监听结果均为空。[合成帧截图](evidence/r7-web-composited-frame-2026-09-24.png) 可见字幕和时间线；[区间预览截图](evidence/r7-web-range-preview-2026-09-24.png) 可见播放器及成功任务。

首次从历史任务点击“Preview”时，Playwright 的真实指针点击被同卡片的“Download”按钮拦截；原因是 `panels.css` 把两个按钮都放在 grid 第 1–2 行、第 2 列。修复按钮分行后，重新打开工程并点击历史区间任务的“Preview”，Chrome `<video>` 的 `readyState=4`、`duration=1`、`error=null`，点击播放后 `currentTime` 从 0 前进到 0.503 秒；页面错误仍为空。根路径原先仅显示 `hello world!`，已改为编辑器入口。

独立读取 SQLite 任务记录并直接核查产物：合成帧 PNG 1920×1080；区间 MP4 1.000 秒；导出 MP4 3.000 秒。两个 MP4 均有 H.264 1920×1080 视频和 AAC 音频；三份文件的实际字节数和 SHA-256 均与任务回执一致。用 FFmpeg 解码前 1 秒音频，区间与导出分别得到 48,000 个非静音样本，RMS 约 0.258，峰值约 0.377。产物任务内建的抽样解码验证也通过。浏览器播放时间推进得到验证，声卡实际听感未验证。

修复后 `npm test` 为 6 文件／31 项通过，`npx tsc --noEmit` 和 `npm run build`（client/SSR）通过。这些检查仅覆盖这次变更和当前 Web 子集。尚缺真实中文访谈、VFR／长素材、拖拽及高频编辑、浏览器与 Agent 并发改写、持续播放与音画同步性能、实体声卡听感、Bun 锁定依赖、部署和跨平台发行；R7 仍为部分完成。Web 当前上传路径仍走旧 `/v1/media/upload`，尚未迁移到项目范围的上传与生成 SDK。
