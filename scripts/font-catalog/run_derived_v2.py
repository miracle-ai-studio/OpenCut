#!/usr/bin/env python3
"""Trusted V2 worker: 64 MiB fixed source, 32 MiB output, bounded local child."""
import json
import sys
import os
import resource
import threading
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import derive_renamed_v2

def run(request_path,output_path):
    memory_limit=2*1024*1024*1024
    if sys.platform!='darwin':resource.setrlimit(resource.RLIMIT_AS,(memory_limit,memory_limit))
    def memory_watch():
        while True:
            peak=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            if sys.platform!='darwin':peak*=1024
            if peak>memory_limit:os._exit(74)
            time.sleep(.01)
    threading.Thread(target=memory_watch,daemon=True).start()
    # The larger reviewed CJK source has an independent V2 CPU ceiling; the
    # native host still enforces its <=120s absolute/cancellation deadline.
    resource.setrlimit(resource.RLIMIT_CPU,(90,90))
    resource.setrlimit(resource.RLIMIT_FSIZE,(32*1024*1024,32*1024*1024))
    resource.setrlimit(resource.RLIMIT_NOFILE,(64,64))
    raw=Path(request_path).read_bytes()
    if len(raw)>256*1024:raise ValueError('recipe envelope exceeded')
    request=json.loads(raw)
    source_path=Path(request['source_file']);license_path=Path(request['license_file'])
    if source_path.stat().st_size>derive_renamed_v2.MAX_SOURCE_BYTES or license_path.stat().st_size>65536:raise ValueError('input envelope exceeded')
    source=source_path.read_bytes();notice=license_path.read_bytes()
    data,proof=derive_renamed_v2.derive(source,notice,request['proof']['source'],request['proof']['tool'])
    if derive_renamed_v2.canonical(proof)!=derive_renamed_v2.canonical(request['proof']):raise ValueError('derived proof differs from fixed output')
    Path(output_path).write_bytes(data)

if __name__ == '__main__':
    try:
        if sys.argv[1:] == ['--identity']:
            print(derive_renamed_v2.canonical(derive_renamed_v2.implementation_identity()).decode())
        else:
            run(sys.argv[1], sys.argv[2])
    except Exception:
        print('Pinned renamed-font derivation refused', file=sys.stderr)
        sys.exit(1)
