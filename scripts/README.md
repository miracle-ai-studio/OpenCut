# 构建与验证工具

[开发说明](../docs/development.md)列出构建、边界和行为检查。只有 distribution 中的环境诊断、运行时与 Agent 配置生成器进入宿主包；验收脚本不是产品运行依赖。

| 入口 | 用途 |
|---|---|
| check-architecture.py、check-contract-boundaries.py | 模块与公开契约边界 |
| check-source-hygiene.py | 待发布文件的素材、数据库、凭据与本机路径检查；不扫描忽略文件或 Git 历史 |
| check-canonical-contract.py --binary <CLI> | 唯一工具与领域声明 |
| package-local.py | 本地包、文件清单与身份信息 |
| distribution/ | 环境诊断、受信运行时与 Agent 连接 |
| verify-web.py | 指定二进制和网页的合成浏览器检查 |
| verify-agent.py、verify-agent-workflow.py | 既有工作区上的 Agent 操作与媒体链 |
| verify-package-exchange.py | 既有工作区与固定请求的工程包交换 |
| verify-relocated-runtime.py | 运行时搬迁检查 |
| font-catalog/ | 固定字体来源、许可、生成与派生 |

使用专项脚本前查看 --help、夹具和宿主前置条件。脚本存在不等于当前候选已通过，维护状态及限制见[交付状态](../docs/status.md)。

合成或获准的公开素材、凭证、数据库、日志与浏览器现场放在仓库外。固定候选、有状态检查串行执行，复用请求与成功任务。观察超时查询原 Job，不用重提或强杀掩盖未知结果。
