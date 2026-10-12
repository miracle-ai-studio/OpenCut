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



def copy_sdk_sources(source, destination):
    """Ship Rust source, excluding local caches and editor residue."""
    destination.mkdir(parents=True)
    for path in sorted(source.iterdir()):
        if path.name in {'.DS_Store', '__pycache__', 'target', 'node_modules'} or path.suffix in {'.pyc', '.bak', '.tmp', '.orig', '.rej'}:
            continue
        if path.is_symlink():
            raise ValueError(f'Symlink in SDK source: {path}')
        target = destination / path.name
        if path.is_dir():
            copy_sdk_sources(path, target)
        elif path.suffix == '.rs':
            shutil.copy2(path, target)
        else:
            raise ValueError(f'Unreviewed file in SDK source delivery: {path}')


def validate_npm_files(files):
    """Only public runtime modules, declarations and package metadata are delivered."""
    expected = {'package.json', 'README.md', 'LICENSE'}
    for item in files:
        name = item['path']
        if name in expected:
            continue
        path = Path(name)
        if (len(path.parts) == 2 and path.parts[0] == 'dist'
                and (path.name.endswith('.js') or path.name.endswith('.d.ts'))
                and not any(marker in path.name for marker in ('.test.', '.spec.'))):
            continue
        raise ValueError(f'Unreviewed file in npm delivery: {name}')

def copy_web_build(source, destination):
    """Ship only public build assets; reject workspace data, maps and symlinks."""
    if not (source / 'index.html').is_file():
        raise ValueError('Web build is missing index.html')
    allowed_root = {'index.html', 'favicon.ico', 'manifest.json', 'robots.txt'}
    allowed_assets = {'.js', '.css', '.svg', '.png', '.jpg', '.jpeg', '.webp', '.woff2'}
    for file in sorted(source.rglob('*')):
        relative = file.relative_to(source)
        if file.is_symlink():
            raise ValueError(f'Symlink in Web build: {relative}')
        if file.is_dir():
            continue
        if str(relative) not in allowed_root and not (relative.parts[0] == 'assets' and file.suffix in allowed_assets):
            raise ValueError(f'Unreviewed Web build file: {relative}')
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(file, target)


def copy_packet_rate_runtime(packages_root, destination):
    """Copy only the fixed Node packet probe, not the browser/DSP runtime."""
    selected = [
        'classic-audio-renderer/package.json',
        'classic-audio-renderer/node_modules/mediabunny/package.json',
        'classic-audio-renderer/node_modules/mediabunny/dist/bundles/mediabunny.mjs',
        'classic-audio-renderer/node_modules/mediabunny/LICENSE',
        'classic-source-art-renderer/package.json',
        'classic-source-art-renderer/host/packet-rate.mjs',
    ]
    packages_root = Path(packages_root)
    resolved_root = packages_root.resolve(strict=True)
    for relative in selected:
        file = packages_root / relative
        for ancestor in [file, *file.parents]:
            if ancestor.is_symlink():
                raise ValueError(f'Symlink in packet runtime: {relative}')
            if ancestor == packages_root:
                break
        if not file.resolve(strict=True).is_relative_to(resolved_root) or not file.is_file():
            raise ValueError(f'Invalid packet runtime file: {relative}')
        limit = 64 * 1024 if file.name == 'package.json' else 8 * 1024 * 1024
        if not 0 < file.stat().st_size <= limit:
            raise ValueError(f'Packet runtime file exceeds budget: {relative}')
    expected = [
        ('classic-audio-renderer/package.json', '@opencut/classic-audio-renderer', '0.1.0'),
        ('classic-source-art-renderer/package.json', '@opencut/classic-source-art-renderer', '0.1.0'),
        ('classic-audio-renderer/node_modules/mediabunny/package.json', 'mediabunny', '1.41.0'),
    ]
    manifests = {}
    for relative, name, version in expected:
        manifest = json.loads((packages_root / relative).read_text())
        if manifest.get('name') != name or manifest.get('version') != version or manifest.get('type') != 'module':
            raise ValueError(f'Unpinned packet runtime package: {relative}')
        manifests[name] = manifest
    dependencies = manifests['@opencut/classic-audio-renderer'].get('dependencies', {})
    if dependencies.get('mediabunny') != '1.41.0' or dependencies.get('playwright-core') != '1.63.0':
        raise ValueError('Unpinned packet runtime dependencies')
    for relative in selected:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(packages_root / relative, target)
    return {
        'profile': 'classic-packet-rate-v1-node-only',
        'mediabunny_version': '1.41.0',
        'library_license': manifests['mediabunny'].get('license'),
        'scope': 'packet-only runtime; package names preserve trusted discovery',
        'external_runtime': 'node',
        'excluded': 'Chromium, full audio/visual rendering and preview require separately configured runtimes',
    }


