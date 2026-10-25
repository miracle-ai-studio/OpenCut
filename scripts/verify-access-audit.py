#!/usr/bin/env python3
"""Exercise committed administration history through real CLI, HTTP and both MCP transports."""
import argparse, hashlib, http.client, json, os, selectors, socket, sqlite3, subprocess, tempfile, time
from pathlib import Path


def verify(binary, root):
    root.mkdir(parents=True, exist_ok=True)
    db=root/'workspace.sqlite'; trace=[]; checks=[]; children=[]; serial=0
    query={'target':{'kind':'workspace'},'limit':500}
    def command(args, value=None, credential=None, error=None):
        args=[str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args
        p=subprocess.run(args,input=json.dumps(value) if value is not None else None,text=True,capture_output=True,timeout=30)
        out=json.loads(p.stdout);trace.append({'entry':'cli','args':args[4:],'response':out,'exit':p.returncode})
        if error:assert p.returncode!=0 and out['error']['code']==error,(p.stderr,out)
        else:assert p.returncode==0 and out['error'] is None,(p.stderr,out)
        return out
    def tool(value=query, credential=None, error=None):
        return command(['call','opencut_access_audit_query','--input','-'],value,credential,error)
    def records(value=query):return tool(value)['data']['records']
    def policy(principal,revision='0',enabled=True,actions=None,error=None):
        return command(['access','set-policy','--input','-'],{'principal_id':principal,'expected_revision':revision,'enabled':enabled,
            'grants':[{'project_id':'project','actions':actions or ['read','read_audit']}]},error=error)
    def issue(principal,name,error=None):return command(['access','issue',principal,'--output',str(root/name)],error=error)
    def sql(text,args=()):
        with sqlite3.connect(db) as c:return c.execute(text,args).fetchall()
    class Stdio:
        def __init__(self,credential=None):
            args=[str(binary),'--database',str(db)]+(['--credential-file',str(credential)] if credential else [])+['mcp','serve']
            self.p=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=(root/('stdio-'+str(len(children))+'.log')).open('w'));children.append(self.p)
            self.sel=selectors.DefaultSelector();self.sel.register(self.p.stdout,selectors.EVENT_READ);self.buf=bytearray()
            self.rpc('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'admin-history-qa','version':'1'}})
            self.p.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n');self.p.stdin.flush()
        def rpc(self,method,args):
            nonlocal serial
            serial+=1;identifier=serial
            self.p.stdin.write((json.dumps({'jsonrpc':'2.0','id':identifier,'method':method,'params':args})+'\n').encode());self.p.stdin.flush()
            deadline=time.monotonic()+15
            while True:
                while b'\n' not in self.buf:
                    remaining=deadline-time.monotonic();assert remaining>0 and self.sel.select(remaining),'MCP timeout'
                    chunk=os.read(self.p.stdout.fileno(),65536);assert chunk,'MCP EOF';self.buf.extend(chunk)
                line,_,self.buf=self.buf.partition(b'\n');message=json.loads(line)
                if message.get('id')==identifier:
                    assert 'error' not in message,message
                    return message['result']
        def call(self,args=query,error=None):
            result=self.rpc('tools/call',{'name':'opencut_access_audit_query','arguments':args});out=result['structuredContent']
            trace.append({'entry':'mcp_stdio','response':out})
            if error:assert result['isError'] and out['error']['code']==error,out
            else:assert out['error'] is None,out
            return out
    def request(secret,path,args,extra=None):
        c=http.client.HTTPConnection('127.0.0.1',port,timeout=15)
        headers={'Authorization':'Bearer '+secret,'Content-Type':'application/json','Accept':'application/json, text/event-stream'};headers.update(extra or {})
        c.request('POST',path,json.dumps(args),headers);r=c.getresponse();value=r.read();status=r.status;hs=dict(r.getheaders());c.close();return status,hs,value
    def rpc(secret,sid,method,args):
        nonlocal serial
        serial+=1;identifier=serial
        status,headers,data=request(secret,'/mcp',{'jsonrpc':'2.0','id':identifier,'method':method,'params':args},
            {'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25'} if sid else {})
        assert status==200,(status,data)
        messages=[json.loads(line[5:].strip()) for line in data.decode().splitlines() if line.startswith('data:') and line[5:].strip()]
        message=next(m for m in messages if m.get('id')==identifier);assert 'error' not in message,message
        return headers.get('mcp-session-id',headers.get('Mcp-Session-Id',sid)),message['result']
    def init(secret):
        sid,_=rpc(secret,None,'initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'admin-history-http-qa','version':'1'}});assert sid
        status,_,_=request(secret,'/mcp',{'jsonrpc':'2.0','method':'notifications/initialized'},{'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25'});assert status==202
        return sid
    try:
        command(['call','opencut_project_create','--input','-'],{'project':{'id':'project','name':'project','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
        assert records()==[]
        p1=policy('reviewer');created=records();assert len(created)==1
        assert created[0]['request_id']==p1['request_id'] and created[0]['policy']==p1['data']
        assert created[0]['actor']=={'principal_id':'local-owner','source':'cli','worker_id':None}
        issued=issue('reviewer','reviewer.secret');secret_file=root/'reviewer.secret';credential=issued['data']['credential_id']
        update=policy('reviewer','1',actions=['read','edit','read_audit'])
        policy("audit' OR '1'='1")
        current=records();assert len(current)==4 and current[0]==created[0]
        assert current[2]['policy']==update['data'] and current[2]['request_id']==update['request_id']
        assert current[1]['credential_id']==credential and current[1]['policy'] is None
        assert current[1]['request_id']==issued['request_id']
        assert records({'target':{'kind':'principal','id':"audit' OR '1'='1"}})==[current[3]]
        assert records({'target':{'kind':'request','id':update['request_id']}})==[current[2]]
        assert records({'target':{'kind':'credential','id':credential}})==[current[1]]
        checks.append('Real policy/issue/update writes expose original actor/request and immutable policy snapshots; typed filters bind parameters')
        paged=[];after='0'
        while True:
            page=tool({'target':{'kind':'workspace'},'after_sequence':after,'limit':1})['data']
            assert page['coverage']=='recorded_administration_since_schema5'
            paged+=page['records'];after=page['last_sequence']
            if not page['has_more']:break
        assert paged==current
        empty=tool({'target':{'kind':'workspace'},'after_sequence':after})['data'];assert empty['records']==[] and empty['last_sequence']==after and not empty['has_more']
        for bad in ['-1','01','+1','1.0','9223372036854775808']:
            tool({'target':{'kind':'workspace'},'after_sequence':bad},error='INVALID_ARGUMENT')
        for limit in [0,501]:tool({'target':{'kind':'workspace'},'limit':limit},error='INVALID_ARGUMENT')
        tool({'target':{'kind':'principal','id':''}},error='INVALID_ARGUMENT')
        tool({**query,'actor':{'principal_id':'owner'}},error='INVALID_ARGUMENT')
        policy('reviewer','0',error='REVISION_CONFLICT');assert records()==current
        checks.append('Exclusive decimal cursors traverse all records exactly once; empty/unknown targets, invalid inputs and failed CAS do not fabricate events')
        assert records({'target':{'kind':'principal','id':'missing'}})==[]
        assert command(['access','events','--input','-'],query)['data']==tool()['data']
        tool(credential=secret_file,error='NOT_AUTHORIZED')
        command(['access','events','--input','-'],query,secret_file,'NOT_AUTHORIZED')
        stdio=Stdio();restricted_stdio=Stdio(secret_file)
        assert stdio.call()['data']==tool()['data'];restricted_stdio.call(error='NOT_AUTHORIZED')
        listed=stdio.rpc('tools/list',{})['tools'];published=next(t for t in listed if t['name']=='opencut_access');assert published['annotations']['readOnlyHint']
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        owner_path=root/'owner-token'
        with (root/'host.log').open('w') as log:
            host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(owner_path)],stdout=log,stderr=log)
        children.append(host);deadline=time.monotonic()+15
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):break
            except OSError:assert time.monotonic()<deadline;time.sleep(.03)
        owner=owner_path.read_text().strip();scoped=secret_file.read_text().strip()
        status,_,data=request(owner,'/api/tools/opencut_access_audit_query',query);out=json.loads(data);assert status==200 and out['data']==tool()['data'];trace.append({'entry':'http','response':out})
        status,_,data=request(scoped,'/api/tools/opencut_access_audit_query',query,{'X-Actor':'local-owner'});assert status==403 and json.loads(data)['error']['code']=='NOT_AUTHORIZED'
        for token,allowed in [(owner,True),(scoped,False)]:
            sid=init(token);_,result=rpc(token,sid,'tools/call',{'name':'opencut_access_audit_query','arguments':query});out=result['structuredContent'];trace.append({'entry':'mcp_http','response':out})
            if allowed:assert out['data']==tool()['data']
            else:assert result['isError'] and out['error']['code']=='NOT_AUTHORIZED'
        assert records()==current
        checks.append('CLI dedicated/generic, REST and both MCP transports return identical owner history; read_audit project grants cannot reveal administration or forge owner')
        sql("CREATE TRIGGER fail_access_audit BEFORE INSERT ON access_events BEGIN SELECT RAISE(ABORT,'injected audit failure'); END")
        policy('reviewer','2',False,error='STORAGE_ERROR')
        issue('reviewer','failed.secret',error='STORAGE_ERROR');assert not (root/'failed.secret').exists()
        command(['access','revoke',credential],error='STORAGE_ERROR')
        assert records()==current and sql('SELECT revoked FROM access_credentials WHERE credential_id=?',(credential,))==[(0,)]
        assert sql('SELECT revision,enabled FROM access_principals WHERE principal_id=?',('reviewer',))==[(2,1)]
        assert sql('SELECT count(*) FROM access_credentials')==[(1,)]
        sql('DROP TRIGGER fail_access_audit')
        revoked=command(['access','revoke',credential]);revocation=records()[-1]
        assert revocation['request_id']==revoked['request_id'] and revocation['action']=='credential.revoked' and revocation['credential_id']==credential
        before=records();command(['access','revoke',credential]);assert records()==before
        disabled=policy('reviewer','2',False);assert records()[-1]['policy']==disabled['data']
        assert [r['action'] for r in records({'target':{'kind':'credential','id':credential}})]==['credential.issued','credential.revoked']
        checks.append('Audit insertion failure rolls back policy, issuance and revocation; successful revoke is recorded once and disable preserves committed policy state')
        before=records();host.terminate();host.wait(timeout=10)
        assert records()==before
        secret=secret_file.read_text().strip();digest=hashlib.sha256(secret.encode()).hexdigest()
        assert secret not in json.dumps(trace) and digest not in json.dumps(trace)
        assert len({r['sequence'] for r in before})==len(before)
        checks.append('Standalone reopen preserves exact history; query is read-only and no response/trace contains secret or digest')
        report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root),'scope':'schema-5 committed administration queries; not failed-login logging or plugin/media maintenance provenance'}
        (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    except Exception as e:
        (root/'failure.json').write_text(json.dumps({'error':repr(e),'completed_checks':checks},indent=2));raise
    finally:
        for p in children:
            if p.poll() is None:
                p.terminate()
                try:p.wait(timeout=8)
                except subprocess.TimeoutExpired:p.kill();p.wait()
        (root/'trace.json').write_text(json.dumps(trace,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path)
    args=parser.parse_args();verify(args.binary.resolve(),(args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-access-audit-'))).resolve())
