# Web 项目素材导入与授权读取验收（2026-09-24）

本轮把 Web 导入及替换素材从 owner-only `/v1/media/upload` 改为已有的项目上传控制面：增量 SHA-256 → `upload_create` → 最多 4 MiB 的原始分块 → `upload_seal` → 公开 `media.import` JobView → `project_media_inspect` → 绑定到工程 → 释放暂存。新 `GET /v1/projects/{project}/media/{media_id}/content` 在当前项目读取授权内解析资源，打开固定 CAS 版本并校验完整长度与 SHA-256 后出流；旧全局媒体读取仍只对 owner 开放。公开 OpenAPI 增至 76 条路径，双 SDK 有项目素材 URL/读取入口。

真实 Chrome 153.0.8010.53 在隔离本地 API/Vite 中上传 45 秒、4,320,078 字节的 48 kHz WAV，整文件 SHA-256 为 `e8d52fa8f9b66cf072156733f87197bcba38f2c0ec6513705200988d6c50b401`。网络记录确认恰好两次 PUT，偏移 0 和 4,194,304；随后执行项目素材核对、项目范围 GET、暂存释放和工程事务，没有请求旧 `/v1/media/upload`。UI 显示保存、45 秒音频片段与原始播放器；Chrome 播放时钟前进，页面／控制台／HTTP 错误为零。[浏览器截图](evidence/r7-web-scoped-import-2026-09-24.png)。隔离库的 3 个测试上传均已释放，未释放的暂存字节为 0。

独立 `verify-project-import.py` 的 13 组实进程检查通过：项目 `read` 账号读到与源文件相同的字节、MIME 和长度；跨项目和撤销凭证拒绝；同大小 CAS 篡改在响应正文前返回 `ARTIFACT_VERIFICATION_FAILED`，修复后可读。该脚本还复验上传冲突、授权、释放／配额、真实合成产物。二进制 SHA-256 为 `6b60ba03fc4774fc432b2c5a271e7466d96b78ddad318cb5e8b397216dc74459`，独立原始记录在 `/private/tmp/opencut-project-media-content-qa-20260924`。Web 31 项测试、client/SSR 构建和类型检查通过；TS SDK 11 项、Rust SDK 1 项与 compile-fail 文档测试、外部 TS SDK 13 项及生成契约检查通过。

首次浏览器试验暴露协议缺口：项目导入资源故意不提供旧全局媒体 URL，Web 因此无法把已成功导入的素材绑定到时间线；这促成了项目范围读取端点。另发现无产物的 `media.import` Job 被错误显示“Preview/Download”，现按真实产物存在性显示按钮，浏览器复测通过。

无效输入动线也在真实 Chrome 中复验：空文件导入显示明确的 1 字节至 1 GiB 范围错误。最后一次 Web 31 项测试、TypeScript 类型检查、client/SSR 生产构建、Rust Clippy `-D warnings` 与格式检查通过。

本轮 macOS x86_64 debug 开发包完成 32 项仓库外验收，包 SHA-256 为 `42e786a5bbe8674b4fbdb5805bc2cae06969230afdb9ae833644c1ff1cf688ba`，二进制 SHA-256 为 `9e2d2f717e88a1005c4e1f8e1194fa5572a978bc08b86c7050475c4aecd94d91`。[包验收收据](evidence/r7-web-scoped-package-acceptance-2026-09-24.json)包含逐项检查及原始证据目录。交付的双 SDK 在仓库外接续同一个上传，核对导入的项目素材字节并释放暂存；同包还跑通历史格式 30 格、权限、真实 Worker、插件、MCP 和重开工程动线。

此处仅证明当前本机合成音频与授权资源动线。Web 的 Blob 原始播放器仍一次性读取整份素材，长视频的流式播放／内存上限、浏览器和 Agent 并发、真实中文访谈与 VFR、跨平台发行、完整 R4/R7 均未放行。
