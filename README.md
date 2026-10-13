# OpenCut

OpenCut 是供外部 Agent 和应用调用的剪辑工具与媒体执行系统。CLI、HTTP、MCP 和 SDK 共用 Rust 编辑内核、应用服务与任务系统；需求理解和创作决策由外部调用方负责。

当前可用于本地开发与集成验证，尚未完成全部专业剪辑能力、跨平台发行和升级验收。当前状态以[交付台账](design/architecture/headless-completion-ledger.md)为准。

迁移范围、模块分工、实施顺序和验收门槛统一见[全量迁移执行计划](design/plans/classic-migration-plan.md)。实际检查范围记录在[验收摘要](design/qa/acceptance.json)；临时日志、媒体和数据库按计划清理，不维护重复的过程报告。

## 构建与启动

需要 Rust 工具链；媒体任务另需 `ffmpeg`、`ffprobe`。Web 构建与 TypeScript SDK 使用 Node 22.12+ 与 npm。

```sh
cargo build --locked --release -p opencut-cli
target/release/opencut --json system capabilities
target/release/opencut --json system domains
```

Agent 可通过 CLI 启动独立后台服务、检查状态和按需停止；HTTP、MCP 与 Worker 一起运行：

```sh
npm ci --prefix apps/web
npm run build --prefix apps/web
target/release/opencut --database .opencut/workspace.sqlite --json server start --web-root "$PWD/apps/web/dist"
target/release/opencut --json server status
target/release/opencut --json server stop
```

浏览器打开状态输出中的 `web_url`（默认 http://127.0.0.1:8788/editor）即可使用，无需输入服务地址或 Access token。页面与 API 同源，会话通过 HttpOnly Cookie 自动建立；Agent 继续使用独立 Bearer 凭证。页眉提供中英文和亮暗主题，偏好保存在本机浏览器。项目地址支持直接打开和刷新恢复。

服务连接在本机 `~/.opencut/server/config.json` 管理，也可通过 `server start --config /absolute/path/server.json` 指定配置。配置存在时使用其中的连接设置，`web_root` 相对配置文件解析：

```json
{
  "listen": "127.0.0.1:8788",
  "web_root": "/absolute/path/OpenCut/apps/web/dist",
  "allow_origin": []
}
```

开发时 `npm run dev --prefix apps/web` 使用本地 Vite 代理，`OPENCUT_SERVER_URL` 可配置开发服务目标；后台服务仍独立运行。

V4 场景声音试听、WAV 与单帧 PNG 需配置共享浏览器运行时，分别见[音频运行时](packages/classic-audio-renderer/README.md)和[视觉运行时](packages/classic-visual-renderer/README.md)。安装锁定依赖并指定已有 Chromium 后，在启动服务前设置 `OPENCUT_CLASSIC_AUDIO_PACKAGE`、`OPENCUT_CLASSIC_AUDIO_CHROMIUM`、`OPENCUT_CLASSIC_VISUAL_PACKAGE`、`OPENCUT_CLASSIC_VISUAL_CHROMIUM` 的绝对路径；Node 可通过对应的 `OPENCUT_CLASSIC_AUDIO_NODE` / `OPENCUT_CLASSIC_VISUAL_NODE` 指定。配置变化需重新启动服务。Web 根据服务能力提供固定修订声音播放、暂停/继续、WAV 导出和场景静帧；播放头来自实际 AudioWorklet 消费，normal 媒体连续声画已有指定范围验收。PNG 当前支持视频、静态 PNG/JPEG、锁定静态字体自由文字、normal 混合、变换、透明度和纯色背景。文字连续预览、完整文字样式、效果、蒙版与视频导出仍待迁移；这些运行时尚未完成正式发行包验收。

固定字体文字使用版本化文字帧入口，Web 可导入 Lato/Ma Shan Zheng、创建和编辑自由文字并渲染场景 PNG；整体验收状态以[交付台账](design/architecture/headless-completion-ledger.md)为准。导入目录中的字体需要在启动服务前设置 `OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD=1`，默认关闭网络获取。仅下载固定提交、固定哈希的开放许可字体，文本内容不发送给字体来源。固定 HTTPS 源优先使用 `HTTPS_PROXY` / `https_proxy`，保留 `NO_PROXY` 规则；仅配置不支持的 SOCKS 代理时明确报错。已导入字体从本机 CAS 读取，重开与渲染不依赖继续启用网络获取。


`start` 等待认证 API 与数据库读取成功，`status` 只有 `ready=true` 时退出 0。默认运行目录为 `~/.opencut/server`，凭证在其 `api-token`，日志在 `server.log`。CLI 退出不停止后台服务；不安装开机启动或崩溃自动拉起。多实例、Agent 调用及 Web/Desktop 开发顺序见[独立服务与客户端优先级](design/architecture/service-and-client-priority.md)。

需要前台运行或交给系统进程管理器时，仍可使用原入口：

