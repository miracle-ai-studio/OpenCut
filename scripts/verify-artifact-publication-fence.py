#!/usr/bin/env python3
"""Cross-process acceptance for a crash-persistent artifact publication fence."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import time


def verify(binary):
    directory = Path(tempfile.mkdtemp(prefix='opencut-artifact-publication-'))
    database = directory / 'workspace.sqlite'
    checks = []

    def call(name, value, expected_error=None):
        process = subprocess.run(
            [str(binary), '--database', str(database), '--json', 'call', name, '--input', '-'],
            input=json.dumps(value), text=True, capture_output=True, timeout=30,
        )
        output = json.loads(process.stdout)
        if expected_error:
            assert process.returncode != 0 and output['error']['code'] == expected_error, output
        else:
            assert process.returncode == 0 and output['error'] is None, output
        return output

    assert call('opencut_project_list', {})['data'] == []
    with sqlite3.connect(database) as connection:
        assert connection.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0] == '18'
    ready = directory / 'intent-committed'
    child = subprocess.Popen([
        sys.executable, '-c', '''import sqlite3,sys,time
from pathlib import Path
with sqlite3.connect(sys.argv[1]) as connection:
 connection.execute("INSERT INTO artifact_gc_plans(plan_id,idempotency_key,input_json,plan_json) VALUES ('agc_qa','qa','{}','{}')")
 connection.execute("INSERT INTO artifact_gc_removals(plan_id,state) VALUES ('agc_qa','removing')")
Path(sys.argv[2]).write_text('committed')
time.sleep(60)
''', str(database), str(ready)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    deadline = time.monotonic() + 10
    while not ready.is_file() and child.poll() is None and time.monotonic() < deadline:
        time.sleep(0.01)
    assert ready.is_file(), child.stderr.read().decode() if child.poll() is not None else 'intent child did not commit'
    child.kill()
    assert child.wait(timeout=5) != 0
    create = {
        'project': {
            'id': 'fenced', 'name': 'Fenced project', 'sequence_id': 'main',
            'width': 160, 'height': 90, 'fps': {'numerator': 10, 'denominator': 1},
        },
        'idempotency_key': 'fenced-create',
    }
    call('opencut_project_create', create, 'ARTIFACT_COLLECTION_RECOVERY_REQUIRED')
    with sqlite3.connect(database) as connection:
        assert connection.execute('SELECT count(*) FROM projects').fetchone()[0] == 0
        assert connection.execute('SELECT count(*) FROM revisions').fetchone()[0] == 0
        assert connection.execute('SELECT count(*) FROM idempotency').fetchone()[0] == 0
        assert connection.execute("SELECT state FROM artifact_gc_removals WHERE plan_id='agc_qa'").fetchone()[0] == 'removing'
    checks.append('a separate delivered CLI process cannot commit a project revision while the removal intent survives a process exit; failed commit leaves no head or receipt')

    with sqlite3.connect(database) as connection:
        connection.execute("UPDATE artifact_gc_removals SET state='changed' WHERE plan_id='agc_qa'")
    created = call('opencut_project_create', create)['data']
    assert created['project']['id'] == 'fenced'
    assert call('opencut_project_create', create)['data'] == created
    with sqlite3.connect(database) as connection:
        assert connection.execute('SELECT count(*) FROM projects').fetchone()[0] == 1
        assert connection.execute('SELECT count(*) FROM revisions').fetchone()[0] == 1
        assert connection.execute('SELECT count(*) FROM idempotency').fetchone()[0] == 1
        assert connection.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    checks.append('resolving the durable fence allows exact-key retry to create one revision and one receipt')

    report = {
        'status': 'passed', 'checks': checks, 'directory': str(directory),
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    path = directory / 'report.json'
    path.write_text(json.dumps(report, indent=2))
    return {'status': 'passed', 'checks': len(checks), 'report': str(path)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve())))
