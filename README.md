# OpenCut

OpenCut 是本地视频剪辑与媒体执行系统。Web 编辑器和外部 Agent 通过 CLI、HTTP、MCP 或 SDK 使用同一套 Rust 应用服务；工程、事务、任务和权限由本机服务管理。

当前提供项目库、多轨剪辑、素材导入、文字字幕、图形、效果、蒙版、转场、连续预览、历史恢复和媒体输出。完整产品验收及跨平台发行尚未完成，见[交付状态](docs/status.md)。

## 构建与启动

需要 Rust、Node.js 22.12+、npm、FFmpeg/ffprobe。浏览器媒体执行另需 Chromium，见[运行配置](docs/operations.md)。

```sh
npm ci --prefix packages/sdk-ts
npm ci --prefix apps/web
cargo build --locked --release -p opencut-cli
npm run build --prefix apps/web

target/release/opencut --database /absolute/path/to/workspace.sqlite --json \
  server --directory /absolute/path/to/service start \
  --listen 127.0.0.1:8788 --web-root "$PWD/apps/web/dist"
```

替换数据库和服务目录为要使用的工作区。恢复既有服务时使用原目录；配置文件存在时会参与启动参数解析。启动返回 `ready=true` 后打开输出中的 `web_url`，浏览器通过同源 HttpOnly Cookie 建立本机会话。

开发网页直接读取源码：

```sh
OPENCUT_SERVER_URL=http://127.0.0.1:8788 npm run dev --prefix apps/web
```

打开 [http://127.0.0.1:5173](http://127.0.0.1:5173)。服务地址必须指向拥有目标工程的工作区。生产页面来自 `apps/web/dist`；修改源码后必须重新构建，切换分支不会自动更新网页产物或正在运行的 Rust 程序。

## 使用资料

- [运行配置与服务管理](docs/operations.md)
- [公共接口与 Agent 接入](docs/contracts.md)
- [架构与依赖边界](docs/architecture.md)
- [开发、测试与打包](docs/development.md)
- [交付范围与限制](docs/status.md)
- [TypeScript SDK](packages/sdk-ts/README.md) / [Rust SDK](crates/sdk-rust/README.md)

用户素材、数据库、凭证、构建产物和测试现场不进入仓库。当前交付入口是 CLI 服务与 Web；`apps/desktop` 是独立 GPUI 壳，未接入完整剪辑服务。

[MIT License](LICENSE)
