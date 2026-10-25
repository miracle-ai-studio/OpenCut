#!/usr/bin/env python3
"""V4 scenario7: an accepted task outlives actual Web/MCP clients.

One new audio-range job is necessary to observe an in-flight disconnect. Reuse
the original300s mono source, fixed document and independent PCM; no import,
ASR, video encode, worker kill/restart or duplicate successful task.
"""
import argparse
from array import array
import json
from pathlib import Path
import sqlite3
import subprocess
import time
from acceptance import Harness, sha


def verify(a):
    h = Harness(a.binary, a.workspace, 'agent-client-detach-v4')
    source = json.loads(a.source_report.read_text()); reference = json.loads(Path(source['reference_report']).read_text())
    expected = Path(reference['pcm_path']).read_bytes(); assert sha(expected) == reference['pcm_sha256']
    assert reference['frames'] == 48000 and reference['sample_rate'] == 48000 and reference['channels'] == 2
    original = h.stage('original-source', lambda: h.inspect(source['project_id']))
    fixed = h.inspect(source['project_id'], source['revision'])
    status = h.cli(['server', '--directory', str(h.workspace / 'service'), 'status'])
    assert status['ready'] and sha(Path(status['executable']).read_bytes()) == h.candidate
    request = {'tool': 'opencut_render_submit', 'input': {'kind': 'audio_range', 'request': {
        'project_id': source['project_id'], 'revision': source['revision'], 'sequence_id': 'long-scene',
        'idempotency_key': h.key('one-inflight-range'), 'sample_rate': 48000,
        'range': {'start_sample': '201600', 'frame_count': '48000'}}}}
    request_file = a.report.with_name('client-detach-request.json'); request_file.write_text(json.dumps(request, indent=2) + '\n')
    assert h.state['requests'].setdefault('one-inflight-range', request) == request; h.save()
    progress_file = h.workspace / 'agent-client-detach-v4-progress.json'
    before = h.stage('before-counts', lambda: counts(h))
    assert before['active_jobs'] == 0, 'One owned stateful acceptance at a time'
    def detach():
        subprocess.run(['node', 'scripts/verify-agent-client-detach.mjs', '--binary', str(h.binary), '--workspace', str(h.workspace),
            '--endpoint', status['endpoint'], '--request', str(request_file), '--progress', str(progress_file), '--ui-project', a.ui_project],
            cwd=Path(__file__).resolve().parents[1] / 'apps/web', check=True, timeout=120)
        result = json.loads(progress_file.read_text())
        assert result['request'] == request and result['before_close']['state'] in ('queued', 'running')
        assert result['clients_closed']['web_page_closed'] and not result['clients_closed']['browser_connected']
        assert result['clients_closed']['mcp_exit'] == {'code': 0, 'signal': None}
        return result
    detached = h.stage('actual-client-detachment', detach)
    accepted = h.call(request['tool'], request['input']); assert accepted['job_id'] == detached['accepted']['job_id']
    job_id = accepted['job_id']
    def completed():
        deadline = time.monotonic() + 180
        while True:
            view = h.call('opencut_job_view', {'job_id': job_id})
            if view['state'] in ('succeeded', 'failed', 'cancelled'):
                assert view['state'] == 'succeeded' and view['result_unavailable'] is None, view
                return view
            assert time.monotonic() < deadline, {'observation_timeout': job_id, 'note': 'Keep original task authoritative; do not restart or resubmit'}
            time.sleep(.5)
    output = h.stage('same-job-completed', completed)
    path = h.artifact(output)
    actual = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path), '-f', 'f32le', '-ac', '2', '-ar', '48000', 'pipe:1'])
    pcm = array('f'); pcm.frombytes(actual); oracle = array('f'); oracle.frombytes(expected)
    assert len(pcm) == len(oracle) == 96000
    maximum = max(abs(x - y) for x, y in zip(pcm, oracle)); assert maximum <= reference['threshold'], maximum
    current = h.cli(['server', '--directory', str(h.workspace / 'service'), 'status'])
    assert current['ready'] and current['instance'] == status['instance'] and current['pid'] == status['pid']
    events = h.call('opencut_job_events', {'job_id': job_id, 'after_sequence': 0, 'limit': 500})
    kinds = [event['event_type'] for event in events]
    assert kinds.count('running') == 1 and kinds.count('succeeded') == 1 and 'recovered' not in kinds
    assert h.inspect(source['project_id']) == original and h.inspect(source['project_id'], source['revision']) == fixed
    after = counts(h)
    assert after['jobs'] == before['jobs'] + 1 and after['projects'] == before['projects'] and after['revisions'] == before['revisions']
    result = {'status': 'passed_scoped', 'binary_sha256': h.candidate, 'workspace': str(h.workspace), 'project_id': source['project_id'],
        'revision': source['revision'], 'job_id': job_id, 'detachment': detached, 'same_http_instance_pid': True, 'events': kinds,
        'reference_report': source['reference_report'], 'pcm_max_absolute_error': maximum, 'pcm_threshold': reference['threshold'],
        'new_jobs': 1, 'new_media_imports': 0, 'new_project_or_revision': 0,
        'scope': 'V4 design-v1 scenario7 on one actual audio-range task: real connected Web closes and MCP exits0 while task nonterminal, CLI observes same task through success; service and original fixed source unchanged. Uses a different readonly UI view, not its own task panel. No browser close during playback, network loss/host crash/worker recovery/multi-host or complete format matrix claim.'}
    a.report.write_text(json.dumps(result, indent=2) + '\n'); print(json.dumps({k: result[k] for k in ['status', 'job_id', 'pcm_max_absolute_error']}))


def counts(h):
    with sqlite3.connect('file:' + str(h.workspace / 'workspace.sqlite') + '?mode=ro', uri=True) as c:
        result = {t: c.execute('SELECT count(*) FROM ' + t).fetchone()[0] for t in ['jobs', 'projects', 'revisions']}
        result['active_jobs'] = c.execute("SELECT count(*) FROM jobs WHERE state IN ('queued','running')").fetchone()[0]
        return result


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'source-report', 'report']: p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--ui-project', required=True)
    verify(p.parse_args())
