#!/usr/bin/env python3
"""One retained300s48k mono PCM source with the original VFR video.

This is a genuinely different source format, not a duplicate original fixture.
Independent PCM is read directly from its authored integer samples. A managed
owned service performs one import and one30-frame H264/AAC output, serially.
"""
import argparse
from array import array
import copy
import json
import math
from pathlib import Path
import subprocess
import time
import wave
from acceptance import Harness, at, sha

RATE, DURATION = 48000, 300


def verify(args):
    h = Harness(args.binary, args.workspace, 'agent-long-mono-v4')
    h.runtime = json.loads(args.native_runtime.read_text())
    original = json.loads(args.source_report.read_text())
    source_before = h.inspect(original['project_id'])
    source = h.workspace / 'agent-long-mono-48k.wav'
    def generate():
        if not source.exists():
            pending = source.with_suffix('.pending')
            patterns = [array('h', (round(.08 * 32767 * math.sin(2 * math.pi * hz * i / RATE))
                                    for i in range(RATE))).tobytes() for hz in (440, 880)]
            with wave.open(str(pending), 'wb') as output:
                output.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
                for second in range(DURATION):
                    output.writeframesraw(patterns[second % 2])
            pending.replace(source)
        with wave.open(str(source), 'rb') as observed:
            assert (observed.getnchannels(), observed.getsampwidth(), observed.getframerate(), observed.getnframes()) == (1, 2, RATE, RATE * DURATION)
        return {'sha256': sha(source.read_bytes()), 'size': source.stat().st_size, 'duration_seconds': DURATION,
            'sample_rate': RATE, 'channels': 1, 'sample_format': 'signed16-little-endian',
            'expected': 'Independently authored0.08 amplitude, round-to-int16; even seconds440Hz, odd880Hz; phase resets at integer seconds'}
    profile = h.stage('mono-source', generate)
    assert sha(source.read_bytes()) == profile['sha256']
    project = 'agent-long-mono-v4-acceptance'
    copied = h.stage('copy-vfr-project', lambda: h.request('copy-vfr-project', 'opencut_project_copy', {
        'source_project_id': original['project_id'], 'source_revision': original['revision'],
        'project_id': project, 'name': 'Original late VFR video with separate48k mono PCM source',
        'idempotency_key': h.key('copy-vfr-project')}))
    def managed_job(name, tool, request, timeout=600, on_running=None):
        accepted = h.request(name, tool, request)
        assert h.call(tool, request)['job_id'] == accepted['job_id']
        deadline = time.monotonic() + timeout
        while True:
            view = h.call('opencut_job_view', {'job_id': accepted['job_id']})
            if view['state'] in ('succeeded', 'failed', 'cancelled'):
                assert view['state'] == 'succeeded' and view['result_unavailable'] is None, view
                return view
            assert time.monotonic() < deadline, {'observation_timeout': accepted['job_id'], 'state': view['state']}
            time.sleep(.5)
    h.job = managed_job
    status = h.cli(['server', '--directory', str(h.workspace / 'service'), 'status'])
    assert status['ready'] and sha(Path(status['executable']).read_bytes()) == h.candidate
    media = h.stage('import-mono-source', lambda: h.import_media('mono-source', project, source))
    assert media['asset']['version']['content_hash'] == profile['sha256']
    def configure():
        before = h.inspect(project, copied['revision'])
        scene = next(s for s in before['project']['sequences'] if s['id'] == 'long-scene')
        placement = h.request('place-plan', 'opencut_media_import_plan', {
            'project_id': project, 'expected_revision': copied['revision'], 'sequence_id': 'long-scene',
            'idempotency_key': h.key('place'), 'label': 'Place two clock-matched mono ranges on one independent audio track',
            'ripple': False, 'bindings': [{'media_id': media['media_id'], 'version': media['asset']['version'],
                'asset_id': 'long-mono-source', 'name': source.name, 'mode': 'bind'}],
            'new_tracks': [{'id': 'long-mono-audio', 'name': '48k mono duplicated to stereo', 'kind': 'audio'}],
            'placements': [{'element_id': identifier, 'name': identifier, 'track_id': 'long-mono-audio',
                'asset_id': 'long-mono-source', 'start': at(start), 'content': {'kind': 'audio',
                    'timing': {'kind': 'source_range', 'range': {'start': at(trim), 'duration': at(duration)},
                               'rate': {'numerator': rate, 'denominator': 4 if rate == 5 else 1}, 'maintain_pitch': False}}}
                for identifier, start, trim, duration, rate in [('long-mono-left', 0, 294, 2, 1), ('long-mono-right', 2, 296, 4, 5)]]})
        disable = []
        for track in scene['tracks']:
            for element in track['elements']:
                if element['content']['kind'] == 'video':
                    changed = copy.deepcopy(element); changed['content']['source_audio_enabled'] = False
                    disable.append({'type': 'element_put', 'sequence_id': 'long-scene', 'track_id': track['id'], 'element': changed})
        planned = h.request('configure-plan', 'opencut_edit_agent_plan', {
            'project_id': project, 'expected_revision': copied['revision'], 'idempotency_key': h.key('configure'),
            'label': 'Atomic original silent VFR visuals plus independent clock-matched mono audio',
            'steps': [{'kind': 'operations', 'operations': placement['transaction']['operations']},
                      {'kind': 'operations', 'operations': disable}]})
        return h.apply('configure', planned['transaction'])
    configured = h.stage('configure', configure)
    scope = {'project_id': project, 'revision': configured['revision'], 'sequence_id': 'long-scene'}
    # Direct authored integer PCM is the independent oracle; do not invoke a
    # product decoder, source clock, resampler, gain compiler or output reader.
    with wave.open(str(source), 'rb') as reader:
        reader.setpos(298 * RATE); tail = array('h'); tail.frombytes(reader.readframes(2 * RATE))
    demuxer = Path(__file__).resolve().parents[1] / 'packages/classic-audio-renderer/node_modules/mediabunny/src/wave/wave-demuxer.ts'
    assert 'const PACKET_SIZE_IN_FRAMES = 2048;' in demuxer.read_text(), 'Requalify the pinned source packet profile if it changes'
    expected, expected_pts = array('f'), array('f')
    for frame in range(RATE):
        numerator = 36000 * 4 + frame * 5
        lower, remainder = divmod(numerator, 4)
        value = (tail[lower] * (4 - remainder) + tail[min(lower + 1, len(tail) - 1)] * remainder) / (4 * 32768)
        expected.extend((value, value))
        absolute = 298 * RATE + lower
        upper = min(absolute + 1, (absolute // 2048 + 1) * 2048 - 1, DURATION * RATE - 1) - 298 * RATE
        pts_value = (tail[lower] * (4 - remainder) + tail[upper] * remainder) / (4 * 32768)
        expected_pts.extend((pts_value, pts_value))
    reference_path = h.workspace / 'agent-long-mono-48k-reference.f32'
    if reference_path.exists():
        assert reference_path.read_bytes() == expected.tobytes()
    else:
        reference_path.write_bytes(expected.tobytes())
    reference = {'status': 'reference_generated', 'role': 'analytic_authored_pcm_oracle', 'source_sha256': profile['sha256'],
        'pcm_path': str(reference_path), 'pcm_sha256': sha(expected.tobytes()), 'frames': RATE, 'channels': 2,
        'sample_rate': RATE, 'pitch_mode': 'unprepared-linear', 'threshold': 1e-7, 'decoder_profile': 'authored-pcm16/32768-exact5/4-two-tap',
        'scope': 'Original48k mono integer samples; exact authored source298.75..300s at5/4, duplicated to stereo. No application/compiler/decoder called.'}
    ref_report = args.report.with_name('long-mono-reference.json'); ref_report.write_text(json.dumps(reference, indent=2) + '\n')
    pts_file = h.workspace / 'agent-long-mono-48k-pts-reference.f32'
    if pts_file.exists():
        assert pts_file.read_bytes() == expected_pts.tobytes()
    else:
        pts_file.write_bytes(expected_pts.tobytes())
    pts_reference = {**reference, 'pcm_path': str(pts_file), 'pcm_sha256': sha(expected_pts.tobytes()),
        'pitch_mode': 'preserve-pts',
        'decoder_profile': 'authored-pcm16/32768-native-pts-in-packet-upper-clamp',
        'packet_frames': 2048, 'demuxer_source_sha256': sha(demuxer.read_bytes()),
        'scope': 'Same independently authored PCM and exact source clock; original native-PTS profile clamps its upper tap inside each pinned2048-frame Wave packet. Separate concat export reference retained.'}
    pts_report = args.report.with_name('long-mono-pts-reference.json'); pts_report.write_text(json.dumps(pts_reference, indent=2) + '\n')
    fault_reference = {'project_id': project, 'revision': configured['revision'], 'source': profile,
                       'legacy_pcm': {'reference_report': str(ref_report)}}
    args.report.with_name('long-mono-fault-reference.json').write_text(json.dumps(fault_reference, indent=2) + '\n')
    browser_ref = {**original, **scope, 'status': 'prepared_reference', 'binary_sha256': None,
        'output_job_id': None, 'native_runtime': h.runtime, 'source': profile, 'video_source': original['source'],
        'legacy_pcm': {'reference_report': str(pts_report)}, 'pcm_threshold': 1e-7, 'preserve_pts_hz': [440, 440],
        'preserve_pts_other_hz': [880, 880], 'scope': 'Original300s VFR video with its audio disabled; new independently authored300s48k mono PCM, two source clock-matched clips; analytic PCM and original source pixels'}
    args.report.with_name('long-mono-browser-reference.json').write_text(json.dumps(browser_ref, indent=2) + '\n')
    if args.prepare_only:
        print(json.dumps({'status': 'references_prepared', **scope, 'export_reference': str(ref_report),
                          'native_pts_reference': str(pts_report)}))
        return
    name = 'mono-range-' + h.candidate[:12] + '-' + sha(json.dumps(h.runtime, sort_keys=True).encode())[:12]
    retained = [key for key in h.state['requests'] if key.startswith('mono-range-')]
    assert len(retained) <= 1, 'One original request; explicit terminal-failure replacement is separate'
    if retained:
        name = retained[0]
    submission = {'kind': 'video_range', 'request': {
        **scope, 'idempotency_key': h.key(name), 'options': {'format': 'mp4', 'quality': 'high', 'include_audio': True},
        'range': {'start_frame': '126', 'frame_count': '30'}}}
    replacement = None
    if args.render_request:
        external = json.loads(args.render_request.read_text())
        assert external['tool'] == 'opencut_render_submit'
        replacement = external.get('replaces_failed_job')
        assert replacement, 'Declare the terminal failed Job being replaced'
        failed = h.call('opencut_job_view', {'job_id': replacement})
        assert failed['state'] == 'failed' and failed['target'] == scope
        expected_request = copy.deepcopy(submission)
        expected_request['request']['idempotency_key'] = external['input']['request']['idempotency_key']
        assert external['input'] == expected_request, 'Only the explicit replacement key can change'
        submission = external['input']
        name = 'explicit-render-' + sha(json.dumps(submission, sort_keys=True).encode())[:16]
    job = h.stage(name, lambda: h.job(name, 'opencut_render_submit', submission))
    path = h.artifact(job)
    raw = array('f'); raw.frombytes(subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path.parent / 'video-audio.wav'),
        '-f', 'f32le', '-ac', '2', '-ar', str(RATE), 'pipe:1'], timeout=30))
    assert len(raw) == len(expected) == RATE * 2
    maximum = max(abs(a-b) for a,b in zip(raw, expected)); assert maximum <= 1e-7, maximum
    assert all(raw[i] == raw[i+1] for i in range(0, len(raw), 2)), 'Mono must duplicate equally into both stereo output channels'
    probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_format', '-show_streams', '-of', 'json', str(path)]))
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video')
    audio = next(s for s in probe['streams'] if s['codec_type'] == 'audio')
    assert video['codec_name'] == 'h264' and (video['width'], video['height'], video['nb_frames']) == (320,180,'30')
    assert audio['codec_name'] == 'aac' and audio['channels'] == 2 and audio['sample_rate'] == str(RATE)
    assert float(video['start_time']) == float(audio['start_time']) == 0
    rgb = subprocess.check_output(['ffmpeg','-v','error','-i',str(path),'-an','-f','rawvideo','-pix_fmt','rgb24','pipe:1'],timeout=30)
    assert len(rgb) == 30 * 320 * 180 * 3
    errors = []
    for frame in range(30):
        pixel = rgb[(frame * 320 * 180 + 90 * 320 + 160) * 3:][:3]
        expected_rgb = original['independent']['expected_rgb'][int(frame >= 6)]
        errors.append(max(abs(a-b) for a,b in zip(pixel,expected_rgb)))
    assert max(errors) <= 8, max(errors)
    encoded_pcm = array('f'); encoded_pcm.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(path),'-vn','-f','f32le','-ac','2','-ar',str(RATE),'pipe:1'],timeout=30))
    assert len(encoded_pcm) >= RATE * 2
    def power(center, frequency):
        start = round(center * RATE) - 480
        return abs(sum(encoded_pcm[2 * (start+i)] * complex(math.cos(2*math.pi*frequency*i/RATE),math.sin(2*math.pi*frequency*i/RATE)) for i in range(960)))
    ratios = [power(.1,550)/max(1e-20,power(.1,1100)), power(.4,1100)/max(1e-20,power(.4,550))]
    assert min(ratios)>8,ratios
    crossings = [i/200 for i in range(20,61) if power(i/200,1100)>power(i/200,550)]
    audio_switch = min(crossings); assert abs(audio_switch-.2)<=1/30,audio_switch
    assert h.inspect(original['project_id']) == source_before
    execution = job['extended_result']['data']['output']['execution']
    assert execution['audio']['module_sha256'] == h.runtime['audio_module_sha256']
    producer = execution['audio']['host_sha256']
    browser_ref.update(status='passed_scoped', binary_sha256=producer, output_job_id=job['job_id'])
    browser_ref.pop('stages', None)
    args.report.with_name('long-mono-browser-reference.json').write_text(json.dumps(browser_ref, indent=2) + '\n')
    report = {'status':'passed_scoped','binary_sha256':producer,'inspection_binary_sha256':h.candidate,'native_runtime':h.runtime,'workspace':str(h.workspace),
        **scope,'source':profile,'video_source_sha256':original['source']['sha256'],'job_id':job['job_id'],
        'output_sha256':job['artifacts'][0]['content_hash'],'replaces_failed_job':replacement,'reference_report':str(ref_report),'pts_reference_report':str(pts_report),'pcm_frames':RATE,
        'analytic_native_pts_export_max_difference':max(abs(a-b) for a,b in zip(expected_pts,expected)),
        'pcm_max_absolute_error':maximum,'pcm_threshold':1e-7,'mono_duplicated_equal':True,'decoded_video_frames':30,'max_pixel_error':max(errors),
        'pixel_threshold':8,'encoded_tone_dominance':ratios,'encoded_audio_switch_seconds':audio_switch,
        'video_switch_seconds':.2,'av_event_difference_seconds':abs(audio_switch-.2),'av_event_threshold_seconds':1/30,
        'encoded_audio_frames_decoded':len(encoded_pcm)//2,'timestamp_av_offset_seconds':0,
        'scope':'One new format fixture/import/30-frame H264+AAC range only; original VFR visual source reused. Independent raw PCM, equal mono duplication, original YUV-derived video colors and encoded-audio frequency switch. Realtime verified separately; not whole format/browser/performance/physical matrix.'}
    args.report.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ['status','job_id','pcm_max_absolute_error','max_pixel_error','av_event_difference_seconds']}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['binary','workspace','source-report','native-runtime','report']:
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--render-request', type=Path)
    verify(parser.parse_args())
