#!/usr/bin/env python3
"""Actual V4 middle-operation failure through CLI and MCP, without new media.

The existing owned document/head and complete public history must remain equal.
A corrected read-only positive control demonstrates the private working graph.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path

from acceptance import Harness, at, sha


def verify(args):
    h = Harness(args.binary, args.workspace, 'agent-v4-atomicity')
    episode = h.candidate[:12] + '-' + sha((args.project + args.revision).encode())[:12]
    trace = []
    def exercise():
        before = h.inspect(args.project)
        assert before['revision'] == args.revision
        history_request = {'project_id': args.project, 'from_revision': None, 'limit': 100}
        history = h.call('opencut_project_history', history_request)
        assert history['next_cursor'] is None, 'Capture complete history, not a truncated first page'
        identifier = h.key(episode + '-bookmark')
        first = {'type': 'bookmark', 'operation': {'kind': 'put', 'bookmark': {
            'id': identifier, 'sequence_id': args.sequence, 'time': at(.2), 'duration': None,
            'color': '#44aaff', 'note': 'This first operation must never be published'}}}
        second = {'type': 'bookmark', 'operation': {'kind': 'move',
            'sequence_id': args.sequence, 'bookmark_id': identifier + '-missing', 'time': at(.8)}}
        third = {'type': 'rename', 'name': before['project']['name'] + ' - unreachable atomicity marker'}
        request = {'project_id': args.project, 'expected_revision': args.revision,
            'idempotency_key': h.key(episode + '-rejected'), 'label': 'Fail operation2 of3 without partial state',
            'operations': [first, second, third]}
        saved = {'tool': 'opencut_edit_apply', 'input': request}
        retained = h.state['requests'].setdefault('rejected-' + episode, saved)
        assert retained == saved, 'Keep the original failed key and payload'
        h.save()
        h.call('opencut_edit_apply', request, error='NOT_FOUND')
        assert h.inspect(args.project) == before
        assert h.call('opencut_project_history', history_request) == history
        trace.append({'transport': 'cli', 'request': request, 'error_code': 'NOT_FOUND',
                      'whole_project_equal': True, 'complete_history_equal': True})
        spec = importlib.util.spec_from_file_location('editorial', Path(__file__).with_name('verify-editorial.py'))
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        output = args.report.parent
        mcp = module.Mcp(h.binary, h.workspace / 'workspace.sqlite', output,
                        [str(h.binary), '--database', str(h.workspace / 'workspace.sqlite'), 'mcp', 'serve'])
        try:
            reply = mcp.request('tools/call', {'name': 'opencut_edit', 'arguments': {
                'action': 'apply', 'input': request}})['structuredContent']
            assert reply['error']['code'] == 'NOT_FOUND', reply
            trace.append({'transport': 'mcp', 'request': request, 'reply': reply})
            assert h.inspect(args.project) == before
            assert h.call('opencut_project_history', history_request) == history
            # Repair only the missing target in a distinct read-only control.
            # The second step must now observe the first step's private insert.
            corrected = copy.deepcopy(second)
            corrected['operation']['bookmark_id'] = identifier
            plan_request = {'project_id': args.project, 'expected_revision': args.revision,
                'idempotency_key': h.key(episode + '-control'), 'label': 'Private sequential positive control',
                'steps': [{'kind': 'operations', 'operations': [operation]}
                          for operation in [first, corrected, third]]}
            control = mcp.tool('opencut_edit', {'action': 'agent.plan', 'input': plan_request})
            assert control['transaction']['expected_revision'] == args.revision
            assert control['transaction']['operations'] == [first, corrected, third]
            trace.append({'transport': 'mcp', 'request': plan_request, 'reply': control})
        finally:
            mcp.close()
        assert h.inspect(args.project) == before
        assert h.call('opencut_project_history', history_request) == history
        return {'status': 'passed_scoped', 'binary_sha256': h.candidate, 'workspace': str(h.workspace),
            'project_id': args.project, 'revision': args.revision, 'history_revisions': len(history['revisions']),
            'cli_error': 'NOT_FOUND', 'mcp_same_request_error': 'NOT_FOUND', 'operation_count': 3,
            'failure_ordinal': 2, 'complete_project_equal': True, 'complete_history_equal': True,
            'positive_control': 'Corrected second step sees first insert in private graph; plan preserves3 ordered public operations and commits nothing',
            'scope': 'Actual public V4 apply middle-operation domain failure, same original failed request retried through MCP, and sequential read-only positive control. No new project, media, Job or product build. Not database crash/disk or concurrent writer fault injection.'}
    try:
        result = h.stage('middle-operation-' + episode, exercise)
        args.report.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2))
    finally:
        if trace:
            args.report.with_name(args.report.stem + '-trace.json').write_text(json.dumps(trace, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'report']:
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ['project', 'revision', 'sequence']:
        parser.add_argument('--' + name, required=True)
    verify(parser.parse_args())
