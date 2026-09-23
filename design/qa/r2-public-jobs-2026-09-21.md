# R2 公共任务、产物引用与任务接受端口验收

2026-09-21。当前版本通过下列限定范围的验收，R2 整体仍未完成。Web/GPUI 实现和真实 UI 按用户指示暂缓。契约见 [公共任务与产物引用](../architecture/public-job-view-contract.md)。

## 交付内容

- 新增 submit/view/query/changes/request_cancel 五个公共工具；八类类型化结果保留编辑所需媒体、精确时间、校验和字体排版事实，允许字段投影剔除 Worker prepared input、租约、原生路径与后端诊断。
- 工程任务在排队期间即提供固定 project/revision/sequence target；稳定分页与持久事件游标可跨宿主重启，错范围及缺失锚点明确失败。
- 产物引用绑定 job 与 SHA-256；认证下载验证哈希、大小和目录约束，并发送同一份已校验私有副本。任务历史成功与文件当前可用性分开表达。
- TS 公共任务与流式下载辅助方法、Rust typed 公共工具／等待／取消／产物 URL；冻结旧 22 工具 Schema，注册表当前 27 个工具，OpenAPI 另含产物 GET 共 28 条路径。
- 任务接受编排从 Automation 迁入 Application，依赖 JobSubmissionStore 与 JobAssetCapture；SQLite 接受端口原子提交任务、幂等回执与排队事件，素材依赖选择与实际渲染共享 Core 逻辑。Application 没有因此引入原生媒体／渲染依赖。

## 当前版本证据

最终 CLI binary SHA-256：`c6d27de30d33eb123f192f3c95a37f26469fd11e5e01dca6bfea3ddd8fd1fab1`。

| 验收 | 实际结果 | 证据 |
|---|---|---|
| 真实公共任务动线 | 11 项通过；八种实际 Worker 成功结果、六种产物 SHA-256＋独立 FFmpeg 红色像素解码、真实字体排版、固定 target、CLI 回读、旧 start 同键重放、冻结集合分页、重启续接、真实失败、损坏／缺失产物，以及仓库外 TS 公共入口 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-job-views-lukcso7c/report.json` |
| 独立 TS 消费 | 11 项通过；包安装、声明检查、真实丢响应后显式重放、冲突、等待与取消分离；OpenAPI 28 路径／756 处引用解析 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-sdk-Kea9mQ/report.json` |
| 独立 Rust 消费与跨 SDK 重放 | 12 项通过；外部源码编译、纯依赖、公共 Job 观察、真实断线、TS 重放同一 Rust 事务 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-rust-sdk-9a8bnxsi/report.json` |
| MCP／HTTP | 各 18 项通过；真实协议连接、共享任务接受、幂等、固定修订、取消、持久事件与 CLI 回读 | `/tmp/opencut-r2-public-jobs-delivery-mcp.json`、`/tmp/opencut-r2-public-jobs-delivery-http.json` |
| Rust 单元／集成／真实媒体／编译失败约束 | 90 项通过；包含移入应用层的用例和两项新增存储端口验收 | `/tmp/opencut-r2-application-jobs-tests.log` |
| Clippy | 五个受影响包所有 target，`-D warnings` 通过 | `/tmp/opencut-r2-application-jobs-clippy.log` |
| 契约与依赖门禁 | 生成文件一致；旧 22 工具 input/output Schema 和 read_only 不变；SDK 纯依赖、Application 不依赖媒体／Worker、Automation 不直接构造 prepared job | `generate.mjs --check --skip-build`、`check-wire-compatibility.py --skip-build`、`check-contract-boundaries.py` |
| 现有 Web 契约消费 | TypeScript `--noEmit` 通过；不代表真实浏览器验收 | `apps/web` 本地 tsc |

任务接受端口的故障验证使用真实 SQLite：queued 事件触发器注入失败后，从独立连接确认没有可见任务、幂等回执或事件；移除故障后同键成功，重开数据库仍保留唯一事件。并发验证让两个连接在幂等预查均为空后同时提交不同 prepared input，确认两个调用返回同一 job 与同一首次输入，数据库只有一个任务和一条 queued 事件。

脚本与报告均保留真实工作目录和服务日志，没有用模拟 Worker 替代渲染，也没有放宽断言掩盖失败。Moon 新增公共任务验收入口；未宣称远程 CI 已运行。

## 过程中修正与未完成项

实现期间修正过 TS 泛型结果 narrowing、原生下载辅助函数 import 和新增端口测试参数顺序；最终编译、类型检查及上表实际动线均通过。前序报告的 hash 对应更早代码，应以上表作为本轮交付版本依据。

源媒体输入仍是 v1 宿主本地路径，旧 JobRecord 仍公开历史执行字段；这次增量接口不是完整远程资源或权限模型。Rust 暂无流式下载封装。公共视图的 artifact availability=unchecked，不能推断永久可下载。字体锁和 CAS GC 也尚未交付。

Application 仍持有 SQLite，当前只落地任务接受事务端口；工程编辑事务、任务租约／查询等端口、存储适配层迁出、预检／插件等剩余用例归位、后端无关合成计划、完整历史版本矩阵与发行继续推进。R3–R7 未因这轮检查通过而完成，不作“全部功能完善／没有 bug”的结论。
