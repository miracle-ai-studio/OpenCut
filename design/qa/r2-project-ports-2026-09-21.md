# R2 工程事务端口验收

2026-09-21。本轮完成工程创建／计划／编辑／恢复的应用编排与 SQLite 原子写入分离，并修复幂等预查与读取 head 之间的并发误冲突。R2 尚未整体放行；Web/GPUI 实现继续按用户指示暂缓。详见 [事务端口契约](../architecture/project-transaction-port-contract.md)。

## 实现与逻辑检查

`projects` 模块仅通过 ProjectRead、ProjectStore 和 AssetPinning 编排工程操作，不导入 SQLite、CAS 或原生文件 I/O。SQLite 工程写入移至 `sqlite_projects`，在同一 IMMEDIATE 事务内复查原始请求回执、比较 head，并写入 revision/head/transaction/idempotency/outbox。计划计算与素材捕获发生在写事务外，不刷新 expected_revision、不针对更新后的 head 自动重算。

共享 `identity` 模块提供适配器所需 v2／legacy 摘要和经过历史摘要证明的匹配方法，避免另一个存储适配器自行发明幂等语义。原 schema v3、工具 Schema、事务结果格式与作用域保持兼容。

代码走查发现并修复：在首个幂等查询之后，同一请求可能已被另一连接提交；此时新的 head 不应直接导致 ALREADY_EXISTS 或 REVISION_CONFLICT。应用在拒绝之前复查回执。新增三个定序用例用真实 SQLite 第二连接精确插入 create／apply／restore，再追加后续编辑，当前调用均返回首次请求的原结果。第四个用例在素材准备钩子中插入竞争编辑，确认旧计划无法提交，也未产生其回执或事件。

## 当前版本证据

最终 CLI SHA-256：`6b4bf4d69c9786d43aee12e024edc256fb4d070096e63211b565ad097feda876`。下列四个带 binary hash 的报告均已与最终 binary 逐一比对。

| 验收 | 实际结果 | 证据 |
|---|---|---|
| 独立 CLI 工程事务验收 | 6 项通过：create/apply/restore 事件故障整体回滚、移除故障后同键成功、后续 head 上原回执重放、不同请求竞争一胜一冲突、四个相同请求只发布一份事务 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-project-transactions-rg4363wg/report.json` |
| TS SDK 仓库外消费 | 11 项通过；含提交后真实断线、显式同请求重试与 CLI 回读 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-sdk-O3borR/report.json` |
| Rust SDK 与跨语言重放 | 12 项通过；TS 使用同一请求／幂等键重放 Rust 编辑只得到原事务 | `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-rust-sdk-6ba1cus7/report.json` |
| 真实任务／媒体动线 | 11 项通过；八种真实 Worker 结果、固定工程修订、六类产物哈希和独立 FFmpeg 解码、游标恢复、损坏拒绝 | `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-job-views-_2tbdhy0/report.json` |
| MCP／HTTP | 各 18 项通过；共享事务与 Job、CLI 独立回读、事件和恢复 | `/tmp/opencut-project-ports-mcp.json`、`/tmp/opencut-project-ports-http.json` |
| Rust 相关检查 | Application 47、Automation 2、CLI 5，共 54 项通过，含上述 4 个定序场景及历史数据库／摘要兼容测试 | `/tmp/opencut-project-ports-tests-final.log` |
| Clippy | Application、Automation、CLI all-targets `-D warnings` 通过 | `/tmp/opencut-project-ports-clippy-final.log` |
| 生成／兼容／依赖 | 生成无漂移；旧 22 个工具 Schema/read_only 保持不变，总工具数 27；纯契约、SDK、应用原生执行依赖门禁通过；工程用例／identity 禁止直接存储 I/O | `generate.mjs --check --skip-build`、`check-wire-compatibility.py --skip-build`、`check-contract-boundaries.py` |

故障验收通过 SQLite 触发器拒绝 outbox 发布，再由独立连接逐表确认回滚，并用后续 CLI 进程回读。触发器和故障数据库只存在于隔离验收目录。报告保留 CLI 请求、响应、退出码、stderr 和最终表计数。没有用单元测试结果替代产品入口验证。

## 剩余范围

当前仍是同 crate 内的职责分离，Application 的包级依赖仍含 SQLite／AssetStore。任务租约、任务查询与分页、工程历史／事件查询和数据库生命周期需要继续抽端口并迁移适配层；其他存储后端尚未实现。预检／插件剩余编排、后端无关合成模型、完整历史矩阵与发行仍待完成。R3–R7 的其他功能和非 UI 验收目标不变。
