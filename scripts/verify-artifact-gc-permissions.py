#!/usr/bin/env python3
"""Verify Unix artifact collection never deletes through revoked file permissions."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess


parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--artifacts-dir', type=Path, required=True)
args = parser.parse_args()
assert os.geteuid() != 0, 'Permission fixture requires a non-root owner'
binary = args.binary.resolve()
root = args.artifacts_dir.resolve()
root.mkdir(parents=True, exist_ok=False)
database = root / 'workspace.sqlite'
source = root / 'source.mp4'
subprocess.run(
    ['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=yellow:s=160x90:r=10:d=1',
     '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(source)],
    check=True, capture_output=True, timeout=30,
)


def tool(name, input, expected_error=None):
    process = subprocess.run(
        [str(binary), '--database', str(database), '--json', 'call', name, '--input', '-'],
        input=json.dumps(input), text=True, capture_output=True, timeout=40,
    )
    value = json.loads(process.stdout)
    if expected_error:
        assert process.returncode and value['error']['code'] == expected_error, (name, value)
    else:
        assert process.returncode == 0 and value['error'] is None, (name, value, process.stderr)
    return value['error'] if expected_error else value['data']


def prepare(name):
    queued = tool('opencut_job_submit', {
        'kind': 'preview.frame', 'idempotency_key': name + '-frame',
        'input': {'input': str(source), 'requested_time': {'ticks': '0', 'timebase': 10},
                  'alignment': 'exact'},
    })
    worker = subprocess.run([str(binary), '--database', str(database), 'worker', 'run', '--once'],
                            capture_output=True, text=True, timeout=90)
    assert worker.returncode == 0, worker.stderr
    job = tool('opencut_job_get', {'job_id': queued['job_id']})
    assert job['state'] == 'succeeded', job
    artifact = job['result']['artifact']
    path = Path(artifact['path'])
    original = path.read_bytes()
    assert hashlib.sha256(original).hexdigest() == artifact['content_hash']
    artifact_id = 'artifact:' + job['job_id'] + ':' + artifact['content_hash']
    assert tool('opencut_artifact_retention_release', {
        'artifact_id': artifact_id, 'idempotency_key': name + '-release',
    })['state'] == 'released'
    return artifact_id, path, original


def plan(artifact_id, name):
    value = tool('opencut_artifact_gc_plan', {
        'artifact_id': artifact_id, 'idempotency_key': name,
    })
    assert value['root_count'] == '0' and value['execution_available'], value
    return {'plan_id': value['plan_id'], 'expected_plan_hash': value['plan_hash']}


directory_id, directory_path, directory_bytes = prepare('directory-permission')
file_id, file_path, file_bytes = prepare('file-permission')
directory_execution = plan(directory_id, 'directory-plan')
file_execution = plan(file_id, 'file-plan')

directory_mode = stat.S_IMODE(directory_path.parent.stat().st_mode)
try:
    directory_path.parent.chmod(0)
    directory_denial = tool('opencut_artifact_gc_execute', directory_execution, 'STORAGE_ERROR')
finally:
    directory_path.parent.chmod(directory_mode)
assert directory_path.read_bytes() == directory_bytes
assert tool('opencut_artifact_gc_execution_inspect', {
    'plan_id': directory_execution['plan_id'],
})['durable'] is False
removed_directory = tool('opencut_artifact_gc_execute', directory_execution)
assert removed_directory['state'] == 'removed' and removed_directory['durable']
assert not directory_path.exists()

file_mode = stat.S_IMODE(file_path.stat().st_mode)
try:
    file_path.chmod(0)
    changed = tool('opencut_artifact_gc_execute', file_execution)
    assert changed['state'] == 'changed' and changed['durable'], changed
finally:
    file_path.chmod(file_mode)
assert file_path.read_bytes() == file_bytes
assert tool('opencut_artifact_gc_execute', file_execution) == changed
new_execution = plan(file_id, 'file-plan-after-permission-restore')
assert new_execution['plan_id'] != file_execution['plan_id']
removed_file = tool('opencut_artifact_gc_execute', new_execution)
assert removed_file['state'] == 'removed' and removed_file['durable']
assert not file_path.exists()

report = {
    'status': 'passed',
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'checks': [
        'Revoked attempt-directory access fails in managed storage admission without deleting bytes or recording an execution intent',
        'Restoring directory access lets the exact original plan remove the unchanged artifact',
        'Revoked file access changes its native identity, so the old plan records changed and preserves bytes',
        'After file permission restoration, a new reviewed plan removes those original bytes',
    ],
    'directory_plan_id': directory_execution['plan_id'],
    'file_plan_id': file_execution['plan_id'],
    'restored_file_plan_id': new_execution['plan_id'],
    'directory_denial_code': directory_denial['code'],
}
(root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'status': 'passed', 'checks': len(report['checks']),
                  'report': str(root / 'report.json')}))
