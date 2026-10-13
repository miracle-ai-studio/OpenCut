#!/usr/bin/env python3
"""Detached same-origin Web acceptance with synthetic local media and a real browser."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import struct
import sys
import tempfile
import uuid
import zlib
from fractions import Fraction

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/opencut')
    parser.add_argument('--web-root', type=Path, help='Verify a supplied built Web tree, including extracted bundles')
    parser.add_argument('--suite', choices=['full', 'timeline-selection', 'media-fps', 'v4-media-fps', 'v4-file-import', 'v4-file-import-boundaries', 'scene-preview', 'scene-preview-hd', 'scene-preview-performance', 'scene-preview-continuity', 'scene-preview-retirement', 'scene-preview-identity', 'scene-preview-revocation', 'scene-preview-acoustics', 'scene-preview-acoustics-hot', 'scene-preview-acoustics-gap', 'scene-preview-gain', 'scene-preview-gain-paused', 'scene-preview-seek', 'scene-preview-seek-pause', 'scene-preview-pcm', 'scene-preview-pcm-terminal', 'scene-preview-boundaries', 'project-covers', 'classic-recovery', 'feedback', 'v4-editor', 'v4-text', 'v4-text-layout', 'classic-audio', 'v4-performance', 'source-art', 'edit-recovery'], action='append', help='Run full acceptance by default; repeat to share one workspace across targeted flows')
    parser.add_argument('--report', type=Path, help='Write the result outside the temporary workspace')
    parser.add_argument('--keep-workspace', action='store_true', help='Retain a successful synthetic workspace for inspection; failures are always retained')
    parser.add_argument('--reuse-workspace', type=Path, help='Reuse retained synthetic media/database; an existing workspace is never automatically removed')
    parser.add_argument('--profile-native-preview', action='store_true', help='Observe native preview phases in the retained continuity fixture; changes observation runtime identity, not a performance release')
    args = parser.parse_args()
    suites = list(dict.fromkeys(args.suite or ['full']))
    if args.profile_native_preview and (suites != ['scene-preview-continuity'] or not args.reuse_workspace or not args.web_root):
        parser.error('Native phase observation requires continuity alone, retained workspace and explicit built Web tree')
    if args.profile_native_preview:
        for kind in ['AUDIO', 'VISUAL']:
            prefix = 'OPENCUT_CLASSIC_' + kind
            package = os.environ.get(prefix + '_PACKAGE')
            if not package or Path(package).resolve() != ROOT / ('packages/classic-' + kind.lower() + '-renderer'):
                parser.error('Native observation requires explicit repository package: ' + prefix + '_PACKAGE')
            for suffix in ['NODE', 'CHROMIUM']:
                executable = os.environ.get(prefix + '_' + suffix)
                if not executable or not Path(executable).is_absolute() or not os.access(executable, os.X_OK):
                    parser.error('Native observation requires explicit executable: ' + prefix + '_' + suffix)
    if len(suites) > 1 and any(suite in {'full', 'classic-recovery'} for suite in suites):
        parser.error('full and classic-recovery must run alone')
    binary = args.binary.resolve()
    if args.web_root:
        web = args.web_root.resolve()
    else:
        subprocess.run(['npm', 'run', 'build', '--prefix', str(ROOT / 'apps/web')], check=True)
        web = ROOT / 'apps/web/dist'
    workspace = args.reuse_workspace.resolve() if args.reuse_workspace else Path(tempfile.mkdtemp(prefix='opencut-web-'))
    if args.reuse_workspace:
        required = ['workspace.sqlite'] + ([] if suites == ['edit-recovery'] else ['sample.mp4', 'sample.wav'])
        if any(suite in suites for suite in ['v4-editor', 'v4-file-import', 'v4-file-import-boundaries']):
            required += ['sample.png', 'replacement.wav']
        if any(s in suites for s in ['scene-preview-boundaries', 'scene-preview-performance', 'scene-preview-continuity', 'scene-preview-retirement', 'scene-preview-identity', 'scene-preview-revocation', 'scene-preview-acoustics', 'scene-preview-acoustics-hot', 'scene-preview-acoustics-gap', 'scene-preview-gain', 'scene-preview-gain-paused', 'scene-preview-seek', 'scene-preview-seek-pause', 'scene-preview-pcm-terminal']) and not any(s in suites for s in ['scene-preview', 'scene-preview-hd']):
            required += ['scene-preview-fixture.json']
        if any(s in suites for s in ['scene-preview-continuity', 'scene-preview-pcm']):
            required += ['scene-preview-dense-fixture.json']
        if 'v4-file-import-boundaries' in suites:
            required += ['fractional.wav', 'fractional-b.wav']
        if 'v4-media-fps' in suites:
            required += ['fps-vfr.mp4', 'fps-v4-fixture.json']
        if not workspace.is_dir() or any(not (workspace / name).is_file() for name in required):
            parser.error('Reused workspace must contain the existing synthetic fixtures and database')
    service = workspace / 'service'
    report = None
    succeeded = False
    run_env = dict(os.environ)
    profile_directory = None
    if args.profile_native_preview:
        profile_directory = workspace / ('native-preview-profile-' + uuid.uuid4().hex)
        profile_directory.mkdir(mode=0o700)
        probe = ROOT / 'apps/web/scripts/profile-native-preview-preload.mjs'
        run_env['OPENCUT_PREVIEW_PROFILE_DIR'] = str(profile_directory)
        run_env['NODE_OPTIONS'] = run_env.get('NODE_OPTIONS', '') + ' --import=' + json.dumps(str(probe))
        probe_files = {name: hashlib.sha256((ROOT / 'apps/web/scripts' / name).read_bytes()).hexdigest()
                       for name in ['native-preview-timing.mjs', 'native-preview-dispatch.mjs', 'profile-native-preview-preload.mjs',
                                    'scene-preview-continuity.mjs', 'summarize-native-preview-timing.mjs',
                                    'verify-v4-scene-preview.mjs', 'verify-v4-scene-preview-continuity.mjs']}
    def cli(*arguments):
        result = subprocess.run([str(binary), '--json', '--database', str(workspace / 'workspace.sqlite'),
                                 'server', '--directory', str(service), *arguments], capture_output=True, text=True, timeout=60, cwd=workspace, env=run_env)
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        return json.loads(result.stdout)['data']
    try:
        if any(suite in suites for suite in ['scene-preview', 'scene-preview-hd']) and not args.reuse_workspace:
            # Independent authored red/green intervals, primary H264 24fps, no
            # embedded audio. WAV is independent 440Hz mono at 48000Hz.
            subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i',
                            f"color=c=red:size={'1280x720' if 'scene-preview-hd' in suites else '160x90'}:rate=24:duration=8",'-vf',
                            "drawbox=x=0:y=0:w=iw:h=ih:color=lime:t=fill:enable='mod(floor(t),2)',colorspace=iall=bt601-6-625:all=bt709:fast=1",
                            '-c:v','libx264','-bf','0','-pix_fmt','yuv420p',str(workspace/'sample.mp4')],check=True)
            subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i',
                            'sine=frequency=440:sample_rate=48000:duration=8','-c:a','pcm_s16le',str(workspace/'sample.wav')],check=True)
        elif not args.reuse_workspace and suites != ['edit-recovery']:
            subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
                        'testsrc2=size=640x360:rate=24:duration=4', '-f', 'lavfi', '-i',
                        'sine=frequency=440:sample_rate=48000:duration=4', '-c:v', 'libx264',
                        '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', str(workspace / 'sample.mp4')], check=True)
            subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','sine=frequency=220:sample_rate=48000:duration=4',str(workspace / 'sample.wav')],check=True)
        if 'source-art' in suites and not args.reuse_workspace:
            # Small, owned format/duration matrix; never duplicate a worktree.
            def art_png_chunk(kind, data):
                return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
            png = b'\x89PNG\r\n\x1a\n' + art_png_chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 24, 8, 6, 0, 0, 0))
            png += art_png_chunk(b'IDAT', zlib.compress((b'\0' + bytes([255, 0, 0, 255]) * 32) * 24))
            (workspace / 'tiny.png').write_bytes(png + art_png_chunk(b'IEND', b''))
            subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(workspace / 'tiny.png'),
                            '-frames:v', '1', '-q:v', '1', '-pix_fmt', 'yuvj444p', str(workspace / 'tiny.jpg')], check=True)
            subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(workspace / 'sample.mp4'),
                            '-frames:v', '12', '-an', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(workspace / 'short.mp4')], check=True)
            subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
                            'sine=frequency=220:sample_rate=48000:duration=12', '-c:a', 'pcm_s16le', str(workspace / 'long.wav')], check=True)
        if 'v4-media-fps' in suites and not args.reuse_workspace:
            # Independent authored PTS: 120 frames at 60 Hz, then 60 at 12 Hz.
            # Classic observes the first 100 packets (60 Hz), not the whole-file
            # average. Preserve every PTS and disable B-frame reordering.
            subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
                            'testsrc2=size=160x90:rate=60:duration=3', '-vf',
                            "setpts='if(lt(N,120),N/(60*TB),(2+(N-120)/12)/TB)'",
                            '-frames:v', '180', '-fps_mode', 'passthrough',
                            '-enc_time_base', '1:60000', '-c:v', 'libx264', '-bf', '0', '-pix_fmt', 'yuv420p',
                            str(workspace / 'fps-vfr.mp4')], check=True)
            probe = json.loads(subprocess.check_output([
                'ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_packets', '-show_streams',
                '-show_entries', 'packet=pts_time,duration_time:stream=avg_frame_rate', '-of', 'json',
                str(workspace / 'fps-vfr.mp4'),
            ], text=True))
            packets = probe['packets']
            assert len(packets) == 180, 'VFR fixture must retain every authored frame'
            for index, packet in enumerate(packets[:100]):
                assert abs(float(packet['pts_time']) - index / 60) <= 1e-6, 'VFR prefix PTS differs from the independent 60 Hz schedule'
                assert abs(float(packet['duration_time']) - 1 / 60) <= 1e-6, 'VFR prefix packet duration differs from 1/60 s'
            header_avg_frame_rate = probe['streams'][0]['avg_frame_rate']
            assert Fraction(header_avg_frame_rate) != 60, 'VFR fixture must distinguish prefix statistics from the header average'
            (workspace / 'fps-v4-fixture.json').write_text(json.dumps({
                'filename': 'fps-vfr.mp4', 'expected_fps': {'numerator': 60, 'denominator': 1},
                'header_avg_frame_rate': header_avg_frame_rate,
                'packet_limit': 100, 'frame_count': 180,
                'authored_schedule': [{'frames': 120, 'fps': 60}, {'frames': 60, 'fps': 12}],
            })+'\n')
        if any(suite in suites for suite in ['v4-editor', 'v4-file-import', 'v4-file-import-boundaries']) and not args.reuse_workspace:
            # Exact RGBA fixture, independent of FFmpeg's YUV color conversion.
            def png_chunk(kind, data):
                return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
            png = b'\x89PNG\r\n\x1a\n' + png_chunk(b'IHDR', struct.pack('>IIBBBBB', 64, 48, 8, 6, 0, 0, 0))
            png += png_chunk(b'IDAT', zlib.compress((b'\0' + bytes([255, 0, 0, 255]) * 64) * 48))
            (workspace / 'sample.png').write_bytes(png + png_chunk(b'IEND', b''))
            subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i',
                            str(workspace / 'sample.wav'), '-af', 'volume=0.5', '-c:a', 'pcm_s16le',
                            str(workspace / 'replacement.wav')], check=True)
        if 'v4-file-import-boundaries' in suites and not args.reuse_workspace:
            # Exact source extent, independent of probe/frame alignment: 48480
            # PCM frames / 48000 Hz = 1.01 s, not a whole 24fps frame span.
            import math
            import wave
            for name, frequency in [('fractional.wav', 330), ('fractional-b.wav', 550)]:
                with wave.open(str(workspace / name), 'wb') as output:
                    output.setparams((1, 2, 48000, 48480, 'NONE', 'not compressed'))
                    output.writeframes(b''.join(struct.pack('<h', round(12000 * math.sin(2 * math.pi * frequency * index / 48000)))
                                                for index in range(48480)))
        state = cli('start', '--listen', '127.0.0.1:0', '--web-root', str(web))
        assert state['ready'] and state['web_url']
        env = {**run_env, 'OPENCUT_WEB_URL': state['endpoint'], 'OPENCUT_WEB_WORKSPACE': str(workspace), 'OPENCUT_WEB_BINARY': str(binary), 'OPENCUT_WEB_SUITE': ','.join(suites)}
        verifier = {'full': 'verify-browser.mjs', 'classic-recovery': 'verify-classic-ui.mjs'}.get(suites[0], 'verify-suite.mjs')
        result = subprocess.run(['node', str(ROOT / 'apps/web/scripts' / verifier)], env=env, cwd=ROOT, timeout=600)
        report_path = workspace / 'browser-report.json'
        report = json.loads(report_path.read_text())
        report['suites'] = suites
        report['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        report['web_files'] = {str(file.relative_to(web)): hashlib.sha256(file.read_bytes()).hexdigest()
                               for file in sorted(web.rglob('*')) if file.is_file()}
        report['workspace'] = str(workspace)
        if result.returncode or report['status'] != 'passed':
            report['status'] = 'failed'
            raise SystemExit(1)
        succeeded = True
    finally:
        try:
            if service.exists():
                cli('stop')
        except Exception:
            print(f'Service cleanup failed; workspace retained: {workspace}', file=sys.stderr)
            raise
        if succeeded and not args.keep_workspace and not args.reuse_workspace:
            shutil.rmtree(workspace)
        if report is not None:
            if profile_directory:
                try:
                    summary = subprocess.run(['node', str(ROOT / 'apps/web/scripts/summarize-native-preview-timing.mjs'), str(profile_directory), str(workspace / 'browser-report.json')],
                                             capture_output=True, text=True, cwd=ROOT, env=os.environ, timeout=30)
                    observed = json.loads(summary.stdout) if summary.returncode == 0 else {
                        'status': 'incomplete', 'summary_error': summary.stderr[-2048:]}
                except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
                    observed = {'status': 'incomplete', 'summary_error': type(error).__name__}
                report['native_phase_observation'] = observed
                if observed.get('status') == 'incomplete':
                    report['status'] = 'failed'
                    succeeded = False
                report['native_phase_observation']['directory'] = str(profile_directory)
                report['native_phase_observation']['probe_files'] = probe_files
                report['native_phase_observation']['identity_scope'] = 'Original module registry plus explicitly injected observer; not the uninstrumented production execution'
            report['workspace_retained'] = workspace.exists()
            encoded = json.dumps(report, indent=2)+'\n'
            if args.report:
                args.report.resolve().write_text(encoded)
                print(json.dumps({'status': report['status'], 'report': str(args.report.resolve()),
                                  'workspace_retained': report['workspace_retained']}, indent=2))
            else:
                print(encoded)
        elif workspace.exists():
            print(f'Failed workspace retained: {workspace}', file=sys.stderr)
    if not succeeded:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
