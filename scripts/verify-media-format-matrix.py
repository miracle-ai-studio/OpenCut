#!/usr/bin/env python3
"""Public HTTP content-sniffing acceptance for FFmpeg-supported containers."""
import argparse
import hashlib
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
args = parser.parse_args()
root = args.artifacts_dir.resolve()
root.mkdir(parents=True, exist_ok=True)
run = root / f'media-formats-{time.time_ns()}'
run.mkdir()
binary = args.binary.resolve()
ffmpeg = shutil.which('ffmpeg')
assert ffmpeg, 'ffmpeg is required for deterministic format fixtures'
assert shutil.which('ffprobe'), 'ffprobe is required by OpenCut'
checks, trace, fixtures = [], [], []
process = None
log = (run / 'host.log').open('wb')


def command(args):
    subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)


def video(name, muxer, codecs):
    output = run / f'{name}.source'
    command([ffmpeg, '-v', 'error', '-y', '-f', 'lavfi', '-i',
             'color=c=red:s=64x64:r=10:d=0.5', *codecs, '-an', '-f', muxer, str(output)])
    return output


def audio(name, muxer, codecs):
    output = run / f'{name}.source'
    command([ffmpeg, '-v', 'error', '-y', '-f', 'lavfi', '-i',
             'sine=frequency=440:sample_rate=44100:duration=0.5', *codecs, '-vn', '-f', muxer, str(output)])
    return output


def image(name, muxer, codecs):
    output = run / f'{name}.source'
    command([ffmpeg, '-v', 'error', '-y', '-f', 'lavfi', '-i',
             'color=c=blue:s=64x64:r=1:d=1', '-frames:v', '1', *codecs, '-f', muxer, str(output)])
    return output


fixture_specs = [
    ('mp4', video('mp4', 'mp4', ['-c:v', 'libx264']), 'video', 'video/mp4', 'mov'),
    ('quicktime_mov', video('quicktime-mov', 'mov', ['-c:v', 'libx264']), 'video', 'video/quicktime', 'mp4'),
    ('quicktime_audio', audio('quicktime-audio', 'mov', ['-c:a', 'aac', '-b:a', '48k']), 'audio', 'audio/quicktime', 'wav'),
    ('three_gpp', video('3gp', '3gp', ['-c:v', 'libx264']), 'video', 'video/3gpp', 'mp4'),
    ('three_gpp_audio', audio('3gp-audio', '3gp', ['-c:a', 'aac', '-b:a', '48k']), 'audio', 'audio/3gpp', 'wav'),
    ('three_gpp2', video('3g2', '3g2', ['-c:v', 'libx264']), 'video', 'video/3gpp2', 'mp4'),
    ('three_gpp2_audio', audio('3g2-audio', '3g2', ['-c:a', 'aac', '-b:a', '48k']), 'audio', 'audio/3gpp2', 'wav'),
    ('matroska', video('matroska', 'matroska', ['-c:v', 'libx264']), 'video', 'application/octet-stream', 'mp4'),
    ('webm', video('webm', 'webm', ['-c:v', 'libvpx', '-deadline', 'realtime', '-cpu-used', '8']), 'video', 'application/octet-stream', 'mp4'),
    ('avi_mjpeg', video('avi-mjpeg', 'avi', ['-c:v', 'mjpeg', '-q:v', '5']), 'video', 'video/x-msvideo', 'jpg'),
    ('mpeg_transport_stream', video('mpeg-ts', 'mpegts', ['-c:v', 'libx264']), 'video', 'video/mp2t', 'mp4'),
    ('mpeg_program_stream', video('mpeg-ps', 'mpeg', ['-c:v', 'mpeg2video']), 'video', 'video/mpeg', 'mp4'),
    ('wav_pcm', audio('wav-pcm', 'wav', ['-c:a', 'pcm_s16le']), 'audio', 'audio/wav', 'mp3'),
    ('mp3', audio('mp3', 'mp3', ['-c:a', 'libmp3lame', '-b:a', '48k']), 'audio', 'audio/mpeg', 'wav'),
    ('flac', audio('flac', 'flac', ['-c:a', 'flac']), 'audio', 'audio/flac', 'wav'),
    ('ogg_opus', audio('ogg-opus', 'ogg', ['-c:a', 'libopus', '-b:a', '32k']), 'audio', 'audio/ogg', 'mp3'),
    ('aac_adts', audio('aac-adts', 'adts', ['-c:a', 'aac', '-b:a', '48k']), 'audio', 'audio/aac', 'mp3'),
    ('m4a_aac', audio('m4a-aac', 'mp4', ['-c:a', 'aac', '-b:a', '48k']), 'audio', 'audio/mp4', 'wav'),
    ('png', image('png', 'image2', ['-c:v', 'png']), 'image', 'image/png', 'mp4'),
    ('jpeg', image('jpeg', 'image2', ['-c:v', 'mjpeg', '-q:v', '5']), 'image', 'image/jpeg', 'mp4'),
    ('bmp', image('bmp', 'image2', ['-c:v', 'bmp']), 'image', 'image/bmp', 'mp4'),
    ('tiff', image('tiff', 'image2', ['-c:v', 'tiff']), 'image', 'image/tiff', 'mp4'),
    ('gif', video('gif', 'gif', ['-vf', 'fps=10']), 'video', 'image/gif', 'mp4'),
]
unknown_brand = run / 'unknown-brand.source'
mp4_bytes = bytearray(next(source.read_bytes() for name, source, *_ in fixture_specs if name == 'mp4'))
ftyp = mp4_bytes.find(b'ftyp')
assert ftyp >= 0 and len(mp4_bytes) >= ftyp + 8
mp4_bytes[ftyp + 4:ftyp + 8] = b'fxyz'
unknown_brand.write_bytes(mp4_bytes)
fixture_specs.insert(1, ('unknown_iso_bmff_brand', unknown_brand, 'video', 'application/octet-stream', 'mp4'))
webp = shutil.which('cwebp')
if webp:
    png_source = next(item[1] for item in fixture_specs if item[0] == 'png')
    webp_source = run / 'webp.source'
    command([webp, str(png_source), '-o', str(webp_source)])
    fixture_specs.append(('webp', webp_source, 'image', 'image/webp', 'mp4'))

