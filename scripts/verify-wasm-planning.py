#!/usr/bin/env python3
"""One retained-workspace canonical WASM contract journey;no installs,media or project copies.

Requests reference already installed,granted example components. The primary and
optional second ABI request must propose a known rename through parameters.name.
An optional historical-payload request checks rejection and actual bounded loop
execution using its real dependency binding. Runtime faults are opt-in when the
sandbox kernel's scope changes;unchanged producer evidence keeps its old identity.
"""
import argparse
import copy
from http.client import HTTPConnection
import importlib.util
import json
import hashlib
import sqlite3
from pathlib import Path
import subprocess
from urllib.parse import urlsplit
from acceptance import Harness, sha


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def stable_workspace(workspace):
    with sqlite3.connect(f"file:{workspace / 'workspace.sqlite'}?mode=ro",uri=True) as db:
        return {'project_ids':[r[0] for r in db.execute('SELECT project_id FROM projects ORDER BY project_id')],
                'jobs_sha256':sha(repr(list(db.execute('SELECT * FROM jobs ORDER BY job_id'))).encode())}


def verify(args):
    h = Harness(args.binary, args.workspace, args.name)
    recipe = json.loads(args.request.read_text())
    additional = json.loads(args.additional_abi_request.read_text()) if args.additional_abi_request else None
    historical = json.loads(args.old_payload_request.read_text()) if args.old_payload_request else None
    project = recipe['plugin']['project_id']
    sdk_root=Path(__file__).resolve().parents[1]/'packages/sdk-ts/dist'
    runtime_files=sorted(p for p in sdk_root.rglob('*') if p.is_file() and p.suffix in {'.js','.mjs','.cjs','.json'})
    assert runtime_files,'Use the already-built integrated SDK;this verifier never builds it'
    sdk_hash=hashlib.sha256()
    for file in runtime_files:
        sdk_hash.update(str(file.relative_to(sdk_root)).encode()+b'\0'+file.read_bytes()+b'\0')
    sdk_runtime=sdk_hash.hexdigest()
    scope = {'candidate': h.candidate, 'request': recipe, 'additional': additional, 'historical': historical,
             'credential_file': str(args.credential.resolve()), 'reader_file': str(args.reader.resolve()),
             'sdk_runtime_sha256':sdk_runtime,'sdk_helper_sha256':sha(Path(__file__).with_name('verify-wasm-sdk.mjs').read_bytes()),
             'runtime_faults': args.runtime_faults, 'revocation': args.check_revocation,
             'agent_bundle': str(args.agent_bundle.resolve()) if args.agent_bundle else None,
             'agent_manifest_sha256':sha((args.agent_bundle/'manifest.json').read_bytes()) if args.agent_bundle else None}
    assert h.state.setdefault('scope', scope) == scope, 'Keep the original candidate,requests and authority checkpoint'
    if 'baseline' not in h.state:
        h.state['baseline'] = h.inspect(project)
        h.state['stable_workspace']=stable_workspace(h.workspace)
        h.save()
    before = h.state['baseline']
    assert not recipe.get('branch_name'), 'This retained journey owns main only;branch publication has separate coverage'
    expected_name = recipe['plugin']['parameters']['name']
    assert isinstance(expected_name, str) and expected_name != before['project']['name']
    expected = copy.deepcopy(before['project'])
    expected['name'] = expected_name
    actor = args.credential.resolve()
    status = h.cli(['server', '--directory', str(args.service_directory or h.workspace / 'service'), 'status'])
    assert status['ready'] and sha(Path(status['executable']).read_bytes()) == h.candidate
    endpoint = urlsplit(status['endpoint'])
    assert endpoint.scheme == 'http' and endpoint.hostname == '127.0.0.1'

    def capture(value):
        value = copy.deepcopy(value)
        assert value['plugin']['project_id'] == project
        assert value['plugin'].get('revision', before['revision']) == before['revision']
        assert not value.get('branch_name')
        value['plugin']['revision'] = before['revision']
        return value

    request = capture(recipe)
    def http(value, error):
        connection=HTTPConnection(endpoint.hostname,endpoint.port,timeout=60)
        try:
            connection.request('POST', '/api/tools/opencut_project_plugin_plan', json.dumps(value), {
                'Authorization': 'Bearer ' + actor.read_text().strip(), 'Content-Type': 'application/json'})
            response = connection.getresponse()
            body = json.loads(response.read())
        finally:
            connection.close()
        assert body['error']['code'] == error, body
        assert 'wasm backtrace' not in json.dumps(body) and str(h.workspace) not in json.dumps(body)
        return {'code': error, 'http_status': response.status}

    editorial = module('wasm_editorial', Path(__file__).with_name('verify-editorial.py'))
    def proposal():
        h.call('opencut_project_plugin_plan', request, credential=args.reader, error='NOT_AUTHORIZED')
        planned = h.request('public-plan', 'opencut_project_plugin_plan', request, credential=actor)
        assert 'schema_version' not in planned and planned['preview'] == expected
        assert planned['transaction']['operations'] == [{'type': 'rename', 'name': expected_name}]
        assert planned['transaction']['expected_revision'] == before['revision']
        sdk_input = h.workspace / (args.name + '-sdk-request.json')
        sdk_output = h.workspace / (args.name + '-sdk-result.json')
        sdk_input.write_text(json.dumps(request, indent=2) + '\n')
        if 'sdk_proof' not in h.state:
            subprocess.run(['node', str(Path(__file__).with_name('verify-wasm-sdk.mjs')),
                status['endpoint'], str(actor), str(sdk_input), str(sdk_output)], check=True, timeout=60)
            assert json.loads(sdk_output.read_text()) == planned
            h.state['sdk_proof'] = True
            h.save()
        if 'mcp_proof' not in h.state:
            client = editorial.Mcp(h.binary, h.workspace / 'workspace.sqlite', h.workspace,
                [str(h.binary), '--database', str(h.workspace / 'workspace.sqlite'), '--credential-file', str(actor), 'mcp', 'serve'])
            try:
                tools = client.request('tools/list', {})['tools']
                plugin = next(tool for tool in tools if tool['name'] == 'opencut_plugin')
                actions = plugin['inputSchema']['properties']['action']['enum']
                assert actions.count('project.plugin.plan') == 1
                assert 'project.plugin.v4.v1.plan' not in actions and 'plan' not in actions
                assert client.tool('opencut_plugin', {'action': 'project.plugin.plan', 'input': request}) == planned
            finally:
                client.close()
            h.state['mcp_proof']=True
            h.save()
        if additional:
            other = capture(additional)
            assert other['plugin']['parameters']['name'] == expected_name
            assert h.request('additional-abi-plan', 'opencut_project_plugin_plan', other, credential=actor)['preview'] == expected
        proofs=h.state.setdefault('fault_proofs',{})
        def fault(name,value,code):
            if name not in proofs:
                proof=http(value,code)
                live=h.cli(['server','--directory',str(args.service_directory or h.workspace/'service'),'status'])
                assert live['ready'] and live['instance']==status['instance'] and live['pid']==status['pid']
                proofs[name]={**proof,'service_instance':live['instance'],'http_pid':live['pid'],'survived':True}
                h.save()
            return proofs[name]
        faults=[]
        if historical:
            old=capture(historical)
            faults.append(fault('historical-payload',old,'INVALID_PLUGIN'))
            old['plugin']['parameters']={'loop':True}
            faults.append(fault('historical-function-loop',old,'PLUGIN_RUNTIME'))
        if args.runtime_faults:
            for name,code in [('loop','PLUGIN_RUNTIME'),('allocate','PLUGIN_RUNTIME'),
                              ('invalid_json','INVALID_PLUGIN'),('legacy_operation','INVALID_PLUGIN'),
                              ('private_resource','INVALID_PLUGIN'),('empty','INVALID_PLUGIN'),
                              ('too_many','INVALID_PLUGIN'),('invalid_sequence','NOT_FOUND')]:
                value=copy.deepcopy(request)
                value['plugin']['parameters']={'fault':name}
                faults.append(fault('current-'+name,value,code))
        assert h.inspect(project) == before
        return {'plan': planned, 'faults': faults, 'additional_abi': bool(additional),
                'cli_http_sdk_mcp_equal': True}

    proposed = h.stage('proposal-and-contract', proposal)
    planned = proposed['plan']
    def delivered():
        bundle = args.agent_bundle.resolve()
        receipt = json.loads((bundle / 'manifest.json').read_text())
        assert receipt['binary_sha256'] == h.candidate
        for relative, digest in receipt['files'].items():
            assert sha((bundle / relative).read_bytes()) == digest
        for adapter in ['codex', 'claude']:
            configuration = json.loads((bundle / adapter / 'plugins/opencut/.mcp.json').read_text())['mcpServers']['opencut']
            client = editorial.Mcp(h.binary, h.workspace / 'workspace.sqlite', h.workspace,
                [configuration['command'], *configuration['args']])
            try:
                assert client.tool('opencut_plugin', {'action': 'project.plugin.plan', 'input': request}) == planned
            finally:
                client.close()
        return {'bundle': str(bundle), 'adapters': ['codex', 'claude'], 'created_new_bundle': False}
    delivery = h.stage('delivered-configurations', delivered) if args.agent_bundle else None

    def apply():
        receipt = h.apply('apply-proposal', planned['transaction'], credential=actor)
        assert h.inspect(project, receipt['revision'])['project'] == expected
        if 'stale_plan_proof' not in h.state:
            http(request, 'REVISION_CONFLICT')
            h.state['stale_plan_proof'] = True
            h.save()
        return receipt
    applied = h.stage('apply-and-replay', apply)

    def revoke():
        grant = planned['grant']
        disabled = h.request('disable-grant', 'opencut_project_plugin_grant', {
            'target': grant['target'], 'package_sha256': grant['package_sha256'], 'operations': grant['operations'],
            'enabled': False, 'expected_revision': grant['revision'], 'idempotency_key': h.key('disable-grant')})
        if 'revoked_proof' not in h.state:
            http(request, 'NOT_AUTHORIZED')
            h.state['revoked_proof'] = True
            h.save()
        enabled = h.request('restore-grant', 'opencut_project_plugin_grant', {
            'target': grant['target'], 'package_sha256': grant['package_sha256'], 'operations': grant['operations'],
            'enabled': True, 'expected_revision': disabled['revision'], 'idempotency_key': h.key('restore-grant')})
        assert h.call('opencut_project_plugin_list', {'project_id': project}, credential=actor).count(enabled) == 1
        return {'rejected': 'NOT_AUTHORIZED', 'restored_grant': enabled}
    revocation = h.stage('revocation', revoke) if args.check_revocation else None

    def restore():
        original = {'project_id': project, 'expected_revision': applied['revision'],
                    'idempotency_key': h.key('restore-name'), 'label': 'Restore original acceptance content',
                    'operations': [{'type': 'rename', 'name': before['project']['name']}]}
        receipt = h.apply('restore-name', original, credential=actor)
        assert h.inspect(project)['project'] == before['project']
        return receipt
    restored = h.stage('restore-content', restore)
    current = h.cli(['server', '--directory', str(args.service_directory or h.workspace / 'service'), 'status'])
    assert current['ready'] and current['instance'] == status['instance'] and current['pid'] == status['pid']
    assert stable_workspace(h.workspace)==h.state['stable_workspace']
    report = {'status': 'passed_scoped','contract':'canonical_project_plugin_plan','sdk_runtime_sha256':sdk_runtime, 'candidate': h.candidate, 'project_id': project,
              'checkpoint': str(h.checkpoint), 'plan_grant': planned['grant'], 'contracts': proposed,
              'apply': applied, 'restore': restored, 'revocation': revocation, 'delivery': delivery,
              'new_projects': 0, 'installs_or_media_jobs': 0,
              'limits': ['Current canonical planner and actual selected dependency ABI scope only.',
                         'No automatic package/component rebuild,workspace copy,ASR,media or intermediate candidate.',
                         'Runtime-fault and delivered-configuration evidence exists only when its explicit option ran.']}
    (h.workspace / (args.name + '-report.json')).write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({'status': report['status'], 'project_id': project, 'candidate': h.candidate}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'request', 'credential', 'reader']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--additional-abi-request', type=Path)
    parser.add_argument('--old-payload-request', type=Path)
    parser.add_argument('--service-directory', type=Path)
    parser.add_argument('--agent-bundle', type=Path, help='Already produced final bundle;never generated here')
    parser.add_argument('--runtime-faults', action='store_true')
    parser.add_argument('--check-revocation', action='store_true')
    parser.add_argument('--name', default='wasm-planning')
    verify(parser.parse_args())
