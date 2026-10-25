#!/usr/bin/env python3
"""A real HTTP download must fence explicit collection until verification ends."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import fcntl
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import urllib.error
import urllib.request


parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--artifacts-dir', type=Path, required=True)
args = parser.parse_args()
binary = args.binary.resolve()
root = args.artifacts_dir.resolve()
root.mkdir(parents=True, exist_ok=False)
database = root / 'workspace.sqlite'
source = root / 'source.mp4'
subprocess.run(
    ['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=red:s=160x90:r=10:d=1',
     '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(source)],
    check=True, capture_output=True, timeout=30,
)


def cli(name, input):
    process = subprocess.run(
        [str(binary), '--database', str(database), '--json', 'call', name, '--input', '-'],
        input=json.dumps(input), text=True, capture_output=True, timeout=30,
    )
    value = json.loads(process.stdout)
    assert process.returncode == 0 and value['error'] is None, (name, value, process.stderr)
    return value['data']


def prepare(name):
    job = cli('opencut_job_submit', {
        'kind': 'preview.frame', 'idempotency_key': name + '-frame',
        'input': {'input': str(source), 'requested_time': {'ticks': '0', 'timebase': 10},
                  'alignment': 'exact'},
    })
    worker = subprocess.run(
        [str(binary), '--database', str(database), 'worker', 'run', '--once'],
        text=True, capture_output=True, timeout=90,
    )
    assert worker.returncode == 0, worker.stderr
    job = cli('opencut_job_get', {'job_id': job['job_id']})
    assert job['state'] == 'succeeded', job
    artifact = job['result']['artifact']
    published = Path(artifact['path'])
    original = published.read_bytes()
    assert hashlib.sha256(original).hexdigest() == artifact['content_hash']
    artifact_id = 'artifact:' + job['job_id'] + ':' + artifact['content_hash']
    assert cli('opencut_artifact_retention_release', {
        'artifact_id': artifact_id, 'idempotency_key': name + '-release',
    })['state'] == 'released'
    plan = cli('opencut_artifact_gc_plan', {
        'artifact_id': artifact_id, 'idempotency_key': name + '-plan',
    })
    assert plan['root_count'] == '0' and plan['execution_available'], plan
    execution = {'plan_id': plan['plan_id'], 'expected_plan_hash': plan['plan_hash']}
    return job, published, original, execution


job, published, original, execution = prepare('download-lock')
_, file_path, file_bytes, file_execution = prepare('file-replacement')
directory_job, directory_path, directory_bytes, directory_execution = prepare('directory-replacement')
_, hardlink_path, hardlink_bytes, hardlink_execution = prepare('hardlink-replacement')

with socket.socket() as probe:
    probe.bind(('127.0.0.1', 0))
    port = probe.getsockname()[1]
token_path = root / 'token'
service_log = (root / 'service.log').open('wb')
service = subprocess.Popen(
    [str(binary), '--database', str(database), 'serve', '--listen', f'127.0.0.1:{port}',
     '--token-file', str(token_path)],
    stdout=service_log, stderr=service_log,
)
gate_fd = None
attempt_fd = None
try:
    deadline = time.monotonic() + 15
    while not token_path.exists():
        assert service.poll() is None and time.monotonic() < deadline
        time.sleep(.02)
    token = token_path.read_text().strip()
    base = f'http://127.0.0.1:{port}'
    while True:
        assert service.poll() is None and time.monotonic() < deadline
        try:
            with urllib.request.urlopen(
                urllib.request.Request(base + '/api/capabilities',
                                       headers={'Authorization': 'Bearer ' + token}),
                timeout=1,
            ) as response:
                assert response.status == 200
            break
        except (urllib.error.URLError, TimeoutError):
            time.sleep(.02)

    def http(path, body=None):
        headers = {'Authorization': 'Bearer ' + token}
        if body is not None:
            body = json.dumps(body).encode()
            headers['Content-Type'] = 'application/json'
        with urllib.request.urlopen(
            urllib.request.Request(base + path, data=body, headers=headers), timeout=20,
        ) as response:
            content = response.read()
        return content if body is None else json.loads(content)

    attempt = published.parent
    attempt_fd = os.open(attempt, os.O_RDONLY | os.O_DIRECTORY)
    fcntl.flock(attempt_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    fcntl.flock(attempt_fd, fcntl.LOCK_UN)
    gate_fd = os.open(root / '.storage-budget', os.O_RDONLY | os.O_DIRECTORY)
    # A shared budget gate lets SQLite readers enter. It holds the download
    # after ArtifactSource acquired its shared attempt lock, just before the
    # anonymous verified copy reserves disk space.
    fcntl.flock(gate_fd, fcntl.LOCK_SH)
    with ThreadPoolExecutor(max_workers=1) as pool:
        downloading = pool.submit(http, '/api/jobs/' + job['job_id'] + '/artifact')
        try:
            deadline = time.monotonic() + 10
            while True:
                assert service.poll() is None and time.monotonic() < deadline
                try:
                    fcntl.flock(attempt_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    fcntl.flock(attempt_fd, fcntl.LOCK_UN)
                except BlockingIOError:
                    break
                time.sleep(.005)
            assert not downloading.done(), 'Download completed before the exclusive-lock check'
            busy = http('/api/tools/opencut_artifact_gc_execute', execution)
            assert busy['error'] is None and busy['data']['state'] == 'busy'
            assert busy['data']['durable'] is False and published.read_bytes() == original
            assert not downloading.done(), 'Download ended while collection reported busy'
        finally:
            fcntl.flock(gate_fd, fcntl.LOCK_UN)
        assert downloading.result(timeout=20) == original
    removed = http('/api/tools/opencut_artifact_gc_execute', execution)
    assert removed['error'] is None and removed['data']['state'] == 'removed', removed
    assert not published.exists()

    replacement = file_path.with_name('replacement.tmp')
    replacement.write_bytes(b'X' * len(file_bytes))
    os.replace(replacement, file_path)
    changed = http('/api/tools/opencut_artifact_gc_execute', file_execution)
    assert changed['error'] is None and changed['data']['state'] == 'changed', changed
    assert changed['data']['durable'] and file_path.read_bytes() == b'X' * len(file_bytes)

    moved = root / (directory_job['job_id'] + '-old-attempt')
    os.rename(directory_path.parent, moved)
    directory_path.parent.mkdir()
    os.rename(moved / directory_path.name, directory_path)
    changed_directory = http('/api/tools/opencut_artifact_gc_execute', directory_execution)
    assert changed_directory['error'] is None and changed_directory['data']['state'] == 'changed', changed_directory
    assert changed_directory['data']['durable'] and directory_path.read_bytes() == directory_bytes

    hardlink = root / 'outside-hardlink.png'
    os.link(hardlink_path, hardlink)
    changed_hardlink = http('/api/tools/opencut_artifact_gc_execute', hardlink_execution)
    assert changed_hardlink['error'] is None and changed_hardlink['data']['state'] == 'changed', changed_hardlink
    assert changed_hardlink['data']['durable']
    assert hardlink.read_bytes() == hardlink_bytes and hardlink_path.read_bytes() == hardlink_bytes
    report = {
        'status': 'passed',
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'checks': [
            'A real HTTP artifact verifier holds the shared attempt inode while its admission waits',
            'Concurrent exact-plan collection reports non-durable busy without unlinking verified bytes',
            'After the HTTP download verifies original bytes, the same plan removes only the published file',
            'A same-size replacement inode is preserved and recorded as changed',
            'Replacing the attempt directory while moving the original file back is preserved and recorded as changed',
            'Adding an external hard link prevents deletion of both names and records changed',
        ],
        'observed_states': [busy['data']['state'], removed['data']['state'],
                            changed['data']['state'], changed_directory['data']['state'],
                            changed_hardlink['data']['state']],
        'download_sha256': hashlib.sha256(original).hexdigest(),
        'directory': str(root),
    }
    (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'status': report['status'], 'checks': len(report['checks']),
                      'report': str(root / 'report.json')}))
finally:
    if gate_fd is not None:
        os.close(gate_fd)
    if attempt_fd is not None:
        os.close(attempt_fd)
    if service.poll() is None:
        service.terminate()
        service.wait(timeout=10)
    service_log.close()
