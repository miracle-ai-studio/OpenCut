# R2 插件发布与恢复增量验收

2026-09-21。插件包／授权文件接入共享发布协议；数据库 v7、36 个工具、8 类 Job、OpenAPI 38 路径。Web/GPUI 实现继续暂缓。R2 和 R0–R7 整体尚未完成。

## 实现与兼容

纯 API 定义插件对象／动作／覆盖范围，泛型 PublicationJournal 和 Query 复用 Application 规则；SQLite 负责独立 plugin_publications / plugin_events 表，PluginHost 负责版本锁、暂存副本、内容校验与文件同步。Native Services 注入真实 Journal，Automation 仅转发。媒体仍使用原类型和独立历史；从已验 dev-9a3f707b4702 包读取原 34 工具，与新生成结果逐项相等。旧 22 工具夹具不修改；新增两项媒体发布工具冻结夹具，防止以后混入插件 enum。

新增 owner-only opencut_plugin_publication_query / opencut_plugin_publication_reconcile。包与授权分别记录 prepared/completed；失败后的核对只追加 observed_*，不改写现场。不完整安装不能执行，inspect 返回 PLUGIN_INSTALL_INCOMPLETE；相同源包重试只补剩余阶段。运行中的 planner 保持共享版本锁，授权更新等调用完成后生效。详见 [发布契约](../architecture/native-publication-contract.md)。

源包先校验，再复制至同文件系统暂存目录；副本清单字节／指纹均须与原校验结果相等，发布后不再读取源包。修复旧指纹规则排除所有授权文件名前缀的问题：现在仅忽略根目录两个精确宿主元数据名，相似／嵌套名字纳入指纹。旧漏算摘要不被静默追认，安装说明明确需审查后用新版本重新安装。

## 已执行检查

- 相关 Rust 95 项通过，包含实际 WASM、SQLite、媒体与插件 SIGKILL 测试；最后 inspect 多包冲突处理另重跑插件 11 项与四窗口进程测试通过。Clippy -D warnings、TS SDK 5 项、纯依赖边界、生成与旧工具契约门禁。
- 新脚本 `scripts/verify-plugin-publication.py`：8 组公开 CLI/HTTP/官方 MCP SDK 真实流程，涵盖实际 WASM 计划、授权撤销／恢复、指纹独立计算、prepare/complete 故障、incomplete 恢复、diverged 观察、损坏包拒绝、无副作用重试、游标、受限访问及重开。
- 首次公开脚本发现测试函数与 http 模块重名，修正测试后完整执行；未将失败执行计入通过证据。
- 调试阶段插件报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-plugin-publication-bdylj6eg/report.json`；媒体 8 组回归：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-maintenance-u9c14x5v/report.json`。该阶段二进制 SHA-256 `9e898795622fc510eb909c8a065b43b7cab8f337225e27daf1da054e3ea0db69`，最终交付以以下 release 证据为准。
- `crates/plugin-host/tests/publication_crash.rs` 四窗口：包／授权在 prepared 后或实际文件发布后暂停；第二个真实进程开始核对并被 OS 锁阻塞；SIGKILL（signal 9）后核对分别记录 unchanged/applied，保留 origin request，重复核对无记录，原安装重试完成。暂停只在测试 Journal，不向生产程序加测试后门。
- 新单测验证：prepare 后修改源包不改变已验证的发布副本；相似／嵌套授权名称的普通文件影响包指纹。
- 首轮测试发现 macOS /var 与 /private/var 别名造成暂存路径校验失败，已先 canonicalize 暂存根再验证。历史单测中手工模拟 v4 的夹具补清理 v7 表；生产迁移没有忽略冲突。
- 历史 SQL v1–v6 夹具未修改，新增独立 db-v7.sql。v1–v7 × legacy/v2 共 14 格；16 组历史检查通过，含 v6 新表冲突整事务回滚、八种旧 Job 结果、旧排队任务实际运行及 80 个并发打开进程。调试证据：`/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-sfpikxx9/report.json`。

最终 debug 插件 8 组再次通过，SHA-256 `e7130d727e248247a23b451340af51c1d040f47a4fd7b8eb457a15675144404d`；证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-plugin-publication-6e42ws8y/report.json`。日志：`/tmp/opencut-plugin-final-tests.log`、`/tmp/opencut-plugin-host-final.log`、`/tmp/opencut-plugin-clippy.log`。

## 交付状态

本机 macOS x86_64 开发包 `dist/local/opencut-0.1.0-dev-a4808c3781b2-x86_64-apple-darwin.tar.gz` 已通过仓库外 24 组验收，26 条记录命令全部 exit 0；此前已验包保持不变。

- Archive SHA-256：`dead3c7c3715241b46289025d17dff154d9049ba7647a8dc28fd9cc93d5fa94b`。
- Release binary SHA-256：`ed7bf4874a668f1f5e44547ad5230bdcf48fca7c01a5e330ca81d162939057a8`。
- 统一证据：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-1lwbsr5t/report.json`；同目录 commands.json 保存完整 26 条命令与输出。归档旁 `.acceptance.json` 保存验收摘要。
- 解包程序再次通过插件 8 组、媒体 8 组、管理历史、actor 来源、受限 CLI/stdio/HTTP/MCP、事件、会话与官方 MCP SDK 回归。
- TS npm 包在仓库外实际安装并执行插件安装，得到包／授权各 prepared+completed 共四条记录；Rust SDK 源码包在仓库外独立编译，强类型读取完全相同的插件记录。两个 SDK 均拒绝受限凭证读取插件／媒体全局历史；原受限编辑及跨语言重放保持通过。
- release 历史 16 组／14 格通过，证据 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-13hho2qe/report.json`；真实上传、预览、导出、修复、独立 RGB/PCM 与停服重开通过。
- 旧真实插件动线另通过 14 项，报告 `/tmp/opencut-plugin-release-legacy.json`；包括 schema/ABI、错误归因、计划无写入、提交幂等和历史修订计划。
- 新旧两个真实归档内 34 个原工具完整定义逐项相等；新包只追加两工具。manifest 数据库 7、OpenAPI 38 路径引用全部可解析；诊断能检测契约文件被篡改。


## 剩余范围

项目范围导入／预检／插件调用、拒绝日志与跨工作区能力仍待完成；插件卸载、扩展类别和工程包属于后续范围。R3 分支／选择性撤销、R4 持续预览、R5 真实分析、R6 完整编辑／工程包、R7 跨平台及整体验收继续保留。当前不声称 SQLite 与文件系统原子事务、硬件断电验证、其他 OS 持久性、公共签名发行或“没有 bug”。
