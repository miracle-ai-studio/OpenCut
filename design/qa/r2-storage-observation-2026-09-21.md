# R2 存储盘点验收 — 2026-09-21

本轮交付六类原生磁盘占用观察，不是完整配额/GC。数据库仍为 v11，52 工具、9 类任务、OpenAPI 55 路径。原 22 工具和两项媒体发布契约冻结不变，新增 HostCapabilities.storage 为可选扩展。见 [存储治理契约](../architecture/storage-governance-contract.md)。

## 实现与代码逻辑检查

- 公共 DTO 无路径与 I/O 类型。Application 定义 owner 权限、预算和 StorageInspector 端口；native-services 承担 Unix descriptor 遍历，Automation 仅装配，四入口复用注册表。
- 逐层 openat/no-follow，并将打开后的目录 identity 与 statat 结果比较；不经子路径拼接重新打开祖先，不读取普通文件内容。
- 全局 device/inode 去重；固定类别顺序避免跨类别重复计费，稀疏文件的 length 和 allocated 分别报告。禁止把它解释为独占／可回收字节。
- 不创建/修复/删除文件，无数据库迁移与业务写入。owner 检查先于原生观察；其他主体拒绝沿现有拒绝审计记录。
- 100000 条目、64 层目录、5000ms 合作式预算、每进程两个并发扫描。完整性、遗漏和卷观察分别表达；此处时间不是可中断文件系统调用的硬超时。

## 当前已通过

`python3 scripts/verify-storage.py --mcp-sdk-root <独立官方 SDK prefix>` 六组实际进程验证，证据目录：
`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-storage-852fgso_`。
开发二进制 SHA：`9dc8e6fcc8c252287d99c4844a10481cbaa70be15a7362f02abe94911adad905`。

1. 空工作区查询不创建目录、不改变工程记录；卷容量独立 statvfs 比对；无效预算/路径/actor 拒绝。
2. 五类托管目录与独立 stat 文件长度/块数比对，包含稀疏文件、跨类别硬链接、总数求和与路径隐藏。
3. 符号链接、FIFO、深度限制及条目限制产生明确部分结果；不统计链接外的敏感文件。
4. CLI/HTTP owner 调用与受限 principal 拒绝；现有审计关联准确。
5. 40 次真实 HTTP 扫描期间并发替换目录/外部符号链接，不越界、不挂起。
6. 仓库外官方 MCP SDK 同时验证 stdio/Streamable HTTP 的 owner 结果及受限拒绝。

Schema 复核补充：最初 max_entries 仅在运行时限制，已将生成 Schema 的 minimum/maximum 补齐并与默认值、运行时共用 MAX_ENTRIES。修正后六组真实动线重新通过，二进制 SHA `9e9b716b1c61675f231896826d3b9fe92a084d7a32d9c26df9bebe7fd122688c`，证据 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-storage-5yifa_1z`，包含公开 Schema 边界检查。

分层门禁、冻结 22+2 契约、Clippy（native-services/automation/cli all-targets）、相关 Rust 3 项、TS 8 项通过。生成漂移检查通过。没有用测试数量推断全产品无 bug。

首次验收脚本运行的 HTTP helper 名称遮蔽 Python http 模块，已修正重跑，失败日志保留 `/tmp/opencut-storage-journey.log`，通过日志为 `/tmp/opencut-storage-journey2.log`。此前 Clippy 指出的跨 Unix stat 字段宽度转换已以局部注释/允许处理，非 Unix 不执行此原生实现。

## 交付

早期候选 dev-9b4c28bacb63 的 38 组／38 条命令实际通过，但因缺少 Schema 数值范围而被修正版替代，不作为本轮最终推荐；原始报告保留在 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-ify5rahc/report.json`。修正后的 release 包构建/仓库外验收进行中。上一已验开发包 dev-7d1c8c705b60 保留且 hash 复核一致。尚不把本轮源码或打包成功等同交付验收。

## 未完成边界

统一物理配额、写入准入与预留、引用图、历史/分支/任务/会话保护和可恢复 GC 尚未实现。跨设备挂载实际场景、不可访问网络文件系统和时间预算硬中断不在本轮已验范围；Linux/Windows 不由 macOS x86_64 验收替代。R2 继续进行，R3–R7 完整目标不变；网页和桌面继续暂缓实际实现。
