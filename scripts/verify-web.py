#!/usr/bin/env python3
"""Detached same-origin Web acceptance with synthetic local media and a real browser."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/opencut')
    parser.add_argument('--web-root', type=Path, help='Verify a supplied built Web tree, including extracted bundles')
    args = parser.parse_args()
    binary = args.binary.resolve()
    if args.web_root:
        web = args.web_root.resolve()
    else:
        subprocess.run(['npm', 'run', 'build', '--prefix', str(ROOT / 'apps/web')], check=True)
        web = ROOT / 'apps/web/dist'
    workspace = Path(tempfile.mkdtemp(prefix='opencut-web-'))
    service = workspace / 'service'
    def cli(*arguments):
        result = subprocess.run([str(binary), '--json', '--database', str(workspace / 'workspace.sqlite'),
                                 'server', '--directory', str(service), *arguments], capture_output=True, text=True, timeout=60, cwd=workspace)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return json.loads(result.stdout)['data']
    try:
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
                        'testsrc2=size=640x360:rate=24:duration=4', '-f', 'lavfi', '-i',
                        'sine=frequency=440:sample_rate=48000:duration=4', '-c:v', 'libx264',
                        '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', str(workspace / 'sample.mp4')], check=True)
        subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','sine=frequency=220:sample_rate=48000:duration=4',str(workspace / 'sample.wav')],check=True)
        state = cli('start', '--listen', '127.0.0.1:0', '--web-root', str(web))
        assert state['ready'] and state['web_url']
        env = {**os.environ, 'OPENCUT_WEB_URL': state['endpoint'], 'OPENCUT_WEB_WORKSPACE': str(workspace)}
        result = subprocess.run(['node', str(ROOT / 'apps/web/scripts/verify-browser.mjs')], env=env, cwd=ROOT, timeout=600)
        report_path = workspace / 'browser-report.json'
        report = json.loads(report_path.read_text())
        report['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        report['web_files'] = {str(file.relative_to(web)): hashlib.sha256(file.read_bytes()).hexdigest()
                               for file in sorted(web.rglob('*')) if file.is_file()}
        report['workspace'] = str(workspace)
        report_path.write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps({'status': report['status'], 'report': str(report_path)}, indent=2))
        if result.returncode or report['status'] != 'passed':
            raise SystemExit(1)
    finally:
        cli('stop')


if __name__ == '__main__':
    main()
