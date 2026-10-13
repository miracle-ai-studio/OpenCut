#!/usr/bin/env python3
"""Independent B2 fixture acceptance; decode only, never copy/generated media.

Frozen before observing the output: 3 s / 90 frames / 320x180 / 30 fps,
H264 yuv420p and AAC stereo 48000 Hz. The existing source is PCM16 mono
48000 Hz, 8 s, independently measured about 440 Hz and RMS 0.088369.
Source-to-output oracle: first 3 s at unit rate, gain 1, mono duplicated to
both channels (explicit pan; no -ac implicit equal-power attenuation).
AAC is lossy: compare 0.1..2.9 s only, no fitted gain, no renderer WAV oracle.
Thresholds are fixed below and must not be tuned using the output.
"""
import argparse
import array
import hashlib
import json
import math
from pathlib import Path
import sqlite3
import subprocess
import sys
import wave

RATE = 48000
SECONDS = 3
EDGE_SECONDS = 0.1
MAX_LAG_SAMPLES = 48          # 1 ms, after FFmpeg honors AAC skip-samples/PTS
SEARCH_LAG_SAMPLES = 96       # diagnose up to 2 ms; reject if best exceeds 1 ms
MIN_SNR_DB = 30.0            # <= 3.162% RMS waveform error; no gain fitting
MAX_REL_RMS_ERROR = 0.01     # 1%, catches unintended attenuation/amplification
MAX_FREQUENCY_ERROR_HZ = 0.2
MAX_STEREO_RMS_MISMATCH = 0.01
DEFAULT_WORKSPACE = Path('/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-web-bsfcr_6o')


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def command(args, maximum=4 * 1024 * 1024):
    completed = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45, check=True)
    require(len(completed.stdout) <= maximum, 'Independent decoder/probe output exceeds fixture bound')
    return completed.stdout


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(65536), b''):
            digest.update(chunk)
    return digest.hexdigest()


def seconds(value):
    return int(value['ticks']) / int(value['timebase'])


def unit_gain(value):
    return ((value.get('unit') == 'decibels' and value.get('value') == 0) or
            (value.get('unit') == 'linear_legacy' and value.get('value') == 1))


def inspect_fixed_source(workspace, evidence, source):
    """Check the authored fixed revision, not latest head or renderer receipts."""
    database = sqlite3.connect((workspace / 'workspace.sqlite').as_uri() + '?mode=ro', uri=True)
    try:
        row = database.execute('SELECT document_json FROM revisions WHERE project_id=? AND revision=?',
                               (evidence['project_id'], evidence['revision'])).fetchone()
        require(row is not None, 'Evidence fixed revision is absent')
        project = json.loads(row[0])
    finally:
        database.close()
    settings = project['settings']['settings']
    require(settings['canvas_size'] == {'width': 320, 'height': 180}, 'Authored canvas differs from frozen fixture')
    require(settings['fps'] == {'numerator': 30, 'denominator': 1}, 'Authored CFR differs from frozen fixture')
    sequence = next(s for s in project['sequences'] if s['id'] == evidence['sequence_id'])
    sources = []
    for track in sequence['tracks']:
        if not track['enabled']:
            continue
        for element in track['elements']:
            if not element['enabled']:
                continue
            content = element['content']
            if content['kind'] == 'video':
                require(content['source_audio_enabled'] is False, 'Video embedded audio must not contribute to PCM oracle')
            if content['kind'] != 'audio':
                continue
            require(not track['muted'] and unit_gain(track['gain']) and not track['channels'], 'Audio track changes independent unit-gain oracle')
            require(not content['audio']['muted'] and content['audio']['stream'] is None and
                    unit_gain(content['audio']['gain']) and not element['channels'], 'Audio element changes independent unit-gain oracle')
            require(content['retime']['rate'] == {'numerator': 1, 'denominator': 1}, 'Audio fixture must have unit retime')
            require(seconds(element['timeline_range']['start']) == 0 and seconds(element['timeline_range']['duration']) == 3,
                    'Audio must cover exactly authored 0..3 s')
            span = element['source_span']
            start = span['trim_start'] if span['profile'] == 'classic_trimmed' else span['range']['start']
            require(seconds(start) == 0, 'Source audio must begin at zero')
            asset = next(a for a in project['assets'] if a['id'] == content['asset_id'])
            require(asset['name'] == 'sample.wav' and asset['version']['content_hash'] == sha256(source),
                    'Independent source WAV does not match the fixed authored asset')
            sources.append({'track_id': track['id'], 'element_id': element['id'], 'asset_id': asset['id'],
                            'source_start_seconds': 0, 'duration_seconds': 3, 'gain': 1, 'rate': 1})
    require(len(sources) == 1, 'Fixture requires exactly one enabled authored WAV voice')
    return sources[0]


