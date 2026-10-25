#!/usr/bin/env python3
"""Verify durable silence, shots and waveform jobs through the real CLI/HTTP host."""
import argparse
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
import wave
import math

def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()

def fixture_audio(path):
    with wave.open(str(path), 'wb') as output:
        output.setnchannels(1); output.setsampwidth(2); output.setframerate(16000)
        values = []
        for sample in range(32000):
            t = sample / 16000
            amplitude = math.sin(2 * math.pi * 440 * t) * 12000 if .5 <= t < 1.5 else 0
            values.append(int(amplitude).to_bytes(2, 'little', signed=True))
        output.writeframes(b''.join(values))

def verify(binary, root):
    root.mkdir(parents=True, exist_ok=True)
    db = root / 'workspace.sqlite'
    trace, checks = [], []
    runner, reader, owner = (root / name for name in ('runner.secret', 'reader.secret', 'owner.secret'))
    host = None
    with socket.socket() as reserved:
        reserved.bind(('127.0.0.1', 0)); port = reserved.getsockname()[1]

    def cli(args, payload=None, credential=None, error=None):
        command = [str(binary), '--database', str(db), '--json']
        if credential: command += ['--credential-file', str(credential)]
        command += args
        process = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                                 text=True, capture_output=True, timeout=45)
        response = json.loads(process.stdout)
        trace.append({'cli': args, 'exit': process.returncode, 'response': response})
        if error: assert process.returncode and response['error']['code'] == error, (response, process.stderr)
        else: assert process.returncode == 0 and response['error'] is None, (response, process.stderr)
        return response.get('data')

    def call(name, payload, credential=None, error=None):
        return cli(['call', name, '--input', '-'], payload, credential, error)

    def request_http(method, path, credential, body=None):
        connection = http.client.HTTPConnection('127.0.0.1', port, timeout=45)
        headers = {'Authorization': 'Bearer ' + credential.read_text().strip()}
        if body is not None: headers['Content-Type'] = 'application/json' if method == 'POST' else 'application/octet-stream'
        connection.request(method, path, body, headers)
        response = connection.getresponse(); data = response.read(); status = response.status
        connection.close()
        return status, json.loads(data)

    def tool(name, payload, credential=runner, error=None):
        status, response = request_http('POST', '/api/tools/' + name, credential, json.dumps(payload).encode())
        trace.append({'tool': name, 'status': status, 'response': response})
        if error: assert status >= 400 and response['error']['code'] == error, (status, response)
        else: assert status == 200 and response['error'] is None, (status, response)
        return response.get('data')

    def wait(job_id, expected='succeeded', limit=90):
        deadline = time.monotonic() + limit
        while time.monotonic() < deadline:
            view = tool('opencut_job_view', {'job_id': job_id})
            if view['state'] in ('succeeded', 'failed', 'cancelled'):
                assert view['state'] == expected, view
                return view
            time.sleep(.1)
        raise AssertionError(f'Job {job_id} did not finish')

    def upload(path, key):
        content = path.read_bytes()
        item = tool('opencut_upload_begin', {'project_id':'analysis', 'idempotency_key':key,
            'name':path.name, 'size_bytes':str(len(content)), 'content_hash':hashlib.sha256(content).hexdigest()})
        for offset in range(0, len(content), 4 * 1024 * 1024):
            chunk = content[offset:offset + 4 * 1024 * 1024]
            query = urlencode({'size_bytes':str(len(chunk)), 'content_hash':hashlib.sha256(chunk).hexdigest()})
            status, response = request_http('PUT', f"/api/uploads/{item['upload_id']}/chunks/{offset}?{query}", runner, chunk)
            assert status == 200 and response['error'] is None, response
        accepted = tool('opencut_upload_complete', {'upload_id':item['upload_id'], 'content_hash':item['content_hash']})
        return wait(accepted['job_id'])['extended_result']['data']['asset']

    def stop():
        nonlocal host
        if host and host.poll() is None:
            host.terminate()
            try: host.wait(timeout=10)
            except subprocess.TimeoutExpired: host.kill(); host.wait(timeout=10)
        host = None

    try:
        assert binary.is_file()
        audio, video = root / 'analysis.wav', root / 'cuts.mp4'
        fixture_audio(audio)
        subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i',
            'color=c=red:s=64x64:r=10:d=1','-f','lavfi','-i','color=c=blue:s=64x64:r=10:d=1',
            '-filter_complex','[0:v][1:v]concat=n=2:v=1:a=0','-c:v','mpeg4','-y',str(video)],
            check=True, timeout=45)
        created = call('opencut_project_create', {'project':{'id':'analysis','name':'Measured analysis fixture',
            'sequence_id':'main','width':640,'height':360,'fps':{'numerator':25,'denominator':1}},
            'idempotency_key':'create'})
        for name, actions in [('runner',['read','edit','run_jobs','cancel_jobs','read_artifacts']),('reader',['read'])]:
            cli(['access','set-policy','--input','-'], {'principal_id':name,'expected_revision':'0',
                'enabled':True,'grants':[{'project_id':'analysis','actions':actions}]})
            cli(['access','issue',name,'--output',str(root / f'{name}.secret')])
        # Delay only analysis filter commands so cancellation can be observed while running.
        wrapper = root / 'ffmpeg-analysis-wrapper.py'
        wrapper.write_text('#!/usr/bin/env python3\nimport os,sys,time\n'
            'if "-af" in sys.argv or "-vf" in sys.argv: time.sleep(4)\n'
            'os.execv("/usr/local/bin/ffmpeg", ["ffmpeg", *sys.argv[1:]])\n')
        wrapper.chmod(0o700)
        env = os.environ.copy(); env['OPENCUT_FFMPEG'] = str(wrapper)
        with (root / 'host.log').open('w') as log:
            host = subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}',
                '--token-file',str(owner)], env=env, stdout=log, stderr=log)
        deadline = time.monotonic() + 20
        while True:
            assert host.poll() is None, 'host exited'
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.25): break
            except OSError:
                assert time.monotonic() < deadline, 'host startup timeout'
                time.sleep(.05)
        capabilities = tool('opencut_capabilities', {})
        assert all(any(item['name'] == f'analysis.{name}' for item in capabilities['job_kinds'])
                   for name in ('silence','shots','waveform'))
        checks.append('All three durable analysis job kinds advertised')
        audio_asset, video_asset = upload(audio,'upload-audio'), upload(video,'upload-video')
        audio_asset.update(id='sound', name='Synthetic tone with silence')
        video_asset.update(id='visual', name='Red to blue cut')
        assert audio_asset['kind'] == 'audio' and video_asset['kind'] == 'video'
        edit = {'project_id':'analysis','request':{'expected_revision':created['revision'],
            'idempotency_key':'bind-assets','label':'bind assets',
            'operations':[{'type':'asset.bind','asset':audio_asset},{'type':'asset.bind','asset':video_asset}]}}
        bound = tool('opencut_edit_apply', edit)
        checks.append('Audio and video uploaded, sealed and bound to fixed revision')
        requests = {
            'silence':{'asset_id':'sound','noise_db':-35,'min_duration_ms':200},
            'shots':{'asset_id':'visual','threshold':0.05},
            'waveform':{'asset_id':'sound','buckets':20},
        }
        results = {}
        for name, options in requests.items():
            request = {'idempotency_key':f'analysis-{name}', 'project_id':'analysis',
                'revision':bound['revision'], **options}
            tool(f'opencut_analysis_{name}',request,reader,'NOT_AUTHORIZED')
            submitted = tool(f'opencut_analysis_{name}',request)
            replay = cli(['domain','media',f'analysis.{name}','--input','-'],request,runner)
            assert replay['job_id'] == submitted['job_id']
            view = wait(submitted['job_id'])
            assert view['extended_result']['kind'] == f'analysis.{name}' and view['result_unavailable'] is None
            data = view['extended_result']['data']
            assert data['asset_id'] == options['asset_id'] and data['revision'] == bound['revision']
            expected_hash = digest(audio if name != 'shots' else video)
            assert data['asset_version']['content_hash'] == expected_hash
            assert data['analysis']['provider'] == 'ffmpeg' and data['analysis']['confidence_known'] is False
            results[name] = {'job_id':submitted['job_id'],'data':data}
        silence = results['silence']['data']['analysis']['intervals']
        assert len(silence) >= 2 and int(silence[0]['start']['ticks']) == 0
        assert int(silence[0]['end']['ticks']) in range(400,601)
        assert int(silence[-1]['start']['ticks']) in range(1400,1601)
        boundaries = results['shots']['data']['analysis']['boundaries']
        assert any(900 <= int(item['at']['ticks']) <= 1100 for item in boundaries)
        buckets = results['waveform']['data']['analysis']['buckets']
        assert 1 <= len(buckets) <= 20 and buckets[0]['peak'] < .001
        assert any(item['peak'] > .2 and item['rms'] > .1 for item in buckets)
        assert all(item['start']['timebase'] == 16000 for item in buckets)
        checks.append('All three fixed-version public results expose measured source times and no invented confidence')
        pages = {}
        for name, value in results.items():
            first = tool('opencut_analysis_evidence_query', {'job_id':value['job_id'],'limit':1})
            assert first['analysis_kind'] == f'analysis.{name}'
            assert first['asset_version']['content_hash'] == value['data']['asset_version']['content_hash']
            assert first['provenance']['provider'] == 'ffmpeg'
            pages[name] = first
        assert pages['silence']['evidence'][0]['kind'] == 'silence_interval'
        assert pages['shots']['evidence'][0]['kind'] == 'shot_boundary'
        assert pages['waveform']['evidence'][0]['kind'] == 'waveform_bucket'
        assert pages['waveform']['next_cursor']
        second = tool('opencut_analysis_evidence_query', {'job_id':results['waveform']['job_id'],
            'limit':1,'cursor':pages['waveform']['next_cursor']})
        assert second['evidence'][0]['index'] == pages['waveform']['evidence'][0]['index'] + 1
        tool('opencut_analysis_evidence_query', {'job_id':results['waveform']['job_id'],
            'limit':1,'cursor':pages['waveform']['next_cursor']}, reader, 'INVALID_CURSOR')
        tool('opencut_analysis_evidence_query', {'job_id':results['silence']['job_id'],
            'text':'silence','limit':1}, runner, 'INVALID_ARGUMENT')
        checks.append('Typed evidence pagination covers silence, shots and waveform; cursor binds result and authorized principal')
        cancel_request = {'idempotency_key':'analysis-cancel','project_id':'analysis',
            'revision':bound['revision'],'asset_id':'visual','threshold':0.05}
        cancelling = tool('opencut_analysis_shots',cancel_request)
        running_deadline = time.monotonic() + 15
        while time.monotonic() < running_deadline:
            view = tool('opencut_job_view', {'job_id':cancelling['job_id']})
            if view['state'] == 'running': break
            assert view['state'] == 'queued', view
            time.sleep(.05)
        else: raise AssertionError('Cancel target did not start')
        tool('opencut_job_request_cancel', {'job_id':cancelling['job_id']})
        wait(cancelling['job_id'],'cancelled',30)
        checks.append('Running analysis cancellation reaches cancelled state')
        stop()
        reopened = call('opencut_job_view', {'job_id':results['silence']['job_id']},runner)
        assert reopened['extended_result']['data'] == results['silence']['data']
        checks.append('Result remains publicly readable after host restart')
        report = {'status':'passed','checks':checks,'binary_sha256':digest(binary),'jobs':{k:v['job_id'] for k,v in results.items()}}
        (root / 'report.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({'status':'passed','report':str(root / 'report.json')}))
    except Exception as error:
        (root / 'failure.json').write_text(json.dumps({'error':repr(error),'checks':checks},indent=2)+'\n')
        raise
    finally:
        stop()
        (root / 'trace.json').write_text(json.dumps(trace,indent=2)+'\n')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--artifacts-dir',type=Path)
    args = parser.parse_args()
    verify(args.binary.resolve(), (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-analysis-'))).resolve())
