#!/usr/bin/env python3
"""Local synthetic >1 GiB file: CAS pin, portable export, source loss and restore.

Streaming digests keep the acceptance harness from masking native memory usage.
This verifies file IO and portability, not 16 GiB realtime playback performance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import time


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        while block := f.read(1024 * 1024):
            h.update(block)
    return h.hexdigest()


def verify(binary, destination):
    started = time.monotonic()
    root = destination.resolve() / f'long-media-{time.time_ns()}'
    origin, target = root / 'origin', root / 'restored'
    origin.mkdir(parents=True)
    target.mkdir()
    source = root / 'synthetic-silence.wav'
    payload_size = 1024**3
    header = struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', payload_size+36, b'WAVE', b'fmt ', 16, 1, 1, 48000, 96000, 2, 16, b'data', payload_size)
    with source.open('wb') as f:
        f.write(header)
        f.truncate(payload_size + len(header))
    expected = digest(source)
    size = source.stat().st_size
    calls = []

    def call(workspace, name, body):
        start = time.monotonic()
        result = subprocess.run([str(binary.resolve()), '--database', str(workspace/'workspace.sqlite'), '--json', 'call', name, '--input', '-'], input=json.dumps(body), text=True, capture_output=True, timeout=300)
        reply = json.loads(result.stdout)
        assert result.returncode == 0 and reply['error'] is None, (reply, result.stderr)
        calls.append({'tool': name, 'seconds': round(time.monotonic()-start,3)})
        return reply['data']

    created = call(origin, 'opencut_project_create', {'project':{'id':'large','name':'Long media','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
    bound = call(origin, 'opencut_edit_apply', {'project_id':'large','request':{'expected_revision':created['revision'],'idempotency_key':'bind','label':'Pin synthetic long source','operations':[{'type':'asset.bind','asset':{'id':'long-audio','name':'Long audio','kind':'audio','uri':str(source),'duration':{'ticks':str(payload_size//2),'timebase':48000},'metadata':{}}}]}})
    exported = call(origin, 'opencut_project_package_export', {'project_id':'large','revision':bound['revision'],'destination':str(root/'portable.opencut'),'mode':'self_contained'})
    package = Path(exported['package_path'])
    manifest = json.loads((package/'manifest.json').read_text())
    asset = manifest['assets'][0]
    assert asset['size'] == size and asset['content_hash'] == expected
    assert digest(package/asset['media_path']) == expected
    source.unlink()
    imported = call(target, 'opencut_project_package_import', {'package_path':str(package),'target_project_id':'restored','expected_manifest_sha256':exported['manifest_sha256'],'idempotency_key':'restore'})
    snapshot = call(target, 'opencut_project_inspect', {'project_id':'restored'})
    restored = snapshot['project']['assets'][0]
    assert restored['version']['size'] == size and restored['version']['content_hash'] == expected
    # Inspect the actual restored CAS bytes independently of the import receipt.
    objects = list((target/'asset-store').rglob(expected))
    assert len(objects) == 1 and digest(objects[0]) == expected, objects
    report = {'status':'passed','binary_sha256':digest(binary),'source_bytes':size,'source_sha256':expected,'source_deleted_before_import':True,'restored_revision':imported.get('revision'),'checks':calls,'elapsed_seconds':round(time.monotonic()-started,3),'workspace':str(root)}
    (root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    return report


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--artifacts-dir',type=Path,required=True)
    a=p.parse_args()
    print(json.dumps(verify(a.binary,a.artifacts_dir),indent=2))
