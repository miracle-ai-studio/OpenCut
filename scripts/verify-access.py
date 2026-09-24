#!/usr/bin/env python3
"""Real scoped CLI/stdio MCP flows, revocation and fixed-revision FFmpeg work."""
import argparse, hashlib, json, os, selectors, sqlite3, subprocess, tempfile, time
from pathlib import Path
from urllib.parse import quote


def verify(binary, root):
    root.mkdir(parents=True, exist_ok=True)
    db=root/'workspace.sqlite'; trace=[]; checks=[]; children=[]
    def command(args, payload=None, credential=None, error=None):
        cmd=[str(binary),'--database',str(db),'--json']
        if credential: cmd+=['--credential-file',str(credential)]
        p=subprocess.run(cmd+args,input=json.dumps(payload) if payload is not None else None,text=True,capture_output=True,timeout=30)
        if args==['worker','run','--once'] and credential is None:
            assert p.returncode==0,p.stderr
            trace.append({'command':args,'exit':p.returncode});return
        out=json.loads(p.stdout); trace.append({'command':args,'credential':credential.name if credential else None,'response':out,'exit':p.returncode})
        if error: assert out['error']['code']==error and p.returncode==3,(p.returncode,out)
        else: assert p.returncode==0 and out['error'] is None,(args,p.stderr,out)
        return out.get('data')
    def call(name,args,credential=None,error=None):return command(['call',name,'--input','-'],args,credential,error)
    def create(name):return call('opencut_project_create',{'project':{'id':name,'name':name,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create-'+name})
    def policy(principal,revision,actions,enabled=True):return command(['access','set-policy','--input','-'],{'principal_id':principal,'expected_revision':str(revision),'enabled':enabled,'grants':[{'project_id':'a','actions':actions}]})
    def issue(principal,name):
        path=root/name; result=command(['access','issue',principal,'--output',str(path)])
        assert 'secret' not in result and len(path.read_text().strip())==64
        assert path.stat().st_mode & 0o777 == 0o600
        return path,result['credential_id']
    def edit(revision,key):return {'project_id':'a','request':{'expected_revision':revision,'idempotency_key':key,'label':key,'operations':[{'type':'sequence.update_settings','sequence_id':'main','name':key}]}}
    class Lines:
        def __init__(self,p):
            self.p=p; self.selector=selectors.DefaultSelector();self.selector.register(p.stdout,selectors.EVENT_READ);self.buffer=bytearray()
        def read(self,timeout=10):
            deadline=time.monotonic()+timeout
            while b'\n' not in self.buffer:
                remaining=deadline-time.monotonic();assert remaining>0,'line timeout'
                assert self.selector.select(remaining),'line timeout'
                data=os.read(self.p.stdout.fileno(),65536);assert data,'unexpected EOF';self.buffer.extend(data)
            line,_,rest=self.buffer.partition(b'\n');self.buffer=bytearray(rest);return json.loads(line)
    class Mcp:
        def __init__(self,credential):
            self.p=subprocess.Popen([str(binary),'--database',str(db),'--credential-file',str(credential),'mcp','serve'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            children.append(self.p);self.lines=Lines(self.p);self.serial=0;self.notifications=[]
            self.request('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'access-qa','version':'1'}})
            self.p.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n');self.p.stdin.flush()
        def request(self,method,args,error=None):
            self.serial+=1;self.p.stdin.write((json.dumps({'jsonrpc':'2.0','id':self.serial,'method':method,'params':args})+'\n').encode());self.p.stdin.flush()
            while True:
                out=self.lines.read()
                if out.get('id')==self.serial:break
                self.notifications.append(out)
            trace.append({'mcp':method,'response':out})
            if error:assert out['error']['data']['code']==error,out;return
            assert 'error' not in out,out;return out['result']
        def call(self,name,args,error=None):
            result=self.request('tools/call',{'name':name,'arguments':args});out=result['structuredContent']
            if error:assert out['error']['code']==error and result['isError'],out
            else:assert out['error'] is None,out
            return out.get('data')
    try:
        a=create('a');b=create('b');source=root/'red.mp4'
        subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=c=red:s=64x48:r=10:d=1','-c:v','libx264','-pix_fmt','yuv420p',str(source)],check=True,timeout=30)
        at=lambda n:{'ticks':str(n),'timebase':10}
        asset={'id':'source','name':'red','kind':'video','uri':str(source),'duration':at(10),'metadata':{}}
        setup={'project_id':'a','request':{'expected_revision':a['revision'],'idempotency_key':'setup','label':'red','operations':[
            {'type':'asset.bind','asset':asset},
            {'type':'track.create','sequence_id':'main','track':{'id':'video','name':'video','kind':'video','enabled':True,'muted':True,'locked':False,'clips':[],'params':{}}},
            {'type':'clip.insert','sequence_id':'main','track_id':'video','clip':{'id':'clip','asset_id':'source','timeline_range':{'start':at(0),'duration':at(10)},'source_range':{'start':at(0),'duration':at(10)},'params':{},'channels':[]}}]}}
        revision=call('opencut_edit_apply',setup)['revision']
        grants=['read','edit','run_jobs','cancel_jobs','read_audit'];policy('editor',0,grants);policy('reader',0,['read'])
        editor,editor_id=issue('editor','editor.secret');reader,reader_id=issue('reader','reader.secret');other,other_id=issue('editor','other.secret')
        assert command(['access','inspect'],credential=editor)['principal_id']=='editor'
        assert [p['project']['id'] for p in call('opencut_project_list',{},editor)]==['a']
        for identity in (reader,editor):
            call('opencut_project_inspect',{'project_id':'b'},identity,'NOT_AUTHORIZED')
            call('opencut_revision_inspect',{'project_id':'b','revision':b['revision']},identity,'NOT_AUTHORIZED')
            command(['access','issue','editor','--output',str(root/'forbidden.secret')],credential=identity,error='NOT_AUTHORIZED')
        assert not (root/'forbidden.secret').exists()
        with sqlite3.connect(db) as c:
            digests=[r[0] for r in c.execute('SELECT secret_digest FROM access_credentials')]
            assert editor.read_text().strip() not in digests
        checks.append('Private credential files, digest-only storage, distinct principals and cross-project denial through real CLI')
        request=edit(revision,'scoped-edit');call('opencut_edit_plan',request,reader,'NOT_AUTHORIZED');call('opencut_edit_apply',request,reader,'NOT_AUTHORIZED')
        call('opencut_edit_plan',request,editor);changed=call('opencut_edit_apply',request,editor);assert call('opencut_edit_apply',request,editor)==changed
        call('opencut_edit_apply',setup,editor,'NOT_AUTHORIZED') # even a known receipt containing host paths
        guessed_artifact='artifact:job_'+'0'*32+':'+'0'*64
        for tool,args in [('opencut_job_get',{'job_id':'guessed'}),('opencut_plugin_list',{}),('opencut_media_inspect',{'id':'guessed'}),('opencut_artifact_retention_inspect',{'artifact_id':guessed_artifact}),('opencut_artifact_references_inspect',{'artifact_id':guessed_artifact}),('opencut_artifact_gc_plan',{'artifact_id':guessed_artifact,'idempotency_key':'forbidden-plan'}),('opencut_artifact_gc_inspect',{'plan_id':'agc_guessed'}),('opencut_artifact_gc_query',{}),('opencut_artifact_retention_release',{'artifact_id':guessed_artifact,'idempotency_key':'forbidden-release'})]:call(tool,args,editor,'NOT_AUTHORIZED')
        call('opencut_job_submit',{'kind':'media.probe','idempotency_key':'path-injection','input':{'input':str(source)}},editor,'NOT_AUTHORIZED')
        command(['worker','run','--once'],credential=editor,error='NOT_AUTHORIZED')
        command(['serve'],credential=editor,error='NOT_AUTHORIZED')
        records=call('opencut_audit_query',{'target':{'kind':'transaction','id':changed['transaction_id']}},editor)['records'];assert len(records)==1 and records[0]['actor']['principal_id']=='editor'
        assert call('opencut_audit_query',{'target':{'kind':'workspace'}},reader)['records']==[]
        checks.append('Read-only denial; scoped plan/apply/retry; no native-path, Worker, global media, plugin or credential administration escalation')
        mcp=Mcp(editor);assert mcp.call('opencut_access_inspect',{})['principal_id']=='editor'
        mcp.call('opencut_project_inspect',{'project_id':'b'},'NOT_AUTHORIZED')
        mcp.request('resources/read',{'uri':'opencut://projects/b/branches/main/summary'},'NOT_AUTHORIZED')
        prompt=mcp.request('prompts/list',{})['prompts'][0]['name']
        mcp.request('prompts/get',{'name':prompt,'arguments':{'project_id':'b','revision':b['revision'],'sequence_id':'main'}},'NOT_AUTHORIZED')
        own_uri='opencut://projects/a/branches/main/summary';mcp.request('resources/read',{'uri':own_uri});mcp.request('resources/subscribe',{'uri':own_uri})
        latest=mcp.call('opencut_edit_apply',edit(changed['revision'],'mcp-edit'))
        assert call('opencut_audit_query',{'target':{'kind':'transaction','id':latest['transaction_id']}},editor)['records'][0]['actor']=={'principal_id':'editor','source':'mcp_stdio','worker_id':None}
        checks.append('Stdio MCP tools, resource and prompt contexts retain the credential; successful writes carry the verified actor')
        job_input={'kind':'preview.composited_frame','idempotency_key':'render-a','input':{'project_id':'a','revision':latest['revision'],'sequence_id':'main','requested_time':at(0),'alignment':'exact'}}
        task=command(['job','start','--input','-'],job_input,editor);assert task['state']=='queued' and 'input' not in task
        command(['worker','run','--once'])
        task=command(['job','wait',task['job_id'],'--timeout-seconds','10'],credential=editor);assert task['state']=='succeeded' and 'input' not in task
        call('opencut_job_events',{'after_sequence':0,'job_id':task['job_id'],'limit':100},editor,'NOT_AUTHORIZED')
        changes=command(['job','events','--job-id',task['job_id']],credential=editor)
        assert changes['events'] and all('payload' not in event for event in changes['events'])
        artifacts=task['artifacts'];assert len(artifacts)==1
        artifact_uri='opencut://artifacts/'+quote(artifacts[0]['artifact_id'],safe='')
        call('opencut_resource_read',{'uri':artifact_uri},editor,'NOT_AUTHORIZED')
        policy('editor',1,grants+['read_artifacts']);call('opencut_resource_read',{'uri':artifact_uri},editor)
        raw=call('opencut_job_get',{'job_id':task['job_id']});files=list((root/'artifacts'/task['job_id']).rglob('*.png'));assert len(files)==1
        assert hashlib.sha256(files[0].read_bytes()).hexdigest()==artifacts[0]['content_hash']
        pixels=subprocess.check_output(['ffmpeg','-v','error','-i',str(files[0]),'-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30)
        assert len(pixels)==64*48*3 and sum(pixels[0::3])/3072>245 and sum(pixels[1::3])/3072<5
        assert call('opencut_job_submit',job_input,editor)['job_id']==task['job_id']
        checks.append('Scoped fixed-revision submission, actual Worker/FFmpeg PNG, independent hash/red pixel checks, public wait and separate artifact permission')
        view=command(['job','list'],credential=editor);assert len(view['jobs'])==1
        observed=call('opencut_event_changes',{'scope':{'kind':'projects'},'limit':100},editor);assert all(e['event']['data']['project_id']=='a' for e in observed['events'])
        watcher=subprocess.Popen([str(binary),'--database',str(db),'--credential-file',str(reader),'events','--format','ndjson','--watch'],stdout=subprocess.PIPE,stderr=subprocess.PIPE);children.append(watcher);lines=Lines(watcher)
        assert lines.read()['type']=='ready'
        command(['access','revoke',reader_id])
        while True:
            line=lines.read()
            if line['type']=='error':break
        assert line['error']['code']=='NOT_AUTHORIZED';assert watcher.wait(timeout=5)==3
        checks.append('Project/job collections and event pages filter before pagination; revocation ends a live NDJSON observation')
        command(['access','revoke',editor_id])
        mcp.call('opencut_project_inspect',{'project_id':'a'},'NOT_AUTHORIZED');mcp.call('opencut_edit_apply',edit(changed['revision'],'mcp-edit'),'NOT_AUTHORIZED')
        mcp.request('resources/read',{'uri':own_uri},'NOT_AUTHORIZED')
        call('opencut_job_submit',job_input,editor,'NOT_AUTHORIZED')
        assert command(['access','inspect'],credential=other)['principal_id']=='editor'
        policy('editor',2,grants+['read_artifacts'],False);call('opencut_project_list',{},other,'NOT_AUTHORIZED')
        checks.append('Revocation rechecks an existing MCP connection and receipt retries; another credential survives until the principal is disabled')
        for path in (editor,reader,other):assert path.read_text().strip() not in json.dumps(trace)
        report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root),'scope':'CLI and stdio MCP scoped credentials; network authorization is verified separately; not full R2 completion'}
        (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    except Exception as error:
        (root/'failure.json').write_text(json.dumps({'error':repr(error),'completed_checks':checks},indent=2));raise
    finally:
        for child in children:
            if child.poll() is None:
                child.terminate()
                try:child.wait(timeout=5)
                except subprocess.TimeoutExpired:child.kill();child.wait()
        (root/'trace.json').write_text(json.dumps(trace,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path)
    args=parser.parse_args();verify(args.binary.resolve(),(args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-access-'))).resolve())
