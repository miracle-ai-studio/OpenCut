#!/usr/bin/env python3
"""Verify varied element/track gain on the retained long V4 source.

Derive independent analytic gain references from the already qualified whole
source PCM. Import no media, decode no new oracle, and keep original source and
render requests. A running owned service executes the one new required range.
"""
import argparse
from array import array
import json
import math
from pathlib import Path
import subprocess
import time

from acceptance import Harness, at, sha

ELEMENT = [(0, -12), (1, -3), (2.4, -9), (3.2, -6)]
TRACK = [(0, -6), (3, 0), (5.2, -3)]
RATE, START, FRAMES = 48000, 201600, 48000


def db_at(knots, seconds):
    if seconds <= knots[0][0]:
        return knots[0][1]
    for (left, a), (right, b) in zip(knots, knots[1:]):
        if seconds <= right:
            return a + (b - a) * (seconds - left) / (right - left)
    return knots[-1][1]


def amplitude(knots, seconds):
    # The frozen Classic animation timebase rounds positive half ticks up.
    seconds = math.floor(seconds * 120000 + .5) / 120000
    return 10 ** (db_at(knots, seconds) / 20)


def gain(knots, sample, realtime):
    seconds = sample / RATE
    if not realtime:
        return amplitude(knots, seconds)
    # Independently specified original60Hz amplitude automation. The generation
    # starts at scene0, hence both the element-local and sequence anchors are0.
    ordinal = math.floor(seconds * 60)
    left, right = ordinal / 60, min((ordinal + 1) / 60, knots[-1][0])
    a = amplitude(knots, left)
    return a if right <= left else a + (amplitude(knots, right) - a) * (seconds - left) / (right - left)


def channel(owner, identifier, knots):
    return {'kind': 'scalar', 'id': identifier, 'target': owner,
        'binding': {'kind': 'property', 'parameter': 'gain_decibels'},
        'keys': [{'id': identifier + '-' + str(index), 'time': at(seconds), 'value': db,
                  'left_handle': None, 'right_handle': None, 'segment_to_next': {'kind': 'linear'},
                  'tangent_mode': 'auto'} for index, (seconds, db) in enumerate(knots)],
        'extrapolation': {'before': 'hold', 'after': 'hold'}}


