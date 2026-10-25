#!/usr/bin/env python3
"""Canonical preset journey in one retained owned workspace;no fixture or package copies.

--requests is a JSON array of one template request and optionally one style
request,using already installed/granted packages,media,fonts and caption tracks.
Revision may be omitted;it is captured from the original baseline/prior receipt.
The optional original red/yellow oracle creates at most one retained output job.
"""
import argparse
import copy
import json
from pathlib import Path
import sqlite3
import subprocess
from acceptance import Harness, sha


def job_rows(workspace):
    with sqlite3.connect(f"file:{workspace / 'workspace.sqlite'}?mode=ro", uri=True) as db:
        return {row[0]: sha(repr(row).encode()) for row in db.execute('SELECT * FROM jobs ORDER BY job_id')}


def red_yellow(output):
    # Original independent preset oracle,retained without changing its thresholds.
    rgb = subprocess.check_output(['ffmpeg', '-v', 'error', '-ss', '0.5', '-i', str(output),
        '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=30)
    assert len(rgb) == 320 * 180 * 3
    assert sum(rgb[0::3]) / (320 * 180) > 180
    yellow = sum(rgb[i] > 150 and rgb[i+1] > 150 and rgb[i+2] < 100
        for i in range(0, len(rgb), 3))
    assert yellow > 20, yellow
    return {'yellow_pixels': yellow, 'output_sha256': sha(output.read_bytes())}


def verify(args):
    h = Harness(args.binary, args.workspace, args.name)
    recipes = json.loads(args.requests.read_text())
    assert isinstance(recipes, list) and 1 <= len(recipes) <= 2
    actor = args.credential.resolve()
    scope = {'candidate': h.candidate, 'project_id': args.project_id, 'recipes': recipes,
             'credential_file': str(actor), 'reader_file': str(args.reader.resolve()) if args.reader else None,
             'red_yellow_oracle': args.red_yellow_oracle}
    assert h.state.setdefault('scope', scope) == scope, 'Keep the original candidate,requests,project and actors'
    view = h.call('opencut_project_inspect', {'project_id': args.project_id, 'revision': None}, credential=actor)
    assert view['kind'] == 'editor'
    if 'baseline' not in h.state:
        h.state['baseline'] = copy.deepcopy(view['snapshot'])
        h.state['original_jobs'] = job_rows(h.workspace)
        h.save()
    baseline = h.state['baseline']

    def journey():
        revision = baseline['revision']
        receipts = []
        for index, recipe in enumerate(recipes):
            request = copy.deepcopy(recipe)
            assert request['target']['project_id'] == args.project_id
            assert not request.get('branch_name'), 'This retained journey owns main only;branch publication has its own evidence'
            assert request.get('revision', revision) == revision
            request['revision'] = revision
            prefix = f'preset-{index}'
            if args.reader:
                h.call('opencut_plugin_preset_plan', request, credential=args.reader, error='NOT_AUTHORIZED')
            plan = h.request(prefix + '-plan', 'opencut_plugin_preset_plan', request, credential=actor)
            transaction = plan['transaction']
            assert transaction['project_id'] == args.project_id
            assert transaction['expected_revision'] == revision
            assert transaction['idempotency_key'] == request['idempotency_key']
            assert transaction['label'] == request['label']
            assert transaction['operations'] and plan['package_sha256'] and plan['spec_sha256']
            assert plan['preview']['assets'] == baseline['project']['assets']
            assert plan['preview']['fonts'] == baseline['project']['fonts']
            receipt = h.apply(prefix + '-apply', transaction, credential=actor)
            fixed = h.inspect(args.project_id, receipt['revision'])
            assert fixed['project'] == plan['preview']
            assert fixed['project']['assets'] == baseline['project']['assets']
            assert fixed['project']['fonts'] == baseline['project']['fonts']
            if prefix + '-moved-head-proof' not in h.state:
                h.call('opencut_plugin_preset_plan', request, credential=actor, error='REVISION_CONFLICT')
                h.state[prefix + '-moved-head-proof'] = True
                h.save()
            revision = receipt['revision']
            receipts.append({'transaction': receipt, 'package_sha256': plan['package_sha256'],
                             'spec_sha256': plan['spec_sha256'], 'normalizations': plan['normalizations']})
        return {'revision': revision, 'receipts': receipts}

    flow = h.stage('preset-journey', journey)
    rendered = None
    if args.red_yellow_oracle:
        def output():
            fixed = h.inspect(args.project_id, flow['revision'])
            settings = fixed['project']['settings']
            assert settings['profile'] == 'classic_global'
            assert settings['settings']['canvas_size'] == {'width': 320, 'height': 180}
            job = h.job('preset-output', 'opencut_render_submit', {'kind': 'video', 'request': {
                'project_id': args.project_id, 'revision': flow['revision'],
                'sequence_id': fixed['project']['default_sequence_id'], 'idempotency_key': h.key('preset-output'),
                'options': {'format': 'mp4', 'quality': 'high', 'include_audio': False}}})
            return {'job_id': job['job_id'], 'producer': job, **red_yellow(h.artifact(job))}
        rendered = h.stage('preset-output', output)

    def restore():
        original = {'project_id': args.project_id, 'expected_revision': flow['revision'],
                    'target_revision': baseline['revision'], 'idempotency_key': h.key('restore-baseline')}
        receipt = h.request('restore-baseline', 'opencut_project_restore', original, credential=actor)
        assert h.call('opencut_project_restore', original, credential=actor) == receipt
        assert h.inspect(args.project_id, receipt['revision'])['project'] == baseline['project']
        assert h.inspect(args.project_id)['project'] == baseline['project']
        return receipt

    h.stage('restore-baseline', restore)
    jobs = job_rows(h.workspace)
    assert all(jobs.get(key) == value for key, value in h.state['original_jobs'].items())
    assert set(jobs) - set(h.state['original_jobs']) == ({rendered['job_id']} if rendered else set())
    report = {'status': 'passed', 'candidate': h.candidate, 'project_id': args.project_id,
              'checkpoint': str(h.checkpoint), 'new_projects': 0, 'package_or_media_copies': 0,
              'receipts': flow['receipts'], 'output': rendered,
              'limits': ['Canonical plan/apply/replay,fixed-head conflict,resource preservation and baseline restoration.',
                         'No package installation,grant mutation,tampering or runtime budget checks repeated.',
                         'Pixel evidence exists only when the explicit original red/yellow output oracle ran;no broad renderer/large-media claim.']}
    (h.workspace / (args.name + '-report.json')).write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(report, ensure_ascii=False))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--project-id', required=True, help='Existing owned synthetic editor project')
    parser.add_argument('--requests', type=Path, required=True)
    parser.add_argument('--credential', type=Path, required=True, help='Existing actor with read/edit/run_jobs/restore')
    parser.add_argument('--reader', type=Path, help='Existing read-only actor for authorization rejection')
    parser.add_argument('--red-yellow-oracle', action='store_true', help='Only for the original 320x180 red/yellow fixture;one retained output')
    parser.add_argument('--name', default='preset-planning')
    verify(parser.parse_args())
