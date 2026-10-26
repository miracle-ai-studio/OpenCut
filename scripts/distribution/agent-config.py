#!/usr/bin/env python3
"""Generate reviewable MCP configurations; never edits an Agent's configuration."""
import argparse
import json
import os
from pathlib import Path
import re
import tempfile


def generate(binary, database, output, name='opencut', credential=None, *, service_directory=None, with_worker=False):
    if not re.fullmatch(r'[A-Za-z0-9_-]+', name):
        raise ValueError('Server name must contain only letters, digits, underscores or hyphens')
    binary, database, output = (Path(p).expanduser().resolve() for p in (binary, database, output))
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError('OpenCut binary must be an executable file')
    if database.is_dir():
        raise ValueError('Database must be a file path')
    args = ['--database', str(database)]
    if credential:
        credential = Path(credential).expanduser().resolve()
        if not credential.is_file():
            raise ValueError('Credential file does not exist')
        args += ['--credential-file', str(credential)]
    if with_worker and credential:
        raise ValueError('A managed Worker requires local-owner mode')
    if with_worker and service_directory is not None:
        raise ValueError('Choose an independent service or an explicit portable Worker')
    args += ['mcp', 'serve']
    if with_worker:
        args += ['--with-worker']
    else:
        service_directory = Path(service_directory or Path.home() / '.opencut/server').expanduser().resolve()
        args += ['--service-directory', str(service_directory)]
    # Claude expands ${...} in config values. Reject these literal paths rather
    # than silently redirecting the executable/database through an environment.
    if any('${' in value for value in [str(binary), *args]):
        raise ValueError('Literal ${ sequences in paths cannot be represented safely in all target hosts')
    server = {'type': 'stdio', 'command': str(binary), 'args': args}
    # JSON basic strings are also TOML strings for these OS paths. ensure_ascii
    # is disabled to avoid JSON surrogate pairs, which TOML does not permit.
    quoted = lambda value: json.dumps(value, ensure_ascii=False)
    codex = (f'[mcp_servers.{name}]\ncommand = {quoted(str(binary))}\n'
             f'args = {quoted(args)}\nstartup_timeout_sec = 30\ntool_timeout_sec = 120\n')
    files = {
        'codex.toml': codex,
        'claude.mcp.json': json.dumps({'mcpServers': {name: server}}, ensure_ascii=False, indent=2)+'\n',
        'mcp-server.json': json.dumps(server, ensure_ascii=False, indent=2)+'\n',
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        raise FileExistsError('Output directory already exists; choose a new directory')
    # Stage in the same parent, then publish a complete directory. A concurrent
    # generator cannot replace a populated destination. No configuration is read.
    with tempfile.TemporaryDirectory(prefix='.opencut-config-', dir=output.parent) as temporary:
        stage = Path(temporary) / 'config'
        stage.mkdir(mode=0o700)
        for filename, content in files.items():
            path = stage / filename
            with path.open('x', encoding='utf-8') as stream:
                stream.write(content)
            path.chmod(0o600)
        stage.rename(output)
    return {'status': 'generated', 'directory': str(output), 'files': list(files),
            'identity': 'restricted' if credential else 'local-owner',
            'managed_worker': with_worker,
            'requires_external_worker': not with_worker,
            'service_directory': str(service_directory) if not with_worker else None}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path(__file__).resolve().parent / 'bin/opencut')
    parser.add_argument('--database', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New output directory; existing directories are refused')
    parser.add_argument('--name', default='opencut')
    parser.add_argument('--credential-file', type=Path)
    parser.add_argument('--service-directory', type=Path, help='Existing independent service (default: ~/.opencut/server)')
    parser.add_argument('--with-worker', action='store_true', help='Explicit portable session; its Worker ends when this MCP session closes')
    args = parser.parse_args()
    try:
        print(json.dumps(generate(args.binary, args.database, args.output, args.name, args.credential_file, service_directory=args.service_directory, with_worker=args.with_worker), ensure_ascii=False, indent=2))
    except (OSError, ValueError) as error:
        parser.exit(1, f'{error}\n')
