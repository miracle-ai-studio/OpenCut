# Agent 数据面 OpenAPI 与交付复验（2026-09-23）

## 契约边界

此前的生成 OpenAPI 只有 61 个叶工具、9 个领域调用和 2 个领域发现路径。实际 HTTP 宿主已实现项目原始分块上传、公开产物下载和项目／任务 SSE，但 Agent 无法从同一 OpenAPI 找到这些数据面路径。当前生成器从被测 Rust 二进制读取工具注册表、HostCapabilities、领域目录和事件流 Schema，新增以下路径：

| 路径 | 契约重点 |
|---|---|
| `PUT /v1/uploads/{upload_id}/chunks/{offset}` | 原始 `application/octet-stream`、规范十进制偏移／大小、SHA-256、成功时的 UploadView envelope；结果不明需先 inspect 偏移 |
| `GET /v1/artifacts/{artifact}` | 绑定 Job ID 和哈希的不可变引用；成功返回实际媒体 MIME 的字节流，失败返回结构化 envelope |
| `POST /v1/changes/stream` | EventChangesInput、`Last-Event-ID` 优先级、`text/event-stream`，`x-opencut-data-schema` 指向同源 StreamRecord Schema |

生成 OpenAPI 现在有 75 条路径；旧工具 Schema 和调用形状未变。事件流夹具也由同一生成命令输出，避免手工版本漂移。这是自动化公开表面的契约，不宣称列尽宿主全部旧 REST 兼容路由。

## 当前版验证

- [独立 TypeScript SDK 报告](evidence/data-plane-sdk-2026-09-23.json)：外部安装、13 项真实消费检查通过；75 条路径与被测宿主一致，1,373 个本地 `$ref` 全部解析。
- [真实网络报告](evidence/data-plane-network-2026-09-23.json)：6 组受限 HTTP/MCP 动线通过，包含 SSE 光标续接、产物下载授权、下载 SHA 与像素解码、凭证撤销后中断大文件下载及活动 SSE。项目原始分块上传另由下述开发包的项目导入验收覆盖。网络脚本用固定随机原始像素生成可复现的多 MiB PNG，以确保背压窗口确实发生。
- [仓库外开发包报告](evidence/data-plane-package-2026-09-23.json)：在独立目录解包并通过 21 组检查，覆盖双 SDK、官方 MCP、30 格历史数据库矩阵、真实盘满/WAL、写入准入、项目导入、Worker 强杀恢复和真实媒体动线。打包清单与二进制 schema 15、61 个叶操作、9 个领域和 75 条 OpenAPI 路径一致。
- `npm --prefix packages/sdk-ts test` 10 项、生成契约 `--check`、旧 22 工具 wire 兼容及依赖边界检查通过。

本地 **debug 开发包**为 `dist/local/opencut-0.1.0-dev-1d39087e4d76-x86_64-apple-darwin.tar.gz`，SHA-256 `6eaea6f2f97f274b20339a749b2fbc33509e2adf553db05e4f6163434d26c52f`；包内二进制 SHA-256 `cccee081dc81b714be5e24f6bea4718941e432798b987036014a3233aca5d6ee`，与上一轮通过 Worker 输出准入的二进制相同。复制到工作区后重新核对 archive 摘要。仍缺真实旧用户库、浏览器/桌面产品操作、release／跨平台发行和 R3–R7 完整能力验收。
