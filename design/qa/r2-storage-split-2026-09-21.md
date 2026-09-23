# R2 原生存储迁出与查询端口验收

2026-09-21。本轮完成 Application 与 SQLite／CAS 原生装配的包级分离，并将分页、历史遍历和公开事件转换置于可注入读端口之后。R2 其他范围仍未全部完成。设计与内部 Rust 接入迁移见 [原生存储边界](../architecture/native-storage-boundary.md)。

## 实现结果

- 新 `opencut-sqlite-store` 包持有原生 workspace facade、schema v1/v2→v3 迁移、WAL/FULL 配置、连接生命周期、SQLite 工程事务／任务生命周期／查询适配和 CAS 装配。
- `opencut-editor-application` 保留 projects、jobs、lifecycle、queries、job_views、identity。Cargo 和全源码门禁均不允许其依赖 SQLite、CAS、tempfile、Worker 或原生执行后端。
- 公开分页与事件游标解析／生成在 Application；JobRead／WorkspaceRead 只传工程、记录、锚点和页信息。SQLite 在同一个 DEFERRED 读事务内完成任务上界／锚点／记录和事件锚点／列表读取，不跨数据库观察混合一页。
- CLI、HTTP、Automation、Worker、原生预检和插件改接 sqlite-store。原生检查移动至新包，未删除原有检查；公开视图的三项纯检查保留在 Application。
- 内部原生 Rust import 路径改变，远程 27 工具和 SDK wire Schema 未改变；数据库仍为 schema v3，不引入本轮格式迁移。

## 最终版本证据

CLI SHA-256：`13819ea56606a738e6e14e906ce4c91d0d2b98c2723fe3a5508b2968345bead2`。

| 检查 | 实际结果 | 证据 |
|---|---|---|
| 工程事务公共故障／并发 | 6 项通过；独立 CLI 和 SQLite 故障验证 create/apply/restore 原子性、重试与并发一次提交 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-project-transactions-28jogi8c/report.json` |
| 公共任务和分页恢复 | 11 项通过；八类真实结果、固定 revision、六类下载及独立解码、冻结成员分页、错过滤与外库锚点拒绝、服务重启续接、真实失败与损坏产物拒绝 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-job-views-pytglria/report.json` |
| TS SDK 仓库外消费 | 11 项通过；打包安装、真实提交后断线、显式重放、CLI 回读、等待与取消分离 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-sdk-FvN2UN/report.json` |
| Rust SDK 与跨语言重放 | 12 项通过；外部源码消费、纯依赖、实际 Worker、TS 重放同一 Rust 事务 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-rust-sdk-qf2obehh/report.json` |
| MCP／HTTP | 各 18 项通过，含任务、历史、事件／SSE 和独立 CLI 回读 | `/tmp/opencut-storage-mcp.json`、`/tmp/opencut-storage-http.json` |
| 真实 WASM 插件 | 11 项通过；组件安装／执行、计划无副作用、事务应用、重试、过期计划拒绝与独立回读 | `/tmp/opencut-storage-plugin.log` |
| 真实 Worker 崩溃恢复 | 5 项通过；活动 FFmpeg 时 SIGKILL，自然租约过期，attempt 2 成功，删除源后仍从 CAS 渲染、清理旧输入及独立像素／哈希验证 | `/tmp/opencut-storage-recovery.json` |
| Rust 原有检查 | 总数保持 66：Application 3、sqlite-store 49、Job Runtime 7、Automation 2、CLI 5 全部通过 | `/tmp/opencut-storage-split-tests.log` |
| Clippy | 五个受影响包 all-targets `-D warnings` 通过 | `/tmp/opencut-storage-split-clippy.log` |
| 契约生成与兼容 | 生成无漂移，旧 22 工具 Schema/read_only 保持不变，总数 27 | `generate.mjs --check --skip-build`、`check-wire-compatibility.py --skip-build` |
| 传递依赖／源码门禁 | Application 102 节点依赖闭包不含 SQLite／CAS／tempfile／执行后端；SDK 和纯契约约束继续通过 | `/tmp/opencut-storage-boundaries.json` |

有 binary hash 的六份报告与最终 binary 逐一核对。数据库故障、租约定序和历史格式测试已跟随 native adapter 迁移，而非删除后用较少的绿灯替代。真实链路使用原生宿主和 FFmpeg／WASM，没有用内存替身证明持久化正确性。

拆包阶段编译指出跨 crate 租约 deadline 校验函数的可见性不足，已作为适配器需调用的纯校验函数公开；原生连接改用本地 wrapper 实现 Application trait，避免违反 Rust orphan rule。随后完整编译和上述验证通过。

## 未证明与剩余范围

本轮证明当前 SQLite／本地 CAS 实现及其调用边界；未提供浏览器／其他数据库后端，也未宣称所有目标 OS 运行通过。UUID 等运行时平台依赖仍需在各未来宿主构建中验证，不能把“无 SQLite”直接等同于 WebAssembly 已验收。

预检、媒体维护和插件等剩余业务编排仍需从 Automation 收拢到合适用例／能力端口；后端无关合成描述、完整历史版本矩阵、SDK／宿主发行与 R3–R7 的其他功能继续推进。Web／GPUI 实现及真实 UI 验收仍按用户指示暂缓。
