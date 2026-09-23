# R2 Rust SDK 与契约拆分验收

本轮交付：纯 api/media/job 契约依赖、内部 prepared input 独立、同源 typed Tool/JobKind、Rust SDK、仓库外编译和跨语言幂等验收。边界和剩余项见 [公共契约边界](../architecture/public-contract-boundaries.md)。Web/GPUI 仍按用户决定暂缓。

## 实际结果

- 新 binary SHA-256：`fa979addc9637a1b36c411d62364d6326193f518a0803413b8dbab95edb8ce54`。生成校验与之前的工具 Schema/OpenAPI/TS 文件完全一致；22 个工具、八类 Job 未漂移。
- Rust SDK 11 项公共验收通过：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-rust-sdk-mn5vzbo1/report.json`。源码复制到独立目录，只带四个纯契约包；真实认证、只读计划、提交、提交后断线、手动重试、冲突、CLI 回读、事件、真实 FFmpeg Worker、等待超时和取消观察均走公开入口。
- 双客户端同一工程验收通过：独立安装的 TS 包使用同一个 fixture、原 revision 和原幂等键重放 Rust 编辑，返回原事务；事件只有一次 commit。Rust 显式 null 与 TS 省略可选字段没有产生重复提交。
- TS SDK 11 项独立消费通过：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-sdk-HPLUqF/report.json`。
- MCP 18 项、HTTP 18 项、真实 WASM 插件 11 项通过，报告分别为 `/tmp/opencut-r2-mcp.json`、`/tmp/opencut-r2-http.json`、`/tmp/opencut-r2-plugin.json`。
- 产品媒体动线 12 项通过：`/private/tmp/opencut-r2-product/journey-1789927313758940000`。包含上传、规划、HTTP/CLI 等价时间重试、分割＋人工字幕、旧修订预览导出、重启恢复、媒体元数据修复、产物损坏拒绝，以及独立 AAC 解码脉冲 `[9600, 9616]`。脚本仍标记 passed_with_remaining_scope，转写／镜头分析／工程包尚未实现。
- 14 个相关 Rust 包共 129 项单元／集成／实际 FFmpeg／编译失败文档检查通过，日志 `/tmp/opencut-r2-contract-tests.log`。相关包全 target Clippy `-D warnings` 通过，日志 `/tmp/opencut-r2-clippy.log`。
- `scripts/check-contract-boundaries.py` 通过：契约和 SDK 依赖闭包不含原生媒体、存储执行、SQLite、Worker 或 Wasmtime；Moon 任务已配置，不宣称远程 CI 已运行。

## 验收中修正

搬迁期间编译暴露调用处原生 String/PathBuf 类型不匹配及测试局部 import 缺失，已在原生边界显式转换并修正 import。首次外部消费脚本将 Cargo metadata 的 stderr 下载提示合入 JSON stdout，导致解析失败；现分开读取，失败目录 opencut-rust-sdk-qq03muel 保留。没有放宽工程、事务或媒体断言取得通过。

## 未证明的范围

Rust 验证环境为本机 Rust 1.97.1；其他 Rust 版本／OS、HTTPS 服务部署、浏览器和移动端未验。Rust SDK 是阻塞客户端，观察取消发生在网络调用之间，不具备即时中断进行中请求的保证。两个 SDK 均未对外 registry 发布。

R2 仍需新公共 Job 视图／legacy 适配、Application 存储端口和用例归位、后端无关合成描述、完整兼容矩阵和发行；R3–R7 未因本轮通过而完成。旧 v1 的宿主路径和 lease 字段继续保留兼容，不声称全部执行细节已从公开协议移除。
