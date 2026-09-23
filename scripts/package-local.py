#!/usr/bin/env python3
"""Build an explicit local development delivery: host, npm SDK and portable Rust SDK source."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def run(args, cwd=ROOT):
    # Cargo/npm progress belongs to stderr, including lock-wait messages on a
    # successful JSON command. Never merge it into machine-readable stdout.
    result = subprocess.run([str(a) for a in args], cwd=cwd, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'Command failed ({result.returncode}): {args}\n{result.stdout}\n{result.stderr}')
    return result.stdout


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def archive(source, target):
    with target.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz, tarfile.open(fileobj=gz, mode='w') as tar:
        for path in sorted(source.rglob('*')):
            if path.is_symlink():
                raise ValueError(f'Unexpected symlink in delivery source: {path}')
            if not path.is_file():
                continue
            info = tar.gettarinfo(str(path), arcname=f'{source.name}/{path.relative_to(source)}')
            info.uid = info.gid = 0
            info.uname = info.gname = ''
            info.mtime = 0
            info.mode = 0o755 if path.parent.name == 'bin' else 0o644
            with path.open('rb') as f:
                tar.addfile(info, f)


def package(output, skip_build, supplied_binary=None):
    if not skip_build and supplied_binary is None:
        run(['cargo', 'build', '--locked', '--release', '-p', 'opencut-cli'])
    binary = supplied_binary or ROOT / 'target/release/opencut'
    assert binary.is_file(), 'Build the release CLI first'
    run(['node', ROOT / 'packages/sdk-ts/scripts/generate.mjs', '--check', '--skip-build', '--binary', binary])
    assert json.loads(run([binary, '--json', 'system', 'schema']))['data'] == json.loads(
        (ROOT / 'packages/contract-fixtures/tools.generated.json').read_text()), 'Release binary and generated contracts differ'
    assert json.loads(run([binary, '--json', 'system', 'events-schema']))['data'] == json.loads(
        (ROOT / 'packages/contract-fixtures/event-stream.generated.json').read_text()), 'Release binary and event stream schema differ'
    target = next(line.split(': ', 1)[1] for line in run(['rustc', '-vV']).splitlines() if line.startswith('host: '))
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    with tempfile.TemporaryDirectory(prefix='opencut-package-schema-') as schema_temp:
        schema_db = Path(schema_temp) / 'schema.sqlite'
        run([binary, '--database', schema_db, '--json', 'project', 'list'])
        with sqlite3.connect(schema_db) as connection:
            database_schema = int(connection.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0])
    domains = json.loads(run([binary, '--json', 'system', 'domains']))['data']
    tools = json.loads(run([binary, '--json', 'system', 'schema']))['data']
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opencut-package-') as temporary:
        stage = Path(temporary)
        sdk_ts = ROOT / 'packages/sdk-ts'
        run(['npm', 'run', 'build'], sdk_ts)
        packed = json.loads(run(['npm', 'pack', '--ignore-scripts', '--json', '--pack-destination', stage], sdk_ts))[0]['filename']
        rust = stage / 'opencut-rust-sdk'
        rust.mkdir()
        pure = ['contracts', 'media-contracts', 'job-contracts', 'api-contracts', 'sdk-rust']
        for name in pure:
            source, destination = ROOT / 'crates' / name, rust / 'crates' / name
            destination.mkdir(parents=True)
            for filename in ['Cargo.toml', 'README.md']:
                if (source / filename).is_file():
                    shutil.copy2(source / filename, destination / filename)
            for folder in ['src', 'examples']:
                if (source / folder).is_dir():
                    shutil.copytree(source / folder, destination / folder)
        (rust / 'Cargo.toml').write_text('[workspace]\nresolver="3"\nmembers=["crates/*"]\n[workspace.package]\n'+f'version="{version}"\nedition="2024"\nlicense="MIT"\n')
        shutil.copy2(ROOT / 'Cargo.lock', rust / 'Cargo.lock')
        shutil.copy2(ROOT / 'LICENSE', rust / 'LICENSE')
        (rust / 'README.md').write_text('OpenCut portable Rust SDK source delivery\n\nUse a path dependency on crates/sdk-rust after extracting this entire archive.\nThe sibling contract crates and workspace Cargo.toml are required.\nRun cargo test --locked -p opencut-sdk from this directory.\nThird-party dependencies are pinned by Cargo.lock and fetched from Cargo registries; this is not an offline vendor bundle.\nNo native runtime or project database is part of this SDK.\n')
        metadata = json.loads(run(['cargo', 'metadata', '--offline', '--format-version', '1'], rust))
        assert all(not p['name'].startswith('opencut-') or p['name'] in ['opencut-contracts', 'opencut-media-contracts', 'opencut-job-contracts', 'opencut-api-contracts', 'opencut-sdk'] for p in metadata['packages'])
        rust_archive = stage / f'opencut-rust-sdk-{version}.tar.gz'
        archive(rust, rust_archive)
        host = stage / f'opencut-{version}-{target}'
        (host / 'bin').mkdir(parents=True)
        shutil.copy2(binary, host / 'bin/opencut')
        shutil.copy2(ROOT / 'LICENSE', host / 'LICENSE')
        shutil.copy2(ROOT / 'scripts/distribution/doctor.py', host / 'doctor.py')
        shutil.copy2(ROOT / 'design/architecture/local-distribution.md', host / 'README.md')
        for contract in ['project-plugin-contract.md', 'denial-audit-contract.md',
                         'storage-governance-contract.md', 'cas-gc-contract.md',
                         'storage-admission-contract.md', 'database-control-admission.md',
                         'native-child-write-contract.md']:
            shutil.copy2(ROOT / 'design/architecture' / contract, host / contract)
        (host / 'sdk').mkdir()
        shutil.copy2(stage / packed, host / 'sdk' / packed)
        shutil.copy2(rust_archive, host / 'sdk' / rust_archive.name)
        (host / 'contracts').mkdir()
        for name in ['tools.generated.json', 'openapi.generated.json', 'event-stream.generated.json', 'edit-journey.json']:
            shutil.copy2(ROOT / 'packages/contract-fixtures' / name, host / 'contracts' / name)
        host_metadata = json.loads(run(['cargo', 'metadata', '--locked', '--format-version', '1']))
        nodes = {n['id']: n for n in host_metadata['resolve']['nodes']}
        packages = {p['id']: p for p in host_metadata['packages']}
        pending = [p['id'] for p in packages.values() if p['name'] == 'opencut-cli']
        reached = set()
        while pending:
            current = pending.pop()
            if current in reached:
                continue
            reached.add(current)
            pending.extend(nodes[current]['dependencies'])
        inventory = sorted([{'name': packages[i]['name'], 'version': packages[i]['version'], 'license': packages[i]['license']} for i in reached], key=lambda p: (p['name'], p['version']))
        (host / 'dependency-inventory.json').write_text(json.dumps(inventory, indent=2)+'\n')
        files = {str(p.relative_to(host)): {'sha256': sha(p), 'size': p.stat().st_size} for p in sorted(host.rglob('*')) if p.is_file()}
        manifest = {'format': 'opencut.local-bundle.v1', 'version': version, 'maturity': 'development', 'target': target,
                    'api_version': '1', 'database_schema': database_schema, 'operation_count': len(tools),
                    'domain_count': len(domains), 'binary_sha256': sha(binary), 'files': files,
                    'external_dependencies': ['ffmpeg', 'ffprobe'], 'scope': 'local CLI, HTTP, MCP and SDKs; full R0-R7 acceptance not complete'}
        (host / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
        bundle_id = sha(host / 'manifest.json')[:12]
        destination = output / f'opencut-{version}-dev-{bundle_id}-{target}.tar.gz'
        assembled = stage / destination.name
        archive(host, assembled)
        if destination.exists():
            assert sha(destination) == sha(assembled), 'Existing delivery differs; refusing to overwrite'
        else:
            shutil.copy2(assembled, destination)
        digest = sha(destination)
        Path(str(destination)+'.sha256').write_text(f'{digest}  {destination.name}\n')
        receipt = {'status': 'packaged_not_yet_accepted', 'archive': str(destination), 'sha256': digest, 'binary_sha256': sha(binary), 'target': target, 'manifest': manifest}
        Path(str(destination)+'.json').write_text(json.dumps(receipt, indent=2)+'\n')
        print(json.dumps({k: v for k, v in receipt.items() if k != 'manifest'}, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, default=ROOT / 'dist/local')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--binary', type=Path, help='Package an already-built local CLI instead of building release')
    args = parser.parse_args()
    package(args.output.resolve(), args.skip_build, args.binary.resolve() if args.binary else None)
