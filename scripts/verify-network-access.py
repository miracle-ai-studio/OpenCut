#!/usr/bin/env python3
"""Real HTTP/MCP scoped identities, cached replay, live streams and artifact revocation."""
import argparse, hashlib, http.client, json, socket, sqlite3, subprocess, tempfile, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.parse import quote


def verify(binary, root):
    root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';owner_file=root/'owner-token'
    trace=[];checks=[];children=[];connections=[];serial=0;revisions={}
    def cli(args,payload=None):
        p=subprocess.run([str(binary),'--database',str(db),'--json']+args,input=json.dumps(payload) if payload is not None else None,text=True,capture_output=True,timeout=30)
        out=json.loads(p.stdout);assert p.returncode==0 and out['error'] is None,(args,p.stderr,out)
        trace.append({'cli':args,'response':out});return out['data']
    def policy(principal,project,actions,enabled=True):
        data=cli(['access','set-policy','--input','-'],{'principal_id':principal,'expected_revision':revisions.get(principal,'0'),'enabled':enabled,'grants':[{'project_id':project,'actions':actions}] if actions else []})
        revisions[principal]=data['revision'];return data
    def issue(principal,name):
        file=root/name;out=cli(['access','issue',principal,'--output',str(file)]);return file.read_text().strip(),out['credential_id']
    def raw(token,method,path,body=None,headers=None,stream=False,small_window=False):
        connection=http.client.HTTPConnection('127.0.0.1',port,timeout=15);connection.connect()
        if small_window:connection.sock.setsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF,4096)
        h={'Authorization':'Bearer '+token,'Content-Type':'application/json','Accept':'application/json, text/event-stream'};h.update(headers or {})
        if body is not None and not isinstance(body,bytes):body=json.dumps(body).encode()
        connection.request(method,path,body=body,headers=h);response=connection.getresponse()
        if stream:connections.append(connection);return connection,response
        value=response.read();status=response.status;hs=dict(response.getheaders());connection.close();return status,hs,value
    def call(token,name,args,error=None,extra=None):
        status,_,data=raw(token,'POST','/api/tools/'+name,args,extra);out=json.loads(data)
        trace.append({'http':name,'status':status,'response':out})
        if error:assert status==403 and out['error']['code']==error,out
        else:assert status==200 and out['error'] is None,out
        return out.get('data')
    def denied(token,method,path,body=None):
        status,_,data=raw(token,method,path,body);out=json.loads(data);assert status==403 and out['error']['code']=='NOT_AUTHORIZED',(status,path,out)
        trace.append({'denied':path,'status':status})
    def sse(response):
        data=[];event_id=None;event=None
        while True:
            line=response.readline()
            if not line:return None
            line=line.decode().rstrip('\r\n')
            if not line:
                if data or event_id:return {'id':event_id,'event':event,'data':'\n'.join(data)}
                continue
            if line.startswith('id:'):event_id=line[3:].strip()
            elif line.startswith('event:'):event=line[6:].strip()
            elif line.startswith('data:'):data.append(line[5:].lstrip(' '))
    def rpc(token,session,method,args,error=None):
        nonlocal serial
        serial+=1;identifier=serial
        headers={'Mcp-Session-Id':session,'Mcp-Protocol-Version':'2025-11-25'} if session else {}
        connection,response=raw(token,'POST','/mcp',{'jsonrpc':'2.0','id':identifier,'method':method,'params':args},headers,True)
        assert response.status==200,(response.status,response.read())
        ids=[]
        while True:
            event=sse(response);assert event,'Missing MCP response'
            if event['id']:ids.append(event['id'])
            if not event['data']:continue
            value=json.loads(event['data'])
            if value.get('id')==identifier:break
        sid=response.getheader('Mcp-Session-Id') or session;connection.close()
        trace.append({'mcp':method,'response':value})
        if error:
            assert value['error']['data']['code']==error,value
            return sid,None,ids
        assert 'error' not in value,value
        return sid,value['result'],ids
    def initialize(token):
        sid,result,_=rpc(token,None,'initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'network-access-qa','version':'1'}})
        assert sid and result['protocolVersion']=='2025-11-25'
        status,_,_=raw(token,'POST','/mcp',{'jsonrpc':'2.0','method':'notifications/initialized'}, {'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25'});assert status==202,status
        return sid
    def mcp_call(token,sid,name,args,error=None):
        _,result,ids=rpc(token,sid,'tools/call',{'name':name,'arguments':args});out=result['structuredContent']
        if error:assert result['isError'] and out['error']['code']==error,out
        else:assert out['error'] is None,out
        return out.get('data'),ids
    def edit(project,revision,key):return {'project_id':project,'request':{'expected_revision':revision,'idempotency_key':key,'label':key,'operations':[{'type':'sequence.update_settings','sequence_id':'main','name':key}]}}
    try:
        with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
        with (root/'host.log').open('w') as log:
            host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(owner_file)],stdout=log,stderr=log)
        children.append(host);deadline=time.monotonic()+20
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):break
            except OSError:assert time.monotonic()<deadline;time.sleep(.03)
        owner=owner_file.read_text().strip();source=root/'noise.png'
        # FFmpeg's geq random expression can correlate channels and compress
        # below the multi-megabyte threshold required by the backpressure test.
        import random
        raw_frame=root/'noise.rgb'
        raw_frame.write_bytes(random.Random(0).randbytes(1920*1080*3))
        try:
            subprocess.run(['ffmpeg','-v','error','-f','rawvideo','-pixel_format','rgb24',
                '-video_size','1920x1080','-i',str(raw_frame),'-frames:v','1',str(source)],check=True,timeout=60)
        finally:raw_frame.unlink(missing_ok=True)
        assert source.stat().st_size>4_000_000
        snapshots={}
        for project,width,height in [('a',1920,1080),('b',64,48)]:
            created=call(owner,'opencut_project_create',{'project':{'id':project,'name':project,'sequence_id':'main','width':width,'height':height,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create-'+project})
            at=lambda n:{'ticks':str(n),'timebase':10}
            setup={'project_id':project,'request':{'expected_revision':created['revision'],'idempotency_key':'setup-'+project,'label':'image','operations':[
                {'type':'asset.bind','asset':{'id':'source','name':'image','kind':'image','uri':str(source),'duration':None,'metadata':{}}},
                {'type':'track.create','sequence_id':'main','track':{'id':'visual','name':'image','kind':'graphic','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}},
                {'type':'clip.insert','sequence_id':'main','track_id':'visual','clip':{'id':'clip','asset_id':'source','timeline_range':{'start':at(0),'duration':at(10)},'source_range':None,'params':{},'channels':[]}}]}}
            snapshots[project]=call(owner,'opencut_edit_apply',setup)['revision']
        full=['read','edit','restore','run_jobs','cancel_jobs','read_artifacts','read_audit']
        policy('editor-a','a',full);policy('editor-b','b',full);policy('reader-a','a',['read'])
        a,a_id=issue('editor-a','a.secret');a2,a2_id=issue('editor-a','a2.secret');b,b_id=issue('editor-b','b.secret');reader,reader_id=issue('reader-a','reader.secret')
        assert 'http' in call(a,'opencut_capabilities',{})['access']['scoped_transports']
        def read_project(token):
            status,_,payload=raw(token,'GET','/api/projects');data=json.loads(payload);assert status==200;return [p['project']['id'] for p in data['data']]
        with ThreadPoolExecutor(max_workers=8) as pool:
            outputs=list(pool.map(read_project,[a,b]*12))
        assert outputs==[['a'],['b']]*12
        denied(a,'GET','/api/projects/b/snapshot');denied(b,'GET','/api/projects/a/revisions/'+snapshots['a'])
        denied(a,'POST','/api/media/upload?name=forbidden',b'no file');denied(a,'GET','/api/media/'+'f'*64)
        for name,args in [('opencut_plugin_list',{}),('opencut_media_inspect',{'hash':'f'*64}),('opencut_job_get',{'job_id':'guessed'})]:call(a,name,args,'NOT_AUTHORIZED')
        call(a,'opencut_job_submit',{'kind':'media.probe','idempotency_key':'native-path','input':{'input':str(source)}},'NOT_AUTHORIZED')
        changed=call(a,'opencut_edit_apply',edit('a',snapshots['a'],'http-a'),extra={'X-Actor':'local-owner','X-Principal-Id':'editor-b'})
        call(reader,'opencut_edit_apply',edit('a',snapshots['a'],'http-a'),'NOT_AUTHORIZED')
        records=call(a,'opencut_audit_query',{'target':{'kind':'transaction','id':changed['transaction_id']}})['records'];assert records[0]['actor']['principal_id']=='editor-a' and records[0]['actor']['source']=='http'
        checks.append('24 concurrent HTTP reads preserve request identity; project/history/media/native tools and read-only replay enforce scope; actor headers cannot impersonate')
        sid=initialize(a);sid_b=initialize(b);sid_owner=initialize(owner)
        assert mcp_call(a,sid,'opencut_access_inspect',{})[0]['principal_id']=='editor-a'
        mcp_call(a,sid,'opencut_project_inspect',{'project_id':'b'},'NOT_AUTHORIZED')
        for token in [a2,b,owner]:
            for method in ['GET','POST','DELETE']:
                body={'jsonrpc':'2.0','id':999,'method':'tools/list','params':{}} if method=='POST' else None
                status,_,data=raw(token,method,'/mcp',body,{'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25'})
                assert status==403,(method,status,data)
        assert mcp_call(a,sid,'opencut_access_inspect',{})[0]['principal_id']=='editor-a'
        # Context methods must read exactly the same authenticated scope.
        _,resource,_=rpc(a,sid,'resources/read',{'uri':'opencut://projects/a/branches/main/summary'})
        _,prompts,_=rpc(a,sid,'prompts/list',{});prompt=prompts['prompts'][0]['name']
        _,context,_=rpc(a,sid,'prompts/get',{'name':prompt,'arguments':{'project_id':'a','revision':changed['revision'],'sequence_id':'main'}})
        assert context['messages']
        rpc(a,sid,'resources/read',{'uri':'opencut://projects/b/branches/main/summary'},'NOT_AUTHORIZED')
        rpc(a,sid,'prompts/get',{'name':prompt,'arguments':{'project_id':'b','revision':snapshots['b'],'sequence_id':'main'}},'NOT_AUTHORIZED')
        rpc(a,sid,'resources/subscribe',{'uri':'opencut://projects/b/branches/main/summary'},'NOT_AUTHORIZED')
        rpc(a,sid,'resources/subscribe',{'uri':'opencut://projects/a/branches/main/summary'})
        latest,_=mcp_call(a,sid,'opencut_edit_apply',edit('a',changed['revision'],'mcp-http-a'))
        audit=call(a,'opencut_audit_query',{'target':{'kind':'transaction','id':latest['transaction_id']}})['records'];assert audit[0]['actor']['source']=='mcp_http' and audit[0]['actor']['principal_id']=='editor-a'
        cached,cache_ids=mcp_call(a,sid,'opencut_project_inspect',{'project_id':'a'});assert cache_ids
        checks.append('MCP HTTP tools/resources/prompts/subscriptions carry verified identity; other credentials including same-principal and owner cannot use GET/POST/DELETE on the session')
        scope={'kind':'projects','project_id':'a'};expected=call(a,'opencut_event_changes',{'scope':scope,'limit':100})['events']
        stream_conn,stream=raw(a,'POST','/api/changes/stream',{'scope':scope,'limit':2},stream=True)
        assert stream.status==200
        first=json.loads(sse(stream)['data']);assert first['format']=='opencut.events.v1' and first['type']=='ready'
        observed=[]
        for _ in expected:
            event=sse(stream);value=json.loads(event['data']);assert value['cursor']==event['id'];observed.append(value)
        assert [v['event'] for v in observed]==[v['event'] for v in expected]
        resume_conn,resume=raw(a,'POST','/api/changes/stream',{'scope':scope,'cursor':'overridden','limit':1},{'Last-Event-ID':observed[0]['cursor']},True)
        assert resume.status==200;assert json.loads(sse(resume)['data'])['type']=='ready';assert json.loads(sse(resume)['data'])['cursor']==observed[1]['cursor'];resume_conn.close()
        denied(a,'POST','/api/changes/stream',{'scope':{'kind':'projects','project_id':'b'}})
        denied(a,'GET','/api/job-events/stream')
        checks.append('Public SSE preserves shared scope, record schema, per-event opaque cursors and Last-Event-ID resume; unauthorized filters and legacy raw job streams reject')
        tasks={}
        for token,project,revision in [(b,'b',snapshots['b']),(a,'a',latest['revision'])]:
            request={'kind':'preview.composited_frame','idempotency_key':'frame-'+project,'input':{'project_id':project,'revision':revision,'sequence_id':'main','requested_time':at(0),'alignment':'exact'}}
            task=call(token,'opencut_job_submit',request);deadline=time.monotonic()+90
            while task['state'] not in ['succeeded','failed','cancelled']:
                assert time.monotonic()<deadline;time.sleep(.05);task=call(token,'opencut_job_view',{'job_id':task['job_id']})
            assert task['state']=='succeeded',task;tasks[project]=task
        call(a,'opencut_job_view',{'job_id':tasks['b']['job_id']},'NOT_AUTHORIZED')
        assert [job['job_id'] for job in call(a,'opencut_job_query',{})['jobs']]==[tasks['a']['job_id']]
        reference=tasks['a']['artifacts'][0];path='/api/artifacts/'+quote(reference['artifact_id'],safe='')
        for token in [b,reader]:denied(token,'GET',path)
        status,headers,data=raw(a,'GET',path);assert status==200 and len(data)==int(reference['size_bytes']) and hashlib.sha256(data).hexdigest()==reference['content_hash']
        output=root/'download.png';output.write_bytes(data)
        decoded=subprocess.check_output(['ffmpeg','-v','error','-i',str(output),'-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30)
        assert len(decoded)==1920*1080*3 and len(set(decoded))>200
        assert len(data)>4_000_000
        checks.append('Scoped HTTP fixed-revision jobs execute real image composition; lists isolate jobs and downloads verify SHA-256 and decoded pixels while read-only/foreign credentials fail')
        # Fault injection: published MIME metadata must not disable authorization
        # (the same path will also serve future JSON analysis artifacts).
        with sqlite3.connect(db) as connection:
            connection.execute("UPDATE jobs SET result_json=json_set(result_json,'$.artifact.mime_type','application/json') WHERE job_id=?",(tasks['a']['job_id'],))
        # The earlier stream legitimately received job events after its first
        # page. Start the revocation probe at the current durable cursor so
        # buffered pre-revocation events cannot be mistaken for a leak.
        stream_conn.close()
        current_events=call(a,'opencut_event_changes',{'scope':scope,'limit':100})['events']
        current_cursor=current_events[-1]['cursor']
        stream_conn,stream=raw(a,'POST','/api/changes/stream',{'scope':scope,'limit':2},
                               {'Last-Event-ID':current_cursor},True)
        assert stream.status==200 and json.loads(sse(stream)['data'])['type']=='ready'
        # Force actual TCP backpressure so revocation occurs while bytes remain unsent.
        download_conn,download=raw(a,'GET',path,stream=True,small_window=True);assert download.status==200 and download.getheader('Content-Type')=='application/json'
        prefix=download.read(8192);assert len(prefix)==8192
        policy('editor-a','a',[action for action in full if action!='read_artifacts'])
        time.sleep(.35)
        received=len(prefix)
        try:
            while True:
                chunk=download.read(65536)
                if not chunk:break
                received+=len(chunk)
        except http.client.IncompleteRead as error:received+=len(error.partial)
        assert received<int(reference['size_bytes']),(received,reference['size_bytes'])
        trace.append({'inflight_download_expected':reference['size_bytes'],'received_before_termination':received})
        download_conn.close();denied(a,'GET',path)
        # Policy epoch changes invalidate cached responses, even with a live credential.
        status,_,data=raw(a,'GET','/mcp',headers={'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25','Last-Event-ID':cache_ids[0]})
        assert status in [403,404],(status,data)
        assert not stream.read(1),'policy-changed SSE emitted bytes after its prior page'
        stream_conn.close()
        checks.append('Policy update interrupts a real backpressured multi-megabyte artifact download and live SSE; old MCP session/replay cannot return cached project data')
        policy('editor-a','a',full)
        fresh=initialize(a);mcp_call(a,fresh,'opencut_project_inspect',{'project_id':'a'})
        get_conn,get_stream=raw(a,'GET','/mcp',headers={'Mcp-Session-Id':fresh,'Mcp-Protocol-Version':'2025-11-25'},stream=True);assert get_stream.status==200
        cli(['access','revoke',a_id]);time.sleep(.35)
        assert not get_stream.read(1);get_conn.close()
        denied(a,'POST','/api/tools/opencut_project_list',{})
        denied(a,'POST','/mcp',{'jsonrpc':'2.0','id':1,'method':'tools/list','params':{}})
        assert call(a2,'opencut_access_inspect',{})['principal_id']=='editor-a'
        policy('editor-a','a',full,False);denied(a2,'GET','/api/projects')
        assert mcp_call(b,sid_b,'opencut_access_inspect',{})[0]['principal_id']=='editor-b'
        assert mcp_call(owner,sid_owner,'opencut_access_inspect',{})[0]['owner']
        checks.append('Credential revocation closes an existing MCP GET, rejects further requests, preserves another credential until principal disable, and leaves unrelated/owner sessions working')
        secrets=[owner,a,a2,b,reader]
        assert not any(secret in json.dumps(trace) for secret in secrets)
        report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root),'scope':'loopback scoped HTTP/MCP control, streams and artifacts; not external OAuth, OS isolation or full R0-R7 acceptance'}
        (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    except Exception as error:
        (root/'failure.json').write_text(json.dumps({'error':repr(error),'completed_checks':checks},indent=2));raise
    finally:
        for connection in connections:connection.close()
        for child in children:
            if child.poll() is None:
                child.terminate()
                try:child.wait(timeout=8)
                except subprocess.TimeoutExpired:child.kill();child.wait()
        (root/'trace.json').write_text(json.dumps(trace,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path)
    args=parser.parse_args();verify(args.binary.resolve(),(args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-network-access-'))).resolve())
