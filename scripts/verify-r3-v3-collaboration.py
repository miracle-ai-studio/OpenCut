#!/usr/bin/env python3
"""Exercise schema 3 causal undo, branches and owner hold through the public CLI."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile


def verify(binary: Path, root: Path):
    root.mkdir(parents=True, exist_ok=True)
    database = root / 'workspace.sqlite'
    secret = root / 'agent.secret'
    checks = []
    trace = []

    def cli(args, payload=None, credential=None, error=None):
        command = [str(binary), '--database', str(database), '--json']
        if credential:
            command += ['--credential-file', str(credential)]
        result = subprocess.run(command + args, input=json.dumps(payload) if payload is not None else None,
                                text=True, capture_output=True, timeout=45)
        response = json.loads(result.stdout)
        trace.append({'command': args, 'exit': result.returncode, 'response': response})
        if error:
            assert result.returncode != 0 and response['error']['code'] == error, (response, result.stderr)
        else:
            assert result.returncode == 0 and response['error'] is None, (response, result.stderr)
        return response.get('data')

    def call(name, payload, credential=None, error=None):
        return cli(['call', name, '--input', '-'], payload, credential, error)

    def edit(revision, key, operations, project='r3v3'):
        return {'project_id': project, 'expected_revision': revision, 'idempotency_key': key,
                'label': key, 'operations': operations}

    def shape(name, x=0):
        return {'type': 'shape.put', 'sequence_id': 'main', 'shape': {'id': name,
            'track_id': 'graphics', 'timeline_range': {'start': {'ticks': '0', 'timebase': 1000},
            'duration': {'ticks': '1000', 'timebase': 1000}}, 'primitive': {'kind': 'rectangle',
            'width_px': 80, 'height_px': 40}, 'fill': {'red': 255, 'green': 0, 'blue': 0, 'alpha': 255},
            'position_x': x, 'position_y': 0, 'rotation_degrees': 0, 'opacity': 1}}

    def rename(name):
        return {'type': 'base.apply', 'operation': {'type': 'sequence.update_settings',
            'sequence_id': 'main', 'name': name, 'width': None, 'height': None, 'fps': None}}

    def legacy_rename(name):
        return {'type': 'sequence.update_settings', 'sequence_id': 'main', 'name': name,
                'width': None, 'height': None, 'fps': None}

    def snapshot():
        return call('opencut_project_v3_inspect', {'project_id': 'r3v3', 'revision': None})

    try:
        created = call('opencut_project_v3_create', {'project': {'id': 'r3v3', 'name': 'R3 V3',
            'sequence_id': 'main', 'width': 640, 'height': 360,
            'fps': {'numerator': 25, 'denominator': 1}}, 'idempotency_key': 'create-v3'})
        track = {'type': 'base.apply', 'operation': {'type': 'track.create', 'sequence_id': 'main',
            'track': {'id': 'graphics', 'name': 'Graphics', 'kind': 'graphic', 'enabled': True,
            'muted': False, 'locked': False, 'clips': [], 'params': {}}}}
        base = call('opencut_edit_v3_apply', edit(created['revision'], 'graphics-track', [track]))
        cli(['access', 'set-policy', '--input', '-'], {'principal_id': 'agent', 'expected_revision': '0',
            'enabled': True, 'grants': [{'project_id': 'r3v3', 'actions': ['read', 'edit']}]})
        issued = cli(['access', 'issue', 'agent', '--output', str(secret)])
        first = call('opencut_edit_v3_apply', edit(base['revision'], 'agent-first', [shape('first')]), secret)
        later = call('opencut_edit_v3_apply', edit(first['revision'], 'owner-later', [shape('later', 120)]))
        undo = {'project_id': 'r3v3', 'expected_revision': later['revision'],
                'target_transaction_id': first['transaction_id'], 'idempotency_key': 'undo-first'}
        plan = call('opencut_edit_v3_undo_plan', undo, secret)
        assert plan['conflicts'] == [] and any('/shapes/first' in change['path'] for change in plan['changes'])
        undone = call('opencut_edit_v3_undo_apply', undo, secret)
        assert call('opencut_edit_v3_undo_apply', undo, secret) == undone
        assert [s['id'] for s in snapshot()['project']['visual'][0]['shapes']] == ['later']
        redo = {'project_id': 'r3v3', 'expected_revision': undone['transaction']['revision'],
                'undo_transaction_id': undone['transaction']['transaction_id'], 'idempotency_key': 'redo-first'}
        redone = call('opencut_edit_v3_redo_apply', redo, secret)
        assert {s['id'] for s in snapshot()['project']['visual'][0]['shapes']} == {'first', 'later'}
        checks.append('Agent first shape undo preserves later independent owner shape; causal redo and retry persist')

        branch = call('opencut_edit_v3_branch_create', {'project_id': 'r3v3', 'branch_name': 'agent',
            'from_revision': redone['transaction']['revision'],
            'expected_revision': redone['transaction']['revision'], 'idempotency_key': 'branch-create'})
        grade = {'type': 'effect.put', 'sequence_id': 'main', 'effect': {'id': 'grade', 'target_id': 'first',
            'effect': {'kind': 'color_adjust', 'exposure_ev': 0.2, 'contrast': 1, 'saturation': 1}}}
        branch_edit = call('opencut_edit_v3_branch_apply', {'project_id': 'r3v3', 'branch_name': 'agent',
            'request': edit(branch['head_revision'], 'branch-grade', [grade])}, secret)
        main = call('opencut_edit_v3_apply', edit(redone['transaction']['revision'], 'main-caption-name',
            [rename('Owner main')]))
        compare = {'project_id': 'r3v3', 'source_branch': 'agent', 'target_branch': 'main',
            'expected_source_revision': branch_edit['revision'], 'expected_target_revision': main['revision']}
        assert call('opencut_edit_v3_branch_compare', compare, secret)['conflicts'] == []
        call('opencut_edit_v3_branch_merge', {**compare, 'expected_target_revision':
            redone['transaction']['revision'], 'idempotency_key': 'stale-merge'}, secret, 'REVISION_CONFLICT')
        merged = call('opencut_edit_v3_branch_merge', {**compare, 'idempotency_key': 'merge'}, secret)
        assert snapshot()['project']['base']['sequences'][0]['name'] == 'Owner main'
        assert snapshot()['project']['visual'][0]['effects'][0]['id'] == 'grade'
        branch_undo = call('opencut_edit_v3_undo_apply', {'project_id': 'r3v3',
            'expected_revision': merged['transaction']['revision'],
            'target_transaction_id': branch_edit['transaction_id'],
            'idempotency_key': 'undo-merged-effect'}, secret)
        assert snapshot()['project']['visual'][0]['effects'] == []
        assert snapshot()['project']['base']['sequences'][0]['name'] == 'Owner main'
        checks.append('Versioned branch merge keeps independent base edit, enforces CAS and permits DAG-selective undo')

        held_revision = branch_undo['transaction']['revision']
        held = call('opencut_edit_control_set', {'project_id': 'r3v3', 'expected_generation': '0',
            'held': True, 'idempotency_key': 'hold'})
        assert held['held'] is True
        blocked = edit(held_revision, 'held-shape', [shape('blocked')])
        assert call('opencut_edit_v3_plan', blocked, secret)['proposed_project']
        call('opencut_edit_v3_apply', blocked, secret, 'EDIT_HELD_BY_OWNER')
        call('opencut_edit_v3_branch_create', {'project_id': 'r3v3', 'branch_name': 'blocked',
            'from_revision': held_revision, 'expected_revision': held_revision,
            'idempotency_key': 'held-branch'}, secret, 'EDIT_HELD_BY_OWNER')
        owner = call('opencut_edit_v3_apply', edit(held_revision, 'owner-held', [shape('owner-held')]))
        call('opencut_edit_control_set', {'project_id': 'r3v3', 'expected_generation': '1',
            'held': False, 'idempotency_key': 'release'})
        call('opencut_edit_v3_apply', edit(owner['revision'], 'agent-resumed', [shape('resumed')]), secret)
        cli(['access', 'revoke', issued['credential_id']])
        call('opencut_project_v3_inspect', {'project_id': 'r3v3', 'revision': None}, secret, 'NOT_AUTHORIZED')
        checks.append('Persistent owner hold blocks scoped V3 writes but permits planning and owner writes; release and revoke apply')

        legacy = call('opencut_project_create', {'project': {'id': 'promoted', 'name': 'Promotion',
            'sequence_id': 'main', 'width': 640, 'height': 360,
            'fps': {'numerator': 25, 'denominator': 1}}, 'idempotency_key': 'legacy-create'})
        legacy_track = call('opencut_edit_apply', {'project_id': 'promoted', 'request': {
            'expected_revision': legacy['revision'], 'idempotency_key': 'legacy-track',
            'label': 'Create graphics track', 'operations': [{'type': 'track.create',
                'sequence_id': 'main', 'track': {'id': 'graphics', 'name': 'Graphics',
                    'kind': 'graphic', 'enabled': True, 'muted': False, 'locked': False,
                    'clips': [], 'params': {}}}]}})
        pre_promotion_edit = call('opencut_edit_apply', {'project_id': 'promoted', 'request': {
            'expected_revision': legacy_track['revision'], 'idempotency_key': 'legacy-rename',
            'label': 'Rename before promotion',
            'operations': [legacy_rename('Edited before promotion')]}})
        promoted = call('opencut_project_v3_promote', {'project_id': 'promoted',
            'expected_revision': pre_promotion_edit['revision'], 'idempotency_key': 'promote'})
        post_promotion_edit = call('opencut_edit_v3_apply', edit(promoted['revision'],
            'post-promotion-shape', [shape('preserved-shape')], project='promoted'))
        old_edit_undo = {'project_id': 'promoted', 'expected_revision': post_promotion_edit['revision'],
            'target_transaction_id': pre_promotion_edit['transaction_id'], 'idempotency_key': 'undo-old-edit'}
        old_edit_plan = call('opencut_edit_v3_undo_plan', old_edit_undo)
        assert old_edit_plan['conflicts'] == []
        assert old_edit_plan['proposed_project']['base']['sequences'][0]['name'] == 'Main'
        assert old_edit_plan['proposed_project']['visual'][0]['shapes'][0]['id'] == 'preserved-shape'
        old_edit_undone = call('opencut_edit_v3_undo_apply', old_edit_undo)
        assert call('opencut_edit_v3_undo_apply', old_edit_undo) == old_edit_undone
        old_edit_redone = call('opencut_edit_v3_redo_apply', {
            'project_id': 'promoted', 'expected_revision': old_edit_undone['transaction']['revision'],
            'undo_transaction_id': old_edit_undone['transaction']['transaction_id'],
            'idempotency_key': 'redo-old-edit'})
        assert call('opencut_project_v3_inspect', {'project_id': 'promoted', 'revision': None})[
            'project']['base']['sequences'][0]['name'] == 'Edited before promotion'
        checks.append('Schema 2 edit history is selectively undone/redone after promotion while later schema 3 visual work survives')
        with sqlite3.connect(database) as connection:
            promotion_id = connection.execute('SELECT transaction_id FROM revisions WHERE project_id=? AND revision=?',
                ('promoted', promoted['revision'])).fetchone()[0]
        call('opencut_edit_v3_undo_plan', {'project_id': 'promoted',
            'expected_revision': old_edit_redone['transaction']['revision'], 'target_transaction_id': promotion_id,
            'idempotency_key': 'undo-promotion'}, error='INVALID_ARGUMENT')
        checks.append('The schema 2-to-3 promotion record remains excluded from content undo')

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
    verify(args.binary.resolve(), (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-r3-v3-'))).resolve())
