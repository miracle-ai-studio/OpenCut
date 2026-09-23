# R0 恢复缺口：交付包双 SDK 接续验收（2026-09-24）

针对[恢复回放失败项核查](recovery-replay-failure-audit-2026-09-23.md)留下的四条精确动线，本轮使用同一 macOS x86_64 debug 开发包，在仓库外解包、安装 npm SDK、离线编译交付的 Rust SDK，再连接交付二进制启动的同一 HTTP Host。原始运行目录、包与二进制 SHA-256、逐项结果见[36 项包验收收据](evidence/r0-cross-sdk-package-acceptance-2026-09-24.json)；对应脚本为 `scripts/verify-local-package.py`。

| 回放条目 | 交接与验收结果 |
|---|---|
| 14185 受限预检 | TS SDK 以仅授予项目 `read/edit/run_jobs/read_artifacts` 的凭证绑定实际导入的 WAV，提交固定修订并获得 `ready=true`、空 issues 的预检；Rust SDK 用同一凭证对同一修订解码为公开 `RenderPreflightInput/ProjectPreflightReport`，结果逐字段一致。两个 SDK 用仅有 `read` 的凭证均得到 `NOT_AUTHORIZED`，预检未改变工程修订。[TS 结果](evidence/r0-cross-sdk-2026-09-24/cross-sdk-preflight-ts.json)、[Rust 结果](evidence/r0-cross-sdk-2026-09-24/cross-sdk-preflight-rust.json)。 |
| 15688 暂存发现／释放／配额 | TS SDK 完整上传 WAV、封存并等待 `media.import` 成功，按项目查询终态上传及保留暂存；Rust SDK 以同一受限凭证用公开类型读取发现页、保留状态和项目配额，执行 `UploadRelease` 后读到零预留；TS SDK 再次确认该上传不在保留页且配额为零。[释放前](evidence/r0-cross-sdk-2026-09-24/cross-sdk-staging-before.json)、[释放后](evidence/r0-cross-sdk-2026-09-24/cross-sdk-staging-after.json)。 |
| 16738 拒绝审计故障 warning | 在隔离工作区给 `denial_records` 注入写入失败触发器，TS 与 Rust SDK 分别请求 owner-only 工具；两次均为 `NOT_AUTHORIZED`，各自只保留一次 `DENIAL_AUDIT_UNAVAILABLE` warning，失败日志未被误记为持久记录。故障触发器随后移除。[TS 结果](evidence/r0-cross-sdk-2026-09-24/cross-sdk-warning-ts.json)、[Rust 结果](evidence/r0-cross-sdk-2026-09-24/cross-sdk-warning-rust.json)。 |
| 17591 TS 审阅、Rust 执行 CAS 计划 | 创建独立的未引用 `.capture` 文件；TS SDK 创建并审阅持久计划及精确 `plan_hash`，计划阶段文件仍在。Rust SDK 用错误 hash 得到 `GC_PLAN_CONFLICT`，再以原计划 ID/hash 执行；TS SDK 复查同一持久回执为 `removed`，实际文件已消失。[审阅计划](evidence/r0-cross-sdk-2026-09-24/cross-sdk-gc-reviewed.json)、[执行回执](evidence/r0-cross-sdk-2026-09-24/cross-sdk-gc-executed.json)。 |

完整包验收为 **36/36 通过**，还包括 30 格冻结历史格式矩阵、真实媒体/Worker/插件/权限动线，以及停服后独立 CLI 重开最终工程。包 SHA-256 为 `42e786a5bbe8674b4fbdb5805bc2cae06969230afdb9ae833644c1ff1cf688ba`，二进制 SHA-256 为 `9e2d2f717e88a1005c4e1f8e1194fa5572a978bc08b86c7050475c4aecd94d91`。四条回放验收意图现可核销；它们不证明真实旧用户库兼容、中文访谈/VFR/长素材性能、release 或跨平台发行，R0 和 R2 整体仍为部分完成。
