#!/usr/bin/env python3
"""Inspect a local bundle and its actual media runtime without opening a workspace."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

# Required by the current renderer; project-specific preflight is authoritative.
FILTERS = {
    'video': {'color', 'fps', 'scale', 'pad', 'crop', 'format', 'overlay', 'trim', 'setpts', 'select'},
    'visual': {'geq', 'eq', 'gblur'},
    'audio': {'anullsrc', 'atrim', 'asetpts', 'aresample', 'aformat', 'asetrate', 'volume', 'adelay', 'apad', 'amix'},
}
ENCODERS = {'libx264', 'aac', 'png'}


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def registered_names(output):
    return {parts[1] for line in output.splitlines()
            if len(parts := line.split()) >= 3 and set(parts[0]) <= set('.TSCAVFBDX') and parts[1] != '='}


def inspect(root):
    root = Path(root).resolve()
    manifest = json.loads((root / 'manifest.json').read_text())
    checks = []
    for name, expected in manifest['files'].items():
        try:
            path = (root / name).resolve()
            valid = path.is_relative_to(root) and path.is_file()
            valid = valid and path.stat().st_size == expected['size'] and sha(path) == expected['sha256']
            checks.append({'check': 'bundle_file', 'path': name, 'passed': valid})
        except OSError as error:
            checks.append({'check': 'bundle_file', 'path': name, 'passed': False, 'error': str(error)})

    def run(args):
        return subprocess.run([str(p) for p in args], capture_output=True, text=True, timeout=20, check=True)

    installed = {}
    needs_node = 'classic_packet_rate_runtime' in manifest or 'node' in manifest.get('external_dependencies', [])
    for name in ['ffmpeg', 'ffprobe'] + (['node'] if needs_node else []):
        override = os.environ.get('OPENCUT_CLASSIC_AUDIO_NODE') if name == 'node' else None
        path = override if override is not None else shutil.which(name)
        try:
            if override is not None:
                configured = Path(override)
                if not configured.is_absolute() or not configured.is_file() or not os.access(configured, os.X_OK):
                    raise FileNotFoundError('Configured Node executable is unavailable')
                path = str(configured.resolve())
            if not path:
                raise FileNotFoundError(f'{name} not found on PATH')
            value = run([path, '--version' if name == 'node' else '-version'])
            installed[name] = path
            checks.append({'check': 'external_tool', 'tool': name, 'passed': True,
                           'path': path, 'version': value.stdout.splitlines()[0]})
        except (OSError, subprocess.SubprocessError, IndexError) as error:
            checks.append({'check': 'external_tool', 'tool': name, 'passed': False, 'error': str(error)})
    try:
        value = run([root / 'bin/opencut', '--version'])
        checks.append({'check': 'binary_executes', 'passed': True, 'version': value.stdout.strip()})
    except (OSError, subprocess.SubprocessError) as error:
        checks.append({'check': 'binary_executes', 'passed': False, 'error': str(error)})
    if 'ffmpeg' in installed:
        try:
            filters = registered_names(run([installed['ffmpeg'], '-hide_banner', '-filters']).stdout)
            encoders = registered_names(run([installed['ffmpeg'], '-hide_banner', '-encoders']).stdout)
            for profile, required in {**FILTERS, 'export_encoders': ENCODERS}.items():
                missing = sorted(required - (encoders if profile == 'export_encoders' else filters))
                checks.append({'check': 'media_capabilities', 'profile': profile, 'passed': not missing, 'missing': missing})
        except (OSError, subprocess.SubprocessError) as error:
            checks.append({'check': 'media_capabilities', 'passed': False, 'error': str(error)})
    if {'ffmpeg', 'ffprobe'} <= installed.keys():
        try:
            with tempfile.TemporaryDirectory(prefix='opencut-doctor-') as temporary:
                output = Path(temporary) / 'smoke.mp4'
                run([installed['ffmpeg'], '-v', 'error', '-f', 'lavfi', '-i', 'color=c=red:s=32x32:r=10:d=0.2',
                     '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000:duration=0.2',
                     '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', output])
                probe = json.loads(run([installed['ffprobe'], '-v', 'error', '-show_streams', '-of', 'json', output]).stdout)
                streams = {stream['codec_type']: stream['codec_name'] for stream in probe['streams']}
                checks.append({'check': 'media_encode_probe', 'passed': streams == {'video': 'h264', 'audio': 'aac'}, 'streams': streams})
        except (OSError, subprocess.SubprocessError, ValueError, KeyError) as error:
            checks.append({'check': 'media_encode_probe', 'passed': False, 'error': str(error)})
    return {'status': 'passed' if all(item['passed'] for item in checks) else 'failed',
            'target': manifest['target'], 'checks': checks,
            'scope': 'Bundle integrity and baseline H.264/AAC runtime; project fonts, alternate codecs and exact rendering require project preflight'
                     + ('; declared packet runtime Node executable checked, packet media acceptance remains separate' if needs_node else '')}


if __name__ == '__main__':
    try:
        report = inspect(Path(__file__).resolve().parent)
    except (OSError, ValueError, KeyError, TypeError) as error:
        report = {'status': 'failed', 'error': str(error)}
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report['status'] == 'passed' else 1)
