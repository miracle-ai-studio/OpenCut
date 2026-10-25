#!/usr/bin/env python3
"""Bounded worker for independently versioned complete static COLR recipes."""
import json
import os
from pathlib import Path
import resource
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import derive_color_static_v1 as color


def run(request_path, output_path):
    memory_limit = 2 * 1024 * 1024 * 1024
    if sys.platform != 'darwin':
        resource.setrlimit(resource.RLIMIT_AS, (memory_limit, memory_limit))

    def memory_watch():
        while True:
            peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            if sys.platform != 'darwin':
                peak *= 1024
            if peak > memory_limit:
                os._exit(74)
            time.sleep(.01)

    threading.Thread(target=memory_watch, daemon=True).start()
    resource.setrlimit(resource.RLIMIT_CPU, (90, 90))
    resource.setrlimit(resource.RLIMIT_FSIZE, (32 * 1024 * 1024, 32 * 1024 * 1024))
    resource.setrlimit(resource.RLIMIT_NOFILE, (64, 64))
    raw = Path(request_path).read_bytes()
    if len(raw) > 256 * 1024:
        raise ValueError('recipe envelope exceeded')
    request = json.loads(raw)
    source, notice = Path(request['source_file']), Path(request['license_file'])
    if source.stat().st_size > color.base.MAX_SOURCE_BYTES or notice.stat().st_size > 65536:
        raise ValueError('fixed input envelope exceeded')
    expected = request['proof']
    data, proof = color.derive(source.read_bytes(), notice.read_bytes(),
                               expected['base_source'], expected['tool'])
    if color.canonical(proof) != color.canonical(expected):
        raise ValueError('complete static color proof differs from pinned output')
    Path(output_path).write_bytes(data)


if __name__ == '__main__':
    try:
        if sys.argv[1:] == ['--identity']:
            print(color.canonical(color.implementation_identity()).decode())
        else:
            run(sys.argv[1], sys.argv[2])
    except Exception:
        print('Pinned static-color derivation refused', file=sys.stderr)
        sys.exit(1)