```sh
target/release/opencut --database .opencut/workspace.sqlite serve \
  --listen 127.0.0.1:8788 \
  --token-file .opencut/api-token
```

通过标准输入输出接入 MCP 时，分别启动 MCP 与 Worker，并使用同一个数据库路径：

```sh
target/release/opencut --database .opencut/workspace.sqlite mcp serve
target/release/opencut --database .opencut/workspace.sqlite worker run
```

这些是持续运行的独立命令。MCP 会话本身不会启动 Worker；所有执行进程停止后任务暂停计算，Worker 重启后恢复符合条件的任务。

## 调用边界

- 普通工具操作使用同一注册表和分派器。相同工程、权限和输入执行相同业务规则。
- MCP 按 `system`、`project`、`edit`、`media`、`task`、`observe`、`plugin`、`access`、`workspace` 发现工具。操作详情可通过领域查询或资源读取获取。
- 编辑前读取工程快照，先检查变更计划，再提交。请求超时后查询状态；重试写入使用原幂等键，不重复创建操作。
- 分析、预览和导出返回任务标识。任务成功后检查产物与执行结果，不把“已接受”视为“已完成”。
- 大文件通过媒体数据接口传输，工具请求传递参数和资源引用。单素材上限为 16 GiB，实际限制以宿主能力和存储策略为准。
- HTTP 使用 Bearer 凭证；CLI 和标准输入输出 MCP 可使用 `--credential-file`。外部调用方应使用所需项目范围的权限。
- 服务启动、Worker 管理和本地故障恢复是宿主操作；各入口的流式传输与通知形式不同。

```sh
target/release/opencut --json system domains --domain edit --schemas
target/release/opencut --database .opencut/workspace.sqlite --json domain edit plan --input request.json
```

## 模块

| 模块 | 职责 |
|---|---|
| `contracts` / `api-contracts` / 媒体与任务契约 | 公共数据、操作、结果和错误；生成 SDK 类型 |
| `editor-core` / `time` / `composition` | 编辑规则、精确时间、工程合成编译 |
| `editor-application` | 用例、权限、幂等、事务和能力接口 |
| `automation` | 工具解析、授权顺序与类型化分派 |
| `host-runtime` | 装配数据库、原生媒体、插件与 Worker |
| `job-runtime` | 任务领取、恢复和结果发布检查 |
| 存储与媒体适配器 | SQLite、素材固定、资源预算、FFmpeg 和插件执行 |

依赖边界由[架构政策](design/architecture/dependency-policy.json)和自动检查约束。Web 由工程状态、素材、时间线、预览、属性、任务和历史模块组成，通过共享 SDK 接入唯一 Rust 服务；Desktop 尚未接入完整服务。Web 边界和复查证据见[当前交付台账](design/architecture/headless-completion-ledger.md)。

## 开发检查

```sh
python3 scripts/check-architecture.py
python3 scripts/check-contract-boundaries.py
python3 -m unittest discover -s scripts/tests
cargo test --workspace --exclude opencut-desktop --locked
cargo clippy --workspace --exclude opencut-desktop --all-targets --locked -- -D warnings
npm ci --prefix packages/sdk-ts
npm test --prefix packages/sdk-ts
npm run boundaries:check --prefix packages/sdk-ts
npm test --prefix apps/web
npm run boundaries:check --prefix apps/web
npm run build --prefix apps/web
```

媒体、权限、崩溃恢复与仓库外接入验收见[验收脚本说明](scripts/README.md)。Desktop 需要单独具备图形构建环境。

## 打包与验收

```sh
python3 scripts/package-local.py
python3 scripts/verify-local-package.py /path/to/package.tar.gz
```

交付包包含可执行程序、独立 SDK、公共契约、环境诊断和操作说明。构建缓存、测试运行现场、历史恢复副本和开发脚本不进入运行包。Rust SDK 以源码交付，保留编译所需模块和自身回归测试。

打包成功不等于验收通过。验收必须针对实际解包文件执行，并绑定程序与产物哈希；本地开发包不作为已签名的跨平台发行包。

## 使用资料

- [安装、凭证、维护与升级](design/architecture/local-distribution.md)
- [MCP 接入](design/architecture/mcp-http-contract.md)
- [权限控制](design/architecture/access-control-contract.md)
- [TypeScript SDK](packages/sdk-ts/README.md) / [Rust SDK](crates/sdk-rust/README.md)
- [宿主装配与 Rust 嵌入](crates/host-runtime/README.md)
- [预览会话](design/architecture/preview-session-contract.md)
- [转写](design/architecture/transcription-contract.md) / [项目插件](design/architecture/project-plugin-contract.md)
- [原生文件发布与故障恢复](design/architecture/native-publication-contract.md)
- [当前实现与待办](design/architecture/headless-completion-ledger.md)

[MIT License](LICENSE)。项目沿用 OpenCut 开源代码；历史对照源码保存在本地隔离目录中，不属于运行依赖。
