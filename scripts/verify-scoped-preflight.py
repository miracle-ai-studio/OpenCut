#!/usr/bin/env python3
"""Authorized preflight -> real preview, cross-entry diagnostics and no-write checks."""
import argparse,copy,hashlib,http.client,json,os,socket,sqlite3,subprocess,tempfile,time
from pathlib import Path
from urllib.parse import quote

def verify(binary,root,mcp_sdk_root=None):
    root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';host=None;trace=[];checks=[]
    def cli(args,payload=None,credential=None,error=None):
        p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=json.dumps(payload) if payload is not None else None,text=True,capture_output=True,timeout=30)
        out=json.loads(p.stdout);trace.append({'cli':args,'response':out,'exit':p.returncode})
        if error:assert p.returncode and out['error']['code']==error,(p.stderr,out)
        else:assert p.returncode==0 and out['error'] is None,(p.stderr,out)
        return out.get('data')
    def call(name,args,credential=None,error=None):return cli(['call',name,'--input','-'],args,credential,error)
    def raw(token,path,body=None):
        c=http.client.HTTPConnection('127.0.0.1',port,timeout=30);c.request('POST' if body is not None else 'GET',path,json.dumps(body) if body is not None else None,{'Authorization':'Bearer '+token.read_text().strip(),'Content-Type':'application/json'});r=c.getresponse();data=r.read();status=r.status;c.close();return status,data
    def http_call(name,args,credential,error=None):
        status,data=raw(credential,'/v1/tools/'+name,args);out=json.loads(data);trace.append({'http':name,'status':status,'response':out})
        if error:assert status==403 and out['error']['code']==error,out
        else:assert status==200 and out['error'] is None,out
        return out.get('data')
    def rows():
        with sqlite3.connect(db) as c:return list(c.iterdump())
    def policy(principal,revision,actions):return cli(['access','set-policy','--input','-'],{'principal_id':principal,'expected_revision':str(revision),'enabled':True,'grants':[{'project_id':'a','actions':actions}]})
    def issue(principal):
        file=root/(principal+'.secret');data=cli(['access','issue',principal,'--output',str(file)]);return file,data['credential_id']
    def preflight(revision,**kwargs):return {'project_id':'a','revision':revision,'sequence_id':'main','container':'mp4','video_codec':'h264',**kwargs}
    def portable(report):
        s=json.dumps(report);assert str(root) not in s and 'private-native-secret' not in s and 'asset-store/' not in s,s
    try:
        created={}
        for id in ['a','b']:
            created[id]=call('opencut_project_create',{'project':{'id':id,'name':id,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':id})
        source=root/'private-native-secret.png';subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=c=red:s=64x48','-frames:v','1',str(source)],check=True,timeout=30)
        at=lambda n:{'ticks':str(n),'timebase':10}
        setup={'project_id':'a','request':{'expected_revision':created['a']['revision'],'idempotency_key':'setup','label':'setup','operations':[
            {'type':'asset.bind','asset':{'id':'source','name':'red','kind':'image','uri':str(source),'duration':None,'metadata':{}}},
            {'type':'track.create','sequence_id':'main','track':{'id':'visual','name':'visual','kind':'graphic','enabled':True,'muted':False,'locked':False,'params':{},'clips':[]}},
            {'type':'clip.insert','sequence_id':'main','track_id':'visual','clip':{'id':'clip','asset_id':'source','timeline_range':{'start':at(0),'duration':at(10)},'source_range':None,'params':{},'channels':[]}}]}}
        revision=call('opencut_edit_apply',setup)['revision'];snapshot=call('opencut_project_inspect',{'project_id':'a'})
        full=['read','run_jobs','read_artifacts'];policy('runner',0,full);policy('reader',0,['read']);policy('editor',0,['read','edit'])
        runner,runner_id=issue('runner');reader,_=issue('reader');editor,_=issue('editor')
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        token=root/'owner-token'
        with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
        deadline=time.monotonic()+15
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):break
            except OSError:assert time.monotonic()<deadline;time.sleep(.03)
        assert http_call('opencut_capabilities',{},runner)['access']['scoped_preflight']
        source.unlink();request=preflight(revision);before=rows();expected=call('opencut_render_preflight',request,runner)
        assert expected['ready'] and expected['issues']==[];assert http_call('opencut_render_preflight',request,runner)==expected
        assert call('opencut_render_preflight',request)==expected;assert rows()==before
        checks.append('Scoped CLI/HTTP preflight and owner agree on actual immutable media after original source deletion; all SQLite tables remain unchanged')
        for identity in [reader,editor]:
            call('opencut_render_preflight',request,identity,'NOT_AUTHORIZED');http_call('opencut_render_preflight',request,identity,'NOT_AUTHORIZED')
        call('opencut_render_preflight',preflight(created['b']['revision'],project_id='b'),runner,'NOT_AUTHORIZED')
        call('opencut_render_preflight',preflight(created['b']['revision']),runner,'NOT_FOUND')
        call('opencut_render_preflight',{**request,'input':'/private/native-injection'},runner,'INVALID_ARGUMENT')
        missing=call('opencut_render_preflight',{**request,'sequence_id':'missing'},runner);assert missing['issues'][0]['code']=='SEQUENCE_NOT_FOUND';portable(missing)
        checks.append('run_jobs is required independently of read/edit; cross-project, foreign revisions and injected native paths reject before execution')
        # Independently constructed historical unversioned revision. Public edits still cannot bind paths.
        legacy=copy.deepcopy(snapshot['project']);legacy['assets'][0]['version']=None;legacy['assets'][0]['uri']=str(root/'private-native-secret-legacy')
        document=json.dumps(legacy,separators=(',',':'));legacy_rev='rev_'+hashlib.sha256(document.encode()).hexdigest()
        with sqlite3.connect(db) as c:c.execute('INSERT INTO revisions(revision,project_id,parent_revision,transaction_id,document_json) VALUES (?,?,NULL,NULL,?)',(legacy_rev,'a',document))
        before=rows();legacy_report=call('opencut_render_preflight',preflight(legacy_rev),runner);assert legacy_report['ready'] is False and legacy_report['issues'][0]['code']=='ASSET_LOCK_REQUIRED';assert legacy_report['issues'][0]['asset_id']=='source';portable(legacy_report);assert rows()==before
        assert not (root/'private-native-secret-legacy').exists()
        checks.append('A historical unversioned asset produces an actionable asset-scoped lock requirement without creating files or changing history')
        version=snapshot['project']['assets'][0]['version'];objects=[p for p in (root/'asset-store').rglob('*') if p.is_file() and p.name==version['content_hash'][2:]]
        if not objects:objects=[p for p in (root/'asset-store').rglob('*') if p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest()==version['content_hash']]
        assert len(objects)==1;object=objects[0];original=object.read_bytes();object.chmod(0o600);bad=bytearray(original);bad[-1]^=1;object.write_bytes(bad)
        damaged=call('opencut_render_preflight',request,runner);assert not damaged['ready'] and any(i['code']=='ASSET_INTEGRITY_ERROR' for i in damaged['issues']);portable(damaged)
        assert http_call('opencut_render_preflight',request,runner)==damaged;object.write_bytes(original);object.chmod(0o444)
        object.rename(object.with_suffix('.saved'));missing_content=call('opencut_render_preflight',request,runner);assert not missing_content['ready'];portable(missing_content);object.with_suffix('.saved').rename(object)
        checks.append('Corrupt and missing CAS objects fail preflight without source fallback; structured codes/entity IDs remain while native paths are excluded across CLI/HTTP')
        # Valid content hash but invalid media: exercise real FFprobe diagnostics, not only CAS validation.
        invalid=root/'private-native-secret-invalid';invalid.write_bytes(b'not image data')
        bound=copy.deepcopy(setup);bound['request'].update(expected_revision=revision,idempotency_key='bad-media');bound['request']['operations']=[{'type':'asset.replace','asset':{**setup['request']['operations'][0]['asset'],'uri':str(invalid)}}]
        bad_revision=call('opencut_edit_apply',bound)['revision'];bad_probe=call('opencut_render_preflight',preflight(bad_revision),runner);assert not bad_probe['ready'];portable(bad_probe)
        assert call('opencut_render_preflight',request,runner)==expected
        checks.append('Real FFprobe failure on a hash-valid invalid file returns portable diagnostics; the earlier immutable revision remains usable after head changes')
        if mcp_sdk_root:
            case={'healthy':request,'expected':expected,'damaged':preflight(bad_revision),'legacy':preflight(legacy_rev),'foreign':preflight(created['b']['revision'],project_id='b'),'database':str(db),'root':str(root)}
            file=root/'mcp-case.json';file.write_text(json.dumps(case))
            p=subprocess.run(['node',str(Path(__file__).with_name('verify-scoped-preflight-mcp.mjs')),str(mcp_sdk_root),str(binary),str(db),f'http://127.0.0.1:{port}',str(runner),str(reader),str(file)],capture_output=True,text=True,timeout=90)
            (root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert p.returncode==0,(p.stdout,p.stderr)
            checks.append('Official MCP SDK on stdio and HTTP agrees on healthy/legacy/invalid-media results and enforces runner/read-only/cross-project boundaries')
        job=http_call('opencut_job_submit',{'kind':'preview.composited_frame','idempotency_key':'after-preflight','input':{'project_id':'a','revision':revision,'sequence_id':'main','requested_time':at(0),'alignment':'exact'}},runner)
        deadline=time.monotonic()+30
        while job['state'] not in ['succeeded','failed','cancelled']:
            assert time.monotonic()<deadline;time.sleep(.05);job=http_call('opencut_job_view',{'job_id':job['job_id']},runner)
        assert job['state']=='succeeded',job
        artifact=job['artifacts'][0];status,data=raw(runner,'/v1/artifacts/'+quote(artifact['artifact_id'],safe=''));assert status==200 and hashlib.sha256(data).hexdigest()==artifact['content_hash'];png=root/'preview.png';png.write_bytes(data)
        pixels=subprocess.check_output(['ffmpeg','-v','error','-i',str(png),'-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30);assert len(pixels)==64*48*3 and sum(pixels[::3])/(64*48)>245 and sum(pixels[1::3])/(64*48)<5
        checks.append('The same scoped identity submits the preflighted historical revision, waits for real Worker output, downloads the content-bound PNG and verifies red pixels independently')
        policy('runner',1,['read']);call('opencut_render_preflight',request,runner,'NOT_AUTHORIZED');http_call('opencut_render_preflight',request,runner,'NOT_AUTHORIZED')
        policy('runner',2,full);assert call('opencut_render_preflight',request,runner)==expected;cli(['access','revoke',runner_id]);call('opencut_render_preflight',request,runner,'NOT_AUTHORIZED');http_call('opencut_render_preflight',request,runner,'NOT_AUTHORIZED')
        checks.append('Removing run_jobs takes effect on the next request; restoring it restores access, and credential revocation denies both CLI and HTTP')
        host.terminate();host.wait(timeout=10);host=None;assert call('opencut_render_preflight',request)==expected
        report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root),'revision':revision,'job_id':job['job_id']}
        (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    except Exception as e:
        (root/'failure.json').write_text(json.dumps({'error':repr(e),'completed_checks':checks},indent=2));raise
    finally:
        if host and host.poll() is None:
            host.terminate()
            try:host.wait(timeout=10)
            except subprocess.TimeoutExpired:host.kill();host.wait()
        (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-scoped-preflight-'))).resolve(),a.mcp_sdk_root)
