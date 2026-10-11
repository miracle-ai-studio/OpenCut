# 开发与交付检查

本目录用于开发、构建和验收；仅 `distribution/` 中的环境诊断和配置生成器随宿主包发布。

| 范围 | 入口 |
|---|---|
| Rust 模块与依赖边界 | `check-architecture.py`、`check-contract-boundaries.py` |
| 已发布工具兼容性 | `check-wire-compatibility.py` |
| 门禁与打包负向测试 | `python3 -m unittest discover -s scripts/tests` |
| 验收执行器故障测试 | `python3 -m unittest discover -s scripts -p test_acceptance_runner.py` |
| 独立后台服务 | `verify-service.py --binary target/release/opencut`；CLI 启停/状态、脱离提交进程后的真实取帧、产物哈希与期望像素 |
| 本地交付包 | `package-local.py`、`verify-local-package.py` |
| 联动剪辑与托管 MCP Worker | `verify-editorial.py`；合成声画、三入口、权限、撤销重做及断线恢复 |
| 真实本地自动字幕 | `verify-auto-captions.py --model <本机模型> --web-root <构建或包内web> --artifacts-dir <隔离目录> --verify-browser`；合成语音、原生ASR、固定修订字幕、撤销重做、字幕像素及独立PCM，然后完整网页生成/取消与视口验收 |
| 各能力实际验收 | `verify-*.py`；部分脚本调用配套的 `.mjs` 客户端 |

`verify-local-package.py` 在仓库外解包，验证可执行文件、SDK、真实媒体及调用结果。选定检查与完整验收分别出具结果，不互相替代。运行前通过脚本的 `--help` 查看参数和本机依赖。

`verify-control-vfs.py` 与 `verify-control-budget-policy.py` 已承担数据库写入准入回归，原先仅用于确认缺口的一次性审计脚本已删除。历史结论仍保留在 QA 记录中。

测试素材应使用合成或获准的公开样本。运行日志和临时工作区放在仓库外，保留程序哈希、命令与结果；不要把用户素材加入源码或远程 CI。


## Web 编辑器验收

`python3 scripts/verify-web.py --binary target/release/opencut` 在仓库外创建隔离服务和合成音视频，通过真实 Chromium 执行连接、新建、上传、编辑、连续预览、字幕、撤销/重做、刷新后导出及下载解码。首次先在 `apps/web` 执行 `npx playwright install chromium`；也可用 `OPENCUT_CHROME` 指定本机浏览器可执行文件。报告包含二进制与页面 hash、浏览器版本、截图目录、实际 PCM 观测。`--web-root <解包目录/web>` 可验证交付包页面；脚本结束会停止自己启动的服务。

此脚本只生成合成素材，不读取用户素材或凭证。浏览器文件保存接口在测试中接入受控字节接收器，随后独立解码；不表示已测试所有操作系统的原生保存对话框。实时播放性能、Safari/Firefox、4K 和大型工程另行验收。

`OPENCUT_BINARY=<binary> OPENCUT_WEB_ROOT=<web> node apps/web/scripts/verify-upload.mjs` 使用本机合成的 64 MiB WAV 验证单次读取、有界顺序分块、摘要冲突和独立 worker 完整性拒绝；每轮报告首块等待、传输与入库时间。可用 `OPENCUT_BASELINE_BINARY` / `OPENCUT_BASELINE_WEB` 指定旧版，连续三轮比较。性能比较应在其他构建/媒体验收结束后运行；仍是单机小样本，不代表 4K 或所有文件类型。


## 本机外部 Agent 插件

- `distribution/agent-plugin.py`：生成独立 Codex / Claude Code 插件包，复用 `agent-config.py`；不安装、不打开数据库。
- `distribution/agent-plugin/SKILL.md`：共享读取、计划/原子写入、预览、撤销与导出工作流。
- `verify-agent-plugin.py`：合成素材、隔离数据库，分别从两个插件配置运行完整 MCP 动线并验证真实像素/MP4。
- `verify-agent-plugin-clients.py`：原生 Codex 目录发现和 Claude 严格校验，无持久安装或模型请求。

用法及限制见 `design/architecture/local-agent-plugins.md`；本轮证据见 `design/qa/evidence/local-agent-plugins-2026-10-07.json`。
