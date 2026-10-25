#!/usr/bin/env python3
"""Actual relocated runtime on existing CAS/project; no fixture or full-suite replay."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import subprocess
from acceptance import Harness, at, sha


def compare_color_grid_png(png, reference):
    raw = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(png), '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgba', '-'])
    expected = reference.read_bytes()
    assert len(raw) == len(expected) == 540 * 300 * 4
    rows = []
    for index in range(18):
        union = intersection = foreground = error = 0
        x0, y0 = (index % 3) * 180, (index // 3) * 50
        for y in range(y0, y0 + 50):
            for x in range(x0, x0 + 180):
                p = (y * 540 + x) * 4
                actual_fg = max(abs(raw[p + c] - 48) for c in range(3)) > 16
                expected_fg = max(abs(expected[p + c] - 48) for c in range(3)) > 16
                union += actual_fg or expected_fg
                intersection += actual_fg and expected_fg
                foreground += expected_fg
                error += sum(abs(raw[p + c] - expected[p + c]) for c in range(3))
        rows.append({'region': index, 'glyph_iou': intersection / max(1, union), 'mean_rgb_error': error / (180 * 50 * 3), 'foreground': foreground})
    assert all(r['foreground'] > 20 and r['glyph_iou'] > 0.99 and r['mean_rgb_error'] <= 2 for r in rows), rows
    return rows


def verify(a):
    root = a.package.resolve()
    runtime_file = root / 'bin/libexec/runtime-bundle.json'
    runtime = json.loads(runtime_file.read_text())
    for key in list(os.environ):
        if key.startswith(('OPENCUT_CLASSIC_', 'OPENCUT_SOURCE_ART_', 'OPENCUT_FONT_DERIVE_', 'OPENCUT_FONT_COLOR_STATIC_')):
            del os.environ[key]
    os.environ['OPENCUT_FONT_DERIVE_CACHE'] = str(a.cache.resolve())
    os.environ['OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD'] = '1'
    h = Harness(root / 'bin/opencut', a.workspace, 'agent-relocated-runtime-v1')
    h.runtime = {'manifest_sha256': sha(runtime_file.read_bytes()),
        'browser_version': runtime['components']['chromium']['version'],
        'browser_sha256': sha((root / 'bin/libexec' / runtime['components']['chromium']['entry']).read_bytes()),
        'package_root': str(root)}
    assert h.state.setdefault('runtime', h.runtime) == h.runtime
    project = 'fb92adbb-b999-414e-b351-35f012b8e7b8'

    def sql(query, values=()):
        with sqlite3.connect(f'file:{h.workspace / "workspace.sqlite"}?mode=ro', uri=True) as c:
            return c.execute(query, values).fetchall()

    def private(revision):
        return json.loads(sql('SELECT document_json FROM revisions WHERE revision=?', (revision,))[0][0])

    def jobs():
        return dict(sql('SELECT state,COUNT(*) FROM jobs GROUP BY state'))

    def baseline():
        counts = jobs()
        assert not counts.get('queued') and not counts.get('running'), 'Observe existing live work before proceeding'
        snapshot = h.inspect(project)
        return {'snapshot': snapshot, 'private': private(snapshot['revision']), 'jobs': counts}

    base = h.stage('baseline', baseline)
    a.report.parent.mkdir(parents=True, exist_ok=True)

    def source_frame():
        asset = next(x for x in base['snapshot']['project']['assets'] if x['kind'] == 'video')
        source = h.workspace / 'asset-store' / asset['version']['content_hash'][:2] / asset['version']['content_hash']
        assert sha(source.read_bytes()) == asset['version']['content_hash']
        probe = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_streams', '-of', 'json', str(source)]))
        stream = next(s for s in probe['streams'] if s['codec_type'] == 'video')
        assert stream['codec_name'] == 'h264' and stream['avg_frame_rate'] == '24/1' and float(stream.get('start_time', '0')) == 0
        request = {'project_id': project, 'revision': base['snapshot']['revision'], 'asset_id': asset['id'],
            'request_id': h.key('actual-h264-source-frame'), 'time': at('0.4375')}
        result = h.call('opencut_source_art_frame', request)
        assert result['identity']['browser_version'] == h.runtime['browser_version']
        assert result['identity']['browser_sha256'] == h.runtime['browser_sha256']
        assert result['version'] == asset['version']
        data = result['data']
        assert data['kind'] == 'thumbnail_ready' and data['observation']['kind'] == 'video'
        observed = data['observation']['actual_timestamp_seconds']
        assert data['observation']['requested_seconds'] == 0.4375
        duration = data['observation']['frame_duration_seconds']
        assert observed <= 0.4375 < observed + duration, 'Actual decoded source sample must cover the requested source time'
        index = round(observed * 24)
        assert abs(observed - index / 24) < 0.000002
        jpeg = bytes.fromhex(data['jpeg_bytes_hex'])
        assert sha(jpeg) == data['content_hash']
        file = a.report.parent / 'actual-h264-source.jpg'
        file.write_bytes(jpeg)
        actual = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(file), '-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'])
        expected = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(source), '-an', '-vf', f'select=eq(n\\,{index})', '-frames:v', '1', '-fps_mode', 'passthrough', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'])
        assert len(actual) == len(expected) == data['width'] * data['height'] * 3
        error = sum(abs(x - y) for x, y in zip(actual, expected)) / len(actual)
        assert error <= 5, {'jpeg_source_rgb_mae': error, 'unchanged_existing_media_threshold': 5}
        assert h.inspect(project) == base['snapshot'] and jobs() == base['jobs']
        return {'request': request, 'identity': result['identity'], 'version': result['version'],
            'observation': data['observation'], 'width': data['width'], 'height': data['height'],
            'jpeg': str(file), 'jpeg_sha256': sha(jpeg), 'independent_source_frame_index': index,
            'independent_rgb_mae': error, 'threshold': 5, 'project_unchanged': True}

    def cold_font():
        pins = json.loads((Path(__file__).resolve().parents[1] / 'crates/editor-application/src/font_color_static/catalog_pins.json').read_text())
        pin = next(p for p in pins if p['face']['id'] == 'google-sixtyfourconvergence-regular-color-static-v1')
        request = {'project_id': project, 'expected_revision': base['snapshot']['revision'],
            'idempotency_key': h.key('cold-font'), 'catalog_id': pin['face']['id'],
            'font_id': 'relocated-runtime-color-font', 'required_text': 'HAMBURG 39'}
        cache = a.cache / 'derived' / pin['face']['content_hash']
        held = a.report.parent / 'retained-original-color-cache'
        output = a.report.parent / 'actual-bundled-color-output'
        if 'cold_receipt' not in h.state:
            assert cache.is_file() and not held.exists()
            assert sha(cache.read_bytes()) == pin['face']['content_hash']
            inode = cache.stat().st_ino
            cache.rename(held)
            try:
                receipt = h.request('cold-font', 'opencut_font_import', request)
                h.state['cold_receipt'] = receipt
                assert cache.is_file() and sha(cache.read_bytes()) == pin['face']['content_hash']
                cache.rename(output)
                h.state['cold_output'] = {'path': str(output), 'sha256': sha(output.read_bytes()), 'original_inode': inode}
                h.save()
            finally:
                if cache.exists():
                    unexpected = a.report.parent / 'retained-unexpected-color-output'
                    assert not unexpected.exists(), 'Keep the original unexpected output for diagnosis'
                    cache.rename(unexpected)
                held.rename(cache)
                assert cache.stat().st_ino == inode
        receipt = h.state['cold_receipt']
        assert h.call('opencut_font_import', request) == receipt
        proof = h.call('opencut_font_derived_proof', {'project_id': project, 'revision': receipt['revision'], 'font_id': request['font_id']})
        assert proof['schema_version'] == 4 and proof['source'] == pin
        assert jobs() == base['jobs']
        return {'request': request, 'receipt': receipt, 'proof_schema': 4, 'output': h.state['cold_output'],
            'scope': 'Bundled Python/worker selected without development overrides, one exact cached raw source, original120s/2GiB bounds; no download or new source copy.'}

    def restore_font():
        imported = h.state['stages']['cold-bundled-color']['result']
        request = h.state['requests'].get('restore-font', {}).get('input') or {
            'project_id': project, 'expected_revision': h.inspect(project)['revision'],
            'target_transaction_id': imported['receipt']['transaction_id'], 'idempotency_key': h.key('restore-font')}
        assert not h.call('opencut_edit_undo_plan', request)['conflicts']
        receipt = h.request('restore-font', 'opencut_edit_undo_apply', request)
        restored = h.inspect(project)
        assert restored['project'] == base['snapshot']['project'] and private(restored['revision']) == base['private']
        return {'revision': restored['revision'], 'public_and_private_graph_equal': True, 'receipt': receipt}

    def frame():
        prior = json.loads((h.workspace / 'agent-font-color-static-v1-checkpoint.json').read_text())
        fixed = prior['stages']['grid-combined-scene']
        request = {'kind': 'frame', 'request': {'project_id': project, 'revision': fixed['revision'],
            'sequence_id': fixed['project']['default_sequence_id'], 'time': at('0.5'), 'idempotency_key': h.key('one-native-color-png')}}
        job = h.job('one-native-color-png', 'opencut_render_submit', request, timeout=180)
        result = job['extended_result']['data']
        assert result['execution']['browser_version'] == h.runtime['browser_version']
        assert result['execution']['browser_sha256'] == h.runtime['browser_sha256']
        assert result['execution']['host_sha256'] == h.candidate
        assert len(result['font_lock']) == 18
        previous = json.loads((a.reference.parent / 'journey.json').read_text())['png']['result']
        assert result['font_lock'] == previous['font_lock']
        png = h.artifact(job)
        rows = compare_color_grid_png(png, a.reference)
        expected = a.reference.read_bytes()
        (a.report.parent / 'native-color-metrics.json').write_text(json.dumps(rows, indent=2) + '\n')
        assert h.inspect(project)['project'] == base['snapshot']['project']
        assert jobs() == {**base['jobs'], 'succeeded': base['jobs']['succeeded'] + 1}
        return {'job_id': job['job_id'], 'artifact': job['artifacts'][0], 'execution': result['execution'],
            'font_lock': result['font_lock'], 'rows': rows, 'reference': str(a.reference), 'reference_sha256': sha(expected),
            'scope': 'Only changed browser/runtime: historical fixed18font scene, original-source/Classic expected pixels, no reimport or new fixture.'}

    if a.phase in ('source', 'all'):
        h.stage('actual-h264-source', source_frame)
    if a.phase in ('font', 'all'):
        h.stage('cold-bundled-color', cold_font)
        h.stage('restore-font-project', restore_font)
    if a.phase in ('frame', 'all'):
        h.stage('actual-native-color-frame', frame)
    report = {'status': 'scoped_stages_completed', 'runtime': h.runtime, 'binary_sha256': h.candidate,
        'checkpoint': str(h.checkpoint), 'stages': h.state['stages'], 'jobs': jobs(),
        'limits': 'Changed-runtime representatives only; not complete per-font/platform/FPS/S1/A7/D8. FFmpeg/ffprobe remain declared external dependencies.'}
    a.report.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'status': report['status'], 'completed': list(report['stages']), 'report': str(a.report)}))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['package', 'workspace', 'cache', 'reference', 'report']:
        p.add_argument('--' + name, type=Path, required=True)
    p.add_argument('--phase', choices=['source', 'font', 'frame', 'all'], default='all')
    verify(p.parse_args())
