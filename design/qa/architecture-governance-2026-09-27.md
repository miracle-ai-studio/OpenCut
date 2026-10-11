# 2026-09-27 架构治理 W0 验收

范围：[整体架构调整方案](../architecture/architecture-renewal-plan.md) 和首批静态依赖治理。当前工作区基于 `8c112728490e7c2a371d330d029141d70646a833`，包含未提交改动。此前剪辑修复保留；本轮没有修改媒体引擎、公共 wire schema、数据库格式或构建新发行二进制。

## 已完成

- 明确领域/合成/应用/入口/宿主/执行/存储/插件/客户端边界，以及同进程端口与跨进程协议的差异。
- 将语义丢失、执行身份、预览、分析证据、编辑规划、节拍、稳像/遮罩、色彩/音频、恢复与发行安排到 W1–W6，并写明每阶段前置与退出证据。
- 登记当前全部 21 个 Rust 模块及 normal/build/dev 依赖。将 12 条既有装配耦合分为 D01/D02/D03，记录归属角色、复核日期和退出条件。
- 新门禁接入根 Moon tasks 和独立 GitHub workflow；根 AGENTS.md 记录后续开发规则；README、目标架构、实现状态和交付台账指向同一方案。

## 本机检查结果

| 检查 | 结果 | 证明范围 |
|---|---|---|
| `python3 -m unittest discover -s scripts/tests -p 'test_architecture.py'` | 14/14 通过 | 未登记模块/依赖、rename+optional+条件依赖、dev/生产隔离、build 依赖、生产环、内部来源替换、未登记 path、到期/已解决例外、永久允许重复、陈旧允许项、移动模块与输入不可变 |
| `python3 scripts/check-architecture.py` | passed；21 modules；12 temporary exception edges | 当前依赖与登记基线一致；并不表示 12 条边已经消除 |
| `python3 scripts/check-contract-boundaries.py` | passed | 既有纯契约、Application 存储/执行、Composition 传递闭包与源码检查继续通过 |
| 空 Cargo 缓存下 `cargo metadata --no-deps --format-version 1 --locked --offline` | exit 0；21 members | 新门禁获取声明依赖图无需下载 crate 或编译原生媒体库 |
| YAML 解析 | workflow、根 moon.yml 通过 Ruby YAML parser | YAML 语法可解析；不是远程 Actions/Moon 执行验收 |
| 方案内相对文件链接 | 无缺失 | 核心文档/源码入口存在 |
| `git diff --check` | 通过 | 已跟踪文件差异无空白错误 |

既有纯契约检查返回闭包规模：contracts=79、media-contracts=79、job-contracts=81、api-contracts=82、sdk=157、application=104、composition=85。该计数仅用于本次环境记录，不应当作稳定协议或健康评分。

扩展文档链接检查发现实现状态中两份历史报告 `r2-mcp-context-2026-09-21.md`、`r2-event-observation-2026-09-21.md` 当前缺失。已保留原文件名并明确标记缺失，未以其他时期的报告替代其证据。

## 被测治理文件身份

| 文件 | SHA-256 |
|---|---|
| `scripts/check-architecture.py` | `548d9e0878012f531a5cff309a537e1fbafbe09b6b669cf309c4d44f49242293` |
| `scripts/tests/test_architecture.py` | `f42feec1cc7e01e605963778b651b1daba50ac0662ab964e8a40ce995604599c` |
| `design/architecture/dependency-policy.json` | `7534cf455d516e1a04417525295a5a108c98daadebf0b4395c6301e4ef566e4d` |
| `.github/workflows/architecture.yml` | `3de8c6f10c04122bea9f0991511cd2d7cd190277ccb346929d9451fde8282e45` |
| `moon.yml` | `517a1323e02f542b160c22b61235654d61e20ed2ebbc515d94157cd3a379679c` |

## 尚未完成

宿主/dispatcher/Worker 抽取、CompileReceipt、ExecutionIdentity、分级复现协议、性能优化和新增剪辑功能仍为计划。本轮没有执行整包发行、媒体回归或跨平台验收，也没有将旧收据提升为当前源码的完整验收。

远程 GitHub Actions、Moon CI 调度和服务端 required check/CODEOWNERS 尚未验证或配置。Rust 函数级越界、TS 导入和动态调用不在新依赖门禁覆盖范围；需通过后续端口抽取、导入限制和行为合同测试继续约束。

结论：**W0 规划与依赖治理基线已完成本地验证；整体项目整治尚未完成。**