def decode(path, ffmpeg, reference=False):
    filters = f'atrim=start=0:end={SECONDS},asetpts=PTS-STARTPTS,aresample={RATE}'
    if reference:
        filters += ',pan=stereo|c0=c0|c1=c0'
    raw = command([ffmpeg, '-v', 'error', '-nostdin', '-i', str(path), '-map', '0:a:0', '-vn',
                   '-af', filters, '-ar', str(RATE), '-ac', '2', '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'])
    values = array.array('f')
    values.frombytes(raw)
    if sys.byteorder != 'little':
        values.byteswap()
    require(len(values) % 2 == 0, 'Decoded stereo PCM is incomplete')
    require(all(math.isfinite(value) for value in values), 'Decoded PCM contains nonfinite values')
    return values[0::2], values[1::2]


def rms(values):
    return math.sqrt(math.fsum(value * value for value in values) / len(values))


def frequency(values):
    # Hysteresis prevents tiny AAC near-zero errors creating extra crossings.
    crossings = []
    armed = False
    threshold = rms(values) * 0.05
    for index in range(1, len(values)):
        if values[index] < -threshold:
            armed = True
        if armed and values[index - 1] < 0 <= values[index]:
            crossings.append(index - 1 + (-values[index - 1]) / (values[index] - values[index - 1]))
            armed = False
    require(len(crossings) > 100, 'Insufficient independent frequency crossings')
    return (len(crossings) - 1) * RATE / (crossings[-1] - crossings[0])


def snr(reference, observed):
    signal = math.fsum(value * value for value in reference)
    noise = math.fsum((a - b) ** 2 for a, b in zip(reference, observed))
    require(signal > 0, 'Independent WAV reference is silent')
    return 10 * math.log10(signal / max(noise, 1e-30))