def verify(args):
    h = Harness(args.binary, args.workspace, 'agent-long-gain-v4')
    h.runtime = json.loads(args.native_runtime.read_text())
    source = json.loads(args.source_report.read_text())
    legacy = json.loads(args.reference_report.read_text())
    original_pcm = Path(legacy['pcm_path']).read_bytes()
    assert sha(original_pcm) == legacy['pcm_sha256']
    assert legacy['source_sha256'] == source['source']['sha256']
    assert legacy['sample_rate'] == RATE and legacy['frames'] == FRAMES and legacy['channels'] == 2
    source_before = h.inspect(source['project_id'])
    project = 'agent-long-varied-gain-copy'
    copied = h.stage('copy', lambda: h.request('copy', 'opencut_project_copy', {
        'source_project_id': source['project_id'], 'source_revision': source['revision'],
        'project_id': project, 'name': 'Same late VFR source with independently timed element and track envelopes',
        'idempotency_key': h.key('copy')}))
    initial = h.inspect(project, copied['revision'])
    scene = next(s for s in initial['project']['sequences'] if s['id'] == 'long-scene')
    track = next(t for t in scene['tracks'] if any(e['id'] == 'long-right' for e in t['elements']))
    channels = [channel({'kind': 'element', 'element_id': 'long-right'}, 'long-varied-element-gain', ELEMENT),
                channel({'kind': 'track', 'track_id': track['id']}, 'long-varied-track-gain', TRACK)]

    def configure():
        planned = h.request('envelopes-plan', 'opencut_keyframe_plan', {
            'project_id': project, 'expected_revision': copied['revision'], 'sequence_id': 'long-scene',
            'idempotency_key': h.key('envelopes'), 'label': 'Element-local and scene-clock gain with distinct slopes and corners',
            'edits': [{'kind': 'channel_put', 'owner': c['target'], 'channel': c} for c in channels]})
        assert h.inspect(project)['revision'] == copied['revision']
        return h.apply('envelopes', planned)

    configured = h.stage('envelopes', configure)
    current = h.inspect(project)
    assert current['revision'] == configured['revision']
    final_scene = next(s for s in current['project']['sequences'] if s['id'] == 'long-scene')
    final_track = next(t for t in final_scene['tracks'] if t['id'] == track['id'])
    right = next(e for e in final_track['elements'] if e['id'] == 'long-right')
    assert channels[0] in right['channels'] and channels[1] in final_track['channels']
    assert right['source_span'] == next(e for e in track['elements'] if e['id'] == 'long-right')['source_span']
    assert not right['content']['retime']['maintain_pitch']
    unit = array('f'); unit.frombytes(original_pcm)
    expected, profile_reports, isolated = {}, {}, {}
    for realtime, mode in [(False, 'export'), (True, 'preview')]:
        values, element_only, track_only = array('f'), array('f'), array('f')
        for i in range(FRAMES):
            e, t = gain(ELEMENT, 105600 + i, realtime), gain(TRACK, START + i, realtime)
            for c in range(2):
                v = unit[2 * i + c]
                values.append(v * e * t); element_only.append(v * e); track_only.append(v * t)
        pcm = values.tobytes()
        file = h.workspace / ('agent-long-varied-gain-' + mode + '-reference.f32')
        if file.exists():
            assert file.read_bytes() == pcm, 'Retain the exact original analytic reference'
        else:
            file.write_bytes(pcm)
        reference = {**legacy, 'pcm_path': str(file), 'pcm_sha256': sha(pcm),
            'parent_reference_report': str(args.reference_report.resolve()), 'pitch_mode': 'prepared-linear',
            'gain_reference': {'element_local_seconds_db': ELEMENT, 'track_scene_seconds_db': TRACK,
                'profile': 'positive120k-tick,60Hz-amplitude-ramp' if realtime else 'positive120k-tick,per-sample-db',
                'output_start_sample': START, 'right_element_start_sample': 96000, 'generation_anchor_sample': 0},
            'scope': 'Analytic gain applied to existing whole-source PCM; no product clock, application compiler, window decoder or new media decode called'}
        path = args.report.with_name('long-varied-gain-' + mode + '-reference.json')
        path.write_text(json.dumps(reference, indent=2) + '\n')
        expected[mode], profile_reports[mode], isolated[mode] = values, path, (element_only, track_only)
    preview = {**source, 'project_id': project, 'revision': configured['revision'],
        'legacy_pcm': {'reference_report': str(profile_reports['preview'])}, 'expected_preview_mode': 'prepared-linear',
        'minimum_observed_time': 4.88, 'pcm_windows': [
            {'id': 'preserve-pts', 'start_sample': 24000, 'frames': 4096},
            {'id': 'prepared-linear', 'start_sample': START, 'frames': 4096, 'reference_offset': 0},
            {'id': 'prepared-linear-gain-corner', 'start_sample': 210240, 'frames': 4096, 'reference_offset': 8640},
            {'id': 'prepared-linear-late-gain', 'start_sample': 230400, 'frames': 4096, 'reference_offset': 28800}],
        'scope': 'Varied element and track dB envelopes on original source, including local2.4s gain corner; original source/color reference reused'}
    args.report.with_name('long-varied-gain-browser-reference.json').write_text(json.dumps(preview, indent=2) + '\n')
    episode = h.candidate[:12] + '-' + sha(json.dumps(h.runtime, sort_keys=True).encode())[:12]
    name = 'audio-range-' + episode

    def render():
        status = h.cli(['server', '--directory', str(h.workspace / 'service'), 'status'])
        assert status['state'] == 'running' and status['ready']
        assert sha(Path(status['executable']).read_bytes()) == h.candidate
        accepted = h.request(name, 'opencut_render_submit', {'kind': 'audio_range', 'request': {
            'project_id': project, 'revision': configured['revision'], 'sequence_id': 'long-scene',
            'idempotency_key': h.key(name), 'sample_rate': RATE,
            'range': {'start_sample': str(START), 'frame_count': str(FRAMES)}}})
        deadline = time.monotonic() + 300
        while True:
            view = h.call('opencut_job_view', {'job_id': accepted['job_id']})
            if view['state'] in ('succeeded', 'failed', 'cancelled'):
                break
            assert time.monotonic() < deadline, {'observation_timeout': accepted['job_id'], 'state': view['state']}
            time.sleep(.5)
        assert view['state'] == 'succeeded' and view['result_unavailable'] is None, view
        assert h.call('opencut_render_submit', h.state['requests'][name]['input'])['job_id'] == view['job_id']
        return view

    job = h.stage(name, render)
    actual_pcm = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(h.artifact(job)),
        '-f', 'f32le', '-ac', '2', '-ar', str(RATE), 'pipe:1'], timeout=30)
    actual = array('f'); actual.frombytes(actual_pcm)
    assert len(actual) == FRAMES * 2
    maximum = max(abs(a-b) for a,b in zip(actual, expected['export']))
    assert maximum <= 1e-5, maximum
    controls = {'no_envelopes': unit, 'element_only': isolated['export'][0], 'track_only': isolated['export'][1]}
    negative = {k: max(abs(a-b) for a,b in zip(actual,v)) for k,v in controls.items()}
    assert min(negative.values()) > 1e-3, negative
    assert h.inspect(source['project_id']) == source_before
    cross_profile = max(abs(a-b) for a,b in zip(expected['preview'],expected['export']))
    report = {'status': 'passed_scoped', 'binary_sha256': h.candidate, 'native_runtime': h.runtime,
        'workspace': str(h.workspace), 'project_id': project, 'revision': configured['revision'],
        'source_project_id': source['project_id'], 'source_revision': source['revision'], 'source_sha256': legacy['source_sha256'],
        'job_id': job['job_id'], 'output_sha256': job['artifacts'][0]['content_hash'],
        'frames': FRAMES, 'sample_rate': RATE, 'channels': 2, 'max_absolute_error': maximum, 'threshold': 1e-5,
        'negative_control_max_errors': negative, 'analytic_preview_export_profile_max_difference': cross_profile,
        'reference_reports': {k: str(v) for k,v in profile_reports.items()}, 'execution': job['extended_result'],
        'scope': 'Actual original-policy one-second48k stereo output from original300s VFR source with independently timed element/track gain and a gain corner. Whole-scene mastering before crop; analytic source/gain references reused. Realtime is separately verified; not post-master/device or full format/browser matrix.'}
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['status', 'project_id', 'revision', 'job_id', 'max_absolute_error', 'negative_control_max_errors', 'analytic_preview_export_profile_max_difference']}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'source-report', 'reference-report', 'native-runtime', 'report']:
        parser.add_argument('--' + name, type=Path, required=True)
    verify(parser.parse_args())
