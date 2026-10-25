#!/usr/bin/env python3
"""Preserve an unavailable effect on retained V4 content; never import or render media."""
import argparse
import copy
from http.client import HTTPConnection
import importlib.util
import json
from pathlib import Path
import sqlite3
import subprocess
from urllib.parse import urlsplit
from acceptance import Harness, sha


def verify(a):
    h = Harness(a.binary, a.workspace, 'agent-readonly-effects')
    source = h.inspect(a.source_project, a.source_revision)
    original_head = h.stage('source-head', lambda: h.inspect(a.source_project))
    scene = next(s for s in source['project']['sequences'] if any(e.get('visual') for t in s['tracks'] for e in t['elements']))
    track, element = next((t, e) for t in scene['tracks'] for e in t['elements'] if e.get('visual'))
    escaped = ''.join('\\u%04x' % ord(c) for c in a.artifact)
    payload = '{\n  "future": ["未识别效果", {"gain": 0.125}], "duplicate": 1, "duplicate": 2,\n  "resource": "' + escaped + '", "resource": null\n}\n'
    assert a.artifact not in payload
    request = {'source_project_id': a.source_project, 'source_revision': a.source_revision,
        'project_id': 'agent-readonly-effects-acceptance', 'name': 'Preserved interview — missing effect',
        'idempotency_key': h.key('create'), 'opaque_effects': [{
            'id': 'preserved-unavailable-effect', 'target': {'sequence_id': scene['id'], 'track_id': track['id'], 'element_id': element['id']},
            'position': 0, 'enabled': True, 'plugin': {'id': 'org.example.unavailable-effect', 'version': '1.0.0',
                'package_sha256': 'a' * 64, 'contribution': 'org.example.unavailable-effect.transform'}, 'payload_json': payload}]}
    def database():
        c = sqlite3.connect('file:' + str(h.workspace / 'workspace.sqlite') + '?mode=ro', uri=True)
        try:
            counts = {t: c.execute('SELECT count(*) FROM ' + t).fetchone()[0] for t in ['projects', 'revisions', 'idempotency', 'outbox', 'audit_records', 'jobs']}
            counts['source_revisions'] = sha(json.dumps(c.execute('SELECT revision,document_json FROM revisions WHERE project_id=? ORDER BY revision', [a.source_project]).fetchall()).encode())
            counts['preserved_revisions'] = c.execute('SELECT revision,document_json FROM revisions WHERE project_id=? ORDER BY revision', [request['project_id']]).fetchall()
            counts['database_schema'] = c.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0]
            return counts
        finally: c.close()
    def creation():
        before = database()
        result = h.request('create', 'opencut_project_readonly_create', request)
        after = database()
        assert after['jobs'] == before['jobs']
        # A checkpoint-lost response may already have committed this exact request.
        assert after['projects'] - before['projects'] in [0, 1]
        return {'view': result, 'before': before, 'after': after}
    created = h.stage('create', creation); view = created['view']
    lookup = {'project_id': view['project_id'], 'revision': view['revision']}
    assert 'document_schema_version' not in view and view['read_only']
    expected = copy.deepcopy(source['project']); expected['id'] = request['project_id']; expected['name'] = request['name']
    assert view['known_content'] == expected
    effect = view['effects'][0]
    assert effect['payload_sha256'] == sha(payload.encode()) and effect['payload_size_bytes'] == len(payload.encode())
    assert effect['unavailable_reason'] == 'not_installed'
    assert view['export_failure_code'] == 'MISSING_PLUGIN_DEPENDENCY'
    assert h.call('opencut_project_readonly_create', request) == view
    status = h.cli(['server', '--directory', str(h.workspace / 'service'), 'status'])
    assert status['ready'] and sha(Path(status['executable']).read_bytes()) == h.candidate
    endpoint = urlsplit(status['endpoint'])
    token = Path(status['token_file'])
    trace = []
    def http(tool, value, error=None, credential=token):
        connection = HTTPConnection(endpoint.hostname, endpoint.port, timeout=60)
        connection.request('POST', '/api/tools/' + tool, json.dumps(value), {'Authorization': 'Bearer ' + credential.read_text().strip(), 'Content-Type': 'application/json'})
        response = connection.getresponse(); body = json.loads(response.read()); connection.close()
        trace.append({'tool': tool, 'status': response.status, 'reply': body})
        if error: assert body['error']['code'] == error, body
        else: assert response.status == 200 and body['error'] is None, body
        return body.get('data')
    manifest = h.call('opencut_project_readonly_manifest', lookup)
    assert manifest['known_content'] == expected and manifest['opaque_effects'] == request['opaque_effects']
    assert str(h.workspace) not in json.dumps(manifest)
    episode = h.candidate[:12]
    def transports():
        def library(tool):
            items, cursor = [], None
            while True:
                page = h.call(tool, {'cursor': cursor, 'limit': 100}); items.extend(page['items']); cursor = page['next_cursor']
                if not cursor: return items
        assert next(p for p in library('opencut_project_list') if p['project_id'] == view['project_id'])['format'] == 'readonly'
        assert h.call('opencut_project_inspect', lookup) == view
        assert http('opencut_project_inspect', lookup) == view
        assert http('opencut_project_readonly_manifest', lookup) == manifest
        spec = importlib.util.spec_from_file_location('readonly_mcp', Path(__file__).with_name('verify-editorial.py'))
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        mcp = module.Mcp(h.binary, h.workspace / 'workspace.sqlite', a.report.parent,
            [str(h.binary), '--database', str(h.workspace / 'workspace.sqlite'), 'mcp', 'serve'])
        try:
            assert mcp.tool('opencut_project', {'action': 'readonly.v1.inspect', 'input': lookup}) == view
            assert mcp.tool('opencut_project', {'action': 'readonly.manifest', 'input': lookup}) == manifest
        finally: mcp.close()
        sdk_request = a.report.with_name('readonly-sdk-request.json'); sdk_output = a.report.with_name('readonly-sdk-result.json')
        sdk_request.write_text(json.dumps({'lookup': lookup, 'create': request}, indent=2) + '\n')
        subprocess.run(['node', str(Path(__file__).with_name('verify-readonly-effects-sdk.mjs')), status['endpoint'], str(token), str(sdk_request), str(sdk_output)], check=True, timeout=60)
        got = json.loads(sdk_output.read_text())
        assert got == {'created': view, 'view': view, 'manifest': manifest}
        return {'cli_http_mcp_sdk_equal': True, 'replay_same_revision': True, 'version_aware_library_visible': True, 'legacy_library_excludes_schema5': True}
    parity = h.stage('transports-' + episode, transports)
    def fences():
        before = database()
        export = {'kind': 'video', 'request': {'project_id': view['project_id'], 'revision': view['revision'], 'sequence_id': scene['id'],
            'idempotency_key': h.key('unavailable-export'), 'options': {'format': 'mp4', 'quality': 'high', 'include_audio': True}}}
        h.call('opencut_render_submit', export, error='MISSING_PLUGIN_DEPENDENCY')
        http('opencut_render_submit', export, error='MISSING_PLUGIN_DEPENDENCY')
        h.call('opencut_edit_apply', {'project_id': view['project_id'], 'expected_revision': view['revision'],
            'idempotency_key': h.key('refused-edit'), 'label': 'Must preserve unknown nodes', 'operations': [{'type': 'rename', 'name': 'Must not mutate'}]}, error='MISSING_PLUGIN_DEPENDENCY')
        assert database() == before
        references = h.call('opencut_artifact_references_inspect', {'artifact_id': a.artifact})
        assert {'kind': 'revision', 'resource_id': view['revision']} in references['roots']
        c = sqlite3.connect('file:' + str(h.workspace / 'workspace.sqlite') + '?mode=ro', uri=True)
        try:
            stored = c.execute('SELECT document_json FROM revisions WHERE revision=?', [view['revision']]).fetchone()[0]
            assert a.artifact not in stored and json.loads(stored)['opaque_effects'][0]['payload_json'] == payload
        finally: c.close()
        return {'formal_export_refused_before_job': True, 'edit_refused_without_writes': True, 'escaped_payload_artifact_reference_protected': True}
    refusal = h.stage('fences-' + episode, fences)
    def scoped_read():
        principal = 'agent-readonly-effects-reader'
        policy = h.stage('reader-policy', lambda: h.cli(['access', 'set-policy', '--input', '-'], {'principal_id': principal, 'expected_revision': '0', 'enabled': True,
            'grants': [{'project_id': view['project_id'], 'actions': ['read']}]}))
        credential = h.workspace / (principal + '.secret')
        h.stage('reader-credential', lambda: h.cli(['access', 'issue', principal, '--output', str(credential)]))
        assert http('opencut_project_inspect', lookup, credential=credential) == view
        assert http('opencut_project_readonly_manifest', lookup, credential=credential) == manifest
        http('opencut_project_readonly_create', request, error='NOT_AUTHORIZED', credential=credential)
        h.call('opencut_project_inspect', {'project_id': a.source_project, 'revision': a.source_revision}, credential=credential, error='NOT_AUTHORIZED')
        h.stage('reader-revoked', lambda: h.cli(['access', 'set-policy', '--input', '-'], {'principal_id': principal,
            'expected_revision': policy['revision'], 'enabled': False, 'grants': []}))
        http('opencut_project_inspect', lookup, credential=credential, error='NOT_AUTHORIZED')
        http('opencut_project_readonly_manifest', lookup, credential=credential, error='NOT_AUTHORIZED')
        return {'current_read_allowed': True, 'create_owner_only': True, 'foreign_project_denied': True, 'revocation_denies_inspect_and_manifest': True}
    access = h.stage('scoped-read-' + episode, scoped_read)
    if a.legacy_binary:
        def barrier():
            before = database()
            command = [str(a.legacy_binary), '--json', '--database', str(h.workspace / 'workspace.sqlite'),
                'call', 'opencut_project_inspect', '--input', '-']
            old = subprocess.run(command, input=json.dumps({'project_id': a.source_project, 'revision': a.source_revision}), text=True, capture_output=True, timeout=60)
            assert old.returncode and json.loads(old.stdout)['error']['code'] == 'UNSUPPORTED_DATABASE_VERSION', old.stdout
            assert database() == before and before['database_schema'] == '32'
            assert h.call('opencut_project_readonly_manifest', lookup) == manifest
            return {'old_binary_sha256': sha(a.legacy_binary.read_bytes()), 'old_host_error': 'UNSUPPORTED_DATABASE_VERSION', 'database_unchanged': True, 'latest_host_reopens': True}
        h.stage('old-host-barrier-' + episode, barrier)
    assert h.inspect(a.source_project) == original_head and h.inspect(a.source_project, a.source_revision) == source
    ready = {'status': 'passed_scoped', 'binary_sha256': h.candidate, 'workspace': str(h.workspace), 'project_id': view['project_id'], 'revision': view['revision'],
        'source_project_id': a.source_project, 'source_revision': a.source_revision, 'payload_sha256': effect['payload_sha256'], 'lookup': lookup,
        'manifest': manifest, 'view': view, 'parity': parity, 'refusal': refusal, 'access': access, 'source_unchanged': True,
        'scope': 'Readonly schema5 unavailable-effect preservation, exact opaque strings and known content, no imports/ASR/renders/new media. Not plugin execution, offline media/history package or full migration acceptance.'}
    a.report.write_text(json.dumps(ready, ensure_ascii=False, indent=2) + '\n')
    a.report.with_name(a.report.stem + '-trace.json').write_text(json.dumps(trace, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({k: ready[k] for k in ['status', 'project_id', 'revision']}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'report']: p.add_argument('--' + name, type=Path, required=True)
    for name in ['source-project', 'source-revision', 'artifact']: p.add_argument('--' + name, required=True)
    p.add_argument('--legacy-binary', type=Path)
    verify(p.parse_args())
