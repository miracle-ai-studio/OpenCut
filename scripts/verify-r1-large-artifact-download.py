#!/usr/bin/env python3
"""Measure one large public HTTP upload -> render -> streaming download journey.

This is a single-host synthetic sample. It does not establish p95 throughput,
cross-platform temporary-file behavior, or real-footage quality.
"""
import argparse
import hashlib
import http.client
import json
import shutil
import signal
import socket
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, default=Path(__file__).resolve().parents[1] / 'target/debug/opencut')
parser.add_argument('--artifacts-dir', type=Path, required=True)
parser.add_argument('--duration-seconds', type=int, default=30)
parser.add_argument('--source-file', type=Path, help='reuse a previously generated source to skip fixture encoding')
parser.add_argument('--minimum-artifact-bytes', type=int, default=32 * 1024 * 1024)
args = parser.parse_args()

assert args.duration_seconds >= 10
root = args.artifacts_dir.resolve()
root.mkdir(parents=True, exist_ok=True)
run = root / f'large-artifact-{time.time_ns()}'
run.mkdir()
binary = args.binary.resolve()
database = run / 'workspace.sqlite'
source = args.source_file.resolve() if args.source_file else run / 'large-source.mp4'
downloaded = run / 'downloaded-artifact.mp4'
trace = []
process = None
log = (run / 'host.log').open('wb')


def command(argv, timeout=600):
    return subprocess.run(argv, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        while block := stream.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def ticks(value, timebase=24):
    return {'ticks': str(value), 'timebase': timebase}


def span(start, duration):
    return {'start': ticks(start), 'duration': ticks(duration)}


with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
base = f'http://127.0.0.1:{port}'
token_path = run / 'token'


def request(path, body=None):
    headers = {'Authorization': f'Bearer {token}'}
    if body is not None:
        body = json.dumps(body).encode()
        headers['Content-Type'] = 'application/json'
    req = urllib.request.Request(base + path, data=body, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=60) as response:
            data, status = response.read(), response.status
    except urllib.error.HTTPError as error:
        data, status = error.read(), error.code
    envelope = json.loads(data)
    trace.append({'path': path, 'status': status, 'response': envelope})
    return envelope


def upload_stream(path, source_path):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=600)
    connection.putrequest('POST', path)
    connection.putheader('Authorization', f'Bearer {token}')
    connection.putheader('Content-Type', 'application/octet-stream')
    connection.putheader('Content-Length', str(source_path.stat().st_size))
    connection.endheaders()
    with source_path.open('rb') as stream:
        while block := stream.read(1024 * 1024):
            connection.send(block)
    response = connection.getresponse()
    data = response.read()
    result = json.loads(data)
    trace.append({'path': path, 'status': response.status, 'response': result})
    connection.close()
    assert response.status < 300 and result.get('error') is None, result
    return result['data']


def stream_download(path, destination):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=600)
    connection.putrequest('GET', path)
    connection.putheader('Authorization', f'Bearer {token}')
    connection.endheaders()
    response = connection.getresponse()
    assert response.status == 200, (response.status, response.read())
    digest = hashlib.sha256()
    byte_count = 0
    with destination.open('wb') as stream:
        while block := response.read(1024 * 1024):
            stream.write(block)
            digest.update(block)
            byte_count += len(block)
    content_type = response.getheader('Content-Type')
    trace.append({'path': path, 'status': response.status, 'content_length': byte_count,
                  'content_type': content_type, 'sha256': digest.hexdigest()})
    connection.close()
    return byte_count, digest.hexdigest(), content_type


def call(path, body=None):
    deadline = time.monotonic() + 45
    while True:
        value = request(path, body)
        if value.get('error') is None:
            return value['data']
        assert value['error']['code'] == 'DATABASE_BUSY' and time.monotonic() < deadline, value
        time.sleep(0.1)


def start():
    global process, token
    process = subprocess.Popen([str(binary), '--database', str(database), 'serve', '--listen',
                                f'127.0.0.1:{port}', '--token-file', str(token_path)],
                               stdout=log, stderr=log)
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f'OpenCut host exited with {process.returncode}')
        if token_path.exists():
            token = token_path.read_text().strip()
            try:
                request('/v1/capabilities')
                return
            except (OSError, urllib.error.URLError):
                pass
        time.sleep(0.05)
    raise TimeoutError('OpenCut host did not become ready')