def package(output, skip_build, supplied_binary=None):
    # Build assets away from the directory a running service may be serving.
    with tempfile.TemporaryDirectory(prefix='opencut-package-web-') as web_directory:
        return package_with_web_root(output, skip_build, supplied_binary, Path(web_directory))


def package_with_web_root(output, skip_build, supplied_binary, web_root):
    if not skip_build and supplied_binary is None:
        run(['cargo', 'build', '--locked', '--release', '-p', 'opencut-cli'])
    run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], ROOT / 'apps/web')
    run(['npm', 'run', 'build', '--', '--outDir', web_root], ROOT / 'apps/web')
    binary = supplied_binary or ROOT / 'target/release/opencut'
    assert binary.is_file(), 'Build the release CLI first'
    run(['node', ROOT / 'packages/sdk-ts/scripts/generate.mjs', '--check', '--skip-build', '--binary', binary])
    tools = json.loads(run([binary, '--json', 'system', 'schema']))['data']
    assert tools == json.loads(
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
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='opencut-package-') as temporary:
        stage = Path(temporary)
        sdk_ts = ROOT / 'packages/sdk-ts'
        run(['npm', 'run', 'build'], sdk_ts)
        npm_package = json.loads(run(['npm', 'pack', '--ignore-scripts', '--json', '--pack-destination', stage], sdk_ts))[0]
        validate_npm_files(npm_package['files'])
        packed = npm_package['filename']
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
                    copy_sdk_sources(source / folder, destination / folder)
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
        packet_runtime = copy_packet_rate_runtime(ROOT / 'packages', host / 'bin/libexec')
        shutil.copy2(ROOT / 'LICENSE', host / 'LICENSE')
        copy_web_build(web_root, host / 'web')
        shutil.copy2(ROOT / 'scripts/distribution/doctor.py', host / 'doctor.py')
        shutil.copy2(ROOT / 'scripts/distribution/agent-config.py', host / 'agent-config.py')
        shutil.copy2(ROOT / 'design/architecture/local-distribution.md', host / 'README.md')
        with (host / 'README.md').open('a') as readme:
            readme.write('\n\nBundled Classic packet runtime\n\n'
                         'bin/libexec contains only the Node packet-rate helper and locked Mediabunny 1.41.0. '
                         'Install Node.js externally on PATH (or configure OPENCUT_CLASSIC_AUDIO_NODE). '
                         'The classic-audio-renderer package name supports trusted discovery; this packet-only subset '
                         'does not include Chromium, complete audio/visual rendering or preview. '
                         'Those features still require a separately configured full runtime.\n')
        for contract in ['project-plugin-contract.md', 'denial-audit-contract.md',
                         'storage-governance-contract.md', 'cas-gc-contract.md',
                         'storage-admission-contract.md', 'database-control-admission.md',
                         'native-child-write-contract.md', 'artifact-retention-contract.md']:
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
                    'external_dependencies': ['ffmpeg', 'ffprobe', 'node'],
                    'classic_packet_rate_runtime': packet_runtime,
                    'scope': 'local Web, CLI, HTTP, MCP and SDKs; bundled Classic packet-only runtime; full browser audio/visual runtime external; full R0-R7 acceptance not complete'}
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
