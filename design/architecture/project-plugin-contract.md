# 项目范围插件契约

状态：2026-09-21 代码及本地真实动线已验；发行验证单独记录，不替代全量 R2/R6 放行。

## 权威与边界

全局安装、WASM 权限及发布审计仍由 Owner 管理。项目授权固定 project/id/version/package SHA-256 和贡献名，不能安装、升级或授予宿主路径/网络。升级数据库 v9→v10 创建空授权表，不推断已有插件的项目权限。原 22 工具契约不改。

四个增量工具共用 CLI `call`、HTTP `/v1/tools/{name}`、MCP stdio/HTTP 和生成的 SDK：

| 工具 | 输入／结果 | 权限与副作用 |
|---|---|---|
| project_plugin_grant | target、包哈希、operations、enabled、expected_revision、idempotency_key → Grant | Owner；原子更新授权、幂等回执、outbox 和审计；不改变工程 head |
| project_plugin_list | project_id → Grant[] | 项目 read；返回该项目至多 64 个授权（包括停用），不枚举全局安装 |
| project_plugin_inspect | target、operation_name → 内联输入/输出 JSON Schema、贡献版本、有效 limits、Grant | 项目 read＋启用授权＋准确包身份＋全局权限；不暴露安装目录或 schema 文件路径 |
| project_plugin_plan | 原 PlanInput → Grant＋PlanInvocation | 项目 read/edit/run_jobs＋同上；固定修订，返回经核心验证的操作和计划，不提交 |

完整工具名均带 `opencut_`。列表表达策略而非当前可用性；inspect 检查实际包及全局权限。贡献名列表按集合规范化。授权版本为规范十进制字符串，首次 expected_revision="0"；与工程 revision 完全独立。单项目至多 64 个身份行（含 disabled），单授权至多 32 个贡献；停用保留审计和 CAS 历史，无删除接口。

## 幂等和撤权

授权写入的幂等作用域为完整 target。当前 Owner 校验先于回执，回执匹配先于授权 CAS；相同请求返回历史回执，不代表当前策略，读取 list 获取现状。更改同键参数返回 IDEMPOTENCY_KEY_REUSED；CAS 失败不产生事件。授权、回执、outbox、审计同一短事务，任何写入失败全部回滚。

规划首先读取授权和当前 principal policy revision；读取指定不可变工程修订，不长期持有 SQLite 事务。原生适配器在插件版本共享文件锁内重验项目授权，再检查实际包哈希、全局权限、ABI 和贡献。编译前及执行前再次重验；返回前重验 principal/credential/project 权限及授权 revision，即使权限被撤销后恢复也不交付旧 ticket 的计划。返回操作还经过项目素材绑定校验，不能借插件绑定任意宿主文件或其他项目素材。

全局安装/权限更新使用同版本排他锁，等待当前调用结束，沿用旧 Host 语义；项目权限撤销无需等待原生锁。编辑提交仍以原 expected_revision、调用者当前权限及显式新幂等键执行；插件授权的撤销不会撤销调用者已有的直接 edit 权限。断开请求不是持久任务取消；此入口是有界同步 planner，不宣称实现 R6 的所有插件任务类型。

## 资源与错误

有效 WASM 限额取 manifest 与 Host 上限较小值：64 MiB 线性内存、1000 ms 执行 epoch、500 万 fuel、256 KiB 返回输出。参数 256 KiB、序列化快照 4 MiB、单份 schema 256 KiB、component 16 MiB。schema 深度不超过 64，引用只允许文档内部 `#`；schema 文件不直接作为公共路径。每个进程至多四个并行项目插件检查/规划；不是跨进程配额。

epoch 约束 WASM 实例化/执行，不包含版本文件锁等待、包完整性读取、schema 编译及 component 编译。输出字节上限在 WASM 返回后检查，不能描述成原生进程内存峰值上限。WASI 无默认文件、网络、环境、参数和 stdio 权限；不声称 OS 进程沙箱或整个请求硬超时。全量插件隔离、取消与分析 Host ABI 留在 R6。

错误保留稳定 code，移除原生路径、内部 schema 值、WASM 栈和嵌套详细错误。项目授权不匹配/撤权返回 NOT_AUTHORIZED；全局权限缺失 PERMISSION_DENIED；包损坏 PLUGIN_INTEGRITY；参数错误 INVALID_ARGUMENT；错误操作 INVALID_PLUGIN；执行预算耗尽 PLUGIN_RUNTIME。未知服务端故障仍遵循统一错误映射，不自动重发写入。

## 客户端动线

Owner 安装真实包并读取 SHA → Owner 配置项目授权 → Agent list → inspect 获取 JSON Schema → 固定 revision 执行 plan → 展示/检查 operations 和 plan → 使用同一工程 revision 显式 edit_apply → 查询新的 snapshot/事件。遇到 revision 冲突重新读取并规划，不自动更新修订重发。Web/桌面实现仍暂缓，其接入必须沿此协议而不能直接访问插件目录或绕过应用服务。
