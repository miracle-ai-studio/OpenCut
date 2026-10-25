#!/usr/bin/env python3
"""Real Chinese TTS audio -> fixed transcription -> editable captions -> decoded export.

This checks the product plumbing with a real local Whisper model. Synthetic speech
does not establish interview transcription quality or human-language accuracy.
"""

import argparse
import copy
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
from urllib.parse import quote, urlencode


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def tick(value, base=1000):
    return {'ticks': str(value), 'timebase': base}


def span(start, duration):
    return {'start': tick(start), 'duration': tick(duration)}


def milliseconds(value):
    return int(Fraction(int(value['ticks']), value['timebase']) * 1000)


def verify(binary, audio, root, model, english_audio=None):
    root.mkdir(parents=True, exist_ok=True)
    db = root / 'workspace.sqlite'
    trace, checks = [], []
    host = None
    owner = root / 'owner.secret'
    runner = root / 'runner.secret'
    reader = root / 'reader.secret'
    with socket.socket() as reserved:
        reserved.bind(('127.0.0.1', 0))
        port = reserved.getsockname()[1]
    base = f'127.0.0.1:{port}'

    def cli(args, payload=None, credential=None, error=None, timeout=40):
        command = [str(binary), '--database', str(db), '--json']
        if credential:
            command += ['--credential-file', str(credential)]
        command += args
        process = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                                 text=True, capture_output=True, timeout=timeout)
        response = json.loads(process.stdout)
        trace.append({'cli': args, 'exit': process.returncode, 'response': response})
        if error:
            assert process.returncode != 0 and response['error']['code'] == error, (response, process.stderr)
        else:
            assert process.returncode == 0 and response['error'] is None, (response, process.stderr)
        return response.get('data')

    def call(name, payload, credential=None, error=None):
        return cli(['call', name, '--input', '-'], payload, credential, error)

    def request(method, path, credential, body=None, content_type='application/json'):
        connection = http.client.HTTPConnection('127.0.0.1', port, timeout=40)
        headers = {'Authorization': 'Bearer ' + credential.read_text().strip()}
        if body is not None:
            headers['Content-Type'] = content_type
        connection.request(method, path, body, headers)
        response = connection.getresponse()
        data = response.read()
        status = response.status
        connection.close()
        return status, data

    def tool(name, payload, credential=runner, error=None):
        status, raw = request('POST', '/api/tools/' + name, credential, json.dumps(payload).encode())
        response = json.loads(raw)
        trace.append({'http': name, 'status': status, 'response': response})
        if error:
            assert status >= 400 and response['error']['code'] == error, (status, response)
        else:
            assert status == 200 and response['error'] is None, (status, response)
        return response.get('data')

    def wait(job_id, limit=240):
        deadline = time.monotonic() + limit
        while True:
            view = tool('opencut_job_view', {'job_id': job_id})
            if view['state'] in ('succeeded', 'failed', 'cancelled'):
                assert view['state'] == 'succeeded', view
                return view
            assert time.monotonic() < deadline, f'Job {job_id} did not finish within {limit}s'
            time.sleep(.25)

    def upload(path, key):
        data = path.read_bytes()
        descriptor = {'project_id': 'transcription', 'idempotency_key': key,
                      'name': path.name, 'size_bytes': str(len(data)),
                      'content_hash': hashlib.sha256(data).hexdigest()}
        view = tool('opencut_upload_begin', descriptor)
        for offset in range(0, len(data), 4 * 1024 * 1024):
            chunk = data[offset:offset + 4 * 1024 * 1024]
            query = urlencode({'size_bytes': str(len(chunk)),
                               'content_hash': hashlib.sha256(chunk).hexdigest()})
            status, raw = request('PUT', f"/api/uploads/{view['upload_id']}/chunks/{offset}?{query}",
                                  runner, chunk, 'application/octet-stream')
            result = json.loads(raw)
            assert status == 200 and result['error'] is None, (status, result)
            assert result['data']['next_offset'] == str(offset + len(chunk)), result
        accepted = tool('opencut_upload_complete', {'upload_id':view['upload_id'], 'content_hash':view['content_hash']})
        imported = wait(accepted['job_id'], 90)['extended_result']['data']
        assert imported['asset']['version']['content_hash'] == descriptor['content_hash'], imported
        return imported

    def snapshot():
        return tool('opencut_project_inspect', {'project_id': 'transcription'})

    def edit(revision, key, operations):
        return {'project_id': 'transcription', 'request': {
            'expected_revision': revision, 'idempotency_key': key,
            'label': key, 'operations': operations}}

    def stop():
        nonlocal host
        if host and host.poll() is None:
            host.terminate()
            try:
                host.wait(timeout=12)
            except subprocess.TimeoutExpired:
                host.kill()
                host.wait(timeout=10)
        host = None

    try:
        assert binary.is_file() and audio.is_file() and model.is_file()
        model_hash, audio_hash, binary_hash = sha(model), sha(audio), sha(binary)
        created = call('opencut_project_create', {'project': {
            'id': 'transcription', 'name': 'Chinese TTS transcription acceptance',
            'sequence_id': 'main', 'width': 640, 'height': 360,
            'fps': {'numerator': 25, 'denominator': 1}}, 'idempotency_key': 'create'})
        for name, actions in [('runner', ['read', 'edit', 'run_jobs', 'read_artifacts']),
                              ('reader', ['read'])]:
            cli(['access', 'set-policy', '--input', '-'], {'principal_id': name,
                'expected_revision': '0', 'enabled': True,
                'grants': [{'project_id': 'transcription', 'actions': actions}]})
            cli(['access', 'issue', name, '--output', str(root / f'{name}.secret')])
        env = os.environ.copy()
        env['OPENCUT_WHISPER_MODEL'] = str(model)
        with (root / 'host.log').open('w') as log:
            host = subprocess.Popen([str(binary), '--database', str(db), 'serve',
                '--listen', base, '--token-file', str(owner)], env=env, stdout=log, stderr=log)
        deadline = time.monotonic() + 20
        while True:
            assert host.poll() is None, 'Host exited during startup'
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.25):
                    break
            except OSError:
                assert time.monotonic() < deadline, 'Host startup timed out'
                time.sleep(.05)
        capabilities = tool('opencut_capabilities', {})
        assert any(item['name'] == 'analysis.transcribe' for item in capabilities['job_kinds'])
        checks.append('Scoped host advertises analysis.transcribe with a host-selected model')

        background = root / 'background.png'
        subprocess.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i',
                        'color=c=0x202836:s=640x360', '-frames:v', '1', str(background)],
                       check=True, timeout=45)
        speech = upload(audio, 'upload-audio')
        image = upload(background, 'upload-background')
        assert speech['asset']['kind'] == 'audio' and image['asset']['kind'] == 'image'
        assert speech['asset']['duration'] is not None
        duration_ms = milliseconds(speech['asset']['duration'])
        assert duration_ms > 1000
        audio_asset = copy.deepcopy(speech['asset'])
        audio_asset.update(id='speech', name='Imported Chinese TTS speech')
        image_asset = copy.deepcopy(image['asset'])
        image_asset.update(id='background', name='Dark caption background')
        track = lambda name, kind: {'id': name, 'name': name, 'kind': kind,
            'enabled': True, 'muted': False, 'locked': False, 'clips': [], 'params': {}}
        clip = lambda name, asset_id, source=None: {'id': name, 'asset_id': asset_id,
            'timeline_range': span(0, duration_ms), 'source_range': source,
            'params': {}, 'channels': []}
        operations = [
            {'type': 'asset.bind', 'asset': audio_asset},
            {'type': 'asset.bind', 'asset': image_asset},
            {'type': 'track.create', 'sequence_id': 'main', 'track': track('voice', 'audio')},
            {'type': 'track.create', 'sequence_id': 'main', 'track': track('background-track', 'graphic')},
            {'type': 'clip.insert', 'sequence_id': 'main', 'track_id': 'voice',
             'clip': clip('whole-speech', 'speech', span(0, duration_ms))},
            {'type': 'clip.insert', 'sequence_id': 'main', 'track_id': 'background-track',
             'clip': clip('dark-card', 'background')},
        ]
        binding = edit(created['revision'], 'bind-whole-audio', operations)
        tool('opencut_edit_plan', binding)
        assert snapshot()['revision'] == created['revision']
        bound = tool('opencut_edit_apply', binding)
        assert call('opencut_edit_apply', binding, runner) == bound
        checks.append('Project-scoped chunk upload, seal/import and whole audio clip bind are durable; edit.plan leaves head unchanged')

        transcription_request = {'idempotency_key': 'transcribe-fixed-speech',
            'project_id': 'transcription', 'revision': bound['revision'],
            'asset_id': 'speech', 'language': 'zh'}
        tool('opencut_analysis_transcribe', transcription_request, reader, 'NOT_AUTHORIZED')
        for field in ('model', 'model_path', 'input', 'path'):
            tool('opencut_analysis_transcribe', {**transcription_request, field: '/private/untrusted-model.bin'},
                 runner, 'INVALID_ARGUMENT')
        submitted = tool('opencut_analysis_transcribe', transcription_request)
        replay = cli(['domain', 'media', 'analysis.transcribe', '--input', '-'],
                     transcription_request, runner)
        assert replay['job_id'] == submitted['job_id']
        legacy_replay = tool('opencut_analysis_transcribe',
                             {**transcription_request, 'word_timestamps': False})
        assert legacy_replay['job_id'] == submitted['job_id']
        tool('opencut_analysis_transcribe', {**transcription_request, 'language': 'en'},
             runner, 'IDEMPOTENCY_KEY_REUSED')
        transcript_job = wait(submitted['job_id'])
        assert transcript_job['result'] is None
        extended = transcript_job['extended_result']
        assert extended['kind'] == 'analysis.transcribe' and extended['schema_version'] == 1
        data = extended['data']
        assert (data['project_id'], data['revision'], data['asset_id']) == (
            'transcription', bound['revision'], 'speech')
        assert data['asset_version']['content_hash'] == audio_hash
        assert int(data['asset_version']['size']) == audio.stat().st_size
        transcription = data['transcription']
        assert transcription['provider'] == 'whisper.cpp'
        assert transcription['model'] == model.name and '/' not in transcription['model']
        assert transcription['model_sha256'] == model_hash
        assert transcription['language'] == 'zh'
        assert transcription['word_timing_status'] == 'not_requested'
        segments = transcription['segments']
        assert segments and all(isinstance(s['text'], str) and isinstance(s['words'], list)
                                for s in segments), transcription
        public_hash = hashlib.sha256(json.dumps(data, sort_keys=True,
            ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
        assert tool('opencut_job_view', {'job_id': submitted['job_id']})['extended_result']['data'] == data
        checks.append('Public fixed-version transcription is readable and hash-stable; replay, conflict, model injection and read-only access are enforced')
        evidence_page = tool('opencut_analysis_evidence_query', {'job_id':submitted['job_id'],
            'text':segments[0]['text'][:2], 'limit':1})
        assert evidence_page['analysis_kind'] == 'analysis.transcribe'
        assert evidence_page['provenance']['model_sha256'] == model_hash
        assert evidence_page['provenance']['parameters']['word_timing_status'] == 'not_requested'
        assert evidence_page['evidence'][0]['kind'] == 'transcript_segment'
        checks.append('Unified typed evidence query returns transcript source range, model digest and timing status')

        cues = []
        for index, segment in enumerate(segments):
            start = max(0, milliseconds(segment['start']))
            end = min(duration_ms, milliseconds(segment['end']))
            text = segment['text'].strip()
            if not text or end <= start:
                continue
            area = span(start, end - start)
            cues.append({'id': f'whisper-{index}', 'timeline_range': area,
                'text': text, 'source_anchor': {'asset_id': 'speech', 'source_range': area},
                'style': {'font_family': 'PingFang SC', 'font_size': 28,
                          'color': '#ffffff', 'background_color': None,
                          'max_width': 590, 'line_height': 34, 'text_align': 'center',
                          'font_policy': {'mode': 'explicit_fallback',
                                          'families': ['Arial Unicode MS', 'Arial']}}})
        assert cues, 'No transcribed segment overlaps the actual imported audio duration'
        caption_request = edit(bound['revision'], 'insert-transcribed-captions',
            [{'type': 'caption.insert', 'sequence_id': 'main', 'cue': cue} for cue in cues])
        plan = tool('opencut_edit_plan', caption_request)
        assert plan is not None and snapshot()['revision'] == bound['revision']
        captioned = tool('opencut_edit_apply', caption_request)
        assert len(snapshot()['project']['sequences'][0]['caption_cues']) == len(cues)
        corrected = copy.deepcopy(cues[0])
        corrected['text'] = corrected['text'] + '（人工修正）'
        changed = tool('opencut_edit_apply', edit(captioned['revision'], 'human-correction',
            [{'type': 'caption.update', 'sequence_id': 'main', 'cue': corrected}]))
        current = snapshot()
        assert current['revision'] == changed['revision']
        persisted_cues = current['project']['sequences'][0]['caption_cues']
        assert next(c for c in persisted_cues if c['id'] == corrected['id'])['text'] == corrected['text']
        assert all(c['source_anchor']['asset_id'] == 'speech' for c in persisted_cues)
        checks.append('Clamped transcription segments become source-anchored editable captions; a human correction persists')

        preflight = tool('opencut_render_preflight', {'project_id': 'transcription',
            'revision': changed['revision'], 'sequence_id': 'main',
            'container': 'mp4', 'video_codec': 'h264'})
        assert preflight['ready'], preflight
        render = tool('opencut_job_submit', {'kind': 'render.project',
            'idempotency_key': 'render-corrected-subtitles', 'input': {
                'project_id': 'transcription', 'revision': changed['revision'],
                'sequence_id': 'main', 'container': 'mp4', 'video_codec': 'h264'}})
        exported = wait(render['job_id'], 180)
        artifact = exported['artifacts'][0]
        status, bytes_out = request('GET', '/api/artifacts/' + quote(artifact['artifact_id'], safe=''), runner)
        assert status == 200 and hashlib.sha256(bytes_out).hexdigest() == artifact['content_hash']
        output = root / 'captioned-export.mp4'
        output.write_bytes(bytes_out)
        probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_format',
            '-show_streams', '-of', 'json', str(output)], timeout=40))
        assert {'video', 'audio'} <= {stream['codec_type'] for stream in probe['streams']}
        assert abs(float(probe['format']['duration']) - duration_ms / 1000) < .3, probe
        pcm = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(output), '-vn',
            '-ac', '1', '-ar', '16000', '-f', 's16le', '-'], timeout=50)
        assert len(pcm) > 16000 and any(pcm)
        first = cues[0]['timeline_range']
        caption_time = (milliseconds(first['start']) + milliseconds(first['duration']) / 2) / 1000
        frame = subprocess.check_output(['ffmpeg', '-v', 'error', '-ss', str(caption_time),
            '-i', str(output), '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=40)
        assert len(frame) == 640 * 360 * 3
        bright = sum(1 for offset in range(0, len(frame), 3) if max(frame[offset:offset + 3]) > 190)
        assert bright > 100, f'Caption did not produce visible bright glyphs: {bright}'
        checks.append('Fixed revision render downloads by artifact hash; ffprobe, PCM and visible subtitle glyphs independently decode')

        if english_audio is not None:
            assert english_audio.is_file()
            english = upload(english_audio, 'upload-english-word-fixture')['asset']
            english.update(id='english-words', name='English word timing fixture')
            english_bound = tool('opencut_edit_apply', edit(changed['revision'],
                'bind-english-word-fixture', [{'type': 'asset.bind', 'asset': english}]))
            word_request = {'idempotency_key': 'transcribe-english-words',
                'project_id': 'transcription', 'revision': english_bound['revision'],
                'asset_id': 'english-words', 'language': 'en', 'word_timestamps': True}
            word_job = tool('opencut_analysis_transcribe', word_request)
            assert tool('opencut_analysis_transcribe', word_request)['job_id'] == word_job['job_id']
            word_view = wait(word_job['job_id'])
            assert word_view['result_unavailable'] is None, word_view
            word_result = word_view['extended_result']['data']['transcription']
            assert word_result['word_timing_status'] == 'available', word_result
            assert word_result['model_sha256'] == model_hash
            words = [word for segment in word_result['segments'] for word in segment['words']]
            assert len(words) > 10 and all(milliseconds(word['end']) > milliseconds(word['start'])
                                           for word in words), word_result
            assert word_view['extended_result']['data']['asset_version']['content_hash'] == sha(english_audio)
            current = snapshot()
            checks.append('English real speech produces validated model-derived word intervals; legacy no-word replay keeps the original job')

        stop()
        reopened = call('opencut_project_inspect', {'project_id': 'transcription'})
        assert reopened == current
        inspected = cli(['domain', 'project', 'inspect', '--input', '-'],
            {'project_id': 'transcription'})
        assert inspected == reopened
        checks.append('After host stop, separate CLI reopen retains corrected source-anchored captions')
        report = {'status': 'passed', 'checks': checks, 'evidence': str(root),
            'binary_sha256': binary_hash, 'model_sha256': model_hash,
            'audio_sha256': audio_hash, 'model': model.name, 'audio': str(audio),
            'job_id': submitted['job_id'], 'transcription_sha256': public_hash,
            'segment_count': len(segments), 'caption_count': len(cues),
            'render_job_id': render['job_id'], 'render_sha256': artifact['content_hash'],
            'scope': 'Real local model with Chinese TTS speech; not human interview quality'}
        (root / 'report.json').write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
        print(json.dumps({'status': 'passed', 'checks': len(checks),
                          'report': str(root / 'report.json')}, ensure_ascii=False))
    except Exception as error:
        (root / 'failure.json').write_text(json.dumps({'error': repr(error),
            'completed_checks': checks, 'evidence': str(root)}, indent=2, ensure_ascii=False) + '\n')
        raise
    finally:
        stop()
        (root / 'trace.json').write_text(json.dumps(trace, indent=2, ensure_ascii=False) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--audio', type=Path, required=True)
    parser.add_argument('--artifacts-dir', type=Path)
    parser.add_argument('--model', type=Path, help='Host-local model; defaults to OPENCUT_WHISPER_MODEL')
    parser.add_argument('--english-audio', type=Path,
                        help='Optional real English speech for public word-timing verification')
    args = parser.parse_args()
    model_path = args.model or (Path(os.environ['OPENCUT_WHISPER_MODEL'])
                                if os.environ.get('OPENCUT_WHISPER_MODEL') else None)
    parser.error('provide --model or OPENCUT_WHISPER_MODEL') if model_path is None else None
    verify(args.binary.resolve(), args.audio.resolve(),
           (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-transcription-'))).resolve(),
           model_path.resolve(), args.english_audio.resolve() if args.english_audio else None)
