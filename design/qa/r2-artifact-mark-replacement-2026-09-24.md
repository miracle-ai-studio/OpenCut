# R2 删除意图期间的产物目录项替换（2026-09-24）

状态：**已复现并修复可误删替换文件的窗口；新 release 二进制通过原生测试与公开 CLI 真实媒体动线，整包验收进行中。** 新二进制 SHA-256 为 `ad0ffa140ca0fdea946ceee1a1eb6454663343551d20bff7432c730bface20b0`。对应开发归档为 `/private/tmp/opencut-r2-artifact-safe-delivery/opencut-0.1.0-dev-c3aebd2a7f10-x86_64-apple-darwin.tar.gz`，SHA-256 为 `5593cf399efc1eebfcf5340608ffb44027adee424bb9660f90aff626efc1fcf1`；不能在整包复验结束前宣称归档通过。

缺口发生在 `ArtifactCandidate::remove_if`：原实现先从锁住的 attempt 目录打开计划文件并核对 inode，随后调用 SQLite 写入持久 `removing`。回调返回后只重查目录身份，就按旧文件名执行 unlink。原生回归用例在回调中移走原文件、换入另一普通文件，修复前得到 `Removed`，替换文件被删；这不是仅有状态口径问题。

现在回调返回后再次通过锁住的目录句柄、`O_NOFOLLOW` 打开当前名字，并核对普通文件、单链接和完整文件身份；不一致返回 `changed`，不会 unlink。原生 12 项测试通过，含回调期间换入普通文件、换入符号链接及同 inode 原地改写的三个定向用例。[公开动线收据](evidence/r2-artifact-mark-replacement-2026-09-24.json)由真实 FFmpeg 预览产物、Owner release／计划和 CLI 执行组成，共 4 项通过。故意绕过 attempt 锁的进程在慢 SQLite 删除意图提交期间换入普通文件，新结果为持久 `changed`，替换文件及移走的原文件均保留，同一计划重放一致。换入符号链接时，本次宿主先被预算扫描以 `STORAGE_ERROR` 安全拒绝，目标与原文件均保留；移除符号链接并恢复原名字后，新审阅计划可删除原文件。无误报的 `removed` 回执或悬挂 `removing` 围栏。

这个修复封住已复现的事务回调窗口；POSIX 没有按预期 inode 条件原子 unlink 的接口。对完全绕过工作区锁协议、恰在最后一次身份核对与 unlink 之间修改同名项的进程，仍不承诺强一致隔离；当前契约的正式保证限于遵守锁协议的本机 Unix 宿主。符号链接场景的 `STORAGE_ERROR` 也缺具体权限／身份恢复提示，纳入公开错误口径待办。
