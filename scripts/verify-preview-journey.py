#!/usr/bin/env python3
"""Exercise one real HTTP continuous-preview session and its safety boundaries."""

import argparse
from fractions import Fraction
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.parse import urlencode


def tick(value, base=1000):
    return {'ticks': str(value), 'timebase': base}


def span(start, duration):
    return {'start': tick(start), 'duration': tick(duration)}


def seconds(value):
    return Fraction(int(value['ticks']), value['timebase'])


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def verify(binary, root, fixture_seconds):
    root.mkdir(parents=True, exist_ok=True)
    database = root / 'workspace.sqlite'
    owner, runner, reader = (root / f'{name}.secret' for name in ('owner', 'runner', 'reader'))
    checks, trace = [], []
    host = None
    second_host = None
    with socket.socket() as reserved:
        reserved.bind(('127.0.0.1', 0))
        port = reserved.getsockname()[1]
    with socket.socket() as reserved:
        reserved.bind(('127.0.0.1', 0))
        second_port = reserved.getsockname()[1]

    def cli(args, payload=None):
        command = [str(binary), '--database', str(database), '--json', *args]
        result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                                text=True, capture_output=True, timeout=60)
        data = json.loads(result.stdout)
        trace.append({'cli': args, 'status': result.returncode, 'response': data})
        assert result.returncode == 0 and data['error'] is None, (data, result.stderr)
        return data['data']

    def request(method, path, credential, body=None, timeout=30, target_port=None):
        connection = http.client.HTTPConnection('127.0.0.1', target_port or port, timeout=timeout)
        headers = {'Authorization': 'Bearer ' + credential.read_text().strip()}
        if body is not None:
            headers['Content-Type'] = 'application/json' if method == 'POST' else 'application/octet-stream'
        connection.request(method, path, body, headers)
        response = connection.getresponse()
        return connection, response

    def tool(name, payload, credential=runner, error=None, target_port=None):
        connection, response = request('POST', '/v1/tools/' + name, credential,
                                       json.dumps(payload).encode(), target_port=target_port)
        result = json.loads(response.read())
        status = response.status
        connection.close()
        trace.append({'tool': name, 'status': status, 'response': result})
        if error:
            assert status >= 400 and result['error']['code'] == error, (status, result)
        else:
            assert status == 200 and result['error'] is None, (status, result)
        return result.get('data')

    def stream(view, credential=runner):
        input_data = {field: view[field] for field in ('session_id', 'generation', 'owner_id', 'lease_id')}
        connection, response = request('POST', '/v1/preview/stream', credential,
                                       json.dumps(input_data).encode(), timeout=45)
        if response.status != 200:
            result = json.loads(response.read())
            connection.close()
            return None, result['error']['code']
        assert response.getheader('Content-Type', '').startswith('video/mp4')
        return (connection, response), None

    def control(view, command, error=None, credential=runner):
        return tool('opencut_preview_control', {
            'session_id': view['session_id'], 'generation': view['generation'],
            'owner_id': view['owner_id'], 'lease_id': view['lease_id'], 'command': command,
        }, credential, error)

    def stop_host():
        nonlocal host
        if host and host.poll() is None:
            host.terminate()
            try:
                host.wait(timeout=12)
            except subprocess.TimeoutExpired:
                host.kill()
                host.wait(timeout=12)
        host = None

    def start_host():
        nonlocal host
        with (root / 'host.log').open('a') as log:
            host = subprocess.Popen([str(binary), '--database', str(database), 'serve',
                                     '--listen', f'127.0.0.1:{port}', '--token-file', str(owner)],
                                    stdout=log, stderr=log, env=os.environ.copy())
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            assert host.poll() is None, 'Host exited while starting'
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.2):
                    return
            except OSError:
                time.sleep(.05)
        raise AssertionError('Host startup timed out')

    def start_second_host():
        nonlocal second_host
        with (root / 'second-host.log').open('a') as log:
            second_host = subprocess.Popen([str(binary), '--database', str(database), 'serve',
                '--listen', f'127.0.0.1:{second_port}', '--token-file', str(owner)],
                stdout=log, stderr=log, env=os.environ.copy())
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            assert second_host.poll() is None, 'Second host exited while starting'
            try:
                with socket.create_connection(('127.0.0.1', second_port), timeout=.2): return
            except OSError: time.sleep(.05)
        raise AssertionError('Second host startup timed out')

    def preview_inputs():
        return set((root / 'previews').glob('.opencut-preview-*'))

    def preview_ffmpeg_processes():
        processes = subprocess.check_output(['ps', '-axo', 'command='], text=True)
        return [line for line in processes.splitlines()
                if 'ffmpeg' in line and str(root / 'previews') in line
                and '.opencut-preview-' in line]

    def upload(path, key):
        content = path.read_bytes()
        item = tool('opencut_upload_create', {'project_id': 'preview', 'idempotency_key': key,
                    'name': path.name, 'size_bytes': str(len(content)),
                    'content_hash': hashlib.sha256(content).hexdigest()})
        for offset in range(0, len(content), 4 * 1024 * 1024):
            chunk = content[offset:offset + 4 * 1024 * 1024]
            query = urlencode({'size_bytes': str(len(chunk)),
                               'content_hash': hashlib.sha256(chunk).hexdigest()})
            connection, response = request('PUT',
                f"/v1/uploads/{item['upload_id']}/chunks/{offset}?{query}", runner, chunk)
            result = json.loads(response.read())
            connection.close()
            assert response.status == 200 and result['error'] is None, result
        accepted = tool('opencut_upload_seal', {'upload_id': item['upload_id']})
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            view = tool('opencut_job_view', {'job_id': accepted['job_id']})
            if view['state'] in ('succeeded', 'failed', 'cancelled'):
                assert view['state'] == 'succeeded', view
                return view['extended_result']['data']['asset']
            time.sleep(.1)
        raise AssertionError('Upload import did not finish')

    try:
        assert binary.is_file()
        fixture = root / 'preview-source.mp4'
        audio_fixture = root / 'preview-source.wav'
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
            f'color=c=0x202836:size=320x180:rate=25:duration={fixture_seconds}', '-f', 'lavfi', '-i',
            f'sine=frequency=440:sample_rate=48000:duration={fixture_seconds}', '-c:v', 'mpeg4', '-c:a', 'aac',
            '-shortest', '-y', str(fixture)], check=True, timeout=max(60, fixture_seconds * 2))
        subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
            f'sine=frequency=440:sample_rate=48000:duration={fixture_seconds}', '-c:a', 'pcm_s16le',
            '-y', str(audio_fixture)], check=True, timeout=max(60, fixture_seconds * 2))
        created = cli(['call', 'opencut_project_create', '--input', '-'], {
            'project': {'id': 'preview', 'name': 'Continuous preview fixture', 'sequence_id': 'main',
                        'width': 320, 'height': 180, 'fps': {'numerator': 25, 'denominator': 1}},
            'idempotency_key': 'create'})
        for name, actions in [('runner', ['read', 'edit', 'run_jobs']), ('reader', ['read'])]:
            cli(['access', 'set-policy', '--input', '-'], {'principal_id': name,
                'expected_revision': '0', 'enabled': True,
                'grants': [{'project_id': 'preview', 'actions': actions}]})
            cli(['access', 'issue', name, '--output', str(root / f'{name}.secret')])
        start_host()
        asset = upload(fixture, 'source-upload')
        assert asset['kind'] == 'video' and asset['version']['content_hash'] == sha(fixture)
        asset.update(id='source', name=f'{fixture_seconds} second audio visual fixture')
        audio_asset = upload(audio_fixture, 'audio-upload')
        assert audio_asset['kind'] == 'audio' and audio_asset['version']['content_hash'] == sha(audio_fixture)
        audio_asset.update(id='sound-source', name=f'{fixture_seconds} second tone')
        track = lambda name, kind: {'id': name, 'name': name, 'kind': kind,
            'enabled': True, 'muted': False, 'locked': False, 'clips': [], 'params': {}}
        clip = lambda name, asset_id: {'id': name, 'asset_id': asset_id,
            'timeline_range': span(0, fixture_seconds * 1000),
            'source_range': span(0, fixture_seconds * 1000), 'params': {}, 'channels': []}
        bound = tool('opencut_edit_apply', {'project_id': 'preview', 'request': {
            'expected_revision': created['revision'], 'idempotency_key': 'bind', 'label': 'bind fixture',
            'operations': [{'type': 'asset.bind', 'asset': asset},
                {'type': 'asset.bind', 'asset': audio_asset},
                {'type': 'track.create', 'sequence_id': 'main', 'track': track('picture', 'video')},
                {'type': 'track.create', 'sequence_id': 'main', 'track': track('sound', 'audio')},
                {'type': 'clip.insert', 'sequence_id': 'main', 'track_id': 'picture', 'clip': clip('video', 'source')},
                {'type': 'clip.insert', 'sequence_id': 'main', 'track_id': 'sound', 'clip': clip('audio', 'sound-source')}],
        }})
        cue = {'id': 'visible-preview-caption', 'timeline_range': span(500, 3000),
            'text': 'VISIBLE PREVIEW', 'source_anchor': None,
            'style': {'font_family': 'Arial', 'font_size': 28, 'color': '#ffffff',
                'background_color': None, 'max_width': 300, 'line_height': 34,
                'text_align': 'center', 'font_policy': {'mode': 'explicit_fallback',
                    'families': ['Helvetica']}}}
        captioned = tool('opencut_edit_apply', {'project_id': 'preview', 'request': {
            'expected_revision': bound['revision'], 'idempotency_key': 'caption',
            'label': 'visible caption', 'operations': [
                {'type': 'caption.insert', 'sequence_id': 'main', 'cue': cue}],
        }})
        fixed_revision = captioned['revision']
        create = {'project_id': 'preview', 'revision': fixed_revision, 'sequence_id': 'main',
            'start_time': tick(0), 'alignment': 'exact', 'audio_enabled': True,
            'idempotency_key': 'preview-session'}
        tool('opencut_preview_create', create, reader, 'NOT_AUTHORIZED')
        ready = tool('opencut_preview_create', create)
        assert ready['generation'] == '0' and ready['state'] == 'ready'
        assert tool('opencut_preview_create', create)['session_id'] == ready['session_id']
        checks.append('Fixed asset revision and scoped idempotent session creation')

        playing = control(ready, {'kind': 'play'})
        assert playing['generation'] == '1' and playing['state'] in ('producing', 'ended')
        current = tool('opencut_preview_inspect', {'session_id': ready['session_id']})
        assert current['presentation'] is None
        # A malformed seek is rejected before it can invalidate the live generation.
        control(playing, {'kind': 'seek', 'sequence_time': tick(fixture_seconds * 1000),
                          'alignment': 'exact'}, 'INVALID_ARGUMENT')
        assert tool('opencut_preview_inspect', {'session_id': ready['session_id']})['generation'] == '1'
        opened, error = stream(playing)
        assert error is None, error
        duplicate, duplicate_error = stream(playing)
        assert duplicate is None and duplicate_error == 'PREVIEW_STREAM_CLAIMED', duplicate_error
        bad_lease = dict(playing, lease_id='wrong')
        rejected, lease_error = stream(bad_lease)
        assert rejected is None and lease_error == 'NOT_AUTHORIZED', lease_error
        checks.append('Invalid seek preserves generation; only one lease-bound stream may open')

        connection, response = opened
        fmp4 = root / 'preview-stream.mp4'
        stream_started = time.monotonic()
        with fmp4.open('wb') as output:
            while True:
                chunk = response.read(64 * 1024)
                if not chunk:
                    break
                output.write(chunk)
        connection.close()
        stream_seconds = time.monotonic() - stream_started
        assert fmp4.stat().st_size > 1024
        probe = json.loads(subprocess.run(['ffprobe', '-v', 'error', '-show_entries',
            'stream=codec_type,codec_name:format=format_name,duration', '-of', 'json', str(fmp4)],
            check=True, capture_output=True, text=True, timeout=30).stdout)
        kinds = {stream['codec_type'] for stream in probe['streams']}
        assert {'video', 'audio'} <= kinds and 'mov' in probe['format']['format_name'], probe
        actual_duration = float(probe['format']['duration'])
        assert abs(actual_duration - fixture_seconds) <= .1, (actual_duration, fixture_seconds)
        subprocess.run(['ffmpeg', '-v', 'error', '-i', str(fmp4), '-f', 'null', '-'],
                       check=True, capture_output=True, timeout=max(45, fixture_seconds * 2))
        frame = subprocess.check_output(['ffmpeg', '-v', 'error', '-ss', '1.5', '-i', str(fmp4),
            '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=30)
        assert len(frame) == 320 * 180 * 3
        bright = sum(1 for offset in range(0, len(frame), 3)
                     if min(frame[offset:offset + 3]) > 180)
        assert bright > 100, f'Caption glyphs missing from decoded preview frame: {bright}'
        checks.append('One continuous generation decodes at the requested duration as fMP4 with video, audio and visible caption glyphs')

        disconnect_ready = tool('opencut_preview_create', {
            **create, 'idempotency_key': 'disconnect-session'})
        disconnect_playing = control(disconnect_ready, {'kind': 'play'})
        disconnect_stream, disconnect_error = stream(disconnect_playing)
        assert disconnect_error is None, disconnect_error
        disconnect_connection, disconnect_response = disconnect_stream
        assert disconnect_response.read(1024), 'Disconnected preview stream produced no bytes'
        disconnect_response.close()
        disconnect_connection.close()
        deadline = time.monotonic() + 10
        disconnected = None
        while time.monotonic() < deadline:
            disconnected = tool('opencut_preview_inspect', {
                'session_id': disconnect_ready['session_id']})
            if disconnected['state'] == 'failed':
                break
            time.sleep(.1)
        assert disconnected and disconnected['state'] == 'failed' \
            and disconnected['failure_code'] == 'PREVIEW_DISCONNECTED', disconnected
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline and preview_ffmpeg_processes():
            time.sleep(.1)
        assert not preview_ffmpeg_processes(), 'HTTP disconnect orphaned preview FFmpeg'
        checks.append('HTTP client disconnect cancels its generation, records PREVIEW_DISCONNECTED and reaps FFmpeg')

        telemetry = {'sequence': '1', 'presented_time': tick(1000), 'media_clock_time': tick(1000),
            'playing': True, 'stalled': False, 'client_observed_at_ms': str(int(time.time() * 1000))}
        reported = control(playing, {'kind': 'report_presentation', 'telemetry': telemetry})
        assert reported['presentation']['telemetry']['presented_time'] == tick(1000)
        control(playing, {'kind': 'report_presentation', 'telemetry': telemetry}, 'PREVIEW_TELEMETRY_ORDER')
        paused = control(playing, {'kind': 'pause'})
        assert paused['state'] == 'paused' and paused['resume_position'] == tick(1000)
        assert paused['resume_position_confirmed']
        resumed = control(paused, {'kind': 'play'})
        assert resumed['generation'] == '2' and seconds(resumed['actual_start']) == 1
        control(playing, {'kind': 'heartbeat'}, 'PREVIEW_GENERATION_CONFLICT')
        control(playing, {'kind': 'report_presentation', 'telemetry': {
            **telemetry, 'sequence': '2'}}, 'PREVIEW_GENERATION_CONFLICT')
        old_stream, old_error = stream(playing)
        assert old_stream is None and old_error == 'PREVIEW_GENERATION_CONFLICT', old_error
        checks.append('Presentation telemetry alone sets pause anchor; old generation rejects control and stream')

        head = tool('opencut_edit_apply', {'project_id': 'preview', 'request': {
            'expected_revision': fixed_revision, 'idempotency_key': 'head-after-session',
            'label': 'add empty track after preview',
            'operations': [{'type': 'track.create', 'sequence_id': 'main',
                            'track': track('later', 'caption')}],
        }})
        assert head['revision'] != fixed_revision
        fixed = tool('opencut_preview_inspect', {'session_id': ready['session_id']})
        assert fixed['revision'] == fixed_revision and fixed['generation'] == resumed['generation']
        events = tool('opencut_preview_events', {'session_id': ready['session_id'], 'cursor': '0'})
        assert any(event['kind'] == 'presentation' for event in events['events'])
        assert any(event['kind'] == 'producer_started' and event['generation'] == '2'
                   for event in events['events'])
        checks.append('Head edit leaves fixed revision intact; events distinguish production and presentation')

        acquire = {'session_id': ready['session_id'], 'expected_generation': resumed['generation']}
        tool('opencut_preview_acquire', acquire, error='PREVIEW_OWNER_ACTIVE')
        released = control(resumed, {'kind': 'release_owner'})
        assert released['state'] == 'paused'
        acquired = tool('opencut_preview_acquire', acquire)
        assert int(acquired['generation']) == int(resumed['generation']) + 1
        assert acquired['lease_id'] != resumed['lease_id']
        control(resumed, {'kind': 'heartbeat'}, 'NOT_AUTHORIZED')
        reacquired_playing = control(acquired, {'kind': 'play'})
        assert int(reacquired_playing['generation']) == int(acquired['generation']) + 1
        checks.append('Active owner blocks takeover; release rotates lease and rejects old controls')

        active_stream, active_error = stream(reacquired_playing)
        assert active_error is None, active_error
        active_connection, active_response = active_stream
        assert active_response.read(1024)
        control(reacquired_playing, {'kind': 'stop'})
        active_connection.close()
        stopped = tool('opencut_preview_inspect', {'session_id': ready['session_id']})
        assert stopped['state'] == 'stopped'
        denied, stop_error = stream(reacquired_playing)
        assert denied is None and stop_error in ('PREVIEW_NOT_PLAYING', 'PREVIEW_STREAM_CLAIMED'), stop_error
        checks.append('Stop cancels producer and rejects another stream')

        another = tool('opencut_preview_create', {**create, 'idempotency_key': 'restart-session'})
        assert preview_inputs()
        start_second_host()
        live = tool('opencut_preview_create', {**create, 'idempotency_key': 'second-host-session'},
                    target_port=second_port)
        live_dirs = preview_inputs()
        assert len(live_dirs) >= 2, live_dirs
        host.kill()
        host.wait(timeout=12)
        host = None
        start_host()
        tool('opencut_preview_inspect', {'session_id': another['session_id']}, error='NOT_FOUND')
        assert tool('opencut_preview_inspect', {'session_id': live['session_id']},
                    target_port=second_port)['state'] == 'ready'
        assert preview_inputs(), 'Recovery deleted another live host preview'
        checks.append('SIGKILL restart reclaims stale private input while preserving another live host lease')
        second_host.terminate()
        second_host.wait(timeout=12)
        second_host = None
        stop_host()
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline and preview_inputs():
            time.sleep(.1)
        assert not preview_inputs(), 'SIGTERM retained preview private inputs'
        assert not preview_ffmpeg_processes(), 'SIGTERM orphaned preview FFmpeg'
        checks.append('SIGTERM reaps preview inputs and FFmpeg; restart terminates ephemeral session')
        report = {'status': 'passed', 'checks': checks, 'binary_sha256': sha(binary),
                  'fixture_duration_seconds': fixture_seconds,
                  'stream_duration_seconds': actual_duration,
                  'stream_transfer_seconds': round(stream_seconds, 3),
                  'stream_bytes': fmp4.stat().st_size, 'stream_sha256': sha(fmp4),
                  'session_id': ready['session_id']}
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'status': 'passed', 'report': str(root / 'report.json')}))
    except Exception as error:
        (root / 'failure.json').write_text(json.dumps({'error': repr(error), 'checks': checks}, indent=2) + '\n')
        raise
    finally:
        stop_host()
        if second_host and second_host.poll() is None:
            second_host.terminate()
            try: second_host.wait(timeout=12)
            except subprocess.TimeoutExpired: second_host.kill(); second_host.wait(timeout=12)
        (root / 'trace.json').write_text(json.dumps(trace, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--artifacts-dir', type=Path)
    parser.add_argument('--fixture-seconds', type=int, default=6,
                        help='continuous A/V duration for the acceptance fixture (6..300)')
    args = parser.parse_args()
    if not 6 <= args.fixture_seconds <= 300:
        parser.error('--fixture-seconds must be between 6 and 300')
    verify(args.binary.resolve(),
           (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-preview-'))).resolve(),
           args.fixture_seconds)
