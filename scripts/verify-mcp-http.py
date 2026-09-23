#!/usr/bin/env python3
"""Real Streamable HTTP lifecycle, auth, session and stream-bound recovery."""
import argparse
import hashlib
import http.client
import json
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time


def verify(binary, root):
    checks, logs, hosts, opened = [], [], [], []
    def host(name='main', timeout=300, alias=False):
        directory = root/name; directory.mkdir()
        db, token = directory/'workspace.sqlite', directory/'token'
        with socket.socket() as s:
            s.bind(('127.0.0.1',0)); port=s.getsockname()[1]
        cmd=[str(binary),'--database',str(db)]
        if alias: cmd += ['mcp','serve','--transport','streamable-http','--session-timeout-seconds',str(timeout)]
        else: cmd += ['serve','--mcp-session-timeout-seconds',str(timeout)]
        cmd += ['--listen',f'127.0.0.1:{port}','--token-file',str(token),'--allow-origin','http://localhost:5173']
        diagnostics=(directory/'host.log').open('wb')
        process=subprocess.Popen(cmd,stdout=diagnostics,stderr=diagnostics)
        hosts.append(process)
        deadline=time.monotonic()+15
        while True:
            assert process.poll() is None, (cmd,(directory/'host.log').read_text())
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2): break
            except OSError:
                assert time.monotonic()<deadline
                time.sleep(.03)
        diagnostics.close()
        return {'db':db,'port':port,'token':token.read_text().strip(),'process':process}
    def raw(h, method='POST', body=None, session=None, headers=None, auth=True, path='/mcp', stream=False, chunked=False):
        hs={'Accept':'application/json, text/event-stream','Content-Type':'application/json'}
        if auth: hs['Authorization']='Bearer '+h['token']
        if session: hs.update({'Mcp-Session-Id':session,'Mcp-Protocol-Version':'2025-11-25'})
        for k,v in (headers or {}).items():
            if v is None: hs.pop(k,None)
            else: hs[k]=v
        conn=http.client.HTTPConnection('127.0.0.1',h['port'],timeout=12)
        conn.request(method,path,body=json.dumps(body).encode() if isinstance(body,(dict,list)) else body,headers=hs,encode_chunked=chunked)
        response=conn.getresponse()
        if stream and response.status==200:
            opened.append((conn,response));return conn,response
        data=response.read(); status=response.status; outheaders=dict(response.getheaders());conn.close()
        logs.append({'method':method,'path':path,'session':session,'status':status,'headers':outheaders,'body':data.decode(errors='replace')[:5000]})
        return status,outheaders,data
    def event(response):
        fields={}
        while True:
            line=response.readline()
            if not line: return None
            line=line.decode().rstrip('\r\n')
            if not line:
                if fields: return fields
                continue
            if line.startswith(':'):continue
            key,_,value=line.partition(':');fields[key]=value.lstrip(' ')
    serial=0
    def request(h,session,method,params=None,extra=None):
        nonlocal serial
        serial+=1;rid=serial
        conn,response=raw(h,body={'jsonrpc':'2.0','id':rid,'method':method,'params':params or {}},session=session,headers=extra,stream=True)
        result=None;events=[]
        while True:
            e=event(response)
            if e is None:break
            events.append(e)
            if e.get('data'):
                msg=json.loads(e['data'])
                if msg.get('id')==rid:result=msg
        assert result is not None,(method,events)
        logs.append({'method':method,'response':result,'events':events})
        response.close();conn.close()
        assert 'error' not in result,result
        return result['result'],events
    def init(h,version='2025-11-25'):
        nonlocal serial
        serial+=1
        conn,response=raw(h,body={'jsonrpc':'2.0','id':serial,'method':'initialize','params':{
            'protocolVersion':version,'capabilities':{},'clientInfo':{'name':'http-acceptance','version':'1'}}},stream=True)
        sid=response.getheader('Mcp-Session-Id');assert sid
        events=[];result=None
        while True:
            e=event(response)
            if e is None:break
            events.append(e)
            if e.get('data'):result=json.loads(e['data'])
        response.close();conn.close()
        assert result['result']['protocolVersion']=='2025-11-25',result
        status,_,data=raw(h,session=sid,body={'jsonrpc':'2.0','method':'notifications/initialized'})
        assert status==202 and not data,(status,data)
        return sid
    def cli(h,name,args):
        p=subprocess.run([str(binary),'--database',str(h['db']),'--json','call',name,'--input','-'],input=json.dumps(args),text=True,capture_output=True,timeout=20)
        assert p.returncode==0,(p.stdout,p.stderr)
        return json.loads(p.stdout)['data']
    def creation(name):
        return {'project':{'id':name,'name':name,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':30,'denominator':1}},'idempotency_key':'create-'+name}
    def edit(h,project,index):
        rev=cli(h,'opencut_project_inspect',{'project_id':project})['revision']
        return cli(h,'opencut_edit_apply',{'project_id':project,'request':{'expected_revision':rev,'idempotency_key':f'edit-{index}','label':str(index),'operations':[{'type':'track.create','sequence_id':'main','track':{'id':f'track-{index}','name':str(index),'kind':'video','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}}]}})
    try:
        h=host()
        capability=cli(h,'opencut_capabilities',{})['mcp']
        assert capability['transports']==['stdio','streamable-http'] and capability['http_path']=='/mcp'
        assert capability['max_sessions']==32 and capability['max_resource_subscriptions']==32
        for method in ('POST','GET','DELETE'):
            assert raw(h,method,body={},auth=False)[0]==403
            assert raw(h,method,body={},headers={'Origin':'https://evil.invalid'})[0]==403
        assert raw(h,body={},headers={'Host':'evil.invalid'})[0]==403
        assert raw(h,body={},headers={'Host':'user@localhost'})[0]==403
        status,hs,_=raw(h,'OPTIONS',headers={'Origin':'http://localhost:5173'},auth=False)
        assert status==204 and 'DELETE' in hs['access-control-allow-methods']
        assert 'MCP-Session-Id' in hs['access-control-allow-headers']
        assert 'MCP-Session-Id' in hs['access-control-expose-headers']
        assert raw(h,body={'jsonrpc':'2.0','id':1,'method':'tools/list'})[0]==400
        for duplicate in ('Authorization','Origin'):
            conn=http.client.HTTPConnection('127.0.0.1',h['port'],timeout=5)
            conn.putrequest('POST','/mcp')
            conn.putheader('Authorization','Bearer '+h['token'])
            conn.putheader('Origin','http://localhost:5173')
            conn.putheader(duplicate,'invalid')
            conn.putheader('Content-Length','2');conn.endheaders(b'{}')
            response=conn.getresponse();assert response.status==403;response.read();conn.close()
        checks.append('Bearer/Origin/Host on all methods, browser preflight/header exposure, mandatory initialization')
        a=init(h);b=init(h);assert a!=b
        future=init(h,'2026-07-28')
        assert request(h,future,'resources/list')[0]['resources']
        raw(h,'DELETE',session=future)
        tools,_=request(h,a,'tools/list')
        assert {t['name'] for t in tools['tools']}=={'opencut_'+d['domain'] for d in json.loads(subprocess.check_output([str(binary),'system','domains','--json']))['data']}
        assert raw(h,session=a,body={},headers={'Mcp-Protocol-Version':'2026-07-28'})[0]==400
        assert raw(h,session=a,body={},headers={'Mcp-Protocol-Version':None})[0]==400
        assert raw(h,session='missing',body={})[0]==404
        assert raw(h,session=a,body={},headers={'Accept':'application/json'})[0]==406
        assert raw(h,session=a,body=b'x'*2_097_153)[0]==413
        assert raw(h,session=a,body=iter([b'x'*1_048_576,b'x'*1_048_577]),chunked=True)[0]==413
        assert raw(h,session=a,body={},headers={'Mcp-Session-Id':''})[0]==400
        status,headers,_=raw(h,'GET',session=a,headers={'Last-Event-ID':'x'*129})
        assert status==400 and headers.get('cache-control')=='no-store'
        checks.append('Session initialization, newer-version negotiation, discovery parity, version/Accept/body bounds')

        result,post_events=request(h,a,'tools/call',{'name':'opencut_project_create','arguments':creation('transport')})
        initial=result['structuredContent']['data'];assert not result.get('isError')
        assert cli(h,'opencut_project_inspect',{'project_id':'transport'})==initial
        uri='opencut://projects/transport/branches/main/summary'
        resource,_=request(h,a,'resources/read',{'uri':uri})
        assert json.loads(resource['contents'][0]['text'])==cli(h,'opencut_resource_read',{'uri':uri})
        prompt,_=request(h,a,'prompts/get',{'name':'caption_review','arguments':{'project_id':'transport','revision':initial['revision'],'sequence_id':'main'}})
        assert len(prompt['messages'])==2
        checks.append('Actual create/CLI persistence, resource parity and immutable prompt via Streamable HTTP')

        # Drop a POST after its persisted priming cursor, before observing its response.
        serial+=1;lost_id=serial
        conn,response=raw(h,session=a,body={'jsonrpc':'2.0','id':lost_id,'method':'tools/call','params':{'name':'opencut_project_create','arguments':creation('lost-response')}},stream=True)
        prime=event(response);assert prime.get('id','').startswith('mcp-event:') and not prime.get('data'),prime
        response.close();conn.close()
        deadline=time.monotonic()+5
        replayed=[]
        while not replayed:
            status,_,data=raw(h,'GET',session=a,headers={'Last-Event-ID':prime['id']})
            assert status==200,(status,data)
            replayed=[json.loads(line[5:].strip()) for line in data.decode().splitlines() if line.startswith('data:') and line[5:].strip()]
            assert time.monotonic()<deadline
            if not replayed:time.sleep(.05)
        assert len(replayed)==1 and replayed[0]['id']==lost_id,replayed
        assert cli(h,'opencut_project_inspect',{'project_id':'lost-response'})['project']['id']=='lost-response'
        assert raw(h,'GET',session=b,headers={'Last-Event-ID':prime['id']})[0]==400
        assert raw(h,'GET',session=a,headers={'Last-Event-ID':'missing-cursor'})[0]==410
        status,_,other=raw(h,'GET',session=a,headers={'Last-Event-ID':post_events[0]['id']})
        assert status==200 and all(json.loads(line[5:].strip()).get('id')!=lost_id for line in other.decode().splitlines() if line.startswith('data:') and line[5:].strip())
        checks.append('Disconnected POST still commits; replay returns only its original response; cross-session/unknown cursors rejected')

        last = post_events[-1]['id']
        status, _, empty = raw(h, 'GET', session=a, headers={'Last-Event-ID': last})
        assert status == 200
        # An ID/retry-only SSE block is not dispatched by standard parsers.
        # Emit an explicit empty data field to preserve the cursor on empty polls.
        assert f'id: {last}' in empty.decode() and 'data:' in empty.decode(), empty
        assert not any(line[5:].strip() for line in empty.decode().splitlines() if line.startswith('data:')), empty

        request(h,a,'resources/subscribe',{'uri':uri})
        c1,s1=raw(h,'GET',session=a,stream=True)
        edit(h,'transport',1)
        notice=event(s1)
        assert notice.get('id','').startswith('mcp-event:') and json.loads(notice['data'])['params']['uri']==uri,notice
        cursor=notice['id'];s1.close();c1.close()
        edit(h,'transport',2);time.sleep(.4)
        status,_,data=raw(h,'GET',session=a,headers={'Last-Event-ID':cursor})
        updates=[json.loads(line[5:].strip()) for line in data.decode().splitlines() if line.startswith('data:') and line[5:].strip()]
        assert status==200 and len(updates)==1 and updates[0]['method']=='notifications/resources/updated',(status,data)
        assert 'retry: 1000' in data.decode()
        # Use 65 sequential observed changes to evict the original common-stream cursor.
        c1,s1=raw(h,'GET',session=a,stream=True)
        for i in range(3,68):
            edit(h,'transport',i)
            received=event(s1)
            assert received and json.loads(received['data'])['params']['uri']==uri,received
        assert raw(h,'GET',session=a,headers={'Last-Event-ID':cursor})[0]==410
        request(h,a,'resources/unsubscribe',{'uri':uri})
        s1.close();c1.close()
        checks.append('Resource notifications survive GET disconnect; bounded stream replay explicitly expires after retention overflow')

        common_conn,common=raw(h,'GET',session=a,stream=True)
        status,_,_=raw(h,'DELETE',session=a);assert status in (200,202,204)
        assert raw(h,session=a,body={'jsonrpc':'2.0','id':900,'method':'tools/list'})[0]==404
        assert event(common) is None
        common.close();common_conn.close()
        assert request(h,b,'resources/list')[0]['resources']
        checks.append('DELETE closes active SSE and invalidates one session while another remains usable')

        short=host('short',timeout=1,alias=True)
        expired=init(short)
        time.sleep(2.3)
        assert raw(short,session=expired,body={'jsonrpc':'2.0','id':1,'method':'tools/list'})[0]==404
        checks.append('MCP CLI transport alias and real idle-session expiry')
        # b occupies one session; deleted a/future do not consume capacity.
        sessions=[b]+[init(h) for _ in range(31)]
        status,_,data=raw(h,body={'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'full','version':'1'}}})
        assert status==429,(status,data)
        raw(h,'DELETE',session=sessions.pop())
        sessions.append(init(h))
        checks.append('32-session admission bound and released slot reuse')
        live_conn,live=raw(h,'GET',session=b,stream=True)
        started=time.monotonic();h['process'].send_signal(signal.SIGINT);h['process'].wait(timeout=12)
        assert h['process'].returncode==0
        assert event(live) is None
        live.close();live_conn.close()
        checks.append('SIGINT shuts down active HTTP/SSE sessions and the shared host')
        return {'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'evidence':str(root)}
    finally:
        for conn,response in opened:
            response.close();conn.close()
        for p in hosts:
            if p.poll() is None:
                p.send_signal(signal.SIGINT)
                try:p.wait(timeout=12)
                except subprocess.TimeoutExpired:p.kill();p.wait()
        (root/'protocol.json').write_text(json.dumps(logs,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path)
    args=parser.parse_args();root=(args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-mcp-http-'))).resolve();root.mkdir(parents=True,exist_ok=True)
    report=verify(args.binary.resolve(),root);(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
