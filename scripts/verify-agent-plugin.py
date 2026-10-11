#!/usr/bin/env python3
"""Exercise both delivered plugin MCP configurations with synthetic local media.

No real projects, credentials, client installation, live server or model calls.
The two adapters share a new database but use different synthetic projects.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

HERE = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify(binary, root):
    root.mkdir(parents=True, exist_ok=False)
    frozen = root / 'opencut'
    shutil.copy2(binary, frozen)
    binary_sha = hashlib.sha256(frozen.read_bytes()).hexdigest()
    database = root / 'workspace.sqlite'
    bundle = root / 'plugins'
    generator = load('agent_plugin', HERE / 'distribution/agent-plugin.py')
    receipt = generator.generate(frozen, database, bundle)
    assert not database.exists(), 'Packaging must not open the database'
    for relative, expected in receipt['files'].items():
        assert hashlib.sha256((bundle / relative).read_bytes()).hexdigest() == expected
    mcp_type = load('editorial', HERE / 'verify-editorial.py').Mcp
    trace, results = [], []

    def tick(n):
        return {'ticks': str(n), 'timebase': 10}

    def span(start, duration):
        return {'start': tick(start), 'duration': tick(duration)}

    try:
        for client in ('codex', 'claude'):
            output = root / client
            output.mkdir()
            server = json.loads((bundle / client / 'plugins/opencut/.mcp.json').read_text())['mcpServers']['opencut']
            assert server['command'] == str(frozen)
            assert server['args'] == ['--database', str(database), 'mcp', 'serve', '--with-worker']
            mcp = mcp_type(frozen, database, output, [server['command'], *server['args']], trace)
            checks = []
            try:
                listed = mcp.request('tools/list', {})['tools']
                assert len(listed) == 9 and any(x['name'] == 'opencut_edit' for x in listed)
                resource = mcp.request('resources/read', {'uri': 'opencut://schemas/domains'})
                assert resource['contents'] and 'timeline.plan' in resource['contents'][0]['text']
                def call(domain, action, value):
                    return mcp.tool('opencut_' + domain, {'action': action, 'input': value})

                # Resolve advertised per-action schemas, as the shared skill instructs.
                for domain in ('system', 'project', 'media', 'edit', 'task'):
                    schemas = mcp.request('resources/read', {'uri': 'opencut://schemas/domains/' + domain})
                    assert 'input_schema' in schemas['contents'][0]['text']
                call('system', 'capabilities', {})
                checks.append('MCP initialize, nine compact tools, capabilities and exact action schema resources')
                project_id = client + '-film'
                created = call('project', 'v3.create', {'project': {
                    'id': project_id, 'name': client + ' synthetic plugin test', 'sequence_id': 'main',
                    'width': 160, 'height': 90, 'fps': {'numerator': 10, 'denominator': 1}},
                    'idempotency_key': 'create-' + client})

                def inspect():
                    return call('project', 'v3.inspect', {'project_id': project_id, 'revision': None})

                def wait_job(job):
                    deadline = time.monotonic() + 120
                    while time.monotonic() < deadline:
                        view = call('task', 'view', {'job_id': job['job_id']})
                        if view['state'] in ('succeeded', 'failed', 'cancelled'):
                            assert view['state'] == 'succeeded', view
                            return view
                        time.sleep(.1)
                    raise AssertionError('Managed Worker timed out')

                def artifact(job, destination):
                    wait_job(job)
                    record = call('task', 'get', {'job_id': job['job_id']})
                    path = Path(record['result']['artifact']['path'])
                    assert path.is_file() and path.stat().st_size > 0
                    shutil.copy2(path, destination)
                    return destination

                def base(operation):
                    return {'type': 'base.apply', 'operation': operation}

                operations = [base({'type': 'track.create', 'sequence_id': 'main', 'track': {
                    'id': 'video', 'name': 'Video', 'kind': 'video', 'enabled': True,
                    'muted': True, 'locked': False, 'clips': []}})]
                media_ids = []
                for i, color in enumerate(('red', 'blue')):
                    source = output / (color + '.mp4')
                    subprocess.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i',
                                    f'color={color}:s=160x90:r=10:d=3', '-c:v', 'libx264',
                                    '-pix_fmt', 'yuv420p', str(source)], check=True, timeout=30)
                    upload = call('media', 'upload.stream.begin', {'project_id': project_id,
                        'idempotency_key': client + '-upload-' + color, 'name': source.name,
                        'size_bytes': str(source.stat().st_size)})
                    # Existing public byte transport, never inline media bytes in MCP JSON.
                    put = subprocess.run([str(frozen), '--database', str(database), '--json',
                        'upload', 'stream-put', upload['upload_id'], '--offset', '0', '--input', str(source)],
                        check=True, capture_output=True, text=True, timeout=30)
                    assert json.loads(put.stdout)['data']['next_offset'] == str(source.stat().st_size)
                    job = wait_job(call('media', 'upload.stream.complete', {'upload_id': upload['upload_id'],
                        'content_hash': hashlib.sha256(source.read_bytes()).hexdigest()}))
                    media_id = job['extended_result']['data']['media_id']
                    media_ids.append(media_id)
                    asset = call('media', 'project.media.inspect', {'project_id': project_id, 'media_id': media_id})['asset']
                    asset['id'] = color
                    call('media', 'upload.release', {'upload_id': upload['upload_id']})
                    operations.extend([base({'type': 'asset.bind', 'asset': asset}), base({
                        'type': 'clip.insert', 'sequence_id': 'main', 'track_id': 'video', 'clip': {
                            'id': color + '-clip', 'asset_id': color, 'timeline_range': span(i * 20, 20),
                            'source_range': span(0, 20), 'params': {}, 'channels': []}})])
                    source.unlink()
                seed = {'project_id': project_id, 'expected_revision': created['revision'],
                        'idempotency_key': client + '-seed', 'label': 'Synthetic red then blue', 'operations': operations}
                call('edit', 'v3.plan', seed)
                call('edit', 'v3.apply', seed)
                before = inspect()
                library = call('project', 'library', {'cursor': None, 'limit': 100})
                assert project_id in [p['project_id'] for p in library['items']]
                assert len(before['project']['base']['assets']) == 2
                checks.append('Project/media reads and managed import of two synthetic videos; originals deleted')
                request = {'project_id': project_id, 'expected_revision': before['revision'],
                    'sequence_id': 'main', 'idempotency_key': client + '-shorten', 'label': 'Remove middle second',
                    'command': {'kind': 'ripple_delete', 'range': span(10, 10)}}
                planned = call('edit', 'timeline.plan', request)
                assert inspect() == before, 'Plan unexpectedly changed project'
                edited = call('edit', 'v3.apply', planned['transaction'])
                assert call('edit', 'v3.apply', planned['transaction']) == edited
                after = inspect()
                clips = after['project']['base']['sequences'][0]['tracks'][0]['clips']
                assert [c['timeline_range'] for c in clips] == [span(0, 10), span(10, 20)]
                stale = mcp.request('tools/call', {'name': 'opencut_edit', 'arguments': {
                    'action': 'timeline.plan', 'input': {**request, 'idempotency_key': client + '-stale'}}})
                assert stale['structuredContent']['error']['code'] == 'REVISION_CONFLICT'
                checks.append('Read-only plan, one atomic ripple edit, identical retry and stale-revision rejection')
                target = {'project_id': project_id, 'revision': after['revision'], 'sequence_id': 'main'}
                preview = artifact(call('task', 'v3.frame', {**target, 'idempotency_key': client + '-preview',
                    'requested_time': tick(15), 'alignment': 'exact'}), output / 'preview.png')

                def blue_pixel(path, movie=False):
                    command = ['ffmpeg', '-v', 'error', '-i', str(path)]
                    if movie:
                        command += ['-vf', 'select=eq(n\\,15)']
                    raw = subprocess.check_output(command + ['-frames:v', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], timeout=30)
                    assert len(raw) == 160 * 90 * 3
                    rgb = raw[(45 * 160 + 80) * 3:(45 * 160 + 80) * 3 + 3]
                    assert rgb[2] > 230 and rgb[0] < 15 and rgb[1] < 15, list(rgb)
                    return list(rgb)

                preview_rgb = blue_pixel(preview)
                checks.append('Rendered preview at 1.5 seconds is independently expected blue after deletion')
                undo = {'project_id': project_id, 'expected_revision': after['revision'],
                    'target_transaction_id': edited['transaction_id'], 'idempotency_key': client + '-undo'}
                assert not call('edit', 'v3.undo.plan', undo)['conflicts']
                undone = call('edit', 'v3.undo.apply', undo)
                assert inspect()['project'] == before['project']
                call('edit', 'v3.redo.apply', {'project_id': project_id,
                    'expected_revision': undone['transaction']['revision'],
                    'undo_transaction_id': undone['transaction']['transaction_id'], 'idempotency_key': client + '-redo'})
                final = inspect()
                assert final['project'] == after['project']
                checks.append('Undo restores exact original document; redo restores intended final edit')
                movie = artifact(call('task', 'v3.render', {**target, 'revision': final['revision'],
                    'idempotency_key': client + '-export', 'container': 'mp4', 'video_codec': 'h264'}), output / 'export.mp4')
                metadata = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_format',
                    '-show_streams', '-of', 'json', str(movie)], timeout=30))
                duration = float(metadata['format']['duration'])
                assert abs(duration - 3) < .12
                assert metadata['streams'][0]['codec_name'] == 'h264'
                movie_rgb = blue_pixel(movie, True)
                checks.append('Export job succeeds; actual H.264 MP4 decodes to three seconds and expected blue frame')
                results.append({'adapter': client, 'checks': checks, 'project_id': project_id,
                    'revision': final['revision'], 'preview': str(preview), 'preview_rgb': preview_rgb,
                    'movie': str(movie), 'movie_rgb': movie_rgb, 'duration': duration,
                    'movie_sha256': hashlib.sha256(movie.read_bytes()).hexdigest()})
            finally:
                mcp.close()
        report = {'status': 'passed', 'binary_sha256': binary_sha, 'bundle': str(bundle),
                  'workspace': str(root), 'database': str(database), 'adapters': results,
                  'limits': ['Deterministic MCP harness, not an external model conversation',
                             'Synthetic silent video; no audio/ASR/full Classic parity acceptance',
                             'Separate client manifest validation recorded outside this report',
                             'Local owner on isolated database; no credentials or live service changes']}
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    finally:
        (root / 'trace.json').write_text(json.dumps(trace, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--artifacts-dir', type=Path, default=Path(tempfile.gettempdir()) / f'opencut-plugin-{time.time_ns()}')
    args = parser.parse_args()
    verify(args.binary.resolve(), args.artifacts_dir.resolve())
