# 公共接口

业务操作由 Rust 注册声明定义，CLI、HTTP、MCP、领域动作和生成 SDK 消费同一声明。不通过手写 DTO、兼容别名或另一套客户端领域模型扩展接口。

```sh
target/release/opencut --json system capabilities
target/release/opencut --json system domains
target/release/opencut --json system schema
target/release/opencut --json system domains --domain edit --schemas
```

CLI 通用入口为 `call <operation> --input <request.json>` 和 `domain <domain> <action> --input <request.json>`。HTTP 暴露相同业务语义，Agent 使用独立 Bearer 凭证。标准输入输出 MCP 使用 `mcp serve`；独立 Worker 使用同一数据库，或由连接的 `--with-worker` 管理。实际选项以 `--help` 为准。

`opencut_project_create/list/inspect` 管理工程，`opencut_edit_plan/apply` 处理原子编辑。读取固定 revision 后构造计划，提交携带 expected revision 与稳定幂等键。冲突或未知结果不能触发隐式重写请求、换 revision 或另一条后备写入口。

素材、字体与资源通过授权引用和版本绑定。上传采用有界顺序分块及完整哈希，导入成功后消费其发布结果。上传完成不等于已把素材放入时间线。

`opencut_render_submit` 统一提交渲染，`opencut_job_view` 读取公开任务。事件、取消及产物操作从相应领域发现。已接受、已排队或生产进度均不证明成功、可下载或已经输出到设备。

下载和连续媒体流使用宿主公布的专用传输合同，不假设所有传输端点都是 JSON 工具。通过公开结果、artifact handle、长度和哈希验证实际产物，不读取私有路径。

SDK 保留结构化错误、request identity、取消信号和未知写入结果；观察超时不终止任务。浏览器使用同源 HttpOnly 会话，Agent 使用独立主体。读取、编辑、任务、资源和管理权限分别校验，执行和结果返回前保持授权围栏。能力发现不是执行许可。

[生成与检查](development.md)使用同一显式已构建 CLI。数据格式版本、数据库迁移编号和第三方包版本是实际兼容合同，必须保持准确。
