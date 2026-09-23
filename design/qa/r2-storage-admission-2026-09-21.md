# R2 持久写入准入验收

日期：2026-09-21。数据库仍为 v12，新增独立原生准入账本 version 1；59 工具、9 类任务、OpenAPI 62 路径。此次接入 CAS 捕获、上传分块/拼接和任务私有输入复制，未声称所有工作区写入或硬物理配额完成。

## 实现

见 [准入契约](../architecture/storage-admission-contract.md)。纯 DTO/算式/权限和原生目录锁/持久账本分层；新 storage-budget 适配器没有进入 SDK 或纯 Application 的依赖闭包。Owner 配置使用 expected_revision 与稳定幂等键，持久回执包含可信 request_id/actor。默认 64 GiB payload 准入上限、64 MiB 卷余量；字节采用规范十进制字符串。

复制之前预留，超出原大小之前扩额；全局锁串行化观察与准入，每个写入者持自己的 OS 存活锁。正常释放和崩溃恢复都不删除 payload，实际文件继续被观察计入；源上传额度与副本准入明确分开。相对数据库路径与绝对路径使用相同工作区。storage_inspect 的 database 类别加入 .storage-budget 控制文件。

## 已完成的实际动线

`verify-budget.py` 六组公开验收通过，开发二进制 SHA-256：`9084db8ebef42c3595197bac63dd07c642fff889ef7fa0638c70f99327e0fd3e`。
证据：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-budget-3dv5npuu`。

1. 策略版本冲突、精确回放和非法字节拒绝；实际编辑额度不足不改变 head，放开后 CAS 大小与独立 stat 相同。
2. 卷余量不足和任务私有复制额度不足产生明确错误；新任务在额度恢复后实际生成红色 PNG，独立 FFmpeg RGB 解码核验。
3. 受限 HTTP 分块不足不推进偏移；使用相对 database 路径的 CLI 分块写入成功；只有源块额度时 Worker 拼接失败，终态暂存可释放；增加额度后新上传实际导入 WAV 并保留素材。
4. 实际 CLI 捕获 512 MiB 稀疏来源时 SIGKILL；存活时 recover 不释放，死亡后账本仍保留预留，显式恢复释放预留但不改变遗留文件哈希/工程 head；随后独立 CAS 计划/执行清理孤儿，外部来源不动。
5. CLI/HTTP 共用策略，受限 principal 不能读取、配置或恢复全局准入。
6. 官方 MCP SDK stdio/Streamable HTTP 验证工具读写语义、配置与回放、读取/恢复及受限拒绝。

原生跨进程集成测试另验证：两个进程争用 256 字节额度，每个申请 150，只有一个获准；它写入 64 字节后被 SIGKILL，恢复只解除预留而保留实际 64 字节，随后 193 字节申请拒绝、192 字节申请成功。另覆盖无副作用观察、扩额边界、符号链接和损坏账本。ignored child 测试入口由父测试实际启动。

首轮扩展回归中，API 回执类型更新与仍在执行的 rustdoc 使用了不同编译时点，导致文档阶段 PolicyChange 未找到。记录保留 /tmp/opencut-budget-regression.log；未把该轮命令记为全部通过，固定源码后重新执行完整受影响回归。

## 最终检查与交付

固定源码后的相关 Rust 回归 117 项和文档检查全部通过（/tmp/opencut-budget-regression-final.log）；Clippy all-targets、分层门禁、冻结 22+2 旧工具、同源生成检查及 TS 8 项通过。六类存储观察的六组实际进程/官方 MCP 回归通过（/tmp/opencut-budget-storage.log）。首候选 dev-02d2025552ec 通过仓库外 42 组／40 条命令（报告 /private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-mrpz449v/report.json），但因缺少 schema 13 的旧写入者兼容门槛而不作为最终交付；记录保留。新 v13 release 构建与独立验收进行中，此前已验包 dev-0bf1481f531f 保留。

## 未完成

完整渲染输出、媒体目录、插件、匿名下载验证副本、SQLite/控制元数据增长还没有全部接入；enforces_all_workspace_writes 保持 false。当前预留不被冒充为完整 job/attempt/子进程资源租约；物理 ENOSPC、所有输出上限、产物保留/下载和持续会话保护仍需实现与验证。主目标 R0–R7 不缩减，网页/桌面按用户要求暂缓实际实现。
