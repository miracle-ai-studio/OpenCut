# 本机外部 Agent 插件

此插件让 Codex / Claude Code 调用 OpenCut。对话和创作规划留在外部 Agent，
OpenCut 继续提供唯一的项目、编辑、预览、撤销和导出服务，不增加内置聊天循环。
本机阶段已交付；远程访问、公开市场发布和真实模型对话验收不在本轮范围。

## 生成

在 checkout 中运行（Python 3.11+）：

```sh
python3 scripts/distribution/agent-plugin.py \
  --binary /absolute/path/to/opencut \
  --database /absolute/path/to/selected-workspace.sqlite \
  --output /absolute/path/to/new-plugin-bundle
```

生成器复用 `agent-config.py` 的连接验证；不打开数据库，不读凭据内容，不修改客户端配置。
输出目录必须不存在；所有路径固定为绝对路径，空格/Unicode 通过 argv 传递。
不支持路径中的字面 `${`，避免客户端变量展开改变连接目标。
可用 `--credential-file` 引用已有凭据，但不创建凭据或修改权限。

输出结构：

```text
bundle/
  README.md                       当前机器的使用命令和连接目标
  manifest.json                   文件与二进制 SHA-256
  configs/                        兼容独立 MCP 客户端的配置片段
  codex/
    .agents/plugins/marketplace.json
    plugins/opencut/.codex-plugin/plugin.json
    plugins/opencut/.mcp.json
    plugins/opencut/skills/edit/SKILL.md
  claude/
    .claude-plugin/marketplace.json
    plugins/opencut/.claude-plugin/plugin.json
    plugins/opencut/.mcp.json
    plugins/opencut/skills/edit/SKILL.md
```

共用同一个 stdio MCP 与 skill 源，不宣称两种客户端的 manifest 通用。
Codex 使用官方仍支持的 compatibility layout，兼容本机已安装的 0.157.1；
Claude Code 使用 `.claude-plugin` 布局。本包不含 hooks 或自动批准工具的配置。

## 加载

Claude Code 单会话加载：`claude --plugin-dir /absolute/bundle/claude/plugins/opencut`，
随后使用 `/opencut:edit`。Codex 通过 `codex plugin marketplace add /absolute/bundle/codex`
及 `codex plugin add opencut@opencut-local` 安装，并在新会话选择 OpenCut Local。
这些安装命令由用户在选定客户端执行；验收不会修改全局配置。

无 credential 的连接是所选数据库的 local-owner，启动 managed Worker，能够修改/删除工程，
并执行该库排队中的任务。不能把本轮演示数据库误当真实项目库。
有 credential 时只复用既有权限，不启动 owner Worker，需要已有执行宿主。
工具仍遵守客户端批准机制。不要为试用插件自动签发凭据、放宽策略或重启生产服务。

`ffmpeg` / `ffprobe` 必须位于启动客户端的 PATH。包没有捆绑系统媒体依赖。
二进制和数据库路径在换机器/移动后需重新生成。本机配置不适用于云端 Agent。

## 可复现验收

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p 'test_agent*.py' -v
PYTHONDONTWRITEBYTECODE=1 python3 scripts/verify-agent-plugin.py \
  --binary target/release/opencut --artifacts-dir /absolute/new-test-workspace
PYTHONDONTWRITEBYTECODE=1 python3 scripts/verify-agent-plugin-clients.py \
  --bundle /absolute/new-test-workspace/plugins --output /absolute/new-client-report
```

第一条检查配置安全和封装边界。第二条冻结已构建 binary，创建独立库和合成素材，
分别直接从两个交付包的 `.mcp.json` 启动服务：

1. MCP 握手、九个 domain tools、能力和精确 action schema 发现。
2. 创建测试工程，导入红/蓝片段，读取工程和素材，删除原始文件。
3. `timeline.plan` 返回只读计划；把其原样 `transaction` 交给 `v3.apply`。
   检查幂等重放、旧修订拒绝及片段位置。
4. 预览 1.5 秒画面应为蓝色，验证真实解码像素。
5. 选择性撤销恢复整个原始工程，重做恢复目标修改。
6. 导出实际 MP4：H.264、3 秒、预期蓝色帧，记录产物 SHA-256。

这是确定性协议客户端验收，不是自然语言模型对话；使用合成无声视频，不代表所有
音频、ASR、Classic Web 功能或跨平台验收。第三条用真实 Codex CLI 读取目录和 manifest，
用真实 Claude Code 严格校验插件及目录，不做安装或模型请求。
完整输出包括原始 MCP trace、Worker 日志、PNG、MP4 和 JSON 报告。

官方格式依据（核对于 2026-10-07）：

- [OpenAI 插件封装与本地目录](https://developers.openai.com/plugins/build/plugins)
- [Claude Code manifest](https://code.claude.com/docs/en/plugins-reference)
- [Claude Code 本地加载](https://code.claude.com/docs/en/plugins)

这套外部 Agent 集成不等同于 OpenCut 工程内部的 WASM planner / preset 插件系统。
后者的权限和安装生命周期保持现有契约。本轮没有修改 Rust 用例、SDK 或 wire schema。
