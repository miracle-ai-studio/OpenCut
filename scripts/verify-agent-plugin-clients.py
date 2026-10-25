#!/usr/bin/env python3
"""Read-only native client checks; never install a plugin or run a model turn."""
import argparse
import json
from pathlib import Path
import subprocess


def verify(bundle, output):
    output.mkdir(parents=True, exist_ok=False)
    runs = []

    def run(label, command):
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        (output / (label + '.log')).write_text(result.stdout + result.stderr)
        runs.append({'check': label, 'command': command, 'exit_code': result.returncode})
        if result.returncode:
            raise RuntimeError(f'{label} failed; see {output / (label + ".log")}')
        return result.stdout

    try:
        receipt = json.loads((bundle / 'manifest.json').read_text())
        manifests = [json.loads((bundle / client / 'plugins/opencut' /
                     f'.{client}-plugin/plugin.json').read_text()) for client in ('codex', 'claude')]
        assert all(manifest['version'] == receipt['version'] for manifest in manifests)
        codex_version = run('codex-version', ['codex', '--version']).strip()
        claude_version = run('claude-version', ['claude', '--version']).strip()
        # Command-scoped config overrides, no marketplace add/install or user config writes.
        listed = json.loads(run('codex-catalog', ['codex', '-c',
            'marketplaces.opencut-local.source_type="local"', '-c',
            'marketplaces.opencut-local.source=' + json.dumps(str(bundle / 'codex')),
            'plugin', 'list', '--available', '--json', '--marketplace', 'opencut-local']))
        entries = listed['available'] + listed['installed']
        entry = next(p for p in entries if p['pluginId'] == 'opencut@opencut-local')
        assert entry['version'] == receipt['version']
        assert Path(entry['source']['path']).resolve() == (bundle / 'codex/plugins/opencut').resolve()
        run('claude-plugin', ['claude', 'plugin', 'validate', '--strict', str(bundle / 'claude/plugins/opencut')])
        run('claude-marketplace', ['claude', 'plugin', 'validate', '--strict', str(bundle / 'claude')])
        report = {'status': 'passed', 'plugin_version': receipt['version'],
                  'codex_version': codex_version, 'claude_version': claude_version,
                  'codex': 'Native local marketplace discovery and plugin manifest parsing',
                  'claude': 'Native strict plugin and marketplace validation',
                  'limits': ['Does not install into global clients or grant persistent tool permissions',
                             'Does not run an external agent conversation or desktop UI',
                             'MCP execution acceptance is recorded by verify-agent-plugin.py'], 'runs': runs}
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    except BaseException:
        (output / 'failed.json').write_text(json.dumps({'status': 'failed', 'runs': runs}, indent=2) + '\n')
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    verify(args.bundle.resolve(), args.output.resolve())