def measure(reference, observed):
    start = round(EDGE_SECONDS * RATE)
    end = round((SECONDS - EDGE_SECONDS) * RATE)
    require(len(reference) == RATE * SECONDS and len(observed) == RATE * SECONDS,
            'PTS-aware decoded/cropped source and output must both contain exactly 144000 samples per channel')
    # Limited search is diagnostic only. A pure tone cannot establish a unique
    # global delay modulo its period; duration/PTS and fixture source ranges
    # are separately checked. Never scale the output to fit the source.
    offsets = range(-SEARCH_LAG_SAMPLES, SEARCH_LAG_SAMPLES + 1)
    score = lambda lag: math.fsum((reference[i] - observed[i + lag]) ** 2 for i in range(start, end, 4))
    lag = min(offsets, key=lambda value: (score(value), abs(value)))
    ref = reference[start:end]
    actual = observed[start + lag:end + lag]
    ref_rms, actual_rms = rms(ref), rms(actual)
    ref_hz, actual_hz = frequency(ref), frequency(actual)
    return {'lag_samples': lag, 'lag_ms': lag * 1000 / RATE, 'source_rms': ref_rms, 'output_rms': actual_rms,
            'relative_rms_error': abs(actual_rms / ref_rms - 1),
            'snr_db_aligned': snr(ref, actual), 'snr_db_zero_lag': snr(ref, observed[start:end]),
            'source_frequency_hz': ref_hz, 'output_frequency_hz': actual_hz,
            'frequency_error_hz': abs(actual_hz - ref_hz), 'output_peak': max(abs(value) for value in actual)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('workspace', type=Path, nargs='?', default=DEFAULT_WORKSPACE)
    parser.add_argument('--ffmpeg', default='ffmpeg')
    parser.add_argument('--ffprobe', default='ffprobe')
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    evidence = json.loads((workspace / 'b2-video-evidence.json').read_text())
    output, source = workspace / 'b2-mixed.mp4', workspace / 'sample.wav'
    require(evidence['job']['state'] == 'succeeded', 'B2 evidence job did not succeed')
    require(Path(evidence['output']).resolve() == output.resolve(), 'Evidence points at another downloaded artifact')
    require(sha256(output) == evidence['sha256'], 'Downloaded MP4 differs from browser evidence')
    authored = inspect_fixed_source(workspace, evidence, source)
    with wave.open(str(source), 'rb') as wav:
        source_format = {'sample_rate': wav.getframerate(), 'channels': wav.getnchannels(),
                         'sample_width_bytes': wav.getsampwidth(), 'frames': wav.getnframes()}
    require(source_format == {'sample_rate': 48000, 'channels': 1, 'sample_width_bytes': 2, 'frames': 384000},
            'Independent WAV changed from inspected PCM16 mono 48kHz 8s fixture')
    probe = json.loads(command([args.ffprobe, '-v', 'error', '-count_frames', '-show_streams', '-of', 'json', str(output)]))
    streams = probe['streams']
    require(len(streams) == 2, 'MP4 requires exactly one video and one audio stream')
    video = next(s for s in streams if s['codec_type'] == 'video')
    audio = next(s for s in streams if s['codec_type'] == 'audio')
    require(video['codec_name'] == 'h264' and video['pix_fmt'] == 'yuv420p' and video['width'] == 320 and video['height'] == 180,
            'Observed video encoding/dimensions differ from frozen profile')
    require(video['avg_frame_rate'] == '30/1' and video['r_frame_rate'] == '30/1' and int(video['nb_read_frames']) == 90,
            'Observed video is not 90-frame CFR30')
    require(abs(float(video['duration']) - 3) <= .002, 'Observed video duration differs from 90/30')
    require(audio['codec_name'] == 'aac' and int(audio['sample_rate']) == RATE and audio['channels'] == 2,
            'Observed audio is not AAC48k stereo')
    require(abs(float(audio['duration']) - 3) <= .05, 'AAC stream duration exceeds frozen framing tolerance')
    reference = decode(source, args.ffmpeg, reference=True)
    observed = decode(output, args.ffmpeg)
    measurements = [measure(reference[c], observed[c]) for c in range(2)]
    stereo_mismatch = abs(measurements[0]['output_rms'] / measurements[1]['output_rms'] - 1)
    thresholds = {'analyzed_seconds': [.1, 2.9], 'min_snr_db': MIN_SNR_DB, 'max_relative_rms_error': MAX_REL_RMS_ERROR,
                  'max_frequency_error_hz': MAX_FREQUENCY_ERROR_HZ, 'max_abs_lag_samples': MAX_LAG_SAMPLES,
                  'max_stereo_relative_rms_mismatch': MAX_STEREO_RMS_MISMATCH,
                  'gain_fitting': False, 'aac_duration_tolerance_seconds': .05}
    failures = []
    for channel, result in enumerate(measurements):
        for accepted, reason in [(abs(result['lag_samples']) <= MAX_LAG_SAMPLES, 'lag'),
                                 (result['snr_db_aligned'] >= MIN_SNR_DB, 'SNR'),
                                 (result['relative_rms_error'] <= MAX_REL_RMS_ERROR, 'RMS'),
                                 (result['frequency_error_hz'] <= MAX_FREQUENCY_ERROR_HZ, 'frequency'),
                                 (abs(result['source_frequency_hz'] - 440) <= .2, 'independently inspected source frequency')]:
            if not accepted:
                failures.append(f'channel {channel}: {reason}')
    if stereo_mismatch > MAX_STEREO_RMS_MISMATCH:
        failures.append('stereo RMS mismatch')
    report = {'passed': not failures, 'workspace': str(workspace), 'revision': evidence['revision'],
              'source_sha256': sha256(source), 'output_sha256': evidence['sha256'], 'source_format': source_format,
              'authored_voice': authored, 'video': {key: video[key] for key in ['codec_name', 'pix_fmt', 'width', 'height', 'avg_frame_rate', 'r_frame_rate', 'nb_read_frames', 'duration']},
              'audio': {key: audio[key] for key in ['codec_name', 'sample_rate', 'channels', 'duration']},
              'thresholds_declared_before_output': thresholds, 'channels': measurements,
              'stereo_relative_rms_mismatch': stereo_mismatch, 'failures': failures,
              'scope': 'Independent source-WAV waveform fidelity after lossy AAC encoding; excludes first/last 100ms, DAC output and general signals/retime/mastering matrices. Pure tone lag is only locally constrained, not a unique global alignment proof. AAC is not bit-exact PCM.'}
    print(json.dumps(report, indent=2, allow_nan=False))
    return 0 if report['passed'] else 1

if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as error:
        print(json.dumps({'passed': False, 'error': str(error)}))
        sys.exit(1)
