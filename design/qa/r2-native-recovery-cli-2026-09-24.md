# R2 本机 owner 预算恢复入口验收（2026-09-24）

状态：**局部完成，R2 未放行**。新增 `opencut --database WORKSPACE.sqlite storage recovery inspect` 和 `storage recovery configure --input -`。两个命令在 SQLite 打开和凭证鉴权前执行，直接调用原生预算账本；configure 沿用 expected revision、幂等键和含可信 actor／request ID 的持久回执。指定 `--credential-file` 会被拒绝，本入口只授予本机 OS owner。数据库不可达期间无法向 SQLite 审计表写记录；账本保留策略变更回执。

[独立 CLI 动线收据](evidence/r2-native-recovery-cli-2026-09-24.json)对应本机 debug 二进制 SHA-256 `a9fcb55c6471c1d5cc9f9180d8e91cb912b63ab6315bf572ccb5672de0b06721`，共 3 项检查通过：

1. owner 将最低卷余量设置成高于真实可用值，独立 SQLite 进程崩溃后留下 WAL 并要求重建 SHM。普通预算检查及工程读取因 VFS 准入返回 `DATABASE_CAPACITY_EXHAUSTED`；恢复入口仍可读取 revision 和原回执。以 revision 1 和稳定幂等键调回门槛后，普通工具恢复；重复配置得到原回执，工程 revision、SQL dump 和 `integrity_check` 均保持正确。
2. 携带受限凭证的 inspect/configure 被拒绝，且不会读取凭证文件或改写账本。
3. 在真实 64 MiB HFS+ 卷上，先以恢复入口把默认 64 MiB 最低空余门槛改为 0，再通过管理型 SQLite 创建工程、重开读取并完成完整性检查。

这只证明**策略锁死时本机 owner 可恢复**和 64 MiB 卷可以初始化。尚无已分配并 fsync 的 repair 文件，也没有在卷可用字节为 0 时完成受限取消／上传释放／checkpoint 与补足预留的事务。外部直接 SQLite 写入仅用于制造遗留 WAL 夹具，不受 OpenCut VFS 管理。R2 的完整物理准入、repair 模式、安全审计和跨平台验收继续待完成。

同一二进制已随[本机开发归档](../../dist/local/opencut-0.1.0-dev-add87b7dcd65-x86_64-apple-darwin.tar.gz)在仓库外通过[33 项整包验收](evidence/r2-native-recovery-package-acceptance-2026-09-24.json)。归档 SHA-256 为 `17d89b80ccba0090ff913c8059d0284cded2c6287bd4c90894d486d8fd31def0`，包内二进制 SHA-256 与上文一致。首次整包运行在上传已终止后遇到 Python HTTP 客户端 `ConnectionResetError`，验收脚本原只处理等价的 `BrokenPipeError`；补齐异常分支后，导入动线独立 13 项通过，整包复验 33/33 通过。服务端未观察到崩溃。此归档没有随附可选 WASM component（`component_sha256=null`），也不是 release／跨平台发行。
