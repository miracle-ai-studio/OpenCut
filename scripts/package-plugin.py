#!/usr/bin/env python3
"""Wrap an already-built, inspected host runtime in one portable local plugin ZIP."""
import argparse
import hashlib
import importlib.util
import json
import posixpath
from pathlib import Path, PurePosixPath
import shutil
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'plugins/opencut'
spec = importlib.util.spec_from_file_location('hygiene', ROOT / 'scripts/check-source-hygiene.py')
hygiene = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hygiene)


def sha(file):
    digest = hashlib.sha256()
    with Path(file).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def publication_findings(relative, content):
    # Playwright 1.63.0 contains this public Docker-user cache template. Only
    # exempt that exact literal in its pinned dependency, not arbitrary homes.
    if relative == 'bin/libexec/classic-audio-renderer/node_modules/playwright-core/lib/coreBundle.js':
        content = content.replace(b'/home/' + b'pwuser/.cache/ms-playwright', b'${PLAYWRIGHT_BROWSER_CACHE}')
    return hygiene.inspect_file(relative, content)


def inspect_runtime(archive):
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        if len(members) > 100000 or sum(m.size for m in members) > 8 * 1024**3:
            raise ValueError('Runtime archive exceeds the delivery budget')
        tops = {PurePosixPath(m.name).parts[0] for m in members}
        if len(tops) != 1:
            raise ValueError('Runtime archive must contain exactly one host')
        top = next(iter(tops))
        manifest = json.load(tar.extractfile(top + '/manifest.json'))
        seen = set()
        for member in members:
            name = PurePosixPath(member.name)
            if member.name in seen or name.is_absolute() or '..' in name.parts or '\\' in member.name:
                raise ValueError('Unsafe or duplicate runtime archive path')
            seen.add(member.name)
            relative = name.relative_to(top).as_posix()
            if member.issym():
                if (manifest.get('links', {}).get(relative) != member.linkname
                        or not posixpath.normpath(posixpath.join(str(name.parent), member.linkname)).startswith(top + '/')):
                    raise ValueError('Unreviewed runtime link')
                continue
            if member.isdir():
                continue
            if not member.isfile() or member.size > 512 * 1024**2:
                raise ValueError('Unsupported runtime archive member')
            content = tar.extractfile(member).read()
            findings = publication_findings(relative, content)
            if findings:
                raise ValueError(f'Publication hygiene failed: {relative}: {[x[1] for x in findings]}')
            if relative != 'manifest.json':
                expected = manifest['files'].get(relative)
                if expected != {'size': len(content), 'sha256': hashlib.sha256(content).hexdigest()}:
                    raise ValueError('Runtime file identity differs: ' + relative)
        actual_files = {str(PurePosixPath(m.name).relative_to(top)) for m in members if m.isfile()} - {'manifest.json'}
        if actual_files != set(manifest['files']):
            raise ValueError('Runtime inventory differs from the archive')
        return top, manifest


def package(runtime_archive, output):
    top, runtime = inspect_runtime(runtime_archive)
    metadata = json.loads((SOURCE / 'plugin.json').read_text())
    interface = metadata['extensions']['com.openai']['interface']
    if len(interface['shortDescription']) > 30:
        raise ValueError('Plugin subtitle exceeds 30 characters')
    version = metadata['version']
    output.mkdir(parents=True, exist_ok=True)
    destination = output / f'opencut-plugin-{version}-{runtime["target"]}.zip'
    if destination.exists():
        raise FileExistsError('Refusing to replace an existing release ZIP')
    with tempfile.TemporaryDirectory(prefix='opencut-portable-plugin-') as temporary:
        stage = Path(temporary) / 'opencut'
        shutil.copytree(SOURCE, stage)
        for file in stage.rglob('*'):
            if file.is_symlink():
                raise ValueError('Plugin components must not be symlinks')
        for name in ('README.md', 'README.zh-CN.md', 'LICENSE'):
            shutil.copy2(ROOT / name, stage / name)
        shutil.copytree(ROOT / 'docs/images', stage / 'docs/images')
        # Retain a compatibility overlay; the portable root remains authoritative.
        compatibility = {key: metadata[key] for key in ('name', 'version', 'description', 'author')}
        compatibility.update(skills='./skills/', mcpServers='./.mcp.json', interface=interface)
        (stage / '.codex-plugin').mkdir()
        (stage / '.codex-plugin/plugin.json').write_text(json.dumps(compatibility, indent=2) + '\n')
        mcp = json.loads((SOURCE / 'mcp.json').read_text())
        (stage / '.mcp.json').write_text(json.dumps({'mcpServers': mcp['mcpServers']}, indent=2) + '\n')
        release = {'format': 'opencut.plugin-release.v1', 'version': version,
                   'runtime_top_dir': top, 'runtime_archive_sha256': sha(runtime_archive),
                   'binary_sha256': runtime['binary_sha256'], 'target': runtime['target'],
                   'external_dependencies': runtime['external_dependencies'],
                   'scope': 'GitHub local-install prerelease; not submitted to the official directory; full product qualification remains incomplete'}
        (stage / 'release.json').write_text(json.dumps(release, indent=2) + '\n')
        shutil.copy2(runtime_archive, stage / 'runtime.tar.gz')
        with zipfile.ZipFile(destination, 'w') as archive:
            for file in sorted(stage.rglob('*')):
                if file.is_file():
                    archive.write(file, 'opencut/' + file.relative_to(stage).as_posix(), compress_type=zipfile.ZIP_STORED if file.name == 'runtime.tar.gz' else zipfile.ZIP_DEFLATED)
    with zipfile.ZipFile(destination) as archive:
        if archive.testzip() is not None:
            raise ValueError('ZIP integrity check failed')
        if json.loads(archive.read('opencut/plugin.json')) != metadata:
            raise ValueError('Final archive manifest differs')
    checksum = sha(destination)
    destination.with_suffix('.zip.sha256').write_text(checksum + '  ' + destination.name + '\n')
    return {'archive': str(destination), 'sha256': checksum, **release}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime-archive', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(package(args.runtime_archive.resolve(), args.output.resolve()), indent=2))
