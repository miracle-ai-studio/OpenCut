#!/usr/bin/env python3
"""Trusted local worker: fixed recipe/file inputs, no network or project data."""
import base64
import json
import resource
import sys
import os
import threading
import time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import derive_static

def run(request_path, output_path):
    memory_limit = 2*1024*1024*1024
    if sys.platform != 'darwin':
        resource.setrlimit(resource.RLIMIT_AS, (memory_limit,memory_limit))
    # Darwin rejects RLIMIT_AS even when the reported hard limit is unlimited.
    # Monitor native getrusage peak resident bytes and terminate this process
    # when its fixed bound is crossed; no shell/process lookup is involved.
    def memory_watch():
        while True:
            peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            if sys.platform != 'darwin': peak *= 1024
            if peak > memory_limit: os._exit(74)
            time.sleep(.01)
    threading.Thread(target=memory_watch,daemon=True).start()
    resource.setrlimit(resource.RLIMIT_CPU, (20,20))
    resource.setrlimit(resource.RLIMIT_FSIZE, (32*1024*1024,32*1024*1024))
    resource.setrlimit(resource.RLIMIT_NOFILE, (64,64))
    request_bytes=Path(request_path).read_bytes()
    if len(request_bytes)>256*1024: raise ValueError('recipe envelope exceeded')
    request=json.loads(request_bytes)
    source=Path(request['source_file']).read_bytes();license_bytes=Path(request['license_file']).read_bytes()
    data,evidence=derive_static.derive(source,license_bytes,request['proof']['source'],request['proof']['tool'])
    if derive_static.canonical(evidence)!=derive_static.canonical(request['proof']): raise ValueError('derived proof differs from pinned output')
    Path(output_path).write_bytes(data)
if __name__=='__main__':
    try:
        if sys.argv[1:] == ['--identity']:
            print(derive_static.canonical(derive_static.implementation_identity()).decode())
        else:
            run(sys.argv[1],sys.argv[2])
    except Exception:
        print('Pinned font derivation refused',file=sys.stderr)
        sys.exit(1)
