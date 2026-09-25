#!/usr/bin/env python3
"""Replace an artifact name while its durable removal intent is committing."""

import argparse
from contextlib import closing
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time


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
    ['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=purple:s=160x90:r=10:d=1',
     '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(source)],
    check=True, capture_output=True, timeout=30,
)


def tool(name, input):
    process = subprocess.run(
        [str(binary), '--database', str(database), '--json', 'call', name, '--input', '-'],
        input=json.dumps(input), text=True, capture_output=True, timeout=40,
    )
    value = json.loads(process.stdout)
    assert process.returncode == 0 and value['error'] is None, (name, value, process.stderr)
    return value['data']


def sql(statement, values=()):
    with closing(sqlite3.connect(database, timeout=30)) as connection:
        with connection:
            return connection.execute(statement, values).fetchall()


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
    plan = tool('opencut_artifact_gc_plan', {
        'artifact_id': artifact_id, 'idempotency_key': name + '-plan',
    })
    assert plan['root_count'] == '0' and plan['execution_available'], plan
    return artifact_id, path, original, {'plan_id': plan['plan_id'], 'expected_plan_hash': plan['plan_hash']}


def execute_with_replacement(path, execution, replace, allow_storage_error=False):
    attempt_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    process = subprocess.Popen(
        [str(binary), '--database', str(database), '--json', 'call',
         'opencut_artifact_gc_execute', '--input', '-'],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    try:
        process.stdin.write(json.dumps(execution))
        process.stdin.close()
        process.stdin = None
        deadline = time.monotonic() + 15
        while True:
            assert process.poll() is None and time.monotonic() < deadline, 'Collector did not reach its exclusive attempt lease'
            try:
                fcntl.flock(attempt_fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
                fcntl.flock(attempt_fd, fcntl.LOCK_UN)
            except BlockingIOError:
                break
            time.sleep(.005)
        replace()
        output, stderr = process.communicate(timeout=60)
        value = json.loads(output)
        if allow_storage_error and value['error'] is not None:
            assert process.returncode != 0 and value['error']['code'] == 'STORAGE_ERROR', (value, stderr)
            return 'storage_error', value['error']
        assert process.returncode == 0 and value['error'] is None, (value, stderr)
        assert value['data']['state'] == 'changed' and value['data']['durable'], value
        return 'changed', value['data']
    finally:
        os.close(attempt_fd)
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)


_, file_path, file_original, file_execution = prepare('during-mark-file')
link_id, link_path, link_original, link_execution = prepare('during-mark-symlink')
sql('''CREATE TRIGGER pause_gc_mark_replacement BEFORE INSERT ON artifact_gc_removals
       WHEN NEW.state='removing'
       BEGIN
           SELECT sum(x) FROM (
               WITH RECURSIVE counter(x) AS
               (VALUES(0) UNION ALL SELECT x+1 FROM counter WHERE x<30000000)
               SELECT x FROM counter
           );
       END''')

file_displaced = root / 'file-original.png'
file_replacement = b'Unrelated replacement bytes'


def replace_file():
    os.rename(file_path, file_displaced)
    file_path.write_bytes(file_replacement)


file_outcome, file_result = execute_with_replacement(file_path, file_execution, replace_file)
assert file_outcome == 'changed'
assert file_path.read_bytes() == file_replacement
assert file_displaced.read_bytes() == file_original
assert tool('opencut_artifact_gc_execute', file_execution) == file_result

link_displaced = root / 'link-original.png'
link_target = root / 'outside.png'
link_target.write_bytes(b'Outside bytes must remain')


def replace_link():
    os.rename(link_path, link_displaced)
    os.symlink(link_target, link_path)


link_outcome, link_result = execute_with_replacement(link_path, link_execution, replace_link,
                                                    allow_storage_error=True)
assert link_path.is_symlink() and link_target.read_bytes() == b'Outside bytes must remain'
assert link_displaced.read_bytes() == link_original
if link_outcome == 'changed':
    assert tool('opencut_artifact_gc_execute', link_execution) == link_result
else:
    assert sql('SELECT count(*) FROM artifact_gc_removals WHERE plan_id=?',
               (link_execution['plan_id'],))[0][0] == 0
sql('DROP TRIGGER pause_gc_mark_replacement')
assert sql("SELECT count(*) FROM artifact_gc_removals WHERE state='removing'")[0][0] == 0
assert sql("SELECT count(*) FROM audit_records WHERE action='artifact.gc.removed'")[0][0] == 0
link_path.unlink()
os.rename(link_displaced, link_path)
restored_plan = tool('opencut_artifact_gc_plan', {
    'artifact_id': link_id, 'idempotency_key': 'during-mark-symlink-restored-plan',
})
assert restored_plan['root_count'] == '0' and restored_plan['execution_available'], restored_plan
restored_execution = {'plan_id': restored_plan['plan_id'],
                      'expected_plan_hash': restored_plan['plan_hash']}
assert tool('opencut_artifact_gc_execute', restored_execution)['state'] == 'removed'
assert not link_path.exists()

report = {
    'status': 'passed',
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'checks': [
        'A different regular file installed while durable intent commits is preserved and receives changed, not removed',
        'The displaced original file remains outside the attempt and the exact plan replays its changed receipt',
        'A symlink installed while durable intent commits is preserved without touching its outside target',
        'No unfinished fence or false removed audit remains; after restoring the original name, a new plan removes it',
    ],
    'file_plan_id': file_execution['plan_id'],
    'symlink_plan_id': link_execution['plan_id'],
    'symlink_outcome': link_outcome,
}
(root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'status': 'passed', 'checks': len(report['checks']),
                  'report': str(root / 'report.json')}))
