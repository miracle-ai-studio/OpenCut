#!/usr/bin/env python3
"""Kill a real import Worker at its probe boundary; recover through the natural lease clock."""
import argparse, hashlib, json, os, shutil, signal, socket, sqlite3, subprocess, sys, tempfile, time
from pathlib import Path


def verify(binary, root):
    root.mkdir(parents=True, exist_ok=True)
    db = root / 'workspace.sqlite'
    worker = None
    trace = []
    def call(name, value):
        p = subprocess.run([str(binary), '--database', str(db), '--json', 'call', name, '--input', '-'],
                           input=json.dumps(value), text=True, capture_output=True, timeout=15)
        out = json.loads(p.stdout)
        assert p.returncode == 0 and out['error'] is None, (out, p.stderr)
        trace.append({'tool':name,'response':out})
        return out['data']
    try:
        snapshot = call('opencut_project_create', {'project':{'id':'recovery','name':'Recovery','sequence_id':'main',
            'width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
        source = root / 'red.png'
        subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=64x48','-frames:v','1',str(source)],check=True,timeout=30)
        data = source.read_bytes()
        upload = call('opencut_upload_create',{'project_id':'recovery','name':'image','size_bytes':str(len(data)),
            'content_hash':hashlib.sha256(data).hexdigest(),'idempotency_key':'import'})
        p = subprocess.run([str(binary),'--database',str(db),'--json','upload','put',upload['upload_id'],
                            '--offset','0','--input',str(source)],capture_output=True,text=True,timeout=15)
        assert p.returncode == 0 and json.loads(p.stdout)['data']['next_offset'] == str(len(data))
        sealed = call('opencut_upload_seal', {'upload_id':upload['upload_id']})
        job_id = sealed['job_id']
        # Only the test PATH adds a pause, after real assembly and before probe execution.
        # Production has no environment-triggered fault hook or altered clock.
        wrapper_dir = root / 'probe-boundary'
        wrapper_dir.mkdir()
        ready = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        ready.bind(('127.0.0.1', 0))
        ready.settimeout(.25)
        ready_port = ready.getsockname()[1]
        wrapper = wrapper_dir / 'ffprobe'
        wrapper.write_text(f'#!{sys.executable}\nimport os,sys,time,socket\n'
            f'if "-version" in sys.argv: os.execv({shutil.which("ffprobe")!r},[{shutil.which("ffprobe")!r}]+sys.argv[1:])\n'
            f'socket.socket(socket.AF_INET,socket.SOCK_DGRAM).sendto(sys.argv[-1].encode(),("127.0.0.1",{ready_port}))\n'
            'while True: time.sleep(.05)\n')
        wrapper.chmod(0o700)
        env = dict(os.environ, PATH=str(wrapper_dir)+os.pathsep+os.environ['PATH'])
        with (root/'first-worker.log').open('w') as log:
            worker = subprocess.Popen([str(binary),'--database',str(db),'worker','run'],env=env,stdout=log,stderr=log,start_new_session=True)
        deadline = time.monotonic()+15
        assembled_name = None
        while assembled_name is None:
            assert worker.poll() is None and time.monotonic()<deadline
            try: assembled_name = ready.recvfrom(4096)[0].decode()
            except socket.timeout: pass
        ready.close()
        assembled = Path(assembled_name)
        assert assembled.read_bytes() == data
        initial = call('opencut_job_get', {'job_id':job_id})
        assert initial['state']=='running' and initial['attempt']==1
        with sqlite3.connect(db) as c:
            assert c.execute('SELECT count(*) FROM project_media').fetchone()[0]==0
        os.killpg(worker.pid,signal.SIGKILL)
        assert worker.wait(timeout=5)==-signal.SIGKILL
        worker = None
        assert assembled.exists()
        source.unlink()
        stopped = call('opencut_job_get', {'job_id':job_id})
        assert stopped['state']=='running' and stopped['attempt']==1
        started = time.monotonic()
        with (root/'recovered-worker.log').open('w') as log:
            worker = subprocess.Popen([str(binary),'--database',str(db),'worker','run'],stdout=log,stderr=log,start_new_session=True)
        deadline = started+50
        while True:
            assert worker.poll() is None
            done = call('opencut_job_get', {'job_id':job_id})
            if done['state'] in ['succeeded','failed','cancelled']: break
            assert time.monotonic()<deadline
            time.sleep(.25)
        elapsed = time.monotonic()-started
        assert done['state']=='succeeded' and done['attempt']==2, done
        assert elapsed>=20, 'Natural 30s lease was bypassed'
        view = call('opencut_job_view', {'job_id':job_id})
        result = view['extended_result']['data']
        assert view['extended_result']['schema_version']==1 and result['asset']['kind']=='image'
        assert result['asset']['metadata']['content_hash']==hashlib.sha256(data).hexdigest()
        assert call('opencut_project_media_inspect',{'project_id':'recovery','media_id':result['media_id']})==result
        assert call('opencut_project_inspect',{'project_id':'recovery'})==snapshot
        assert call('opencut_upload_seal',{'upload_id':upload['upload_id']})['job_id']==job_id
        assert not assembled.exists()
        assert not list((root/'artifacts').rglob('upload.media'))
        with sqlite3.connect(db) as c:
            assert c.execute('SELECT count(*) FROM project_media').fetchone()[0]==1
            for action in ['media.imported','job.succeeded']:
                assert c.execute('SELECT count(*) FROM audit_records WHERE job_id=? AND action=?',(job_id,action)).fetchone()[0]==1
        report = {'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root),
            'checks':['Actual process group SIGKILL after durable assembled input, before any project resource publication',
                      'Unmodified 30s lease expires naturally; real FFprobe on attempt 2 succeeds after deleting the source',
                      'Exactly one resource and success audit, stable seal identity, unchanged head and abandoned input cleanup'],
            'recovery_seconds':elapsed,'job_id':job_id,'attempt':done['attempt']}
        (root/'report.json').write_text(json.dumps(report,indent=2))
        print(json.dumps(report,indent=2))
    finally:
        (root/'trace.json').write_text(json.dumps(trace,indent=2))
        if worker is not None:
            os.killpg(worker.pid,signal.SIGTERM)
            try: worker.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(worker.pid,signal.SIGKILL);worker.wait(timeout=5)

if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));p.add_argument('--artifacts-dir',type=Path);a=p.parse_args()
    verify(a.binary.resolve(),a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-import-recovery-')))
