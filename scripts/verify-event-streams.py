#!/usr/bin/env python3
"""Real-process acceptance for cursor-based CLI observation; stdlib only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import sqlite3
import subprocess
import tempfile
import time


def verify(binary, root):
    checks = []
    commands = []
    db = root / 'workspace.sqlite'
    processes = []

    def call(name, args, database=db, expected=0):
        cmd = [str(binary), '--database', str(database), '--json', 'call', name, '--input', '-']
        p = subprocess.run(cmd, input=json.dumps(args), text=True, capture_output=True, timeout=15)
        commands.append({'command': cmd, 'input': args, 'exit': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr})
        assert p.returncode == expected, (p.returncode, p.stdout, p.stderr)
        return json.loads(p.stdout)

    def create(name, database=db):
        return call('opencut_project_create', {'project': {'id': name, 'name': name, 'sequence_id': 'main',
            'width': 640, 'height': 360, 'fps': {'numerator': 30, 'denominator': 1}},
            'idempotency_key': 'create-' + name}, database)['data']

    def edit(name, suffix):
        revision = call('opencut_project_inspect', {'project_id': name})['data']['revision']
        return call('opencut_edit_apply', {'project_id': name, 'request': {'expected_revision': revision,
            'idempotency_key': suffix, 'label': suffix, 'operations': [{'type': 'track.create',
            'sequence_id': 'main', 'track': {'id': suffix, 'name': suffix, 'kind': 'video',
            'enabled': True, 'muted': False, 'locked': False, 'clips': [], 'params': {}}}]}})['data']

    class Stream:
        def __init__(self, args, database=db):
            self.cmd = [str(binary), '--database', str(database), *args]
            self.p = subprocess.Popen(self.cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            processes.append(self.p)
            self.selector = selectors.DefaultSelector()
            self.selector.register(self.p.stdout, selectors.EVENT_READ)
            self.buffer = bytearray()
            self.records = []
        def line(self, timeout=10):
            deadline = time.monotonic() + timeout
            while b'\n' not in self.buffer:
                assert self.selector.select(max(0, deadline - time.monotonic())), ('stream timeout', self.cmd)
                chunk = os.read(self.p.stdout.fileno(), 65536)
                assert chunk, ('unexpected EOF', self.cmd, self.p.poll(), self.p.stderr.read())
                self.buffer.extend(chunk)
            line, _, rest = self.buffer.partition(b'\n')
            self.buffer[:] = rest
            record = json.loads(line)
            assert record['format'] == 'opencut.events.v1', record
            assert record['type'] in ('ready', 'event', 'end', 'error'), record
            self.records.append(record)
            return record
        def finish(self, expected=0):
            code = self.p.wait(timeout=6)
            assert code == expected, (self.cmd, code, self.p.stderr.read())
            diagnostics = self.p.stderr.read().decode()
            commands.append({'command': self.cmd, 'exit': code, 'records': self.records, 'stderr': diagnostics})
            self.selector.close()
            return diagnostics
        def drain(self, expected=0):
            while True:
                row = self.line()
                if row['type'] in ('end', 'error'):
                    break
            self.finish(expected)
            return self.records

    def events(*args, database=db, expected=0):
        return Stream(['events', '--format', 'ndjson', *args], database).drain(expected)
    def start_job(key):
        return call('opencut_job_start', {'kind': 'media.probe', 'idempotency_key': key,
            'input': {'input': '/private/secret-awaiting-worker.wav'}})['data']['job_id']
    def state(job):
        return call('opencut_job_get', {'job_id': job})['data']['state']

    try:
        escaped_id = '\x01' * 1024
        escape_db = root / 'escaped-filter.sqlite'
        create(escaped_id, escape_db)
        escaped = events('--project-id', escaped_id, database=escape_db)
        assert len(escaped[-1]['cursor']) > 8192
        assert events('--project-id', escaped_id, '--cursor', escaped[-1]['cursor'], database=escape_db)[-1]['reason'] == 'caught_up'
        checks.append('Maximum-size escaped filter produces a cursor accepted by the same contract')
        create('p'); create('other')
        for i in range(4):
            edit('p', f'track-{i}')
        all_records = events('--limit', '1')
        rows = [r for r in all_records if r['type'] == 'event']
        assert len(rows) >= 4
        assert all('sequence' not in r['event']['data'] for r in rows)
        assert all_records[-1]['cursor'] == rows[-1]['cursor']
        scope = {'kind': 'projects', 'project_id': None}
        page = call('opencut_event_changes', {'scope': scope, 'limit': 500})['data']
        assert [{'cursor': r['cursor'], 'event': r['event']} for r in rows] == page['events']
        legacy = call('opencut_events_read', {})['data']
        assert [e['event_id'] for e in legacy] == [r['event']['data']['event_id'] for r in rows]
        checks.append('Shared application query, legacy event identity parity, drain across page boundaries')

        first = events('--max-events', '2', '--limit', '500')
        assert first[-1]['reason'] == 'limit_reached' and len(first) == 4
        resumed = events('--cursor', first[-1]['cursor'])
        assert [r for r in first + resumed if r['type'] == 'event'] == rows
        assert events('--cursor', rows[-1]['cursor'])[-1]['cursor'] == rows[-1]['cursor']
        checks.append('Stop mid-page and resume without skipped/duplicated events; empty-page cursor preservation')

        watch = Stream(['events', '--format', 'ndjson', '--watch', '--cursor', rows[-1]['cursor']])
        assert watch.line()['type'] == 'ready'
        edit('p', 'after-ready')
        changed = watch.line()
        assert changed['type'] == 'event' and changed['cursor'] != rows[-1]['cursor']
        watch.p.send_signal(signal.SIGINT)
        watch.finish(130)
        edit('p', 'after-disconnect')
        reopened = events('--cursor', changed['cursor'])
        assert len([r for r in reopened if r['type'] == 'event']) == 1
        checks.append('Live committed event delivery, SIGINT 130, reconnect after independently committed write')

        filtered = events('--project-id', 'p')
        assert all(r['event']['data']['project_id'] == 'p' for r in filtered if r['type'] == 'event')
        wrong_scope = events('--cursor', rows[0]['cursor'], '--project-id', 'p', expected=2)
        assert wrong_scope[0]['error']['code'] == 'INVALID_CURSOR'
        otherdb = root / 'other.sqlite'
        create('p', otherdb)
        foreign = events('--cursor', rows[0]['cursor'], database=otherdb, expected=4)
        assert foreign[0]['error']['code'] == 'CURSOR_EXPIRED'
        for arg in (['--cursor', 'bad'], ['--cursor', 'events-v1:zz'], ['--cursor', 'events-v1:' + 'a' * 16_386],
                    ['--limit', '0'], ['--max-events', '0'], ['--after', '1'], ['--timeout-seconds', '0'], ['--bogus']):
            rejected = events(*arg, expected=2)
            assert len(rejected) == 1 and rejected[0]['type'] == 'error', rejected
        checks.append('Scoped/foreign/malformed/oversized cursors and CLI parsing failures are structured errors')

        timeout = Stream(['events', '--format', 'ndjson', '--watch', '--cursor', reopened[-1]['cursor'], '--timeout-seconds', '1'])
        assert timeout.line()['type'] == 'ready'
        timeout.finish(124)
        checks.append('Watch deadline exits 124')

        job = start_job('observe-job')
        jobwatch = Stream(['job', 'events', '--job-id', job, '--format', 'ndjson', '--watch'])
        assert jobwatch.line()['type'] == 'ready'
        queued = jobwatch.line()
        assert queued['event']['data']['event_type'] == 'queued'
        assert '/private/secret' not in json.dumps(queued)
        jobwatch.p.send_signal(signal.SIGINT); jobwatch.finish(130)
        assert state(job) == 'queued'
        call('opencut_job_cancel', {'job_id': job})
        cancelled = Stream(['job', 'events', '--job-id', job, '--format', 'ndjson', '--cursor', queued['cursor']]).drain()
        assert [r['event']['data']['event_type'] for r in cancelled if r['type'] == 'event'] == ['cancelled']
        assert events('--cursor', queued['cursor'], expected=2)[0]['error']['code'] == 'INVALID_CURSOR'
        checks.append('Job event safe projection, queued/cancelled resume, observing never cancels durable work')

        for cancel in (False, True):
            job = start_job('wait-' + str(cancel))
            cmd = [str(binary), '--database', str(db), '--json', 'job', 'wait', job]
            if cancel: cmd.append('--cancel-on-interrupt')
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            processes.append(p)
            time.sleep(0.5)
            p.send_signal(signal.SIGINT)
            stdout, stderr = p.communicate(timeout=6)
            assert p.returncode == 130, (p.returncode, stdout, stderr)
            response = json.loads(stdout)
            assert response['error']['code'] == 'USER_INTERRUPTED'
            assert state(job) == ('cancelled' if cancel else 'queued')
            assert response['error']['details']['cancellation_requested'] == cancel
            commands.append({'command': cmd, 'exit': p.returncode, 'response': response})
        job = start_job('wait-timeout')
        p = subprocess.run([str(binary), '--database', str(db), '--json', 'job', 'wait', job,
            '--timeout-seconds', '0', '--cancel-on-interrupt'], capture_output=True, timeout=6)
        assert p.returncode == 124 and json.loads(p.stdout)['error']['code'] == 'JOB_WAIT_TIMEOUT'
        assert state(job) == 'queued'
        checks.append('Wait Ctrl-C 130: default preserves job; explicit flag cancels; timeout still preserves job')

        # Artificial outbox faults/large positions use SQL only in disposable acceptance workspaces.
        with sqlite3.connect(db) as connection:
            anchor_id = rows[0]['event']['data']['event_id']
            connection.execute('DELETE FROM outbox WHERE event_id=?', [anchor_id])
        assert events('--cursor', rows[0]['cursor'], expected=4)[0]['error']['code'] == 'CURSOR_EXPIRED'
        with sqlite3.connect(db) as connection:
            connection.execute('UPDATE outbox SET sequence=sequence+?', [2**53])
        huge = events('--limit', '1', '--max-events', '1')
        parsed = json.loads(bytes.fromhex(huge[-1]['cursor'].split(':', 1)[1]))
        assert isinstance(parsed[1], str) and int(parsed[1]) > 2**53
        huge_resume = events('--cursor', huge[-1]['cursor'], '--max-events', '1')
        assert huge_resume[1]['cursor'] != huge[1]['cursor']
        checks.append('Deleted anchor explicitly expires; positions above JavaScript integer precision resume exactly')

        broken = Stream(['events', '--format', 'ndjson'])
        broken.p.stdout.close()
        broken.finish(0)
        with sqlite3.connect(db) as connection:
            connection.execute('UPDATE outbox SET payload_json=?', [json.dumps({'padding': 'x' * 1024 * 1024})])
        slow = Stream(['events', '--format', 'ndjson', '--watch'])
        time.sleep(0.7)  # Let the OS pipe fill; deliberately never read stdout.
        slow.p.send_signal(signal.SIGINT)
        slow.finish(130)
        checks.append('Broken stdout pipe exits cleanly; blocked unread pipe still permits Ctrl-C 130')
        return {'status': 'passed', 'binary': str(binary), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'checks': checks, 'evidence': str(root)}
    finally:
        for p in processes:
            if p.poll() is None:
                p.kill(); p.wait(timeout=5)
        (root / 'commands.json').write_text(json.dumps(commands, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=Path('target/debug/opencut'))
    parser.add_argument('--artifacts-dir', type=Path)
    args = parser.parse_args()
    root = (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-events-'))).resolve()
    root.mkdir(parents=True, exist_ok=True)
    report = verify(args.binary.resolve(), root)
    (root / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
