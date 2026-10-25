#!/usr/bin/env python3
"""Finish the actual retained sound UI slice through Agent export and independent PCM."""
import argparse
from array import array
import importlib.util
import json
import math
from fractions import Fraction
from pathlib import Path
import subprocess
import time
import wave
from acceptance import Harness, sha


def verify(a):
    h = Harness(a.binary, a.workspace, 'agent-sound-catalog-export-v4')
    ui = json.loads(a.browser_report.read_text())
    assert ui['binary_sha256'] == h.candidate
    project = ui['project_id']; snapshot = h.inspect(project, ui['revision'])
    before = h.inspect(project); assert before == snapshot
    assets = snapshot['project']['assets']
    pinned = {s['name']: s for s in json.loads((Path(__file__).resolve().parents[1] / 'crates/editor-application/src/catalog_resources/sounds.json').read_text())}
    licensed = [s for s in assets if s['origin']['kind'] == 'online' and s['origin']['provider'] == 'kenney-ui-audio-v1']
    assert len(licensed) == 51 and {s['name'] for s in licensed} == set(pinned)
    def resource(asset):
        digest = asset['version']['content_hash']; path = h.workspace / 'asset-store' / digest[:2] / digest
        assert sha(path.read_bytes()) == digest
        return path
    for asset in licensed:
        assert asset['version']['content_hash'] == pinned[asset['name']]['sha256'] and asset['version']['size'] == pinned[asset['name']]['size']
        assert asset['license']['identifier'] == 'CC0-1.0'; resource(asset)
    lookup = {'project_id': project, 'revision': ui['revision'], 'asset_id': ui['generated']['asset']['id']}
    recording = h.call('opencut_catalog_resources_recipe', lookup)
    assert recording['recording']['recipe'] == ui['generated']['request']['recipe']
    assert recording['version'] == ui['generated']['asset']['version']
    for asset in assets:
        if asset['origin']['kind'] == 'generated' and asset['origin']['provider'] == 'opencut-local-sfx-v1':
            proof = h.call('opencut_catalog_resources_recipe', {**lookup, 'asset_id': asset['id']})
            assert asset['origin']['generation_id'] is None and proof['recording']['sample_format'] == 'pcm_s16le'
    def negatives():
        history_input = {'project_id': project, 'from_revision': None, 'limit': 100}
        history = h.call('opencut_project_history', history_input); assert history['next_cursor'] is None
        h.call('opencut_catalog_resources_generate', {'project_id': project, 'expected_revision': snapshot['revision'],
            'asset_id': 'must-not-publish', 'idempotency_key': h.key('over-limit'), 'recipe': {'kind': 'whoosh', 'duration_ms': 5001, 'seed': 18, 'level_db': -12}}, error='INVALID_ARGUMENT')
        h.call('opencut_catalog_resources_import', {'project_id': project, 'expected_revision': snapshot['revision'],
            'asset_id': 'must-not-import', 'idempotency_key': h.key('unknown-source'), 'catalog_id': 'kenney-ui-audio:unknown', 'uploaded': None}, error='INVALID_ARGUMENT')
        assert h.inspect(project) == snapshot and h.call('opencut_project_history', history_input) == history
        return {'over_limit_and_unknown_source_refused': True, 'project_and_complete_history_unchanged': True}
    negative = h.stage('negative-publish-fences', negatives)
    def reader():
        principal = 'agent-sound-catalog-reader'
        h.stage('reader-policy', lambda: h.cli(['access', 'set-policy', '--input', '-'], {'principal_id': principal, 'expected_revision': '0', 'enabled': True,
            'grants': [{'project_id': project, 'actions': ['read']}]}))
        credential = h.workspace / (principal + '.secret')
        h.stage('reader-credential', lambda: h.cli(['access', 'issue', principal, '--output', str(credential)]))
        assert h.call('opencut_catalog_resources_recipe', lookup, credential=credential) == recording
        h.call('opencut_catalog_resources_recipe', {**lookup, 'project_id': 'agent-real-interview-v4-acceptance'}, credential=credential, error='NOT_AUTHORIZED')
        return {'current_read_only_recipe_allowed': True, 'foreign_project_denied': True}
    access = h.stage('scoped-read', reader)
    status = h.cli(['server', '--directory', str(h.workspace / 'service'), 'status'])
    assert status['ready'] and sha(Path(status['executable']).read_bytes()) == h.candidate
    spec = importlib.util.spec_from_file_location('catalog_mcp', Path(__file__).with_name('verify-editorial.py'))
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    mcp = module.Mcp(h.binary, h.workspace / 'workspace.sqlite', a.report.parent,
        [str(h.binary), '--database', str(h.workspace / 'workspace.sqlite'), 'mcp', 'serve'])
    try:
        assert mcp.tool('opencut_media', {'action': 'catalog.resources.recipe', 'input': lookup}) == recording
        assert mcp.tool('opencut_media', {'action': 'catalog.resources.import', 'input': ui['imported']['request']}) == ui['imported']['receipt']
        assert mcp.tool('opencut_media', {'action': 'catalog.resources.generate', 'input': ui['generated']['request']}) == ui['generated']['receipt']
        for placement in ['first', 'second']:
            plan = ui['placement'][placement]['transaction']
            assert mcp.tool('opencut_edit', {'action': 'apply', 'input': plan}) == ui['placement'][placement]['receipt']
        assert h.inspect(project) == snapshot
        request = {'kind': 'video', 'request': {'project_id': project, 'revision': snapshot['revision'], 'sequence_id': 'sound-scene',
            'idempotency_key': h.key('one-sound-export'), 'options': {'format': 'mp4', 'quality': 'high', 'include_audio': True}}}
        assert h.state['requests'].setdefault('one-sound-export', {'tool': 'opencut_render_submit', 'input': request})['input'] == request; h.save()
        accepted = h.stage('export-accepted', lambda: mcp.tool('opencut_task', {'action': 'render.submit', 'input': request}))
    finally: mcp.close()
    assert h.call('opencut_render_submit', request)['job_id'] == accepted['job_id']
    def finished():
        deadline = time.monotonic() + 180
        while True:
            view = h.call('opencut_job_view', {'job_id': accepted['job_id']})
            if view['state'] in ('failed', 'cancelled', 'succeeded'):
                assert view['state'] == 'succeeded' and view['result_unavailable'] is None, view
                return view
            assert time.monotonic() < deadline, {'observation_timeout': accepted['job_id'], 'note': 'Retain the original job; do not create a replacement'}
            time.sleep(.5)
    job = h.stage('same-export-completed', finished)
    path = h.artifact(job)
    probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_streams', '-of', 'json', str(path)]))
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video'); audio = next(s for s in probe['streams'] if s['codec_type'] == 'audio')
    assert video['codec_name'] == 'h264' and (video['width'], video['height'], video['nb_frames']) == (320, 180, '30')
    assert audio['codec_name'] == 'aac' and audio['sample_rate'] == '48000' and audio['channels'] == 2
    decision = ui['placement']['second']['reviewed_plan']['decisions']['placements'][0]
    def exact_time(value): return Fraction(int(value['ticks']), value['timebase'])
    assert exact_time(decision['requested_start']) == Fraction(249, 1000)
    # The existing placement use case rounds to the nearest project frame.
    # Compute the expectation independently; do not use rendered PCM to find it.
    expected_start = Fraction(round(Fraction(249, 1000) * 30), 30)
    assert expected_start == Fraction(7, 30) and exact_time(decision['resolved_start']) == expected_start
    start = int(expected_start * 48000)
    expected_audio_frames = start + 751 * 48
    pcm = array('f'); pcm.frombytes(subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path.parent / 'video-audio.wav'), '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1']))
    assert len(pcm) == expected_audio_frames * 2, (len(pcm), expected_audio_frames)
    generated = ui['generated']['asset']; raw = resource(generated)
    with wave.open(str(raw), 'rb') as wave_source:
        assert (wave_source.getnchannels(), wave_source.getsampwidth(), wave_source.getframerate(), wave_source.getnframes()) == (1, 2, 48000, 36048)
        integer = array('h'); integer.frombytes(wave_source.readframes(36048))
    # Short standalone PCM stays on Chromium's existing decodeAudioData path.
    # Its signed16 conversion uses +32767/-32768, already recorded by the
    # renderer browser acceptance. Derive expected samples from source integers,
    # with the same fixed 1e-7 bound; never fit a scale to the rendered output.
    symmetric_normalization_error = max(abs(pcm[(start + i) * 2 + c] - sample / 32768) for i, sample in enumerate(integer) for c in [0, 1])
    generated_error = max(abs(pcm[(start + i) * 2 + c] - sample / (32767 if sample >= 0 else 32768)) for i, sample in enumerate(integer) for c in [0, 1])
    assert generated_error <= 1e-7, generated_error
    gap_peak = max(abs(v) for v in pcm[8000 * 2:start * 2]); assert gap_peak == 0
    assert max(abs(x) for x in integer) > 100 and integer[0] == integer[-1] == 0
    clip = ui['imported']['asset']; reference = array('f'); reference.frombytes(subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(resource(clip)), '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1']))
    length = min(len(reference) // 2, 3500); ref = reference[:length * 2]
    correlation = sum(pcm[i] * ref[i] for i in range(len(ref))) / math.sqrt(sum(pcm[i] ** 2 for i in range(len(ref))) * sum(x*x for x in ref))
    assert correlation > .98, correlation
    decoded_aac = array('f'); decoded_aac.frombytes(subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path), '-vn', '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1']))
    assert len(decoded_aac) >= len(pcm)
    product_energy = sum(x*x for x in pcm); encoded_energy = sum(decoded_aac[i]**2 for i in range(len(pcm)))
    aac_correlation = sum(decoded_aac[i]*pcm[i] for i in range(len(pcm))) / math.sqrt(product_energy * encoded_energy)
    aac_energy_ratio = encoded_energy / product_energy
    assert aac_correlation > .90 and .8 <= aac_energy_ratio <= 1.2, (aac_correlation, aac_energy_ratio)
    rgb = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path), '-an', '-f', 'rawvideo', '-pix_fmt', 'rgb24', 'pipe:1'])
    assert len(rgb) == 30 * 320 * 180 * 3 and max(rgb) <= 2
    sdk_input = a.report.with_name('catalog-sounds-sdk-input.json'); sdk_result = a.report.with_name('catalog-sounds-sdk-result.json')
    sdk_input.write_text(json.dumps({'lookup': lookup, 'artifact': job['artifacts'][0]}, indent=2) + '\n')
    subprocess.run(['node', str(Path(__file__).with_name('verify-catalog-sounds-sdk.mjs')), status['endpoint'], status['token_file'], str(sdk_input), str(sdk_result)], check=True, timeout=60)
    sdk = json.loads(sdk_result.read_text()); assert sdk['recipe'] == recording and sdk['downloaded_sha256'] == job['artifacts'][0]['content_hash']
    assert h.inspect(project) == snapshot
    result = {'status': 'passed_scoped', 'binary_sha256': h.candidate, 'project_id': project, 'revision': snapshot['revision'], 'job_id': job['job_id'],
        'licensed_sources': 51, 'generated_sources': len(assets) - 51, 'typed_recipe_cli_mcp_sdk_http_equal': True, 'recording': recording,
        'negative': negative, 'access': access, 'generated_pcm_max_absolute_error': generated_error, 'generated_pcm_threshold': 1e-7, 'pcm16_normalization': 'Chromium153 short decodeAudioData: positive32767/negative32768; direct authored source integers', 'symmetric32768_negative_control_error': symmetric_normalization_error, 'requested_start_seconds': '249/1000', 'resolved_start_seconds': str(expected_start), 'generated_start_sample': start,
        'catalog_clip_ffmpeg_correlation': correlation, 'catalog_clip_correlation_threshold': .98, 'silent_gap_peak': gap_peak, 'black_video_max_channel': max(rgb),
        'video_frames': 30, 'audio_sample_rate': 48000, 'unencoded_audio_frames': expected_audio_frames, 'aac_correlation': aac_correlation, 'aac_correlation_threshold': .90, 'aac_energy_ratio': aac_energy_ratio, 'aac_energy_ratio_bounds': [.8, 1.2], 'public_download': sdk, 'scope': 'F18 actual fixed catalog/source acquisition, generation/typed metadata, UI source audition/download, original-response recovery and public placement/Agent export; one1s H264/AAC output with independent authored WAV samples and fixed Ogg decoder reference. Provider404/429/timeout and full physical/broad format matrix remain open.'}
    a.report.write_text(json.dumps(result, indent=2) + '\n'); print(json.dumps({k: result[k] for k in ['status', 'job_id', 'generated_pcm_max_absolute_error', 'catalog_clip_ffmpeg_correlation']}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'workspace', 'browser-report', 'report']: p.add_argument('--' + name, type=Path, required=True)
    verify(p.parse_args())
