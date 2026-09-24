#!/usr/bin/env python3
"""Kill the delivered collector inside both durable removal boundaries."""

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
    ['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=blue:s=160x90:r=10:d=1',
     '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(source)],
    check=True, capture_output=True, timeout=30,
)


def command(name, input, expected_error=None):
    process = subprocess.run(
        [str(binary), '--database', str(database), '--json', 'call', name, '--input', '-'],
        input=json.dumps(input), text=True, capture_output=True, timeout=30,
    )
    value = json.loads(process.stdout)
    if expected_error:
        assert process.returncode != 0 and value['error']['code'] == expected_error, value
    else:
        assert process.returncode == 0 and value['error'] is None, (name, value, process.stderr)
    return value.get('data')


def sql(statement, values=()):
    with closing(sqlite3.connect(database, timeout=30)) as connection:
        with connection:
            return connection.execute(statement, values).fetchall()


def prepare(name):
    queued = command('opencut_job_submit', {
        'kind': 'preview.frame', 'idempotency_key': name + '-frame',
        'input': {'input': str(source), 'requested_time': {'ticks': '0', 'timebase': 10},
                  'alignment': 'exact'},
    })
    worker = subprocess.run(
        [str(binary), '--database', str(database), 'worker', 'run', '--once'],
        text=True, capture_output=True, timeout=90,
    )
    assert worker.returncode == 0, worker.stderr
    job = command('opencut_job_get', {'job_id': queued['job_id']})
    assert job['state'] == 'succeeded', job
    artifact = job['result']['artifact']
    path = Path(artifact['path'])
    contents = path.read_bytes()
    assert hashlib.sha256(contents).hexdigest() == artifact['content_hash']
    artifact_id = 'artifact:' + job['job_id'] + ':' + artifact['content_hash']
    assert command('opencut_artifact_retention_release', {
        'artifact_id': artifact_id, 'idempotency_key': name + '-release',
    })['state'] == 'released'
    plan = command('opencut_artifact_gc_plan', {
        'artifact_id': artifact_id, 'idempotency_key': name + '-plan',
    })
    assert plan['root_count'] == '0' and plan['execution_available'], plan
    execution = {'plan_id': plan['plan_id'], 'expected_plan_hash': plan['plan_hash']}
    return path, contents, execution


def long_trigger(name, table, condition):
    # A bounded CPU-only SQL loop keeps the real CLI inside the chosen SQLite
    # transaction long enough for SIGKILL. It does not alter product code.
    sql(f'''CREATE TRIGGER {name} BEFORE INSERT ON {table}
        WHEN {condition}
        BEGIN
            SELECT sum(x) FROM (
                WITH RECURSIVE counter(x) AS
                (VALUES(0) UNION ALL SELECT x+1 FROM counter WHERE x<100000000)
                SELECT x FROM counter
            );
        END''')


def start():
    return subprocess.Popen(
        [str(binary), '--database', str(database), '--json', 'call',
         'opencut_artifact_gc_execute', '--input', '-'],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )


first_path, first_bytes, first_execution = prepare('kill-before-intent')
second_path, second_bytes, second_execution = prepare('kill-before-receipt')

long_trigger('pause_gc_mark', 'artifact_gc_removals', "NEW.state='removing'")
attempt_fd = os.open(first_path.parent, os.O_RDONLY | os.O_DIRECTORY)
first = start()
try:
    first.stdin.write(json.dumps(first_execution))
    first.stdin.close()
    deadline = time.monotonic() + 15
    while True:
        assert first.poll() is None and time.monotonic() < deadline, 'Collector did not reach its exclusive attempt lease'
        try:
            fcntl.flock(attempt_fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
            fcntl.flock(attempt_fd, fcntl.LOCK_UN)
        except BlockingIOError:
            break
        time.sleep(.005)
    first.kill()
    assert first.wait(timeout=10) != 0
finally:
    os.close(attempt_fd)
    if first.poll() is None:
        first.kill()
        first.wait()
assert first_path.read_bytes() == first_bytes
assert sql('SELECT count(*) FROM artifact_gc_removals WHERE plan_id=?',
           (first_execution['plan_id'],))[0][0] == 0
assert sql("SELECT count(*) FROM audit_records WHERE action='artifact.gc.removing'")[0][0] == 0
sql('DROP TRIGGER pause_gc_mark')
first_removed = command('opencut_artifact_gc_execute', first_execution)
assert first_removed['state'] == 'removed' and first_removed['durable'] and not first_path.exists()

long_trigger('pause_gc_result', 'audit_records', "NEW.action='artifact.gc.removed'")
second = start()
try:
    second.stdin.write(json.dumps(second_execution))
    second.stdin.close()
    deadline = time.monotonic() + 15
    while second_path.exists():
        assert second.poll() is None and time.monotonic() < deadline, 'Collector did not unlink after durable intent'
        time.sleep(.005)
    assert sql('SELECT state FROM artifact_gc_removals WHERE plan_id=?',
               (second_execution['plan_id'],))[0][0] == 'removing'
    second.kill()
    assert second.wait(timeout=10) != 0
finally:
    if second.poll() is None:
        second.kill()
        second.wait()
assert not second_path.exists()
assert command('opencut_artifact_gc_execution_inspect', {
    'plan_id': second_execution['plan_id'],
})['state'] == 'removing'
blocked_project = {
    'project': {'id': 'fenced', 'name': 'Fenced', 'sequence_id': 'main',
                'width': 160, 'height': 90,
                'fps': {'numerator': 10, 'denominator': 1}},
    'idempotency_key': 'fenced-create',
}
command('opencut_project_create', blocked_project, 'ARTIFACT_COLLECTION_RECOVERY_REQUIRED')
sql('DROP TRIGGER pause_gc_result')
recovered = command('opencut_artifact_gc_execute', second_execution)
assert recovered['state'] == 'absent' and recovered['durable'], recovered
assert command('opencut_artifact_gc_execute', second_execution) == recovered
assert command('opencut_project_create', blocked_project)['project']['id'] == 'fenced'
assert sql('PRAGMA integrity_check')[0][0] == 'ok'

report = {
    'status': 'passed',
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'checks': [
        'SIGKILL after exclusive attempt lease but before durable intent rolls back the mark and preserves source bytes',
        'The exact original plan then removes those bytes once and records a durable receipt',
        'SIGKILL after durable intent and unlink but before receipt leaves a persistent removing fence',
        'A new project publication is fenced until exact retry records absent and restores writes',
    ],
    'first_plan_id': first_execution['plan_id'],
    'second_plan_id': second_execution['plan_id'],
    'directory': str(root),
}
(root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'status': report['status'], 'checks': len(report['checks']),
                  'report': str(root / 'report.json')}))
