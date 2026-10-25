#!/usr/bin/env python3
"""Exercise current delivered V4 MCP adapters in an owned acceptance workspace.

Reuse an exact successful render request, its source revision and media. Never
install clients, create credentials, import fixtures or submit a new render.
Legacy V3 editing/render coverage remains in verify-editorial.py.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3

from acceptance import Harness, at

HERE = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    pending = path.with_suffix('.pending')
    pending.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    pending.replace(path)


def verify(args):
    root, workspace, binary = args.artifacts_dir.resolve(), args.workspace.resolve(), args.binary.resolve()
    root.mkdir(parents=True, exist_ok=True)
    database = workspace / 'workspace.sqlite'
    assert database.is_file(), 'An existing owned workspace is required'
    replay = json.loads(args.replay_checkpoint.read_text())['requests'][args.replay_request]
    assert replay['tool'] == 'opencut_render_submit'
    target = replay['input']['request']
    source_scope = {key: target[key] for key in ('project_id', 'revision', 'sequence_id')}
    with sqlite3.connect(f'file:{database}?mode=ro', uri=True) as db:
        assert db.execute("SELECT count(*) FROM jobs WHERE state IN ('queued','running')").fetchone()[0] == 0, \
            'Managed MCP workers need the owned queue to be idle before adapter verification'
    baseline = Harness(binary, workspace, 'agent-plugin-source')
    source_before = baseline.inspect(source_scope['project_id'])
    original = baseline.call(replay['tool'], replay['input'])
    original_view = baseline.call('opencut_job_view', {'job_id': original['job_id']})
    assert original_view['state'] == 'succeeded' and not original_view['result_unavailable']
    artifact = original_view['artifacts'][0]
    original_path = baseline.artifact(original_view)
    assert digest(original_path) == artifact['content_hash']
    candidate = digest(binary)
    bundle = root / 'plugins'
    generator = load('agent_plugin', HERE / 'distribution/agent-plugin.py')
    if not bundle.exists():
        before = database.stat().st_mtime_ns
        generator.generate(binary, database, bundle, with_worker=True)
        assert database.stat().st_mtime_ns == before, 'Packaging must not mutate the workspace'
    receipt = json.loads((bundle / 'manifest.json').read_text())
    assert receipt['binary_sha256'] == candidate and receipt['database'] == str(database)
    assert receipt['version'] == generator.PLUGIN_VERSION
    for relative, expected in receipt['files'].items():
        assert digest(bundle / relative) == expected
    Mcp = load('editorial', HERE / 'verify-editorial.py').Mcp
    results, trace = [], []
    try:
        for client in ('codex', 'claude'):
            output = root / client
            output.mkdir(exist_ok=True)
            plugin = bundle / client / 'plugins/opencut'
            manifest = json.loads((plugin / f'.{client}-plugin/plugin.json').read_text())
            assert manifest['version'] == receipt['version']
            server = json.loads((plugin / '.mcp.json').read_text())['mcpServers']['opencut']
            assert server['command'] == str(binary)
            assert server['args'] == ['--database', str(database), 'mcp', 'serve', '--with-worker']
            h = Harness(binary, workspace, f'agent-plugin-v4-{client}-{candidate[:12]}')
            h.runtime = {'bundle_manifest_sha256': digest(bundle / 'manifest.json')}
            accepted = h.state['stages'].get('delivered-mcp')
            if accepted:
                assert accepted['runtime'] == h.runtime, 'Retained evidence belongs to another bundle'
            # Fixture setup uses the existing public copy operation. It shares the
            # original CAS and does not depend on discovery exposing copy by default.
            project_id = h.name
            copied = h.stage('copy', lambda: h.request('copy', 'opencut_project_copy', {
                'source_project_id': source_scope['project_id'], 'source_revision': source_scope['revision'],
                'project_id': project_id, 'name': f'{client} delivered V4 adapter acceptance',
                'idempotency_key': h.key('copy')}))

            def exercise():
                mcp = Mcp(binary, database, output, [server['command'], *server['args']], trace)
                try:
                    tools = mcp.request('tools/list', {})['tools']
                    assert 0 < len(tools) <= 12
                    catalog = mcp.request('resources/read', {'uri': 'opencut://schemas/domains'})
                    assert 'agent.plan' in catalog['contents'][0]['text']
                    routes = {}
                    for domain in ('system', 'project', 'edit', 'media', 'analysis', 'render', 'catalog', 'task', 'observe'):
                        resource = mcp.request('resources/read', {'uri': f'opencut://schemas/domains/{domain}'})
                        payload = json.loads(resource['contents'][0]['text'])
                        assert payload['payload']['kind'] == 'domains'
                        for group in payload['payload']['data']:
                            for action in group['actions']:
                                assert action.get('input_schema') and action.get('output_schema')
                                routes[action['operation']] = (group['domain'], action['action'])

                    def call(operation, value, error=None, credential=None):
                        assert credential is None, 'This scope is the explicitly owned local workspace'
                        domain, action = routes[operation]
                        arguments = {'action': action, 'input': value}
                        result = mcp.request('tools/call', {'name': 'opencut_' + domain,
                                                          'arguments': arguments})['structuredContent']
                        trace.append({'adapter': client, 'tool': operation, 'request': value, 'reply': result})
                        if error:
                            assert result['error']['code'] == error, result
                        else:
                            assert result.get('error') is None, result
                        return result.get('data')

                    h.call = call
                    call('opencut_capabilities', {})
                    scope = {**source_scope, 'project_id': project_id, 'revision': copied['revision']}
                    summary = call('opencut_project_summary', {**{k: scope[k] for k in ('project_id', 'revision')},
                                                                'sequence_offset': 0, 'limit': 1})
                    assert summary['revision'] == copied['revision']
                    # Copying changes the aggregate identity/revision. Pin every read
                    # to the copy's actual immutable revision, not the source hash.
                    before = h.inspect(project_id, copied['revision'])
                    scope['revision'] = copied['revision']
                    query = call('opencut_project_timeline_query', {**scope,
                        'range': {'start': at(0), 'duration': at(1)}, 'track_ids': [],
                        'include_annotations': True, 'offset': 0, 'limit': 1})
                    assert query['revision'] == copied['revision']

                    def plan():
                        request = {'project_id': project_id, 'expected_revision': copied['revision'],
                            'idempotency_key': h.key('bookmark'), 'label': 'Delivered adapter V4 atomic edit',
                            'steps': [{'kind': 'operations', 'operations': [{'type': 'bookmark', 'operation': {
                                'kind': 'put', 'bookmark': {'id': h.key('mark'),
                                    'sequence_id': scope['sequence_id'], 'time': at(.5),
                                    'duration': None, 'color': '#44aaff', 'note': 'Adapter V4 readback'}}}]}]}
                        planned = h.request('bookmark-plan', 'opencut_edit_agent_plan', request)
                        assert call('opencut_edit_agent_plan', request) == planned
                        assert h.inspect(project_id)['revision'] == copied['revision'], 'Planning published a change'
                        return planned

                    planned = h.stage('plan', plan)
                    committed = h.apply('bookmark', planned['transaction'])
                    after = h.inspect(project_id, committed['revision'])
                    assert after['project'] != before['project']
                    stale = {**h.state['requests']['bookmark-plan']['input'], 'idempotency_key': h.key('stale')}
                    # On a resumed episode the head may already include undo/redo;
                    # the original base is still stale and must never be re-planned.
                    call('opencut_edit_agent_plan', stale, error='REVISION_CONFLICT')
                    undo_request = {'project_id': project_id, 'expected_revision': committed['revision'],
                        'target_transaction_id': committed['transaction_id'], 'idempotency_key': h.key('undo')}
                    undo_plan = h.stage('undo-plan', lambda: call('opencut_edit_undo_plan', undo_request))
                    assert not undo_plan['conflicts']
                    undone = h.request('undo', 'opencut_edit_undo_apply', undo_request)
                    assert call('opencut_edit_undo_apply', undo_request) == undone
                    assert h.inspect(project_id, undone['transaction']['revision'])['project'] == before['project']
                    redo_request = {'project_id': project_id, 'expected_revision': undone['transaction']['revision'],
                        'undo_transaction_id': undone['transaction']['transaction_id'], 'idempotency_key': h.key('redo')}
                    redo_plan = h.stage('redo-plan', lambda: call('opencut_edit_redo_plan', redo_request))
                    assert not redo_plan['conflicts']
                    redone = h.request('redo', 'opencut_edit_redo_apply', redo_request)
                    assert call('opencut_edit_redo_apply', redo_request) == redone
                    final = h.inspect(project_id, redone['transaction']['revision'])
                    assert final['project'] == after['project']
                    assert call(replay['tool'], replay['input'])['job_id'] == original['job_id']
                    view = call('opencut_job_view', {'job_id': original['job_id']})
                    assert view['state'] == 'succeeded' and view['artifacts'] == original_view['artifacts']
                    access = call('opencut_artifact_get', {'artifact_id': artifact['artifact_id']})
                    assert access['artifact']['content_hash'] == artifact['content_hash']
                    assert access['resource_uri'] and access['http_path'].startswith('/api/')
                    return {'adapter': client, 'plugin_version': receipt['version'], 'compact_tools': len(tools),
                        'project_id': project_id, 'revision': final['revision'], 'job_id': original['job_id'],
                        'artifact_id': artifact['artifact_id'], 'artifact_sha256': artifact['content_hash'],
                        'checks': ['actual packaged stdio configuration and connected V4 schemas',
                            'fixed-revision summary and bounded timeline read',
                            'read-only deterministic plan, atomic apply and original request replay',
                            'stale revision rejection, exact document undo/redo and receipt replay',
                            'existing fixed-revision render submission replay, terminal job and public artifact handle']}
                finally:
                    mcp.close()
            results.append(h.stage('delivered-mcp', exercise))
        assert baseline.inspect(source_scope['project_id']) == source_before, 'Source project changed'
        report = {'status': 'passed_scoped', 'binary_sha256': candidate, 'bundle': str(bundle),
            'plugin_version': receipt['version'], 'workspace': str(workspace), 'database': str(database),
            'source': source_scope, 'adapters': results, 'reused_render_request': replay,
            'reused_artifact_path': str(original_path),
            'limits': ['Deterministic MCP clients, not installed client UI or model conversations',
                'Fixture project copies share existing CAS; no new source media or rendering',
                'Prior pixel/PCM/render evidence retains its own producer identity and scope',
                'Native client manifest parsing is recorded separately; not full A7/D8 acceptance']}
        write(root / 'report.json', report)
        print(json.dumps(report, indent=2))
    except BaseException as error:
        write(root / 'failed.json', {'status': 'failed', 'error': str(error), 'completed_adapters': results})
        raise
    finally:
        previous = root / 'trace.json'
        old = json.loads(previous.read_text()) if previous.exists() else []
        write(previous, old + trace)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--replay-checkpoint', type=Path, required=True)
    parser.add_argument('--replay-request', required=True)
    parser.add_argument('--artifacts-dir', type=Path, required=True)
    verify(parser.parse_args())
