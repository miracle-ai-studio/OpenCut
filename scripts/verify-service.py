#!/usr/bin/env python3
"""Detached service acceptance using only synthetic local media and external CLI clients."""
import argparse
import hashlib
import http.client
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
from urllib.parse import urlsplit


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/opencut'))
    args = parser.parse_args()
    binary = args.binary.resolve()
    root = Path(tempfile.mkdtemp(prefix='opencut-service-'))
    directory = root / 'service'
    report = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'scope': 'local detached service, synthetic media, not UI or rendering performance',
              'workspace': str(root), 'checks': []}
    base = [str(binary), '--json', '--database', str(root / 'workspace.sqlite')]

    def cli(arguments, stdin=None):
        result = subprocess.run(base + arguments, input=stdin, text=True, capture_output=True,
                                timeout=45, cwd=root)
        value = json.loads(result.stdout)
        assert result.returncode == 0, (arguments, value, result.stderr)
        return value['data']

    def lifecycle(*arguments):
        return cli(['server', '--directory', str(directory), *arguments])

    def check(name):
        report['checks'].append(name)
        print(name, flush=True)

    try:
        started = time.monotonic()
        state = lifecycle('start', '--listen', '127.0.0.1:0')
        report['start_ms'] = round((time.monotonic() - started) * 1000, 2)
        assert state['ready']
        check('start returned ready after launcher exited')
        assert lifecycle('start', '--listen', '127.0.0.1:0')['instance'] == state['instance']
        check('repeat start retained the same instance')
        endpoint = state['endpoint']
        report['instance'] = state['instance']

        def call(name, body):
            return cli(['--endpoint', endpoint, '--credential-file', str(directory / 'api-token'),
                        'call', name, '--input', '-'], json.dumps(body))

        capabilities = call('opencut_capabilities', {})
        assert capabilities
        check('independent CLI authenticated over HTTP')
        source = root / 'blue.mp4'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
                        'color=c=blue:s=64x64:r=24:d=1', '-c:v', 'libx264', '-pix_fmt', 'yuv420p',
                        str(source)], check=True, timeout=30, capture_output=True)
        job = call('opencut_job_start', {'kind': 'preview.frame', 'idempotency_key': 'service-frame',
                   'input': {'input': str(source), 'requested_time': {'ticks': '0', 'timebase': 24},
                             'alignment': 'nearest'}})
        # Submission process has exited. The independent server owns execution.
        deadline = time.monotonic() + 30
        while True:
            job = call('opencut_job_get', {'job_id': job['job_id']})
            if job['state'] in ('succeeded', 'failed', 'cancelled'):
                break
            assert time.monotonic() < deadline, job
            time.sleep(.1)
        assert job['state'] == 'succeeded', job
        check('background worker completed real FFmpeg frame job after submitter exit')
        address = urlsplit(endpoint)
        connection = http.client.HTTPConnection(address.hostname, address.port, timeout=10)
        connection.request('GET', f"/v1/jobs/{job['job_id']}/artifact",
                           headers={'Authorization': 'Bearer ' + (directory / 'api-token').read_text().strip()})
        response = connection.getresponse()
        assert response.status == 200, response.status
        frame = response.read()
        connection.close()
        digest = hashlib.sha256(frame).hexdigest()
        assert digest == job['result']['artifact']['content_hash'], job['result']
        destination = root / 'frame.png'
        destination.write_bytes(frame)
        raw = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(destination),
                                       '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=15)
        assert len(raw) == 64 * 64 * 3
        pixel = list(raw[(32 * 64 + 32) * 3:(32 * 64 + 32) * 3 + 3])
        assert pixel[0] < 10 and pixel[1] < 10 and pixel[2] > 240, pixel
        report.update(artifact_sha256=digest, center_rgb=pixel, job_id=job['job_id'])
        check('authenticated artifact download matched hash and independently expected blue pixels')
        durations = []
        for _ in range(10):
            before = time.monotonic()
            assert lifecycle('status')['ready']
            durations.append(round((time.monotonic() - before) * 1000, 2))
        report['status_cli_ms'] = {'samples': durations, 'median': statistics.median(durations),
                                   'max': max(durations)}
        check('ten separate status clients found service ready')
        assert lifecycle('stop')['state'] == 'stopped'
        check('graceful stop released service lock')
        second = lifecycle('start', '--listen', '127.0.0.1:0')
        assert second['instance'] != state['instance']
        check('restart obtained a fresh instance')
        report['status'] = 'passed'
    finally:
        try:
            lifecycle('stop')
        finally:
            path = root / 'report.json'
            path.write_text(json.dumps(report, indent=2) + '\n')
            print(f'Report: {path}', flush=True)


if __name__ == '__main__':
    main()
