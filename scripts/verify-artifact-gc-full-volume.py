#!/usr/bin/env python3
"""Real macOS ENOSPC at both artifact collection database commit windows."""

import argparse
from contextlib import closing
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import sqlite3
import subprocess
import time


parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--artifacts-dir', type=Path, required=True)
args = parser.parse_args()
if platform.system() != 'Darwin':
    raise RuntimeError('Physical full-volume fixture requires macOS hdiutil')
binary = args.binary.resolve()
root = args.artifacts_dir.resolve()
root.mkdir(parents=True, exist_ok=False)
mount = root / 'volume'
mount.mkdir()
image = root / 'volume.dmg'
source = root / 'source.mp4'
subprocess.run(
    ['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'color=green:s=160x90:r=10:d=1',
     '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(source)],
    check=True, capture_output=True, timeout=30,
)
subprocess.run(['hdiutil', 'create', '-size', '64m', '-fs', 'HFS+',
                '-volname', 'OpenCutArtifactGcFullQA', str(image)],
               check=True, capture_output=True, text=True, timeout=45)
attached = False
process = None
trace = []


def run(*arguments, input=None, timeout=45):
    result = subprocess.run([str(binary), '--database', str(database), '--json', *arguments],
                            input=json.dumps(input or {}), text=True, capture_output=True,
                            timeout=timeout)
    value = json.loads(result.stdout)
    trace.append({'command': arguments, 'exit_code': result.returncode,
                  'error': value['error'], 'stderr': result.stderr[-500:]})
    return result, value


def tool(name, input=None, error=None):
    result, value = run('call', name, '--input', '-', input=input)
    if error:
        assert result.returncode and value['error']['code'] == error, (name, value, result.stderr)
    else:
        assert result.returncode == 0 and value['error'] is None, (name, value, result.stderr)
    return value.get('data')


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
    return path, original, {'plan_id': plan['plan_id'], 'expected_plan_hash': plan['plan_hash']}


def slow_trigger(name, table, condition):
    sql(f'''CREATE TRIGGER {name} BEFORE INSERT ON {table}
        WHEN {condition}
        BEGIN
            SELECT sum(x) FROM (
                WITH RECURSIVE counter(x) AS
                (VALUES(0) UNION ALL SELECT x+1 FROM counter WHERE x<100000000)
                SELECT x FROM counter
            );
        END''')


def launch(execution):
    return subprocess.Popen(
        [str(binary), '--database', str(database), '--json', 'call',
         'opencut_artifact_gc_execute', '--input', '-'],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )


def full_volume():
    filled = 0
    with filler.open('r+b', buffering=0) as stream:
        stream.seek(0, os.SEEK_END)
        for size in (1 << 20, 4096, 512):
            block = b'X' * size
            while True:
                try:
                    filled += stream.write(block)
                except OSError as exc:
                    assert exc.errno == errno.ENOSPC, exc
                    break
        try:
            os.fsync(stream.fileno())
        except OSError as exc:
            assert exc.errno == errno.ENOSPC, exc
    available = os.statvfs(mount).f_bavail * os.statvfs(mount).f_frsize
    assert available < 4096, available
    return available, filled


try:
    subprocess.run(['hdiutil', 'attach', '-nobrowse', '-mountpoint', str(mount), str(image)],
                   check=True, capture_output=True, text=True, timeout=45)
    attached = True
    assert mount.stat().st_dev != root.stat().st_dev
    database = mount / 'workspace.sqlite'
    result, recovery = run('storage', 'recovery', 'configure', '--input', '-', input={
        'expected_revision': 0, 'idempotency_key': 'small-volume',
        'payload_limit_bytes': str(64 << 30), 'minimum_available_bytes': '0',
        'media_output_limit_bytes': str(1 << 20),
    })
    assert result.returncode == 0 and recovery['error'] is None, recovery
    after_path, _, after_execution = prepare('full-after-unlink')
    before_path, before_bytes, before_execution = prepare('full-before-mark')
    filler = mount / 'external-filler'
    filler.touch()

    slow_trigger('pause_gc_result_full', 'audit_records', "NEW.action='artifact.gc.removed'")
    process = launch(after_execution)
    process.stdin.write(json.dumps(after_execution))
    process.stdin.close()
    process.stdin = None
    deadline = time.monotonic() + 15
    while after_path.exists():
        assert process.poll() is None and time.monotonic() < deadline, 'Unlink window was not reached'
        time.sleep(.005)
    assert sql('SELECT state FROM artifact_gc_removals WHERE plan_id=?',
               (after_execution['plan_id'],))[0][0] == 'removing'
    available_after_unlink, filled_after_unlink = full_volume()
    output, stderr = process.communicate(timeout=60)
    after_error = json.loads(output)['error']['code']
    trace.append({'window': 'after-unlink', 'exit_code': process.returncode,
                  'error_code': after_error, 'stderr': stderr[-500:]})
    assert process.returncode != 0 and after_error == 'DATABASE_CAPACITY_EXHAUSTED', (output, stderr)
    assert not after_path.exists()
    assert sql('SELECT state FROM artifact_gc_removals WHERE plan_id=?',
               (after_execution['plan_id'],))[0][0] == 'removing'
    assert sql("SELECT count(*) FROM audit_records WHERE action='artifact.gc.removed'")[0][0] == 0
    filler.unlink()
    sql('DROP TRIGGER pause_gc_result_full')
    blocked = {
        'project': {'id': 'fenced', 'name': 'Fenced', 'sequence_id': 'main',
                    'width': 160, 'height': 90,
                    'fps': {'numerator': 10, 'denominator': 1}},
        'idempotency_key': 'fenced-create',
    }
    tool('opencut_project_create', blocked, 'ARTIFACT_COLLECTION_RECOVERY_REQUIRED')
    recovered = tool('opencut_artifact_gc_execute', after_execution)
    assert recovered['state'] == 'absent' and recovered['durable'], recovered
    assert tool('opencut_artifact_gc_execute', after_execution) == recovered
    assert tool('opencut_project_create', blocked)['project']['id'] == 'fenced'

    filler.touch()
    mark_audit_before = sql("SELECT count(*) FROM audit_records WHERE action='artifact.gc.removing'")[0][0]
    slow_trigger('pause_gc_mark_full', 'artifact_gc_removals', "NEW.state='removing'")
    attempt_fd = os.open(before_path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        process = launch(before_execution)
        process.stdin.write(json.dumps(before_execution))
        process.stdin.close()
        process.stdin = None
        deadline = time.monotonic() + 15
        while True:
            assert process.poll() is None and time.monotonic() < deadline, 'Mark window was not reached'
            try:
                fcntl.flock(attempt_fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
                fcntl.flock(attempt_fd, fcntl.LOCK_UN)
            except BlockingIOError:
                break
            time.sleep(.005)
        available_before_mark, filled_before_mark = full_volume()
        output, stderr = process.communicate(timeout=60)
        before_error = json.loads(output)['error']['code']
        trace.append({'window': 'before-mark', 'exit_code': process.returncode,
                      'error_code': before_error, 'stderr': stderr[-500:]})
        assert process.returncode != 0 and before_error == 'DATABASE_CAPACITY_EXHAUSTED', (output, stderr)
    finally:
        os.close(attempt_fd)
    assert before_path.read_bytes() == before_bytes
    assert sql('SELECT count(*) FROM artifact_gc_removals WHERE plan_id=?',
               (before_execution['plan_id'],))[0][0] == 0
    assert sql("SELECT count(*) FROM audit_records WHERE action='artifact.gc.removing'")[0][0] == mark_audit_before
    filler.unlink()
    sql('DROP TRIGGER pause_gc_mark_full')
    assert tool('opencut_artifact_gc_execute', before_execution)['state'] == 'removed'
    assert not before_path.exists()
    assert sql('PRAGMA integrity_check')[0][0] == 'ok'
    report = {
        'status': 'passed',
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'volume_capacity_bytes': os.statvfs(mount).f_blocks * os.statvfs(mount).f_frsize,
        'available_after_unlink_bytes': available_after_unlink,
        'available_before_mark_bytes': available_before_mark,
        'after_unlink_error_code': after_error,
        'before_mark_error_code': before_error,
        'filler_after_unlink_bytes': filled_after_unlink,
        'filler_before_mark_bytes': filled_before_mark,
        'checks': [
            'At physical ENOSPC after unlink, final receipt fails and the durable removing fence remains',
            'After external capacity returns, the same plan records absent once and restores publication',
            'At physical ENOSPC before durable intent, the original bytes and audit remain unchanged',
            'After external capacity returns, the exact original plan removes bytes and database integrity is ok',
        ],
    }
    (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'status': 'passed', 'checks': len(report['checks']),
                      'report': str(root / 'report.json')}))
finally:
    if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=10)
    (root / 'trace.json').write_text(json.dumps(trace, indent=2) + '\n')
    if attached:
        for _ in range(10):
            result = subprocess.run(['hdiutil', 'detach', str(mount)], capture_output=True,
                                    text=True, timeout=30)
            if result.returncode == 0 or 'busy' not in result.stderr.lower():
                break
            time.sleep(.3)
        assert result.returncode == 0, (result.stdout, result.stderr)