try:
    ffmpeg = shutil.which('ffmpeg')
    ffprobe = shutil.which('ffprobe')
    assert ffmpeg and ffprobe
    generation_started = time.monotonic()
    if args.source_file:
        assert source.is_file(), source
    else:
        command([ffmpeg, '-v', 'error', '-y', '-f', 'lavfi', '-i',
                 f'testsrc2=size=1920x1080:rate=24:duration={args.duration_seconds}', '-f', 'lavfi', '-i',
                 f'sine=frequency=440:sample_rate=48000:duration={args.duration_seconds}',
                 '-filter_complex', '[0:v]noise=alls=30:allf=t+u[v]', '-map', '[v]', '-map', '1:a',
                 '-c:v', 'libx264', '-preset', 'ultrafast',
                 '-crf', '14', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-b:a', '128k',
                 '-movflags', '+faststart', str(source)])
    generation_seconds = time.monotonic() - generation_started
    input_duration = float(json.loads(command([ffprobe, '-v', 'error', '-show_entries',
                                               'format=duration', '-of', 'json', str(source)]).stdout)
                           ['format']['duration'])
    assert input_duration >= args.duration_seconds - 0.1, (input_duration, args.duration_seconds)
    source_hash = sha256_file(source)
    source_bytes = source.stat().st_size
    start()
    created = call('/v1/projects', {'project': {'id': 'large-download', 'name': 'Large artifact QA',
                      'sequence_id': 'main', 'width': 1920, 'height': 1080,
                      'fps': {'numerator': 24, 'denominator': 1}}, 'idempotency_key': 'create-large'})
    upload_started = time.monotonic()
    uploaded = upload_stream('/v1/media/upload?name=large-source.mp4', source)
    upload_seconds = time.monotonic() - upload_started
    asset = uploaded['asset']
    assert asset['kind'] == 'video' and asset['metadata']['content_hash'] == source_hash, asset
    duration_frames = args.duration_seconds * 24
    track = {'id': 'large-video', 'name': 'Video', 'kind': 'video', 'enabled': True,
             'muted': False, 'locked': False, 'clips': [], 'params': {}}
    clip = {'id': 'large-clip', 'asset_id': asset['id'],
            'timeline_range': span(0, duration_frames), 'source_range': span(0, duration_frames),
            'params': {}, 'channels': []}
    edit = {'expected_revision': created['revision'], 'idempotency_key': 'bind-large-source',
            'label': 'bind large source', 'operations': [
                {'type': 'asset.bind', 'asset': asset},
                {'type': 'track.create', 'sequence_id': 'main', 'track': track},
                {'type': 'clip.insert', 'sequence_id': 'main', 'track_id': 'large-video', 'clip': clip}]}
    bound = call('/v1/projects/large-download/branches/main/transactions', edit)
    job = call('/v1/jobs', {'kind': 'render.project', 'idempotency_key': 'large-render',
                'input': {'project_id': 'large-download', 'revision': bound['revision'],
                          'sequence_id': 'main', 'container': 'mp4', 'video_codec': 'h264'}})
    render_started = time.monotonic()
    deadline = time.monotonic() + 1800
    while job['state'] not in ('succeeded', 'failed', 'cancelled'):
        assert time.monotonic() < deadline, job
        time.sleep(0.5)
        job = call('/v1/jobs/' + job['job_id'])
    render_seconds = time.monotonic() - render_started
    assert job['state'] == 'succeeded', job
    artifact = job['result']['artifact']
    expected_bytes = int(artifact['size'])
    assert expected_bytes >= args.minimum_artifact_bytes, (
        f'output {expected_bytes} bytes was below the requested large-file threshold '
        f'{args.minimum_artifact_bytes}; increase --duration-seconds')
    download_started = time.monotonic()
    actual_bytes, actual_hash, mime = stream_download('/v1/jobs/' + job['job_id'] + '/artifact', downloaded)
    download_seconds = time.monotonic() - download_started
    assert actual_bytes == expected_bytes == downloaded.stat().st_size, (actual_bytes, expected_bytes)
    assert actual_hash == artifact['content_hash'] == sha256_file(downloaded), (actual_hash, artifact)
    probe = json.loads(command([ffprobe, '-v', 'error', '-show_entries', 'format=duration,size',
                                '-of', 'json', str(downloaded)]).stdout)
    disk_free_after = shutil.disk_usage(run).free
    report = {
        'status': 'passed',
        'scope': 'single-host synthetic HTTP upload, project binding, render Job and streamed verified download; no p95 or cross-platform claim',
        'binary_sha256': sha256_file(binary),
        'ffmpeg_version': command([ffmpeg, '-version'], timeout=30).stdout.decode().splitlines()[0],
        'duration_seconds': args.duration_seconds,
        'minimum_artifact_bytes': args.minimum_artifact_bytes,
        'source': {'size_bytes': source_bytes, 'sha256': source_hash, 'generation_seconds': generation_seconds},
        'artifact': {'job_id': job['job_id'], 'size_bytes': actual_bytes, 'sha256': actual_hash,
                     'mime_type': mime, 'media_duration_seconds': float(probe['format']['duration']),
                     'generation_seconds': render_seconds, 'download_seconds': download_seconds,
                     'download_mib_per_second': round(actual_bytes / max(download_seconds, 1e-9) / 1048576, 3)},
        'upload_seconds': upload_seconds,
        'disk_free_bytes_after_run': disk_free_after,
        'directory': str(run),
    }
    (run / 'report.json').write_text(json.dumps(report, indent=2))
    (run / 'trace.json').write_text(json.dumps(trace, indent=2))
    print(json.dumps(report, indent=2))
except Exception as error:
    failure = {'status': 'failed', 'error': repr(error), 'directory': str(run)}
    (run / 'report.json').write_text(json.dumps(failure, indent=2))
    raise
finally:
    if process and process.poll() is None:
        process.send_signal(signal.SIGINT)
        process.wait(timeout=20)
    log.close()
