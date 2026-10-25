#!/usr/bin/env python3
"""Real fixed Kenney acquisition and local synthesis in the retained workspace.

No user-media uploads, fixture copies or replacement source URLs. Preserve
original requests, archive/CAS and every completed acquisition across retries.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import socket
import sqlite3
import threading
import time
from acceptance import Harness, sha


def verify_source_faults(a):
    """One controlled transport timeout and exact original-request recovery.

    Reuse the existing catalog project, pinned archive and acquired sound bytes.
    This never changes the configured source URL, TLS trust or default proxy;
    only the diagnostic child uses a loopback proxy which cannot forward traffic.
    """
    live_authority = a.source_authority_only
    h = Harness(a.binary, a.workspace, 'agent-catalog-source-authority-v4' if live_authority else 'agent-catalog-source-faults-v4')
    project = 'agent-sound-catalog-v4-acceptance'
    archive_hash = '946fc23a63d535d693eb31b2eabb80c8c28d6351e2186b344ceb71b2cb1d5eb6'
    archive = h.workspace / 'asset-store' / archive_hash[:2] / archive_hash
    retained = archive.with_name(archive.name + '.source-fault-retained')

    def jobs():
        with sqlite3.connect(f'file:{h.workspace / "workspace.sqlite"}?mode=ro', uri=True) as db:
            counts = dict(db.execute('SELECT state,COUNT(*) FROM jobs GROUP BY state'))
        assert not counts.get('queued') and not counts.get('running'), 'Wait for original live jobs; do not restart them'
        return counts

    def private(revision):
        with sqlite3.connect(f'file:{h.workspace / "workspace.sqlite"}?mode=ro', uri=True) as db:
            return json.loads(db.execute('SELECT document_json FROM revisions WHERE revision=?', (revision,)).fetchone()[0])

    if 'baseline' not in h.state:
        baseline = h.inspect(project)
        h.state['baseline'] = baseline
        h.state['private_baseline'] = private(baseline['revision'])
        h.state['job_counts'] = jobs()
        h.save()
    before = h.state['baseline']
    request = {'project_id': project, 'expected_revision': before['revision'],
        'idempotency_key': h.key('original-acquisition'), 'catalog_id': 'kenney-ui-audio:click2',
        'asset_id': 'catalog-source-authority-recovery-click2' if live_authority else 'catalog-source-fault-recovery-click2', 'uploaded': None}
    original = {'tool': 'opencut_catalog_resources_import', 'input': request}
    assert h.state['requests'].setdefault('original-acquisition', original) == original
    h.save()

    def scoped_editor():
        principal = h.name
        policy = h.cli(['access', 'set-policy', '--input', '-'], {'principal_id': principal,
            'expected_revision': '0', 'enabled': True,
            'grants': [{'project_id': project, 'actions': ['read', 'edit']}]})
        credential = a.report.parent / 'source-authority-original.secret'
        issued = h.cli(['access', 'issue', principal, '--output', str(credential)])
        # issue writes the secret directly to a private file; never to evidence.
        return {'principal': principal, 'policy_revision': policy['revision'],
            'credential_id': issued['credential_id'], 'credential_file': str(credential)}

    editor = h.stage('scoped-editor', scoped_editor) if live_authority else None

    def timeout():
        assert h.inspect(project) == before and jobs() == h.state['job_counts']
        # A previous interrupted verifier may have retained the one archive.
        # Restore its exact inode before starting another diagnostic, never copy
        # or overwrite another canonical object.
        if retained.exists():
            assert not archive.exists() and not retained.is_symlink()
            assert sha(retained.read_bytes()) == archive_hash
            retained.rename(archive)
        assert not archive.is_symlink() and sha(archive.read_bytes()) == archive_hash
        identity = (archive.stat().st_ino, archive.stat().st_size, archive.stat().st_mtime_ns)
        listener = socket.socket()
        listener.bind(('127.0.0.1', 0))
        listener.listen(1)
        listener.settimeout(10)
        release, reached = threading.Event(), threading.Event()
        faults = []

        def stalled_proxy():
            try:
                with listener.accept()[0] as connection:
                    connection.settimeout(2)
                    header = b''
                    while len(header) < 4096 and not header.endswith(b'\r\n\r\n'):
                        byte = connection.recv(1)
                        assert byte, 'Incomplete native CONNECT request'
                        header += byte
                    assert header.startswith(b'CONNECT kenney.nl:443 HTTP/1.1\r\n')
                    reached.set()
                    release.wait(55)
            except Exception as error:
                faults.append(type(error).__name__ + ': ' + str(error))
            finally:
                listener.close()

        thread = threading.Thread(target=stalled_proxy, daemon=True)
        thread.start()
        environment = dict(os.environ)
        environment['HTTPS_PROXY'] = 'http://127.0.0.1:' + str(listener.getsockname()[1])
        environment['NO_PROXY'] = '127.0.0.1,localhost'
        environment['OPENCUT_ALLOW_PUBLIC_RESOURCE_DOWNLOAD'] = '1'
        started = time.monotonic()
        revocation, revoke_errors = [], []

        def revoke_after_connect():
            try:
                assert reached.wait(10), 'Original Native acquisition never entered source I/O'
                revocation.append(h.cli(['access', 'revoke', editor['credential_id']]))
            except Exception as error:
                revoke_errors.append(type(error).__name__ + ': ' + str(error))
            finally:
                release.set()

        revoker = threading.Thread(target=revoke_after_connect, daemon=True) if live_authority else None
        archive.rename(retained)
        try:
            if revoker:
                revoker.start()
            attempt = subprocess.run([str(h.binary), '--json', '--database', str(h.workspace / 'workspace.sqlite'),
                *(['--credential-file', editor['credential_file']] if editor else []),
                'call', original['tool'], '--input', '-'], input=json.dumps(request), text=True,
                capture_output=True, env=environment, timeout=60)
            reply = json.loads(attempt.stdout)
            if live_authority:
                (a.report.parent / 'source-authority-original-response.json').write_text(json.dumps(reply, indent=2) + '\n')
                revoker.join(timeout=12)
                assert len(revocation) == 1 and not revoke_errors, revoke_errors
            expected_error = 'NOT_AUTHORIZED' if live_authority else 'RESOURCE_SOURCE_TIMEOUT'
            assert attempt.returncode and reply['error']['code'] == expected_error, reply
            assert reached.is_set() and not faults, faults
            assert h.inspect(project) == before and private(before['revision']) == h.state['private_baseline']
            assert jobs() == h.state['job_counts']
            assert not archive.exists(), 'Failed source must not publish a replacement archive'
            return {'error': reply['error']['code'], 'elapsed_seconds': time.monotonic() - started,
                'fixed_source_connect_observed': True, 'project_revision_unchanged': True,
                'no_archive_published': True, 'transport': 'controlled loopback CONNECT stall; no upstream HTTP response',
                **({'actual_revocation_during_source_io': revocation[0], 'authority_error_precedes_transport_error': True} if live_authority else {})}
        finally:
            release.set()
            if not reached.is_set():
                listener.close()
            thread.join(timeout=11)
            if revoker is not None and revoker.ident is not None:
                revoker.join(timeout=11)
                assert not revoker.is_alive(), 'Original revocation observer is still live'
            try:
                assert not archive.exists(), 'Refuse to overwrite a canonical cache object'
                retained.rename(archive)
                assert (archive.stat().st_ino, archive.stat().st_size, archive.stat().st_mtime_ns) == identity
                assert sha(archive.read_bytes()) == archive_hash
            finally:
                assert not thread.is_alive(), 'Original diagnostic proxy is still live'

    observed = h.stage('live-source-credential-withdrawal' if live_authority else 'real-cli-source-timeout', timeout)

    def replacement_credential():
        credential = a.report.parent / 'source-authority-recovery.secret'
        issued = h.cli(['access', 'issue', editor['principal'], '--output', str(credential)])
        return {'credential_id': issued['credential_id'], 'credential_file': str(credential)}

    replacement = h.stage('new-credential-same-principal', replacement_credential) if live_authority else None

    def recover():
        # All 51 original sources were already acquired. Rebind the same exact
        # click2 CAS bytes only to prove the previously failed original command.
        inventory = json.loads((Path(__file__).resolve().parents[1] / 'crates/editor-application/src/catalog_resources/sounds.json').read_text())
        pin = next(entry for entry in inventory if entry['name'] == 'click2')
        source = h.workspace / 'asset-store' / pin['sha256'][:2] / pin['sha256']
        assert sha(source.read_bytes()) == pin['sha256']
        receipt = h.call(original['tool'], request, credential=replacement['credential_file']) if replacement else h.request('original-acquisition', original['tool'], request)
        assert h.call(original['tool'], request, credential=replacement['credential_file'] if replacement else None) == receipt
        snapshot = h.inspect(project)
        asset = next(asset for asset in snapshot['project']['assets'] if asset['id'] == request['asset_id'])
        assert asset['version']['content_hash'] == pin['sha256'] and asset['version']['size'] == pin['size']
        assert asset['license']['identifier'] == 'CC0-1.0'
        assert len(snapshot['project']['assets']) == len(before['project']['assets']) + 1
        return {'receipt': receipt, 'asset_content_hash': pin['sha256'], 'snapshot': snapshot}

    recovered = h.stage('same-original-request-recovery', recover)

    def restore():
        current = h.inspect(project)
        undo = {'project_id': project, 'expected_revision': current['revision'],
            'target_transaction_id': recovered['receipt']['transaction_id'], 'idempotency_key': h.key('restore')}
        plan = h.request('restore-plan', 'opencut_edit_undo_plan', undo)
        assert not plan['conflicts'], plan
        receipt = h.request('restore', 'opencut_edit_undo_apply', undo)
        restored = h.inspect(project)
        assert restored['project'] == before['project'] and private(restored['revision']) == h.state['private_baseline']
        assert h.call(original['tool'], request) == recovered['receipt']
        assert h.inspect(project) == restored and jobs() == h.state['job_counts']
        return {'revision': restored['revision'], 'private_and_public_graph_restored': True,
            'original_receipt_replay_after_undo_does_not_reacquire': True, 'undo_receipt': receipt}

    restored = h.stage('restore-original-graph', restore)
    if live_authority:
        def retire():
            h.cli(['access', 'revoke', replacement['credential_id']])
            return h.cli(['access', 'set-policy', '--input', '-'], {'principal_id': editor['principal'],
                'expected_revision': editor['policy_revision'], 'enabled': False, 'grants': []})
        h.stage('retire-diagnostic-principal', retire)
    result = {'status': 'passed_scoped_live_source_authority_withdrawal_and_original_request_recovery' if live_authority else 'passed_scoped_cli_controlled_source_timeout_and_recovery',
        'binary_sha256': h.candidate, 'workspace': str(h.workspace), 'project_id': project,
        'request_sha256': sha(json.dumps(request, sort_keys=True, separators=(',', ':')).encode()),
        **({'authority_withdrawal': observed} if live_authority else {'timeout': observed}),
        'recovery': {'receipt': recovered['receipt'], 'existing_source_hash': recovered['asset_content_hash']},
        'restoration': restored, 'job_counts_unchanged': jobs(),
        'reuse': 'Original project/archive and click2 bytes; no new project, source download, media copy, decode, generation, ASR or render job.',
        'limits': ['Controlled proxy transport timeout is not genuine origin404/429 or upstream timeout.',
            'Only actual credential withdrawal during Native source I/O is claimed; UI stop-waiting, explicit Native cancel and genuine origin fault responses remain separate.' if live_authority else 'Live request cancellation/permission withdrawal and UI stop-waiting are not claimed by this CLI case.',
            'Original CAS inode restored in finally; one temporary asset binding undone, immutable history remains.']}
    a.report.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'status': result['status'], 'report': str(a.report)}))


def verify(a):
    h = Harness(a.binary, a.workspace, 'agent-sound-catalog-v4')
    project = 'agent-sound-catalog-v4-acceptance'
    initial = h.stage('create', lambda: h.request('create', 'opencut_project_create', {
        'project': {'id': project, 'name': 'Licensed catalog and recorded local sounds', 'sequence_id': 'sound-scene',
            'width': 320, 'height': 180, 'fps': {'numerator': 30, 'denominator': 1}}, 'idempotency_key': h.key('create')}))
    catalog = h.call('opencut_catalog_resources_list', {})
    sounds = [e for e in catalog['entries'] if e['category'] == 'sounds']
    assert len(sounds) == 51 and all(e['license']['identifier'] == 'CC0-1.0' and e['source_sha256'] for e in sounds)
    inventory = json.loads((Path(__file__).resolve().parents[1] / 'crates/editor-application/src/catalog_resources/sounds.json').read_text())
    pinned = {x['name']: x for x in inventory}
    def original(name, tool, fields):
        retained = h.state['requests'].get(name)
        if retained: return retained['input']
        before = h.inspect(project)
        return {'project_id': project, 'expected_revision': before['revision'], 'idempotency_key': h.key(name), **fields}
    first = original('acquire-click2', 'opencut_catalog_resources_import', {
        'catalog_id': 'kenney-ui-audio:click2', 'asset_id': 'catalog-click2', 'uploaded': None})
    def offline_refusal():
        before = h.inspect(project)
        assert not catalog['sound_download_available']
        h.call('opencut_catalog_resources_import', first, error='CAPABILITY_UNAVAILABLE')
        assert h.inspect(project) == before
        return {'error': 'CAPABILITY_UNAVAILABLE', 'project_unchanged': True}
    h.stage('initial-offline-refusal', offline_refusal)
    def first_download():
        previous = os.environ.get('OPENCUT_ALLOW_PUBLIC_RESOURCE_DOWNLOAD')
        os.environ['OPENCUT_ALLOW_PUBLIC_RESOURCE_DOWNLOAD'] = '1'
        try: return h.request('acquire-click2', 'opencut_catalog_resources_import', first)
        finally:
            if previous is None: os.environ.pop('OPENCUT_ALLOW_PUBLIC_RESOURCE_DOWNLOAD', None)
            else: os.environ['OPENCUT_ALLOW_PUBLIC_RESOURCE_DOWNLOAD'] = previous
    h.stage('actual-fixed-download', first_download)
    assert h.call('opencut_catalog_resources_import', first)['revision'] == h.state['stages']['actual-fixed-download']['result']['revision']
    # The service remains download-disabled. Following native acquisitions and
    # the UI consumer must use the same verified cached archive.
    assert h.call('opencut_catalog_resources_list', {})['sound_download_available']
    for entry in sounds:
        name = entry['name']
        if name in ('click1', 'click2'): continue  # click1 is the actual UI intent.
        stage = 'acquire-' + name
        def acquire(entry=entry, stage=stage):
            request = original(stage, 'opencut_catalog_resources_import', {
                'catalog_id': entry['id'], 'asset_id': 'catalog-' + entry['name'], 'uploaded': None})
            result = h.request(stage, 'opencut_catalog_resources_import', request)
            assert h.call('opencut_catalog_resources_import', request)['revision'] == result['revision']
            return result
        h.stage(stage, acquire)
    for kind, duration in [('click', 100), ('impact', 600)]:
        stage = 'generate-' + kind
        def generate(kind=kind, duration=duration, stage=stage):
            request = original(stage, 'opencut_catalog_resources_generate', {'asset_id': 'generated-' + kind,
                'recipe': {'kind': kind, 'duration_ms': duration, 'seed': 17, 'level_db': -12}})
            result = h.request(stage, 'opencut_catalog_resources_generate', request)
            assert h.call('opencut_catalog_resources_generate', request)['revision'] == result['revision']
            return result
        h.stage(stage, generate)
    snapshot = h.inspect(project)
    archive = h.workspace / 'asset-store/94/946fc23a63d535d693eb31b2eabb80c8c28d6351e2186b344ceb71b2cb1d5eb6'
    assert sha(archive.read_bytes()) == archive.name and archive.stat().st_size == 411949
    observed = []
    for asset in snapshot['project']['assets']:
        content = asset['version']['content_hash']; source = h.workspace / 'asset-store' / content[:2] / content
        data = source.read_bytes(); assert sha(data) == content and len(data) == asset['version']['size']
        if asset['id'].startswith('catalog-'):
            item = pinned[asset['name']]; assert content == item['sha256'] and len(data) == item['size']
            assert asset['origin']['kind'] == 'online' and asset['license']['identifier'] == 'CC0-1.0'
        pcm = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(source), '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1'])
        assert len(pcm) >= 8 and len(pcm) % 8 == 0
        observed.append({'id': asset['id'], 'name': asset['name'], 'content_hash': content, 'decoded_frames': len(pcm) // 8})
    result = {'status': 'native_prepared_for_ui', 'binary_sha256': h.candidate, 'workspace': str(h.workspace), 'project_id': project,
        'revision': snapshot['revision'], 'catalog_entries': 51, 'native_acquired': len([x for x in observed if x['id'].startswith('catalog-')]),
        'native_generated': len([x for x in observed if x['id'].startswith('generated-')]), 'decoded': observed,
        'archive_sha256': archive.name, 'license_source': 'https://kenney.nl/assets/ui-audio',
        'scope': 'One real fixed source archive download, verified CAS/provenance/independent decode and original-request native replay; UI click1/whoosh audition/place/export and failure matrix are not yet included.'}
    a.report.write_text(json.dumps(result, indent=2) + '\n'); print(json.dumps({k: result[k] for k in ['status', 'project_id', 'native_acquired', 'native_generated']}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'report']: p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--source-faults-only', action='store_true', help='Reuse the catalog project/CAS; only controlled source timeout and original-request recovery')
    p.add_argument('--source-authority-only', action='store_true', help='Revoke one scoped credential during actual pending Native source I/O, then recover the exact original request')
    a = p.parse_args()
    assert not (a.source_faults_only and a.source_authority_only), 'Choose one scoped source mode'
    (verify_source_faults if a.source_faults_only or a.source_authority_only else verify)(a)
