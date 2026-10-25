#!/usr/bin/env python3
"""Real media/plugin publication and anonymous HTTP copy lifetime acceptance."""
import argparse, hashlib, http.client, json, os, shutil, socket, sqlite3, subprocess, tarfile, tempfile, time, wave
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]

def verify(binary, root, component):
    root.mkdir(parents=True, exist_ok=True)
    db = root/'workspace.sqlite'; token = root/'owner.token'; trace = []; checks = []; host = None; port = None; sockets = []
    def call(name, data=None, error=None):
        p = subprocess.run([str(binary),'--database',str(db),'--json','call',name,'--input','-'],input=json.dumps(data or {}),text=True,capture_output=True,timeout=45)
        out = json.loads(p.stdout); trace.append({'tool':name,'input':data,'response':out,'exit':p.returncode})
        if error: assert p.returncode and out['error']['code'] == error, (out,p.stderr)
        else: assert p.returncode == 0 and out['error'] is None, (out,p.stderr)
        return out['data']
    def publications(prefix):
        with sqlite3.connect(db) as c:
            return {suffix:c.execute(f'SELECT * FROM {prefix}_{suffix} ORDER BY rowid').fetchall() for suffix in ['publications','events']}
    def inspect(): return call('opencut_storage_budget_inspect')
    def configure(limit, key):
        return call('opencut_storage_budget_configure',{'expected_revision':inspect()['policy']['revision'],'idempotency_key':key,'payload_limit_bytes':str(limit),'minimum_available_bytes':'0'})
    def settled():
        deadline = time.monotonic()+15
        while True:
            state = inspect()
            if state['reserved_bytes'] == '0': return state
            assert time.monotonic()<deadline, state
            time.sleep(.03)
    def raw(method, path, body=None, error=None):
        c = http.client.HTTPConnection('127.0.0.1',port,timeout=60)
        try:
            c.request(method,path,body,{'Authorization':'Bearer '+token.read_text().strip()});r=c.getresponse();data=r.read()
            if error: assert r.status==422 and json.loads(data)['error']['code']==error,(r.status,data)
            else: assert r.status==200,(r.status,data)
            return data
        finally: c.close()
    def slow(path, method='GET', chunk=None):
        s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF,4096);s.settimeout(30);s.connect(('127.0.0.1',port));sockets.append(s)
        extra='Transfer-Encoding: chunked\r\n' if chunk is not None else ''
        s.sendall((f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token.read_text().strip()}\r\n{extra}\r\n').encode())
        if chunk is not None: s.sendall(f'{len(chunk):x}\r\n'.encode()+chunk+b'\r\n')
        else:
            header=b''
            while b'\r\n\r\n' not in header: header+=s.recv(1)
            assert header.startswith(b'HTTP/1.1 200'),header
        return s
    def live(category, minimum_bytes=1):
        deadline=time.monotonic()+15
        while True:
            state=inspect();active=[r for r in state['reservations'] if r['category']==category and int(r['reserved_bytes'])>=minimum_bytes]
            if active: return state,active
            assert time.monotonic()<deadline,state
            time.sleep(.02)
    def start():
        nonlocal host,port
        with socket.socket() as s: s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        with (root/'host.log').open('a') as log: host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
        deadline=time.monotonic()+20
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):
                    if token.exists(): return
            except OSError: pass
            assert time.monotonic()<deadline;time.sleep(.03)
    try:
        project=call('opencut_project_create',{'project':{'id':'writers','name':'Writers','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
        state=inspect();assert {'media_upload','media_inspection','media_metadata','plugin_package','plugin_grants','artifact_download'} <= set(state['coverage']);assert not state['enforces_all_workspace_writes']
        previous=ROOT/'dist/local/opencut-0.1.0-dev-02881994ee9f-x86_64-apple-darwin.tar.gz'
        if previous.is_file():
            assert hashlib.sha256(previous.read_bytes()).hexdigest()==Path(str(previous)+'.sha256').read_text().split()[0]
            with tarfile.open(previous) as tar:
                manifest=json.load(tar.extractfile(next(m for m in tar if m.name.endswith('/manifest.json'))));old=tar.extractfile(next(m for m in tar if m.name.endswith('/bin/opencut'))).read()
            assert manifest['database_schema']==13 and hashlib.sha256(old).hexdigest()==manifest['binary_sha256']
            oldpath=root/'old-opencut';oldpath.write_bytes(old);oldpath.chmod(0o755)
            with sqlite3.connect(db) as c: before=list(c.iterdump())
            p=subprocess.run([str(oldpath),'--database',str(db),'--json','project','list'],capture_output=True,text=True,timeout=20)
            assert p.returncode and json.loads(p.stdout)['error']['code']=='UNSUPPORTED_DATABASE_VERSION'
            with sqlite3.connect(db) as c: assert list(c.iterdump())==before
            checks.append('Delivered schema-13 host refuses the extended writer protocol without changing business history')
        start();wav=root/'tone.wav'
        with wave.open(str(wav),'wb') as w: w.setnchannels(1);w.setsampwidth(2);w.setframerate(8000);w.writeframes(b'\0\0'*800)
        data=wav.read_bytes();configure(0,'upload-deny');raw('POST','/api/media/upload?name=tone.wav',data,'STORAGE_BUDGET_EXCEEDED');settled();assert not list((root/'media').glob('.upload-*'))
        # Cross the batched disk-write boundary while leaving the HTTP body open.
        partial = data + b'\0' * ((8 << 20) - len(data))
        configure(1<<30,'upload-room');s=slow('/api/media/upload?name=partial.wav','POST',partial);state,active=live('media_upload',len(partial));recovery=call('opencut_storage_budget_recover');assert recovery['recovered_reservations']==0 and recovery['reserved_bytes']==state['reserved_bytes'];s.close();settled();assert not list((root/'media').glob('.upload-*'))
        uploaded=json.loads(raw('POST','/api/media/upload?name=tone.wav',data))['data'];settled();content=hashlib.sha256(data).hexdigest();assert (root/'media'/content).read_bytes()==data
        checks.append('Global HTTP upload rejects before bytes are written; live chunked upload survives recovery and disconnect cleans file and reservation; valid WAV publishes exact bytes')
        metadata=root/'media'/f'{content}.json';metadata.write_text('{}');entry=call('opencut_media_inspect',{'content_hash':content});request={'content_hash':content,'expected_metadata_hash':entry['metadata_hash']}
        before_publication=publications('maintenance');used=int(inspect()['observed_payload_bytes']);configure(used,'repair-copy-deny');call('opencut_media_repair',request,'STORAGE_BUDGET_EXCEEDED');assert metadata.read_text()=='{}';settled()
        configure(used+2*len(data),'repair-metadata-deny');call('opencut_media_repair',request,'STORAGE_BUDGET_EXCEEDED');assert metadata.read_text()=='{}';assert not list((root/'media').glob('.inspect-*'));settled()
        assert publications('maintenance')==before_publication
        configure(1<<30,'repair-room');repaired=call('opencut_media_repair',request);assert repaired['asset']['kind']=='audio' and (root/'media'/content).read_bytes()==data;settled()
        checks.append('Real corrupted media catalog repair separately refuses inspection copy and metadata publication; raised policy reconciles and repairs without changing source bytes')
        package=root/'package';package.mkdir();shutil.copyfile(component,package/'planner.wasm');(package/'input.json').write_text(json.dumps({'type':'object','properties':{'track_id':{'type':'string'}},'required':['track_id'],'additionalProperties':False}));(package/'output.json').write_text('{"type":"array"}')
        permissions=['project.snapshot.read','edit.plan.propose'];manifest={'manifest_version':1,'id':'org.example.budget','version':'1.0.0','display_name':'Budget QA','plugin_api':'>=1.0.0, <2.0.0','runtime':{'kind':'wasm-component','entry':'planner.wasm','world':'opencut:planner/planner@1.0.0'},'targets':['native'],'permissions':permissions,'contributes':{'operations':[{'name':'org.example.budget.plan','version':1,'kind':'job','input_schema':'input.json','output_schema':'output.json'}]},'limits':{'memory_mib':32,'wall_time_ms':2000,'output_bytes':65536,'fuel':10000000}}
        (package/'manifest.json').write_text(json.dumps(manifest));install={'package_root':str(package),'granted_permissions':[]}
        configure(int(inspect()['observed_payload_bytes']),'plugin-deny');call('opencut_plugin_install',install,'STORAGE_BUDGET_EXCEEDED');assert call('opencut_plugin_list')==[];assert not list((root/'plugins').rglob('.install-staging'));settled()
        configure(1<<30,'plugin-room');installed=call('opencut_plugin_install',install);grant=(root/'plugins'/installed['manifest']['id']/installed['manifest']['version']/installed['package_sha256'])/'.opencut-grants.json';assert json.loads(grant.read_text())==[]
        before_publication=publications('plugin');configure(int(inspect()['observed_payload_bytes']),'grant-deny');call('opencut_plugin_install',{**install,'granted_permissions':permissions},'STORAGE_BUDGET_EXCEEDED');assert json.loads(grant.read_text())==[];settled()
        assert publications('plugin')==before_publication
        configure(1<<30,'grant-room');call('opencut_plugin_install',{**install,'granted_permissions':permissions});inspected=call('opencut_plugin_inspect',{'id':manifest['id'],'version':'1.0.0','kind':'wasm'});assert set(inspected['granted_permissions'])==set(permissions);settled()
        checks.append('Real WASM package and grant writes enforce admission, failed staging cleans up, existing grants survive refusal, and restored capacity restores verified permission metadata;execution has separate canonical WASM coverage')
        # A real large PNG frame output reliably exceeds HTTP/socket buffers.
        pixels=os.urandom(3072*2048*3);source=root/'noise.png'
        subprocess.run(['ffmpeg','-v','error','-f','rawvideo','-pixel_format','rgb24','-video_size','3072x2048','-i','-','-frames:v','1','-threads','1',str(source)],input=pixels,capture_output=True,check=True,timeout=45)
        accepted=call('opencut_job_submit',{'kind':'preview.frame','idempotency_key':'large-frame','input':{'input':str(source),'requested_time':{'ticks':'0','timebase':10},'alignment':'exact'}})
        deadline=time.monotonic()+90
        while True:
            job=call('opencut_job_view',{'job_id':accepted['job_id']})
            if job['state'] in ['succeeded','failed','cancelled']: break
            assert time.monotonic()<deadline;time.sleep(.1)
        assert job['state']=='succeeded',job
        ref=job['artifacts'][0];size=int(ref['size_bytes']);assert size>10<<20;path=f'/api/jobs/{job["job_id"]}/artifact';used=int(settled()['observed_payload_bytes'])
        configure(used,'download-deny');raw('GET',path,error='STORAGE_BUDGET_EXCEEDED');settled()
        configure(used+size,'download-one');s=slow(path);state,active=live('artifact_download');assert int(active[0]['reserved_bytes'])==size;assert not list((root/'artifacts').glob('.download-*'));assert int(state['observed_payload_bytes'])==used
        recovery=call('opencut_storage_budget_recover');assert recovery['recovered_reservations']==0 and int(recovery['reserved_bytes'])==size
        raw('GET',path,error='STORAGE_BUDGET_EXCEEDED');s.close();settled()
        downloaded=raw('GET',path);settled();assert hashlib.sha256(downloaded).hexdigest()==ref['content_hash'];result=root/'download.png';result.write_bytes(downloaded)
        decoded=subprocess.check_output(['ffmpeg','-v','error','-i',str(result),'-f','rawvideo','-pix_fmt','rgb24','-'],timeout=45);assert decoded==pixels
        checks.append('Real 18 MiB PNG download refuses low capacity; anonymous copy retains exact reservation under backpressure, survives recovery, excludes a second download, releases on disconnect, and full download decodes to source pixels')
        s=slow(path);live('artifact_download');host.kill();host.wait(timeout=15);host=None;s.close();pending=inspect();assert int(pending['reserved_bytes'])==size
        # A killed host can also leave its SQLite control reservation behind.
        expected_recovered=len(pending['reservations'])+len(pending['control']['reservations'])
        recovery=call('opencut_storage_budget_recover')
        assert recovery['recovered_reservations']==expected_recovered and recovery['reserved_bytes']=='0'
        assert recovery['reservations']==[] and recovery['control']['reservations']==[]
        assert recovery['control']['reserved_control_bytes']=='0' and int(recovery['observed_payload_bytes'])==used
        start();assert hashlib.sha256(raw('GET',path)).hexdigest()==ref['content_hash'];settled()
        checks.append('SIGKILL during a live download leaves a recoverable reservation, no anonymous payload residue, unchanged published artifact and a successful download after restart')
        report={'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    except Exception as e:
        (root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
    finally:
        for s in sockets:s.close()
        if host and host.poll() is None:host.terminate();host.wait(timeout=15)
        (root/'trace.json').write_text(json.dumps(trace,indent=2))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--component',type=Path,default=ROOT/'crates/plugin-host/examples/planner-guest/target/wasm32-wasip2/release/opencut_example_planner_guest.wasm');a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-budget-writers-'))).resolve(),a.component.resolve())
