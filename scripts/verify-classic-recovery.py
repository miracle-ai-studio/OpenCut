#!/usr/bin/env python3
"""Verify native recovery of the actual synthetic browser-captured Classic input.

Uses a fresh database, exact pinned media, all three public transports, explicit
loss review, exact retry and independent red pixels/800 Hz PCM expectations.
"""
import argparse
import array
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import shutil
import socket
import subprocess
import time
import urllib.request


def verify(binary, source_report, root):
    root.mkdir(parents=True, exist_ok=False)
    source = json.loads(source_report.read_text())
    assert source['status'] == 'passed'
    frozen = root / 'opencut'
    shutil.copy2(binary, frozen)
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    assert hashlib.sha256(frozen.read_bytes()).hexdigest() == binary_hash
    db = root / 'workspace.sqlite'
    trace, checks = [], []
    mcp = host = None
    token = root / 'http-token'

    def command(args, value=None, error=None, credential=None):
        arguments = [str(frozen), '--database', str(db), '--json']
        if credential:
            arguments += ['--credential-file', str(credential)]
        result = subprocess.run(arguments + args, input=json.dumps(value) if value is not None else None,
                                capture_output=True, text=True, timeout=90)
        reply = json.loads(result.stdout)
        trace.append({'transport': 'cli', 'command': args, 'request': value,
            'reply': {'credential_issued': True} if args[:2] == ['access', 'issue'] else reply})
        if error:
            assert result.returncode and reply['error']['code'] == error, (reply, result.stderr)
        else:
            assert not result.returncode and reply['error'] is None, (reply, result.stderr)
        return reply.get('data')

    def call(name, value, **kwargs):
        return command(['call', name, '--input', '-'], value, **kwargs)

    def http(name, value):
        request = urllib.request.Request(f'http://127.0.0.1:{port}/v1/tools/{name}',
            data=json.dumps(value).encode(), headers={'Content-Type': 'application/json',
            'Authorization': 'Bearer ' + token.read_text().strip()})
        with urllib.request.urlopen(request, timeout=90) as response:
            reply = json.load(response)
        assert reply['error'] is None, reply
        trace.append({'transport': 'http', 'tool': name, 'request': value, 'reply': reply})
        return reply['data']

    try:
        created = call('opencut_project_create', {'project': {'id': 'library', 'name': 'Recovery input',
            'sequence_id': 'main', 'width': 320, 'height': 180, 'fps': {'numerator': 10, 'denominator': 1}}, 'idempotency_key': 'create'})
        operations = []
        files = []
        for media in source['media']:
            file = root / media['name']
            shutil.copy2(source_report.parent / media['name'], file)
            assert hashlib.sha256(file.read_bytes()).hexdigest() == media['sha256'] == source['hashes'][media['id']]
            files.append(file)
            asset = {'id': media['id'], 'name': media['name'], 'kind': media['type'], 'uri': str(file), 'metadata': {}}
            if media['type'] != 'image':
                asset['duration'] = {'ticks': '240000', 'timebase': 120000}
            operations.append({'type': 'asset.bind', 'asset': asset})
        pinned = call('opencut_edit_apply', {'project_id': 'library', 'request': {'expected_revision': created['revision'],
            'idempotency_key': 'pin', 'label': 'Synthetic captured media', 'operations': operations}})
        for file in files:
            file.unlink()
        mappings = [{'classic_media_id': media['id'], 'source_project_id': 'library', 'source_revision': pinned['revision'],
            'asset_id': media['id']} for media in source['media']]
        value = {'content': source['decoded']['content'], 'target_project_id': 'recovered', 'media_mappings': mappings}
        analyze_tool, import_tool = 'opencut_classic_recovery_analyze_v1', 'opencut_classic_recovery_import_v1'
        analyzed = call(analyze_tool, value)
        assert not analyzed['blockers'] and len(analyzed['proposed_project']['assets']) == 3, analyzed
        assert not any(loss['code'] == 'UNUSED_MEDIA_MAPPING' for loss in analyzed['losses'])
        assert any(loss['code'] == 'LEGACY_SHADOW_FIELD' for loss in analyzed['losses'])
        for asset in analyzed['proposed_project']['assets']:
            media = next(item for item in source['media'] if item['name'] == asset['name'])
            assert asset['version']['content_hash'] == media['sha256']
            assert asset['uri'] == 'opencut-package://sha256/' + media['sha256']
        legacy = call('opencut_classic_analyze', value)
        assert legacy['proposed_project'] is None and legacy['blockers']
        # The legacy API still drops unused mappings with review, never changes semantics.
        canonical = json.loads(json.dumps({key: source['decoded']['project'][key] for key in ('version', 'metadata', 'settings', 'scenes', 'currentSceneId')}))
        for scene in canonical['scenes']:
            for track in [scene['tracks']['main'], *scene['tracks']['overlay'], *scene['tracks']['audio']]:
                for element in track['elements']:
                    for key in ('transform', 'opacity', 'volume', 'muted'):
                        element.pop(key, None)
        legacy_clean = call('opencut_classic_analyze', {**value, 'content': json.dumps(canonical)})
        assert not legacy_clean['blockers'] and len(legacy_clean['proposed_project']['assets']) == 2, legacy_clean
        assert any(loss['code'] == 'UNUSED_MEDIA_MAPPING' for loss in legacy_clean['losses'])
        checks.append('Real browser v1→31 output resolves removed-source pinned A/V and preserves the unused shelf image; legacy analysis remains strict and drops unused media with review')
        for principal, actions in [('reader', ['read']), ('editor', ['read', 'edit'])]:
            command(['access', 'set-policy', '--input', '-'], {'principal_id': principal, 'expected_revision': '0', 'enabled': True,
                'grants': [{'project_id': 'library', 'actions': actions}]})
            credential = root / (principal + '.credential')
            command(['access', 'issue', principal, '--output', str(credential)])
            for name in (analyze_tool, import_tool):
                call(name, {}, credential=credential, error='NOT_AUTHORIZED')
            credential.unlink()
        checks.append('Actual reader/editor credentials cannot use either owner-only recovery tool, before malformed input is resolved')
        spec = importlib.util.spec_from_file_location('editorial', Path(__file__).with_name('verify-editorial.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        mcp = module.Mcp(frozen, db, root, trace=trace)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
        with (root / 'http.log').open('wb') as log:
            host = subprocess.Popen([str(frozen), '--database', str(db), 'serve', '--listen', f'127.0.0.1:{port}',
                '--token-file', str(token)], stdout=log, stderr=log)
        deadline = time.monotonic() + 20
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.2):
                    if token.exists():
                        break
            except OSError:
                pass
            assert time.monotonic() < deadline
            time.sleep(.05)
        assert analyzed == mcp.tool(analyze_tool, value) == http(analyze_tool, value)
        checks.append('CLI/HTTP/MCP produce identical read-only analysis of exact captured content and fixed media mappings')
        request = {'source': value, 'expected_source_content_id': analyzed['source_content_id'],
            'accepted_loss_codes': sorted({loss['code'] for loss in analyzed['losses']}), 'idempotency_key': 'recover'}
        call(import_tool, {**request, 'accepted_loss_codes': []}, error='INVALID_ARGUMENT')
        call(import_tool, {**request, 'expected_source_content_id': 'wrong'}, error='INVALID_ARGUMENT')
        call('opencut_project_inspect', {'project_id': 'recovered'}, error='NOT_FOUND')
        imported = http(import_tool, request)
        assert imported == call(import_tool, request) == mcp.tool(import_tool, request)
        changed = {**request, 'accepted_loss_codes': [*request['accepted_loss_codes'], 'EXTRA']}
        call(import_tool, changed, error='IDEMPOTENCY_KEY_REUSED')
        call('opencut_project_delete', {'projects': [{'project_id': 'library', 'expected_revision': pinned['revision']}], 'idempotency_key': 'remove-stage'})
        assert imported == http(import_tool, request) == mcp.tool(import_tool, request)
        snapshot = call('opencut_project_inspect', {'project_id': 'recovered'})
        assert snapshot['revision'] == imported['revision'] and len(snapshot['project']['assets']) == 3
        checks.append('Missing loss consent/hash mismatch publish nothing; HTTP commit replays exactly through CLI/MCP after staging project deletion; changed retry is rejected')
        damaged = json.loads(value['content'])
        damaged['scenes'][0]['tracks']['main']['elements'][0]['effects'] = [{'type': 'unknown'}]
        blocked = http(analyze_tool, {**value, 'content': json.dumps(damaged), 'target_project_id': 'blocked', 'media_mappings': [
            {**mapping, 'source_project_id': 'recovered', 'source_revision': imported['revision'],
             'asset_id': next(asset['id'] for asset in snapshot['project']['assets'] if asset['name'] == next(media['name'] for media in source['media'] if media['id'] == mapping['classic_media_id']))}
            for mapping in mappings]})
        assert blocked['proposed_project'] is None and any(f['path'].endswith('effects') for f in blocked['blockers'])
        checks.append('Unsupported effects remain blockers with source JSON paths')
        job = mcp.tool('opencut_job_start', {'kind': 'render.project', 'idempotency_key': 'render', 'input': {'project_id': 'recovered',
            'revision': imported['revision'], 'sequence_id': 'main', 'container': 'mp4', 'video_codec': 'h264'}})
        deadline = time.monotonic() + 120
        while True:
            done = call('opencut_job_get', {'job_id': job['job_id']})
            if done['state'] in ('succeeded', 'failed', 'cancelled'):
                break
            assert time.monotonic() < deadline
            time.sleep(.1)
        assert done['state'] == 'succeeded', done
        artifact = done['result']['artifact']; output = Path(artifact['path'])
        assert hashlib.sha256(output.read_bytes()).hexdigest() == artifact['content_hash']
        rgb = subprocess.check_output(['ffmpeg', '-v', 'error', '-ss', '0.5', '-i', str(output), '-frames:v', '1',
            '-vf', 'scale=1:1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=30)
        assert rgb[0] > 230 and max(rgb[1:]) < 25, list(rgb)
        pcm = array.array('h'); pcm.frombytes(subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(output),
            '-vn', '-ac', '2', '-ar', '48000', '-f', 's16le', '-'], timeout=30))
        # The native renderer uses FFmpeg's equal-power mono→stereo matrix.
        # A 4095-peak sine becomes 4095/sqrt(2) peak in each stereo channel,
        # so its independent per-channel RMS is 4095/2, not the source mono RMS.
        samples = pcm[48000:144000:2]
        right = pcm[48001:144000:2]
        rms = math.sqrt(sum(n*n for n in samples) / len(samples))
        assert abs(rms - 4095 / 2) < 100, rms
        stereo_rmse = math.sqrt(sum((a-b)**2 for a,b in zip(samples, right)) / len(samples))
        assert stereo_rmse < 10, stereo_rmse
        power = {hz: sum(n * math.sin(2*math.pi*hz*i/48000) for i, n in enumerate(samples))**2 +
            sum(n * math.cos(2*math.pi*hz*i/48000) for i, n in enumerate(samples))**2 for hz in (330, 660, 800)}
        assert power[800] > 1000 * max(power[330], power[660]), power
        checks.append('Recovered project exports after staging/original files disappear; independent red RGB and unity-gain 800 Hz PCM match expected values')
        report = {'status': 'passed', 'checks': checks, 'binary_sha256': binary_hash,
            'browser_report_sha256': hashlib.sha256(source_report.read_bytes()).hexdigest(), 'source_content_id': analyzed['source_content_id'],
            'source_snapshot_digest': source['decoded']['sourceDigest'], 'revision': imported['revision'], 'rgb': list(rgb), 'audio_rms': rms,
            'frequency_power': power, 'stereo_rmse': stereo_rmse, 'expected_per_channel_rms': 4095/2,
            'render_sha256': artifact['content_hash'], 'trace': trace}
        (root / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
        return root / 'report.json'
    finally:
        if host:
            host.terminate()
            try: host.wait(timeout=20)
            except subprocess.TimeoutExpired: host.kill(); host.wait()
        if mcp:
            mcp.close()
        token.unlink(missing_ok=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--source-report', type=Path, required=True)
    parser.add_argument('--artifacts-dir', type=Path, required=True)
    args = parser.parse_args()
    print(verify(args.binary.resolve(), args.source_report.resolve(), args.artifacts_dir.resolve()))
