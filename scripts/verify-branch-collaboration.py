#!/usr/bin/env python3
"""One retained-workspace branch/history/hold contract;no media jobs or fixture copies."""
import argparse
import copy
import json
from pathlib import Path
import sqlite3
from acceptance import Harness


def verify(args):
    h = Harness(args.binary, args.workspace, args.name)
    scope = {'candidate': h.candidate, 'project_id': args.project_id,
             'credential_file': str(args.credential.resolve())}
    assert h.state.setdefault('scope', scope) == scope, 'Keep the original candidate/project/actor checkpoint'
    project = args.project_id
    prefix = h.key('graph')
    branch = 'acceptance-' + h.candidate[:12]
    actor = args.credential.resolve()
    # Fail before changing owned data if the caller cannot observe this project.
    h.call('opencut_project_inspect', {'project_id': project, 'revision': None}, credential=actor)
    if 'baseline' not in h.state:
        h.state['baseline'] = h.inspect(project)
        h.save()
    baseline = h.state['baseline']
    scene_id = baseline['project']['default_sequence_id']

    def request(revision, name, operations, named=None):
        value = {'project_id': project, 'expected_revision': revision,
                 'idempotency_key': h.key(name), 'label': name, 'operations': operations}
        if named:
            value['branch_name'] = named
        return value

    def element_ids(snapshot):
        return {e['id'] for s in snapshot['project']['sequences']
                for t in s['tracks'] for e in t['elements']}

    def graphic(revision, name, named=None):
        original = {'project_id': project, 'expected_revision': revision, 'sequence_id': scene_id,
                    'idempotency_key': h.key(name), 'command': {'kind': 'defaults', 'geometry': 'star',
                    'start': {'kind': 'exact', 'time': {'ticks': '0', 'timebase': 1}}}}
        initial = h.request(name + '-defaults-plan', 'opencut_graphic_plan', original)['initial']
        values = {'name': name, 'graphic': initial['graphic'], 'transform': initial['transform'],
                  'opacity': initial['opacity'], 'start': {'kind': 'exact', 'time': initial['timeline_range']['start']},
                  'duration': {'kind': 'exact', 'time': initial['timeline_range']['duration']}}
        original['command'] = {'kind': 'create', 'element_id': prefix + ':' + name,
                               'track': {'kind': 'new', 'track_id': prefix + ':track:' + name}, 'values': values}
        planned = h.request(name + '-plan', 'opencut_graphic_plan', original)['transaction']
        planned = copy.deepcopy(planned)
        if named:
            planned['branch_name'] = named
        return h.apply(name, planned, credential=actor)

    def scene_rename(revision, name, named=None):
        sequence = next(s for s in h.inspect(project, revision)['project']['sequences'] if s['id'] == scene_id)
        sequence['name'] = name
        return request(revision, name, [{'type': 'sequence_put', 'sequence': sequence}], named)

    def collaboration():
        first = graphic(baseline['revision'], 'first')
        later = graphic(first['revision'], 'later')
        undo_input = {'project_id': project, 'expected_revision': later['revision'],
                      'target_transaction_id': first['transaction_id'], 'idempotency_key': h.key('undo-first')}
        plan = h.request('undo-first-plan', 'opencut_edit_undo_plan', undo_input, credential=actor)
        assert not plan['conflicts']
        undone = h.request('undo-first', 'opencut_edit_undo_apply', undo_input, credential=actor)
        assert h.call('opencut_edit_undo_apply', undo_input, credential=actor) == undone
        fixed = h.inspect(project, undone['transaction']['revision'])
        assert prefix + ':first' not in element_ids(fixed) and prefix + ':later' in element_ids(fixed)
        redo_input = {'project_id': project, 'expected_revision': fixed['revision'],
                      'undo_transaction_id': undone['transaction']['transaction_id'], 'idempotency_key': h.key('redo-first')}
        redone = h.request('redo-first', 'opencut_edit_redo_apply', redo_input, credential=actor)
        base = redone['transaction']['revision']
        creating = {'project_id': project, 'branch_name': branch, 'from_revision': base,
                    'expected_revision': base, 'idempotency_key': h.key('branch-create')}
        reference = h.request('branch-create', 'opencut_edit_branch_create', creating, credential=actor)
        assert h.call('opencut_edit_branch_create', creating, credential=actor) == reference
        h.call('opencut_edit_branch_create', {**creating, 'branch_name': branch + '-other'},
               credential=actor, error='IDEMPOTENCY_KEY_REUSED')
        if 'duplicate_ref_proof' not in h.state:
            h.call('opencut_edit_branch_create', {**creating, 'idempotency_key': h.key('duplicate-ref')},
                   credential=actor, error='ALREADY_EXISTS')
            h.state['duplicate_ref_proof'] = True
            h.save()
        branched = graphic(base, 'branch-star', branch)
        wrong_scope = copy.deepcopy(h.state['requests']['branch-star']['input'])
        wrong_scope['branch_name'] = branch + '-missing'
        h.call('opencut_edit_apply', wrong_scope, credential=actor, error='IDEMPOTENCY_KEY_REUSED')
        if 'named_edit_main_proof' not in h.state:
            assert h.inspect(project)['revision'] == base
            h.state['named_edit_main_proof'] = {'main_revision': base, 'branch_revision': branched['revision']}
            h.save()
        owner_input = scene_rename(base, 'Independent owner scene')
        owner = h.apply('owner-scene', owner_input)
        comparing = {'project_id': project, 'source_branch': branch, 'target_branch': 'main',
                     'expected_source_revision': branched['revision'], 'expected_target_revision': owner['revision']}
        plan = h.request('merge-plan', 'opencut_edit_branch_compare', comparing, credential=actor)
        assert not plan['conflicts'] and plan['proposed_project']
        h.call('opencut_edit_branch_merge', {**comparing, 'expected_target_revision': base,
               'idempotency_key': h.key('stale-merge')}, credential=actor, error='REVISION_CONFLICT')
        merging = {**comparing, 'idempotency_key': h.key('merge')}
        merged = h.request('merge', 'opencut_edit_branch_merge', merging, credential=actor)
        assert h.call('opencut_edit_branch_merge', merging, credential=actor) == merged
        saved = h.inspect(project, merged['transaction']['revision'])
        assert {prefix + ':first', prefix + ':later', prefix + ':branch-star'} <= element_ids(saved)
        assert next(s for s in saved['project']['sequences'] if s['id'] == scene_id)['name'] == 'Independent owner scene'
        if 'merge_source_ref_proof' not in h.state:
            refs = h.call('opencut_edit_branch_list', {'project_id': project}, credential=actor)
            assert next(r for r in refs if r['branch_name'] == branch)['head_revision'] == branched['revision']
            h.state['merge_source_ref_proof'] = {'source_revision': branched['revision'], 'merge_revision': saved['revision']}
            h.save()
        with sqlite3.connect('file:' + str(h.workspace / 'workspace.sqlite') + '?mode=ro', uri=True) as db:
            row = db.execute('SELECT source_revision FROM project_merge_parents WHERE project_id=? AND revision=?',
                             (project, merged['transaction']['revision'])).fetchone()
        assert row == (branched['revision'],)
        # Preserve the original DAG-selective-undo case,not just undo of the
        # whole merge. The independent owner change must survive this inversion.
        undo_branch = {'project_id': project, 'expected_revision': saved['revision'],
                       'target_transaction_id': branched['transaction_id'], 'idempotency_key': h.key('undo-merged-source')}
        removed = h.request('undo-merged-source', 'opencut_edit_undo_apply', undo_branch, credential=actor)
        fixed = h.inspect(project, removed['transaction']['revision'])
        assert prefix + ':branch-star' not in element_ids(fixed) and prefix + ':later' in element_ids(fixed)
        assert next(s for s in fixed['project']['sequences'] if s['id'] == scene_id)['name'] == 'Independent owner scene'
        redo_branch = {'project_id': project, 'expected_revision': fixed['revision'],
                       'undo_transaction_id': removed['transaction']['transaction_id'], 'idempotency_key': h.key('redo-merged-source')}
        restored = h.request('redo-merged-source', 'opencut_edit_redo_apply', redo_branch, credential=actor)
        assert prefix + ':branch-star' in element_ids(h.inspect(project, restored['transaction']['revision']))
        divergent = h.apply('source-divergence', scene_rename(branched['revision'], 'Divergent source scene', branch), credential=actor)
        conflict = {**comparing, 'expected_source_revision': divergent['revision'],
                    'expected_target_revision': restored['transaction']['revision']}
        rejected = h.request('conflict-plan', 'opencut_edit_branch_compare', conflict, credential=actor)
        assert rejected['conflicts'] and rejected['proposed_project'] is None
        h.call('opencut_edit_branch_merge', {**conflict, 'idempotency_key': h.key('conflicting-merge')},
               credential=actor, error='CAUSAL_CONFLICT')
        assert h.inspect(project)['revision'] == restored['transaction']['revision']
        return {'revision': restored['transaction']['revision'], 'branch': branch,
                'source_revision': divergent['revision'], 'merge_revision': merged['transaction']['revision'],
                'checks': ['independent selective undo/redo', 'named edit leaves main fixed', 'fixed-head compare and CAS',
                           'exact create/merge replay', 'both merge parents', 'DAG source undo/redo preserves owner work',
                           'divergence rejects without publication']}

    flow = h.stage('collaboration', collaboration)

    def hold_contract():
        control = h.call('opencut_edit_control_get', {'project_id': project})
        proof = h.state.get('hold_proof')
        pending = h.state.get('pending_hold')
        if proof is None:
            if control['held']:
                assert pending, 'An unrelated owner hold is preserved'
                holding = pending
            else:
                holding = {'project_id': project, 'expected_generation': str(control['generation']),
                           'held': True, 'idempotency_key': h.key('hold-' + str(control['generation']))}
                h.state['pending_hold'] = holding
                h.save()
            held = h.request('hold-' + holding['expected_generation'], 'opencut_edit_control_set', holding)
            assert held['held']
            try:
                proposed = request(flow['revision'], 'held-edit', [{'type': 'rename', 'name': 'Read-only proposal'}])
                assert h.call('opencut_edit_plan', proposed, credential=actor)['id'] == project
                h.call('opencut_edit_apply', proposed, credential=actor, error='EDIT_HELD_BY_OWNER')
                h.call('opencut_edit_branch_create', {'project_id': project, 'branch_name': branch + '-held',
                       'from_revision': flow['revision'], 'expected_revision': flow['revision'],
                       'idempotency_key': h.key('held-create')}, credential=actor, error='EDIT_HELD_BY_OWNER')
                h.state['hold_proof'] = {'read_only_plan': True, 'edit_and_ref_writes_blocked': True}
                h.save()
            finally:
                releasing = {'project_id': project, 'expected_generation': str(held['generation']),
                             'held': False, 'idempotency_key': h.key('release-' + str(held['generation']))}
                h.request('release-' + str(held['generation']), 'opencut_edit_control_set', releasing)
        else:
            # An interrupted observer may have saved proof before releasing its
            # own hold. Retry only that original control action;never clear a
            # different generation or an unrelated human hold.
            if control['held']:
                assert pending
                held = h.request('hold-' + pending['expected_generation'], 'opencut_edit_control_set', pending)
                assert str(control['generation']) == str(held['generation'])
                releasing = {'project_id': project, 'expected_generation': str(held['generation']),
                             'held': False, 'idempotency_key': h.key('release-' + str(held['generation']))}
                h.request('release-' + str(held['generation']), 'opencut_edit_control_set', releasing)
        assert not h.call('opencut_edit_control_get', {'project_id': project})['held']
        return h.state['hold_proof']

    h.stage('owner-hold', hold_contract)

    def restore_baseline():
        restoring = {'project_id': project, 'expected_revision': flow['revision'],
                     'target_revision': baseline['revision'], 'asset_versions': {}, 'idempotency_key': h.key('restore-baseline')}
        receipt = h.request('restore-baseline', 'opencut_project_restore', restoring)
        assert h.call('opencut_project_restore', restoring) == receipt
        assert h.inspect(project)['project'] == baseline['project']
        return receipt

    h.stage('restore-baseline', restore_baseline)
    report = {'status': 'passed', 'candidate': h.candidate, 'project_id': project, 'checks': flow['checks'],
              'checkpoint': str(h.checkpoint), 'media_jobs': 0, 'new_projects': 0,
              'limits': ['No renderer/pixel/PCM or large-media acceptance claim.',
                         'Original historical promotion/reader behavior remains covered by existing native storage fixtures.',
                         'Named refs and immutable transaction history are retained;main content is restored.']}
    (h.workspace / (args.name + '-report.json')).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--project-id', required=True, help='Existing owned synthetic editor project')
    parser.add_argument('--credential', type=Path, required=True, help='Existing scoped actor with read/edit/restore')
    parser.add_argument('--name', default='branch-collaboration')
    verify(parser.parse_args())
