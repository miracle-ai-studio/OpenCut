# 项目范围渲染预检契约

共享 `opencut_render_preflight` 现允许受限主体使用；请求和响应 DTO 保持原形状。预检是无工程写入、无 Job 和产物发布的查询，但会校验媒体和宿主渲染能力，因此要求 project 的 read + run_jobs。仅 read 或 edit 不足以运行预检。owner 原有行为保留。

## 权威边界与分层

API 输入仅 project_id/revision/sequence_id/container/video_codec，拒绝额外路径字段。纯 Application 的 ProjectExecutionRead 端口在一次一致读取中取得当前权限、指定项目的不可变 revision 和可信 SourceAccess。SQLite 适配用只读事务完成凭据／策略验证与修订读取，事务结束后才进入 NativePreflight。SourceAccess 不是 JSON DTO，不能由 Agent 自报。

Application 校验端口返回的 project/revision 对应请求。受限路径在原生后端执行前，使用与完整渲染相同的 Core 依赖选择，要求相关素材具备 AssetVersion；未使用或禁用轨道不增加依赖。未锁定素材返回 ready=false、ASSET_LOCK_REQUIRED 和 asset_id，不访问其 URI。旧工程需要 owner 通过已支持的绑定／替换固定内容后再做受限预检；owner 仍可预检旧本机来源。这是预检的内容边界，不修改旧 Job 的兼容读取或 owner 导入行为。

Native Services 沿用只读 CAS 解析、内容 hash/size 验证、真实 FFprobe、编码器能力与合成验证。锁定内容缺失／损坏不回退 provenance URI；原始文件删除后仍从 CAS 校验。原生适配不依赖 SQLite，公开 Host 只转发同一 Application 用例。

## 诊断、时序与权限

ready 必须与 issues 是否为空一致，报告 revision/sequence 必须等于请求。受限输出保留结构化 code 和 asset_id/clip_id/cue_id，message 由应用层安全解释表产生；不会原样传出后端 stderr、命令行、CAS 路径和原生错误 details。owner 可保留原有详细诊断。版本不属于已授权项目返回 NOT_FOUND；未授权项目先返回 NOT_AUTHORIZED，不查询其修订存在性。

普通查询按取得读取资格时的权限执行；之后移除 run_jobs／撤销 credential 会阻止新请求，不取消已经获得资格的预检或先前提交的 Job。预检不保留媒体执行承诺，随后提交任务仍独立校验权限、修订、内容和能力。不能把 ready=true 当成后续永不失败的保证。MCP 会话本身的 policy revision 失效规则不变。

CLI、REST、stdio MCP、Streamable HTTP、TS/Rust SDK 使用同一工具。能力资源的 access.scoped_preflight 默认 false，新宿主返回 true；这使 `opencut_resource_read` 的 capabilities 分支输出 Schema 增加一个非 required 字段，其他 35 个工具完整 Schema 不变。旧 22 工具和原媒体发布两工具冻结契约均保持。

## 验收要求

真实内容绑定后删除源文件，四入口预检一致；仅 read/edit、跨项目、他项目 revision、额外宿主路径、缺失 sequence、legacy 未锁定媒体、CAS 丢失／同大小篡改、hash 有效但媒体无效、头修订变化和撤权全部覆盖。预检前后所有 SQLite 表相同。随后同身份提交相同历史 revision，经真实 Worker 产生 PNG、内容绑定下载并独立解码 RGB。独立安装的 TS/Rust SDK 对已锁定图像进行同样预检和跨项目拒绝。

本轮没有开放全局素材目录、插件安装／执行或任意路径 Job，也未完成工程范围素材导入。下一步仍是工程范围资源引用与项目插件授权，随后按 R3–R7 原需求继续开发。
