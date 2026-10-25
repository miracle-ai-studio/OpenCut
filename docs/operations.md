# 本地运行

工作区包含 SQLite 数据库及其关联 CAS、任务、上传与媒体数据。启动前选定数据库绝对路径，不能以页面标题或端口判断工作区身份。数据库与关联资源一起维护。

## 媒体运行时

除 FFmpeg/ffprobe 外，浏览器媒体执行使用固定依赖的 Node 和 Chromium。安装包可提供受信运行时清单，源码运行可以显式配置：

```sh
npm ci --prefix packages/classic-audio-renderer
npm ci --prefix packages/classic-visual-renderer
export OPENCUT_CLASSIC_AUDIO_PACKAGE="$PWD/packages/classic-audio-renderer"
export OPENCUT_CLASSIC_VISUAL_PACKAGE="$PWD/packages/classic-visual-renderer"
export OPENCUT_SOURCE_ART_PACKAGE="$PWD/packages/classic-source-art-renderer"
export OPENCUT_CLASSIC_AUDIO_NODE="$(command -v node)"
export OPENCUT_CLASSIC_VISUAL_NODE="$OPENCUT_CLASSIC_AUDIO_NODE"
export OPENCUT_CLASSIC_AUDIO_CHROMIUM=/absolute/path/to/chromium
export OPENCUT_CLASSIC_VISUAL_CHROMIUM="$OPENCUT_CLASSIC_AUDIO_CHROMIUM"
```

配置必须在服务启动前设置，修改后重启同一服务。参阅[音频执行](../packages/classic-audio-renderer/README.md)和[视觉执行](../packages/classic-visual-renderer/README.md)的预算、受信文件及运行约束。

本地转写使用 `OPENCUT_WHISPER_MODEL`，可用 `OPENCUT_WHISPER_CLI` 指定程序。字体网络获取默认关闭，确需固定许可来源导入时设置 `OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD=1`；已导入资源从本机 CAS 读取。用户素材不发送给未授权提供方。

## 服务管理

```sh
target/release/opencut --database /absolute/path/to/workspace.sqlite --json \
  server --directory /absolute/path/to/service start \
  --listen 127.0.0.1:8788 --web-root "$PWD/apps/web/dist"
target/release/opencut --json server --directory /absolute/path/to/service status
target/release/opencut --json server --directory /absolute/path/to/service stop
```

服务目录必须属于当前用户且仅本人可访问，保存连接状态、凭证和日志。不指定时使用 `~/.opencut/server`，其中 `config.json` 会参与连接、网页和转写配置解析。启动后核对返回的 database、executable、web_url 与 ready，不输出或提交 api-token。

`start` 等待认证 API 与数据库读取通过，CLI 退出后服务继续运行。`status` 仅在 ready 时成功退出，`stop` 请求本实例优雅停止。不默认安装开机自启或崩溃自动拉起。

## 页面与升级

生产服务读取构建目录；Vite 读取源码并代理到 `OPENCUT_SERVER_URL`。工程地址为 `/editor/<project-id>`。切换服务必须确认目标数据库，修改源码后重新构建生产网页。

更新前核对运行的二进制、网页目录、实例与工作区，停止对应服务，按 SQLite 要求备份数据库及关联资源，再替换经过检查的程序和网页。更新后确认 readiness 与产物身份并重开既有工程。不得覆盖执行中任务使用的运行时或删除活动资源。

存储维护使用公开 inspect、plan、execute 与恢复接口，遵守计划哈希和发布围栏。任务超时先查原 Job；未知写入结果继续用原请求和幂等键。

## 缩略图与波形资源

浏览器显示缓存上限为 64 MiB；同一服务工作区的宿主派生数据缓存保持 32 MiB，同时只运行一个后台解码。不同来源的解码请求按接收顺序排队，同源请求合并，缓存命中可直接读取。活动与等待请求共最多 256 个，等待沿用原请求的权限、取消标识与期限。

页面显示“正在排队或生成缩略图/波形”表示后台仍在处理。真正超出缓存或请求容量时会提示查看技术详情。该显示缓存与连续视频预览的帧缓存分别计量。