with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
base = f'http://127.0.0.1:{port}'
token_path = run / 'token'
process = subprocess.Popen([str(binary), '--database', str(run / 'workspace.sqlite'), 'serve',
                            '--listen', f'127.0.0.1:{port}', '--token-file', str(token_path)],
                           stdout=log, stderr=log)
deadline = time.monotonic() + 20
token = None
while time.monotonic() < deadline:
    if process.poll() is not None:
        raise RuntimeError(f'OpenCut host exited with {process.returncode}')
    if token_path.exists():
        token = token_path.read_text().strip()
        try:
            req = urllib.request.Request(base + '/api/capabilities', headers={'Authorization': f'Bearer {token}'})
            with urllib.request.urlopen(req, timeout=1):
                break
        except (urllib.error.URLError, TimeoutError):
            pass
    time.sleep(0.05)
assert token, 'OpenCut host did not publish its token'


def request(path, body=None, content_type=None):
    headers = {'Authorization': f'Bearer {token}'}
    if content_type:
        headers['Content-Type'] = content_type
    req = urllib.request.Request(base + path, data=body, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return response.status, response.read(), response.headers.get('Content-Type')
    except urllib.error.HTTPError as error:
        return error.code, error.read(), error.headers.get('Content-Type')


try:
    for name, source, expected_kind, expected_mime, misleading_extension in fixture_specs:
        payload = source.read_bytes()
        status, body, _ = request('/api/media/upload?name=' + urllib.parse.quote(f'agent-input.{misleading_extension}'),
                                  payload, 'application/octet-stream')
        envelope = json.loads(body)
        trace.append({'case': name, 'status': status, 'response': envelope})
        assert status == 200 and envelope['error'] is None, (name, envelope)
        asset = envelope['data']['asset']
        actual_kind = asset['kind']
        actual_mime = asset['metadata']['mime_type']
        assert actual_kind == expected_kind, (name, actual_kind, expected_kind, asset)
        assert actual_mime == expected_mime, (name, actual_mime, expected_mime, asset)
        media_status, media_bytes, media_mime = request(asset['metadata']['media_url'])
        assert media_status == 200 and media_bytes == payload, name
        assert media_mime == expected_mime, (name, media_mime, expected_mime)
        fixtures.append({'case': name, 'asset_kind': actual_kind, 'mime_type': actual_mime,
                         'size_bytes': len(payload), 'content_sha256': asset['metadata']['content_hash'],
                         'download_sha256': hashlib.sha256(media_bytes).hexdigest(),
                         'wrong_extension': misleading_extension})
        checks.append(f'{name}: content probe selects {actual_kind}/{actual_mime}; authenticated media read returns identical bytes')

    report = {'status': 'passed', 'scope': 'public HTTP upload and media read; synthetic fixtures; generated format matrix',
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'verifier_script': 'scripts/verify-media-format-matrix.py',
              'verifier_script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'ffmpeg_version': subprocess.check_output([ffmpeg, '-version'], text=True).splitlines()[0],
              'checks_count': len(checks), 'checks': checks, 'fixtures': fixtures, 'run_directory': str(run)}
    (run / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
finally:
    if process.poll() is None:
        process.send_signal(signal.SIGINT)
        process.wait(timeout=15)
    log.close()
    (run / 'trace.json').write_text(json.dumps(trace, indent=2))
