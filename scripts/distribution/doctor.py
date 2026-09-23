#!/usr/bin/env python3
"""Inspect this unpacked local development bundle; never opens a workspace."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parent
manifest = json.loads((root / 'manifest.json').read_text())
checks = []
for name, expected in manifest['files'].items():
    path = (root / name).resolve()
    valid = path.is_relative_to(root) and path.is_file()
    valid = valid and path.stat().st_size == expected['size'] and hashlib.sha256(path.read_bytes()).hexdigest() == expected['sha256']
    checks.append({'check': 'bundle_file', 'path': name, 'passed': valid})
for name in ['ffmpeg', 'ffprobe']:
    path = shutil.which(name)
    try:
        value = subprocess.run([path, '-version'], capture_output=True, text=True, timeout=10) if path else None
        checks.append({'check': 'external_tool', 'tool': name, 'passed': value is not None and value.returncode == 0,
                       'version': value.stdout.splitlines()[0] if value and value.stdout else None})
    except (OSError, subprocess.TimeoutExpired) as e:
        checks.append({'check': 'external_tool', 'tool': name, 'passed': False, 'error': str(e)})
try:
    run = subprocess.run([str(root / 'bin/opencut'), '--version'], capture_output=True, text=True, timeout=10)
    checks.append({'check': 'binary_executes', 'passed': run.returncode == 0, 'version': run.stdout.strip()})
except (OSError, subprocess.TimeoutExpired) as e:
    checks.append({'check': 'binary_executes', 'passed': False, 'error': str(e)})
passed = all(item['passed'] for item in checks)
print(json.dumps({'status': 'passed' if passed else 'failed', 'target': manifest['target'], 'checks': checks}, indent=2))
raise SystemExit(0 if passed else 1)
