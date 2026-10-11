#!/usr/bin/env python3
"""Build separate local Codex and Claude Code plugins without installing either."""
import argparse
import hashlib
import importlib.util
import json
import shlex
from pathlib import Path
import tempfile


HERE = Path(__file__).resolve().parent


def generate(binary, database, output, credential=None):
    output = Path(output).expanduser().resolve()
    if output.exists():
        raise FileExistsError('Output already exists; choose a new directory')
    spec = importlib.util.spec_from_file_location('opencut_agent_config', HERE / 'agent-config.py')
    config = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(config)
    skill = (HERE / 'agent-plugin' / 'SKILL.md').read_text()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.opencut-plugin-', dir=output.parent) as temporary:
        stage = Path(temporary) / 'bundle'
        stage.mkdir(mode=0o700)
        connection = config.generate(binary, database, stage / 'configs', credential=credential)
        server = json.loads((stage / 'configs' / 'mcp-server.json').read_text())

        def write(relative, value):
            path = stage / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            content = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, indent=2) + '\n'
            path.write_text(content)
            path.chmod(0o600)

        identity = {'name': 'opencut', 'version': '0.1.0',
                    'description': 'Read, edit, preview, undo and export local OpenCut projects.',
                    'author': {'name': 'OpenCut'}}
        for client in ('codex', 'claude'):
            plugin = f'{client}/plugins/opencut'
            manifest = dict(identity)
            if client == 'codex':
                # Supported compatibility layout also works with the locally installed CLI.
                manifest.update(skills='./skills/', mcpServers='./.mcp.json', interface={
                    'displayName': 'OpenCut Local', 'shortDescription': identity['description'],
                    'developerName': 'OpenCut', 'category': 'Productivity', 'capabilities': ['Read', 'Write']})
            write(f'{plugin}/.{client}-plugin/plugin.json', manifest)
            write(f'{plugin}/.mcp.json', {'mcpServers': {'opencut': server}})
            write(f'{plugin}/skills/edit/SKILL.md', skill)
        write('codex/.agents/plugins/marketplace.json', {
            'name': 'opencut-local', 'interface': {'displayName': 'OpenCut Local'},
            'plugins': [{'name': 'opencut', 'source': {'source': 'local', 'path': './plugins/opencut'},
                         'policy': {'installation': 'AVAILABLE', 'authentication': 'ON_INSTALL'},
                         'category': 'Productivity'}]})
        write('claude/.claude-plugin/marketplace.json', {
            'name': 'opencut-local', 'owner': {'name': 'OpenCut'},
            'metadata': {'description': 'Local OpenCut editing tools and workflow.'},
            'plugins': [{**identity, 'source': './plugins/opencut'}]})
        write('README.md', f'''# OpenCut 本机 Agent 插件

分别提供 Codex 和 Claude Code 适配包，共用 OpenCut stdio MCP 和剪辑工作流。
这是当前机器的配置包，包含绝对路径；移动可执行文件或换机器后请重新生成。
不会修改 Agent 配置、创建凭据、启动公网服务或打开数据库。

- 可执行文件：`{server['command']}`
- 数据库：`{Path(database).expanduser().resolve()}`
- 身份：`{connection['identity']}`
- 内置 Worker：`{connection['managed_worker']}`
- 运行依赖：Python 3.11+（生成/验收），FFmpeg/ffprobe（渲染，须可从客户端 PATH 找到）。

## 使用

Claude Code 可仅对本次会话加载，不改全局插件配置：

```sh
claude --plugin-dir {shlex.quote(str(output / 'claude/plugins/opencut'))}
```

然后使用 `/opencut:edit`，例如“列出项目和素材；把指定片段缩短一秒，预览结果，并导出 MP4”。

Codex 支持本地 marketplace；下列安装命令会修改你自己的 Codex 配置，生成器不会代为执行：

```sh
codex plugin marketplace add {shlex.quote(str(output / 'codex'))}
codex plugin add opencut@opencut-local
```

在新会话中选择 OpenCut Local 插件并请求剪辑。桌面端可在插件目录选择本地来源后安装。
如果当前客户端不支持插件目录，`configs/codex.toml` 是可审核的 MCP 配置片段，
`configs/claude.mcp.json` 是独立 MCP 配置；仅配置 MCP 不会自动安装工作流 skill。

本包不预先批准任何工具。未指定 credential 的 stdio 连接是所选数据库的本机 owner，
具备写入/删除能力，Worker 会处理该数据库中的排队任务；不应拿演示配置替换生产配置。
有 credential 时复用其现有权限且不内置 Worker，需要已有受管理 Worker。
成功提交任务不等于渲染成功；工作流会检查终态和实际产物。
不含自动授权、账号连接、后台安装钩子、远程部署或 OpenCut 内置聊天界面。

## 格式依据

- https://developers.openai.com/plugins/build/plugins （Codex compatibility layout）
- https://code.claude.com/docs/en/plugins-reference （Claude manifest）
- https://code.claude.com/docs/en/plugins （session-only --plugin-dir）
''')
        files = {str(p.relative_to(stage)): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in sorted(stage.rglob('*')) if p.is_file()}
        receipt = {'format': 'opencut.local-agent-plugins.v1', 'version': '0.1.0',
                   'identity': connection['identity'], 'managed_worker': connection['managed_worker'],
                   'binary_sha256': hashlib.sha256(Path(server['command']).read_bytes()).hexdigest(),
                   'database': str(Path(database).expanduser().resolve()), 'files': files,
                   'scope': 'local client adapters; not installed; no client conversation acceptance'}
        write('manifest.json', receipt)
        stage.rename(output)
    return {'status': 'generated', 'directory': str(output), **receipt}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--database', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--credential-file', type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(generate(args.binary, args.database, args.output, args.credential_file), ensure_ascii=False, indent=2))
    except (OSError, ValueError) as error:
        parser.exit(1, f'{error}\n')
