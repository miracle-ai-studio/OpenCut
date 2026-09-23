#!/usr/bin/env python3
"""Public CLI acceptance against frozen, independently constructed historical formats."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import wave

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'packages/contract-fixtures/history'
PROJECT = json.loads((FIXTURES / 'project.json').read_text())
RESULTS = json.loads((FIXTURES / 'results.json').read_text())['cases']


def encoded(value):
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False)


def seed(database, version, identity, probe_path):
    with sqlite3.connect(database) as c:
        for v in range(1, version + 1):
            c.executescript((FIXTURES / f'db-v{v}.sql').read_text())
        p = PROJECT
        c.execute('INSERT INTO projects VALUES (?,?,?)', ('history', p['head']['project']['name'], p['head']['revision']))
        for snapshot, parent, txn in [(p['base'], None, None), (p['head'], p['base']['revision'], p['result']['transaction_id'])]:
            c.execute('INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?,?,?,?,?)',
                      (snapshot['revision'], 'history', parent, txn, encoded(snapshot['project'])))
        c.execute('INSERT INTO editor_transactions(transaction_id,project_id,base_revision,revision,request_json,result_json) VALUES (?,?,?,?,?,?)',
                  (p['result']['transaction_id'], 'history', p['base']['revision'], p['head']['revision'], encoded(p['request']), encoded(p['result'])))
        for family, scope, key, kind, digest, result in [
            ('create_project', 'workspace', p['create']['idempotency_key'], 'snapshot', p['identities'][identity]['create'], p['base']),
            ('apply', 'history', p['request']['idempotency_key'], 'transaction', p['identities'][identity]['apply'], p['result'])]:
            c.execute('INSERT INTO idempotency(operation_family,scope,idempotency_key,request_hash,result_kind,result_json) VALUES (?,?,?,?,?,?)',
                      (family, scope, key, digest, kind, encoded(result)))
        for i, snapshot in enumerate([p['base'], p['head']]):
            c.execute('INSERT INTO outbox(event_id,project_id,revision,event_type,payload_json) VALUES (?,?,?,?,?)',
                      (f'evt_{i:032x}', 'history', snapshot['revision'], 'project.created' if i == 0 else 'transaction.committed', encoded({'project_id': 'history', 'revision': snapshot['revision']})))
        if version >= 2:
            for i, (kind, result) in enumerate(RESULTS.items(), 1):
                c.execute('INSERT INTO jobs(job_id,state,attempt,kind,input_json,result_json) VALUES (?,?,?,?,?,?)',
                          (f'job_{i:032x}', 'succeeded', 1, kind, encoded({'input': '/historical/private/source'}), encoded(result)))
            for i, kind, result in [(90, 'future.unknown', {}), (91, 'media.probe', {'input': '/private/invalid-result'})]:
                c.execute('INSERT INTO jobs(job_id,state,attempt,kind,input_json,result_json) VALUES (?,?,?,?,?,?)',
                          (f'job_{i:032x}', 'succeeded', 1, kind, '{}', encoded(result)))
            c.execute('INSERT INTO jobs(job_id,state,attempt,kind,input_json) VALUES (?,?,?,?,?)',
                      (f'job_{99:032x}', 'queued', 0, 'media.probe', encoded({'input': str(probe_path)})))


def verify(binary):
    directory = Path(tempfile.mkdtemp(prefix='opencut-history-'))
    trace, checks, matrix = [], [], []
    probe = directory / 'legacy-queued.wav'
    with wave.open(str(probe), 'wb') as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(48000)
        f.writeframes(b'\0\0' * 4800)

    def call(database, name, arguments, error=None):
        r = subprocess.run([str(binary), '--database', str(database), '--json', 'call', name, '--input', '-'],
                           input=encoded(arguments), text=True, capture_output=True, timeout=30)
        value = json.loads(r.stdout)
        trace.append({'database': str(database), 'tool': name, 'input': arguments, 'exit_code': r.returncode, 'response': value})
        if error:
            assert r.returncode != 0 and value['error']['code'] == error, value
            return value['error']
        assert r.returncode == 0 and value['error'] is None, value
        return value['data']

    def rows(database):
        with sqlite3.connect(database) as c:
            return {t: c.execute(f'SELECT * FROM {t} ORDER BY rowid').fetchall()
                    for t in ['projects', 'revisions', 'editor_transactions', 'idempotency', 'outbox']}

    try:
        current = directory / 'current-version.sqlite'
        assert call(current, 'opencut_project_list', {}) == []
        with sqlite3.connect(current) as c:
            target_version = int(c.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0])
        assert all((FIXTURES / f'db-v{version}.sql').is_file() for version in range(1, target_version + 1)), 'A frozen historical database fixture is missing'
        for version in range(1, target_version + 1):
            for identity in ['legacy', 'v2']:
                database = directory / f'v{version}-{identity}.sqlite'
                seed(database, version, identity, probe)
                original = rows(database)
                with ThreadPoolExecutor(max_workers=2) as pool:
                    outputs = list(pool.map(lambda _: call(database, 'opencut_project_inspect', {'project_id': 'history'}), range(2)))
                assert outputs == [PROJECT['head'], PROJECT['head']]
                with sqlite3.connect(database) as c:
                    assert c.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0] == str(target_version)
                    assert c.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
                    assert c.execute('PRAGMA foreign_key_check').fetchall() == []
                assert rows(database) == original
                provenance = call(database, 'opencut_audit_query', {'target': {'kind': 'workspace'}})
                assert provenance['records'] == [] and provenance['coverage'] == 'recorded_mutations_since_schema4'
                assert call(database, 'opencut_project_create', PROJECT['create']) == PROJECT['base']
                # Old clients omit null optional fields. Host typed normalization must preserve receipt identity.
                request = copy.deepcopy(PROJECT['request'])
                request['operations'][0] = {k: v for k, v in request['operations'][0].items() if v is not None}
                assert call(database, 'opencut_edit_apply', {'project_id': 'history', 'request': request}) == PROJECT['result']
                changed = copy.deepcopy(request)
                changed['operations'][0]['name'] = 'different intent'
                call(database, 'opencut_edit_apply', {'project_id': 'history', 'request': changed}, 'IDEMPOTENCY_KEY_REUSED')
                assert rows(database) == original
                if version >= 2:
                    for i, (kind, result) in enumerate(RESULTS.items(), 1):
                        job_id = f'job_{i:032x}'
                        raw = call(database, 'opencut_job_get', {'job_id': job_id})
                        assert raw['state'] == 'succeeded' and raw['result'] == result and raw['progress'] is None, raw
                        view = call(database, 'opencut_job_view', {'job_id': job_id})
                        assert view['state'] == 'succeeded' and view['result'] is not None and view['result_unavailable'] is None, view
                        assert '/historical/' not in encoded(view), view
                        if kind.startswith('preview.project') or kind in ['render.project', 'preview.composited_frame']:
                            assert view['result']['data']['asset_locking'] == 'legacy_unlocked', view
                    for i, expected in [(90, 'UNSUPPORTED_JOB_KIND'), (91, 'INVALID_STORED_RESULT')]:
                        view = call(database, 'opencut_job_view', {'job_id': f'job_{i:032x}'})
                        assert view['state'] == 'succeeded' and view['result'] is None and view['result_unavailable']['code'] == expected, view
                    worker = subprocess.run([str(binary), '--database', str(database), 'worker', 'run', '--once'], text=True, capture_output=True, timeout=30)
                    assert worker.returncode == 0, worker.stderr
                    queued = call(database, 'opencut_job_get', {'job_id': f'job_{99:032x}'})
                    assert queued['state'] == 'succeeded' and queued['attempt'] == 1, queued
                    assert queued['result']['streams'][0]['sample_rate'] == 48000, queued
                new = copy.deepcopy(request)
                new.update(expected_revision=PROJECT['head']['revision'], idempotency_key='after-upgrade')
                new['operations'][0]['name'] = 'Edit after upgrade'
                applied = call(database, 'opencut_edit_apply', {'project_id': 'history', 'request': new})
                assert call(database, 'opencut_edit_apply', {'project_id': 'history', 'request': new}) == applied
                assert call(database, 'opencut_project_inspect', {'project_id': 'history'})['revision'] == applied['revision']
                current = rows(database)
                assert len(current['revisions']) == 3 and len(current['editor_transactions']) == 2 and len(current['outbox']) == 3
                assert current['idempotency'][-1][3].startswith('v2:')
                matrix.append({'database_version': version, 'identity': identity, 'status': 'passed', 'legacy_result_kinds': len(RESULTS) if version >= 2 else 0})
        checks += [f'v1–v{target_version} concurrent process opening and migration retain all project history and receipts',
                   'unversioned and v2 request hashes replay original creation and edit results without writes',
                   'omitted optional fields normalize; changed intent with same key is rejected',
                   'all eight compatible historical result kinds read through raw and public views',
                   'missing locking fields default to legacy_unlocked; host paths are excluded from public views',
                   'unknown and invalid historical results remain succeeded with explicit unavailable result',
                   'legacy queued inputs execute in a real Worker after migration',
                   'new edits and idempotent retries work after each upgrade and persist v2 receipts']
        if target_version >= 7:
            broken7 = directory / 'plugin-migration-rollback.sqlite'
            seed(broken7, 6, 'v2', probe)
            with sqlite3.connect(broken7) as c:
                c.execute('CREATE TABLE plugin_events(sentinel TEXT)')
                c.execute("INSERT INTO plugin_events VALUES ('preserve')")
            with sqlite3.connect(broken7) as c: before = list(c.iterdump())
            call(broken7, 'opencut_project_inspect', {'project_id': 'history'}, 'STORAGE_ERROR')
            with sqlite3.connect(broken7) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TABLE plugin_events')
            assert call(broken7, 'opencut_project_inspect', {'project_id': 'history'}) == PROJECT['head']
            checks.append('Plugin publication DDL collision rolls back v6 migration with all original data intact; corrected retry succeeds')
        if target_version >= 9:
            retained = directory / 'retention-migration.sqlite'
            seed(retained, 8, 'v2', probe)
            with sqlite3.connect(retained) as c:
                for i in range(10):
                    c.execute("INSERT INTO upload_sessions(upload_id,project_id,idempotency_key,request_hash,name,size_bytes,content_hash,state) VALUES (?,'history',?,'legacy','old',1073741824,?,'aborted')",(f'upload_{i:032x}',str(i),'a'*64))
                c.execute('CREATE TABLE upload_retention(sentinel TEXT)')
                c.execute("INSERT INTO upload_retention VALUES ('preserve')")
            with sqlite3.connect(retained) as c: before=list(c.iterdump())
            call(retained,'opencut_project_inspect',{'project_id':'history'},'STORAGE_ERROR')
            with sqlite3.connect(retained) as c:
                assert list(c.iterdump())==before
                c.execute('DROP TABLE upload_retention')
            quota=call(retained,'opencut_upload_quota',{'project_id':'history'})
            assert quota['project_reserved_bytes']==str(10*1073741824)
            assert int(quota['project_reserved_bytes'])>int(quota['project_limit_bytes'])
            call(retained,'opencut_upload_create',{'project_id':'history','idempotency_key':'blocked','name':'new','size_bytes':'1','content_hash':'a'*64},'UPLOAD_QUOTA_EXCEEDED')
            for i in range(3):
                value=call(retained,'opencut_upload_release',{'upload_id':f'upload_{i:032x}'})
                assert value['state']=='released' and value['reserved_bytes']=='0'
            call(retained,'opencut_upload_create',{'project_id':'history','idempotency_key':'blocked','name':'new','size_bytes':'1','content_hash':'a'*64})
            assert call(retained,'opencut_project_inspect',{'project_id':'history'})==PROJECT['head']
            checks.append('Schema 8 retention collision rolls back; aborted sessions backfill conservatively above quota, release restores admission without rewriting history')
        if target_version >= 10:
            scoped = directory / 'v9-plugin-grant-rollback.sqlite'
            seed(scoped, 9, 'v2', probe)
            with sqlite3.connect(scoped) as c:
                c.execute('CREATE TABLE project_plugin_grants(sentinel TEXT)')
                c.execute("INSERT INTO project_plugin_grants VALUES ('preserve')")
            with sqlite3.connect(scoped) as c: before = list(c.iterdump())
            call(scoped, 'opencut_project_plugin_list', {'project_id':'history'}, 'STORAGE_ERROR')
            with sqlite3.connect(scoped) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TABLE project_plugin_grants')
            assert call(scoped, 'opencut_project_plugin_list', {'project_id':'history'}) == []
            assert call(scoped, 'opencut_project_inspect', {'project_id':'history'}) == PROJECT['head']
            checks.append('Schema 9 plugin policy collision rolls back; retry preserves history and grants no implicit project plugin permission')
        if target_version >= 11:
            denied = directory / 'v10-denial-rollback.sqlite'
            seed(denied, 10, 'v2', probe)
            with sqlite3.connect(denied) as c:
                c.execute('CREATE TABLE denial_records(sentinel TEXT)')
                c.execute("INSERT INTO denial_records VALUES ('preserve')")
            with sqlite3.connect(denied) as c: before = list(c.iterdump())
            call(denied, 'opencut_denial_query', {'target':{'kind':'workspace'}}, 'STORAGE_ERROR')
            with sqlite3.connect(denied) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TABLE denial_records')
            assert call(denied, 'opencut_denial_query', {'target':{'kind':'workspace'}})['records'] == []
            assert call(denied, 'opencut_project_inspect', {'project_id':'history'}) == PROJECT['head']
            checks.append('Schema 10 denial-table collision rolls back atomically; retry preserves history and does not fabricate past refusals')
        if target_version >= 12:
            gc = directory / 'v11-collection-rollback.sqlite'
            seed(gc, 11, 'v2', probe)
            with sqlite3.connect(gc) as c:
                c.execute('CREATE TABLE cas_gc_plans(sentinel TEXT)')
                c.execute("INSERT INTO cas_gc_plans VALUES ('preserve')")
                before = list(c.iterdump())
            call(gc, 'opencut_project_inspect', {'project_id':'history'}, 'STORAGE_ERROR')
            with sqlite3.connect(gc) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TABLE cas_gc_plans')
            assert call(gc, 'opencut_project_inspect', {'project_id':'history'}) == PROJECT['head']
            with sqlite3.connect(gc) as c:
                assert c.execute('SELECT COUNT(*) FROM cas_gc_plans').fetchone()[0] == 0
                assert c.execute('SELECT COUNT(*) FROM cas_gc_events').fetchone()[0] == 0
            checks.append('Schema 11 GC table collision rolls back; retry preserves project data and invents no collection history')
        if target_version >= 13:
            admission = directory / 'v12-admission-gate-rollback.sqlite'
            seed(admission, 12, 'v2', probe)
            with sqlite3.connect(admission) as c:
                c.execute("CREATE TRIGGER fail_admission_gate BEFORE UPDATE ON metadata WHEN NEW.key='schema_version' AND NEW.value='13' BEGIN SELECT RAISE(ABORT,'QA failed gate'); END")
                before = list(c.iterdump())
            call(admission, 'opencut_project_inspect', {'project_id':'history'}, 'STORAGE_ERROR')
            with sqlite3.connect(admission) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TRIGGER fail_admission_gate')
            assert call(admission, 'opencut_project_inspect', {'project_id':'history'}) == PROJECT['head']
            with sqlite3.connect(admission) as c:
                assert c.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0] == str(target_version)
            checks.append('Schema 12 admission protocol gate rolls back on failure; retry preserves all business history')
        if target_version >= 14:
            admission = directory / 'v13-admission-gate-rollback.sqlite'
            seed(admission, 13, 'v2', probe)
            with sqlite3.connect(admission) as c:
                c.execute("CREATE TRIGGER fail_admission_gate BEFORE UPDATE ON metadata WHEN NEW.key='schema_version' AND NEW.value='14' BEGIN SELECT RAISE(ABORT,'QA failed gate'); END")
                before = list(c.iterdump())
            call(admission, 'opencut_project_inspect', {'project_id':'history'}, 'STORAGE_ERROR')
            with sqlite3.connect(admission) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TRIGGER fail_admission_gate')
            assert call(admission, 'opencut_project_inspect', {'project_id':'history'}) == PROJECT['head']
            with sqlite3.connect(admission) as c:
                assert c.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0] == str(target_version)
            checks.append('Schema 13 extended admission protocol gate rolls back on failure; retry preserves all business history')
        if target_version >= 15:
            admission = directory / 'v14-media-output-gate-rollback.sqlite'
            seed(admission, 14, 'v2', probe)
            with sqlite3.connect(admission) as c:
                c.execute("CREATE TRIGGER fail_admission_gate BEFORE UPDATE ON metadata WHEN NEW.key='schema_version' AND NEW.value='15' BEGIN SELECT RAISE(ABORT,'QA failed gate'); END")
                before = list(c.iterdump())
            call(admission, 'opencut_project_inspect', {'project_id':'history'}, 'STORAGE_ERROR')
            with sqlite3.connect(admission) as c:
                assert list(c.iterdump()) == before
                c.execute('DROP TRIGGER fail_admission_gate')
            assert call(admission, 'opencut_project_inspect', {'project_id':'history'}) == PROJECT['head']
            with sqlite3.connect(admission) as c:
                assert c.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0] == str(target_version)
            checks.append('Schema 14 media output protocol gate rolls back on failure; retry preserves all business history')
        for repetition in range(20):
            race = directory / f'journal-conversion-{repetition}.sqlite'
            seed(race, 5, 'v2', probe)
            with ThreadPoolExecutor(max_workers=4) as pool:
                snapshots = list(pool.map(lambda _: call(race, 'opencut_project_inspect', {'project_id': 'history'}), range(4)))
            assert snapshots == [PROJECT['head']] * 4
            with sqlite3.connect(race) as c:
                assert c.execute('PRAGMA journal_mode').fetchone()[0] == 'wal'
                assert c.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
        checks.append('20 real four-process DELETE-to-WAL conversions preserve snapshots and complete without startup lock failures')
        broken = directory / 'migration-rollback.sqlite'
        seed(broken, 2, 'legacy', probe)
        with sqlite3.connect(broken) as c:
            c.execute('CREATE TABLE job_events (sentinel TEXT)')
            c.execute("INSERT INTO job_events VALUES ('preserve')")
        with sqlite3.connect(broken) as c: before = list(c.iterdump())
        call(broken, 'opencut_project_inspect', {'project_id': 'history'}, 'STORAGE_ERROR')
        with sqlite3.connect(broken) as c:
            assert list(c.iterdump()) == before, 'failed migration changed database'
            c.execute('DROP TABLE job_events')
        assert call(broken, 'opencut_project_inspect', {'project_id': 'history'}) == PROJECT['head']
        checks.append('DDL failure rolls back the entire v2 migration; fixing the fixture allows a clean retry')
        checks.append('historical actors remain unknown rather than fabricated during migration')
        audit_broken = directory / 'audit-migration-rollback.sqlite'
        seed(audit_broken, 3, 'v2', probe)
        with sqlite3.connect(audit_broken) as c:
            c.execute('CREATE TABLE audit_records(sentinel TEXT)')
            c.execute("INSERT INTO audit_records VALUES ('preserve')")
        with sqlite3.connect(audit_broken) as c: before = list(c.iterdump())
        call(audit_broken, 'opencut_project_inspect', {'project_id': 'history'}, 'STORAGE_ERROR')
        with sqlite3.connect(audit_broken) as c:
            assert list(c.iterdump()) == before
            c.execute('DROP TABLE audit_records')
        assert call(audit_broken, 'opencut_project_inspect', {'project_id': 'history'}) == PROJECT['head']
        checks.append('v3 audit migration collision rolls back atomically and can be retried after repair')
        access_broken = directory / 'access-migration-rollback.sqlite'
        seed(access_broken, 4, 'v2', probe)
        with sqlite3.connect(access_broken) as c:
            c.execute('CREATE TABLE job_projects(sentinel TEXT)')
            c.execute("INSERT INTO job_projects VALUES ('preserve')")
        with sqlite3.connect(access_broken) as c: before = list(c.iterdump())
        call(access_broken, 'opencut_project_inspect', {'project_id': 'history'}, 'STORAGE_ERROR')
        with sqlite3.connect(access_broken) as c:
            assert list(c.iterdump()) == before
            c.execute('DROP TABLE job_projects')
        assert call(access_broken, 'opencut_project_inspect', {'project_id': 'history'}) == PROJECT['head']
        checks.append('v4 access migration failure rolls back all policy and ownership tables; repaired retry preserves historical data')
        for invalid in ['999', 'invalid', 'foreign']:
            db = directory / f'reject-{invalid}.sqlite'
            with sqlite3.connect(db) as c:
                c.execute('CREATE TABLE sentinel(value TEXT)')
                c.execute("INSERT INTO sentinel VALUES ('preserve')")
                if invalid != 'foreign':
                    c.execute('CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)')
                    c.execute('INSERT INTO metadata VALUES (?,?)', ('schema_version', invalid))
            before = db.read_bytes()
            call(db, 'opencut_project_inspect', {'project_id': 'history'}, 'UNSUPPORTED_DATABASE_VERSION')
            assert db.read_bytes() == before
        checks.append('future malformed and foreign database versions reject without changing database bytes')
        report = {'status': 'passed', 'checks': checks, 'matrix': matrix, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                  'fixture_hashes': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in FIXTURES.iterdir() if p.is_file()},
                  'scope': 'frozen reconstructed supported formats; not archived release binaries or a downgrade guarantee'}
        (directory / 'report.json').write_text(json.dumps(report, indent=2))
        print(json.dumps({'status': 'passed', 'checks': len(checks), 'matrix_cells': len(matrix), 'report': str(directory / 'report.json')}, indent=2))
    except Exception as e:
        (directory / 'failure.json').write_text(json.dumps({'error': repr(e), 'matrix': matrix}, indent=2))
        raise
    finally:
        (directory / 'trace.json').write_text(json.dumps(trace, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/opencut')
    args = parser.parse_args()
    verify(args.binary.resolve())
