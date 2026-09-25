#!/usr/bin/env python3
"""Exercise selective undo/redo and independent branch merge through the real CLI."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def verify(binary: Path, root: Path):
    root.mkdir(parents=True, exist_ok=True)
    database = root / 'workspace.sqlite'
    agent_key = root / 'agent.secret'
    checks = []
    trace = []

    def cli(args, payload=None, credential=None, error=None):
        command = [str(binary), '--database', str(database), '--json']
        if credential:
            command += ['--credential-file', str(credential)]
        command += args
        result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                                text=True, capture_output=True, timeout=45)
        response = json.loads(result.stdout)
        trace.append({'command': args, 'exit': result.returncode, 'response': response})
        if error:
            assert result.returncode != 0 and response['error']['code'] == error, (response, result.stderr)
        else:
            assert result.returncode == 0 and response['error'] is None, (response, result.stderr)
        return response.get('data')

    def call(tool, payload, credential=None, error=None):
        return cli(['call', tool, '--input', '-'], payload, credential, error)

    def edit(revision, key, operations):
        return {'project_id': 'collab', 'request': {'expected_revision': revision,
                'idempotency_key': key, 'label': key, 'operations': operations}}

    def snapshot():
        return call('opencut_project_inspect', {'project_id': 'collab'})

    def track(project):
        return next(item for item in project['sequences'][0]['tracks'] if item['id'] == 'voice')

    try:
        created = call('opencut_project_create', {'project': {'id': 'collab', 'name': 'Collaboration',
            'sequence_id': 'main', 'width': 640, 'height': 360,
            'fps': {'numerator': 25, 'denominator': 1}}, 'idempotency_key': 'create'})
        base_track = {'id': 'voice', 'name': 'Voice', 'kind': 'audio', 'enabled': True,
                      'muted': False, 'locked': False, 'clips': [], 'params': {'volume': 1.0}}
        base = call('opencut_edit_apply', edit(created['revision'], 'track',
            [{'type': 'track.create', 'sequence_id': 'main', 'track': base_track}]))
        cli(['access', 'set-policy', '--input', '-'], {'principal_id': 'agent', 'expected_revision': '0',
            'enabled': True, 'grants': [{'project_id': 'collab', 'actions': ['read', 'edit']}]})
        cli(['access', 'issue', 'agent', '--output', str(agent_key)])

        agent = call('opencut_edit_apply', edit(base['revision'], 'agent-volume',
            [{'type': 'parameter.set_track', 'sequence_id': 'main', 'track_id': 'voice',
              'params': {'volume': 0.4}}]), agent_key)
        cue = {'id': 'human-cue', 'timeline_range': {'start': {'ticks': '0', 'timebase': 1000},
                'duration': {'ticks': '1000', 'timebase': 1000}}, 'text': 'human edit',
               'source_anchor': None, 'style': {'font_family': 'Arial', 'font_size': 18,
               'color': '#ffffff', 'background_color': None}}
        human = call('opencut_edit_apply', edit(agent['revision'], 'human-caption',
            [{'type': 'caption.insert', 'sequence_id': 'main', 'cue': cue}]))
        undo = {'project_id': 'collab', 'expected_revision': human['revision'],
                'target_transaction_id': agent['transaction_id'], 'idempotency_key': 'undo-agent'}
        plan = call('opencut_edit_undo_plan', undo, agent_key)
        assert plan['conflicts'] == [] and any('volume' in change['path'] for change in plan['changes'])
        committed = call('opencut_edit_undo_apply', undo, agent_key)
        assert call('opencut_edit_undo_apply', undo, agent_key) == committed
        state = snapshot()['project']
        assert track(state)['params']['volume'] == 1.0
        assert [item['id'] for item in state['sequences'][0]['caption_cues']] == ['human-cue']
        checks.append('Agent volume undo preserves later human caption; exact retry shares transaction')

        redo = {'project_id': 'collab', 'expected_revision': committed['transaction']['revision'],
                'undo_transaction_id': committed['transaction']['transaction_id'],
                'idempotency_key': 'redo-agent'}
        assert call('opencut_edit_redo_plan', redo, agent_key)['conflicts'] == []
        redone = call('opencut_edit_redo_apply', redo, agent_key)
        assert track(snapshot()['project'])['params']['volume'] == 0.4
        checks.append('Causal redo restores Agent volume while retaining caption')

        branch = call('opencut_edit_branch_create', {'project_id': 'collab', 'branch_name': 'mix',
            'from_revision': redone['transaction']['revision'],
            'expected_revision': redone['transaction']['revision'], 'idempotency_key': 'branch-mix'})
        branch_edit = call('opencut_edit_branch_apply', {'project_id': 'collab', 'branch_name': 'mix',
            'request': edit(branch['head_revision'], 'branch-mute',
                [{'type': 'track.update', 'sequence_id': 'main', 'track_id': 'voice',
                  'name': None, 'enabled': None, 'muted': True, 'locked': None}])['request']}, agent_key)
        main_edit = call('opencut_edit_apply', edit(redone['transaction']['revision'], 'main-name',
            [{'type': 'track.update', 'sequence_id': 'main', 'track_id': 'voice',
              'name': 'Human voice', 'enabled': None, 'muted': None, 'locked': None}]))
        assert track(snapshot()['project'])['muted'] is False
        compare = {'project_id': 'collab', 'source_branch': 'mix', 'target_branch': 'main',
                   'expected_source_revision': branch_edit['revision'],
                   'expected_target_revision': main_edit['revision']}
        preview = call('opencut_edit_branch_compare', compare, agent_key)
        assert preview['conflicts'] == [] and len(preview['changes']) == 1
        merge = call('opencut_edit_branch_merge', {**compare, 'idempotency_key': 'merge-mix'}, agent_key)
        assert call('opencut_edit_branch_merge', {**compare, 'idempotency_key': 'merge-mix'}, agent_key) == merge
        state = snapshot()['project']
        assert track(state)['muted'] is True and track(state)['name'] == 'Human voice'
        assert [item['id'] for item in state['sequences'][0]['caption_cues']] == ['human-cue']
        branches = call('opencut_edit_branch_list', {'project_id': 'collab'})
        assert next(item for item in branches if item['branch_name'] == 'mix')['head_revision'] == branch_edit['revision']
        checks.append('Independent branch merge preserves main name and caption; source head stays fixed')

        later = call('opencut_edit_branch_apply', {'project_id': 'collab', 'branch_name': 'mix',
            'request': edit(branch_edit['revision'], 'branch-volume',
                [{'type': 'parameter.set_track', 'sequence_id': 'main', 'track_id': 'voice',
                  'params': {'volume': 0.7}}])['request']}, agent_key)
        again = {'project_id': 'collab', 'source_branch': 'mix', 'target_branch': 'main',
                 'expected_source_revision': later['revision'],
                 'expected_target_revision': merge['transaction']['revision']}
        assert call('opencut_edit_branch_compare', again)['conflicts'] == []
        merged_again = call('opencut_edit_branch_merge', {**again, 'idempotency_key': 'merge-again'})
        assert track(snapshot()['project'])['params']['volume'] == 0.7
        checks.append('Second merge uses recorded branch ancestry and carries only its new change')

        conflict_branch = call('opencut_edit_branch_apply', {'project_id': 'collab', 'branch_name': 'mix',
            'request': edit(later['revision'], 'branch-conflict',
                [{'type': 'parameter.set_track', 'sequence_id': 'main', 'track_id': 'voice',
                  'params': {'volume': 0.6}}])['request']}, agent_key)
        conflict_main = call('opencut_edit_apply', edit(merged_again['transaction']['revision'], 'main-conflict',
            [{'type': 'parameter.set_track', 'sequence_id': 'main', 'track_id': 'voice',
              'params': {'volume': 0.9}}]))
        conflict = {'project_id': 'collab', 'source_branch': 'mix', 'target_branch': 'main',
                    'expected_source_revision': conflict_branch['revision'],
                    'expected_target_revision': conflict_main['revision']}
        assert any('volume' in item['path'] for item in call('opencut_edit_branch_compare', conflict)['conflicts'])
        call('opencut_edit_branch_merge', {**conflict, 'idempotency_key': 'conflict'}, error='BRANCH_MERGE_CONFLICT')
        assert track(snapshot()['project'])['params']['volume'] == 0.9
        checks.append('Same-property divergence reports conflict and leaves main intact')

        merged_branch_undo = {'project_id': 'collab', 'expected_revision': conflict_main['revision'],
            'target_transaction_id': branch_edit['transaction_id'],
            'idempotency_key': 'undo-merged-branch-mute'}
        assert call('opencut_edit_undo_plan', merged_branch_undo)['conflicts'] == []
        call('opencut_edit_undo_apply', merged_branch_undo)
        state = snapshot()['project']
        assert track(state)['muted'] is False and track(state)['params']['volume'] == 0.9
        assert track(state)['name'] == 'Human voice' and state['sequences'][0]['caption_cues'][0]['id'] == 'human-cue'
        checks.append('Merged branch transaction is a causal ancestor; selective undo retains later main edits')

        held_head = snapshot()['revision']
        hold_request = {'project_id': 'collab', 'expected_generation': '0', 'held': True,
                        'idempotency_key': 'owner-hold'}
        hold = call('opencut_edit_control_set', hold_request)
        assert hold['generation'] == '1' and hold['held'] is True
        assert call('opencut_edit_control_set', hold_request) == hold
        assert call('opencut_edit_control_get', {'project_id': 'collab'}, agent_key) == hold
        agent_held_edit = edit(held_head, 'held-agent-edit',
            [{'type': 'track.update', 'sequence_id': 'main', 'track_id': 'voice',
              'name': 'Agent blocked', 'enabled': None, 'muted': None, 'locked': None}])
        assert call('opencut_edit_plan', agent_held_edit, agent_key)['base_revision'] == held_head
        call('opencut_edit_apply', agent_held_edit, agent_key, 'EDIT_HELD_BY_OWNER')
        call('opencut_edit_branch_create', {'project_id': 'collab', 'branch_name': 'held',
            'from_revision': held_head, 'expected_revision': held_head,
            'idempotency_key': 'held-branch'}, agent_key, 'EDIT_HELD_BY_OWNER')
        call('opencut_edit_branch_apply', {'project_id': 'collab', 'branch_name': 'mix',
            'request': edit(conflict_branch['revision'], 'held-branch-edit',
                [{'type': 'track.update', 'sequence_id': 'main', 'track_id': 'voice',
                  'name': 'Blocked branch', 'enabled': None, 'muted': None, 'locked': None}])['request']},
            agent_key, 'EDIT_HELD_BY_OWNER')
        call('opencut_edit_control_set', {**hold_request, 'idempotency_key': 'agent-false-hold'},
             agent_key, 'NOT_AUTHORIZED')
        owner_held = call('opencut_edit_apply', edit(held_head, 'owner-while-held',
            [{'type': 'track.update', 'sequence_id': 'main', 'track_id': 'voice',
              'name': 'Owner held', 'enabled': None, 'muted': None, 'locked': None}]))
        released = call('opencut_edit_control_set', {'project_id': 'collab',
            'expected_generation': '1', 'held': False, 'idempotency_key': 'owner-release'})
        assert released['generation'] == '2' and released['held'] is False
        call('opencut_edit_apply', edit(owner_held['revision'], 'agent-after-release',
            [{'type': 'track.update', 'sequence_id': 'main', 'track_id': 'voice',
              'name': 'Agent resumed', 'enabled': None, 'muted': None, 'locked': None}]), agent_key)
        checks.append('Persistent owner hold permits read/plan and owner edits, fences Agent writes, then release restores Agent edits')

        # Each CLI invocation reopens the database; these checks establish durable public state.
        assert len(call('opencut_project_history', {'project_id': 'collab', 'limit': 50})['revisions']) >= 6
        report = {'status': 'passed', 'checks': checks,
                  'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'status': 'passed', 'report': str(root / 'report.json')}))
    finally:
        (root / 'trace.json').write_text(json.dumps(trace, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--artifacts-dir', type=Path)
    args = parser.parse_args()
    verify(args.binary.resolve(), (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-r3-'))).resolve())
