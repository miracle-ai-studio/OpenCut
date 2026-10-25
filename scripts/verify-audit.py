#!/usr/bin/env python3
"""Real CLI/HTTP/stdio/network MCP provenance, retries and Worker artifact trace."""
import argparse
import hashlib
import http.client
import json
from pathlib import Path
import signal
import socket
import sqlite3
import subprocess
import tempfile
import time

def verify(binary, root):
    root.mkdir(parents=True, exist_ok=True)
    db, token_path = root/'workspace.sqlite', root/'token'
    checks, trace, children = [], [], []
    def cli(name, value, error=None):
        r = subprocess.run([str(binary),'--database',str(db),'--json','call',name,'--input','-'], input=json.dumps(value), text=True, capture_output=True, timeout=30)
        out=json.loads(r.stdout)
        trace.append({'entry':'cli','tool':name,'response':out})
        if error: assert out['error']['code']==error and r.returncode!=0,out
        else: assert r.returncode==0 and out['error'] is None,out
        return out
    def audit(kind='workspace', value=None, **args):
        target={'kind':kind}
        if value is not None:target['id']=value
        return cli('opencut_audit_query',{'target':target,**args})['data']
    def create(name):
        return {'project':{'id':name,'name':name,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create-'+name}
    def edit(revision, key):
        return {'project_id':'audit','request':{'expected_revision':revision,'idempotency_key':key,'label':key,'operations':[{'type':'sequence.update_settings','sequence_id':'main','name':key}]}}
    def request(h, method, path, body=None, extra=None):
        headers={'Authorization':'Bearer '+h['token'],'Content-Type':'application/json','Accept':'application/json, text/event-stream'}
        headers.update(extra or {})
        conn=http.client.HTTPConnection('127.0.0.1',h['port'],timeout=30)
        conn.request(method,path,body=json.dumps(body) if body is not None else None,headers=headers)
        response=conn.getresponse();data=response.read();status=response.status;hs=dict(response.getheaders());conn.close()
        return status,hs,data
    def http_call(h,name,value,headers=None):
        status,_,data=request(h,'POST','/api/tools/'+name,value,headers)
        out=json.loads(data);assert status==200 and out['error'] is None,out
        trace.append({'entry':'http','tool':name,'response':out});return out
    serial=0
    def stdio_call(p,method,params):
        nonlocal serial
        serial+=1
        p.stdin.write(json.dumps({'jsonrpc':'2.0','id':serial,'method':method,'params':params})+'\n');p.stdin.flush()
        while True:
            line=p.stdout.readline();assert line,'MCP closed unexpectedly'
            value=json.loads(line)
            if value.get('id')==serial:
                assert 'error' not in value,value
                return value['result']
    def mcp_http(h,sid,method,params):
        nonlocal serial
        serial+=1
        headers={'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25'} if sid else {}
        status,hs,data=request(h,'POST','/mcp',{'jsonrpc':'2.0','id':serial,'method':method,'params':params},headers)
        assert status==200,(status,data)
        messages=[json.loads(line[5:].strip()) for line in data.decode().splitlines() if line.startswith('data:') and line[5:].strip()]
        result=next(v for v in messages if v.get('id')==serial);assert 'error' not in result,result
        return hs,result['result']
    try:
        first=cli('opencut_project_create',create('audit'))
        records=audit('project','audit')['records'];assert len(records)==1
        assert records[0]['request_id']==first['request_id'] and records[0]['actor']=={'principal_id':'local-owner','source':'cli','worker_id':None}
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        log=(root/'host.log').open('w')
        host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token_path)],stdout=log,stderr=log);children.append(host);log.close()
        deadline=time.monotonic()+15
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):break
            except OSError:
                assert time.monotonic()<deadline;time.sleep(.03)
        h={'port':port,'token':token_path.read_text().strip()}
        retried=http_call(h,'opencut_project_create',create('audit'))
        assert retried['data']==first['data'] and retried['request_id']!=first['request_id']
        assert audit('project','audit')['records']==records
        checks.append('CLI request ID is persisted; HTTP retry returns original creation without changing actor or adding audit')

        changed=http_call(h,'opencut_edit_apply',edit(first['data']['revision'],'http-edit'),{'X-Actor':'forged-owner','X-Request-Id':'forged-request'})
        records=audit('transaction',changed['data']['transaction_id'])['records']
        assert len(records)==1 and records[0]['request_id']==changed['request_id']
        assert records[0]['actor']['source']=='http' and 'forged' not in json.dumps(records)
        forged=edit(changed['data']['revision'],'forged-body');forged['actor']={'principal_id':'admin'}
        cli('opencut_edit_apply',forged,'INVALID_ARGUMENT')
        checks.append('HTTP mutation uses host-derived actor/request ID; forged headers have no effect and body identity injection rejects')

        diag=(root/'stdio.log').open('w')
        p=subprocess.Popen([str(binary),'--database',str(db),'mcp','serve'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=diag,text=True);children.append(p);diag.close()
        stdio_call(p,'initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'forged-principal','version':'1'}})
        p.stdin.write(json.dumps({'jsonrpc':'2.0','method':'notifications/initialized'})+'\n');p.stdin.flush()
        raw=stdio_call(p,'tools/call',{'name':'opencut_edit_apply','arguments':edit(changed['data']['revision'],'stdio-edit')})
        stdio=raw['structuredContent'];assert not raw.get('isError'),raw
        assert audit('transaction',stdio['data']['transaction_id'])['records'][0]['actor']['source']=='mcp_stdio'
        hs,_=mcp_http(h,None,'initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'fake-admin','version':'1'}})
        sid=hs['mcp-session-id']
        status,_,_=request(h,'POST','/mcp',{'jsonrpc':'2.0','method':'notifications/initialized'},{'Mcp-Session-Id':sid,'Mcp-Protocol-Version':'2025-11-25'});assert status==202
        _,raw=mcp_http(h,sid,'tools/call',{'name':'opencut_edit_apply','arguments':edit(stdio['data']['revision'],'network-edit')})
        network=raw['structuredContent'];assert not raw.get('isError'),raw
        record=audit('transaction',network['data']['transaction_id'])['records'][0]
        assert record['actor']['source']=='mcp_http' and record['actor']['principal_id']=='local-owner'
        assert record['request_id']==network['request_id']
        checks.append('Real stdio and Streamable HTTP edits preserve distinct host sources; MCP clientInfo never becomes principal')

        source=root/'private-source.mp4'
        subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=64x48:r=10:d=1','-pix_fmt','yuv420p',str(source)],check=True,capture_output=True,timeout=30)
        raw=stdio_call(p,'tools/call',{'name':'opencut_job_submit','arguments':{'kind':'preview.frame','idempotency_key':'audit-frame','input':{'input':str(source),'requested_time':{'ticks':'0','timebase':10},'alignment':'exact'}}})
        accepted=raw['structuredContent'];assert not raw.get('isError'),raw
        job_id=accepted['data']['job_id'];deadline=time.monotonic()+30
        while True:
            view=cli('opencut_job_view',{'job_id':job_id})['data']
            if view['state'] in ['succeeded','failed','cancelled']:break
            assert time.monotonic()<deadline;time.sleep(.1)
        assert view['state']=='succeeded' and view['artifacts'],view
        records=audit('job',job_id)['records']
        assert records[0]['action']=='job.accepted' and records[0]['request_id']==accepted['request_id']
        assert records[0]['actor']['source']=='mcp_stdio'
        assert all(r['origin_request_id']==accepted['request_id'] for r in records)
        done=records[-1];assert done['action']=='job.succeeded' and done['attempt']==1 and done['actor']['source']=='worker'
        assert done['artifact_ids']==[a['artifact_id'] for a in view['artifacts']]
        assert source.as_posix() not in json.dumps(records) and h['token'] not in json.dumps(records)
        assert audit('request',accepted['request_id'])['records']==records
        checks.append('Real FFmpeg Worker artifact binds accepted MCP request, runtime actor and attempt without source paths or credentials')

        all_records=audit()['records'];seen=[];position='0'
        while True:
            page=audit(after_sequence=position,limit=1);seen.extend(page['records']);position=page['last_sequence']
            if not page['has_more']:break
        assert seen==all_records and audit(after_sequence=position)['last_sequence']==position
        for position in ['01','-1','1e2','9223372036854775808']:
            cli('opencut_audit_query',{'target':{'kind':'workspace'},'after_sequence':position},'INVALID_ARGUMENT')
        for limit in [0,501]:cli('opencut_audit_query',{'target':{'kind':'workspace'},'limit':limit},'INVALID_ARGUMENT')
        checks.append('Paged audit traversal has no gaps/duplicates; exact decimal positions and bounds reject malformed requests')

        # Stop the autonomous Worker before injecting faults into this database.
        host.send_signal(signal.SIGINT);host.wait(timeout=10)
        p.stdin.close();p.wait(timeout=5)
        before=audit()['records']
        with sqlite3.connect(db) as c:
            c.execute("CREATE TRIGGER audit_failure BEFORE INSERT ON audit_records BEGIN SELECT RAISE(ABORT,'audit failure'); END")
        cli('opencut_project_create',create('rolled-back'),'STORAGE_ERROR')
        cli('opencut_project_inspect',{'project_id':'rolled-back'},'NOT_FOUND')
        cli('opencut_edit_apply',edit(network['data']['revision'],'audit-failure'),'STORAGE_ERROR')
        assert cli('opencut_project_inspect',{'project_id':'audit'})['data']['revision']==network['data']['revision']
        assert audit()['records']==before
        with sqlite3.connect(db) as c:c.execute('DROP TRIGGER audit_failure')
        assert cli('opencut_edit_apply',edit(network['data']['revision'],'audit-failure'))['data']['revision']!=network['data']['revision']
        checks.append('Real CLI audit insert failure rolls back project creation/edit; same key succeeds after repairing persistence')
        report={'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'evidence':str(root),'scope':'local owner mutation provenance; restricted principal paths require separate access verification'}
        (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    finally:
        for p in children:
            if p.poll() is None:
                p.terminate()
                try:p.wait(timeout=5)
                except subprocess.TimeoutExpired:p.kill();p.wait()
        (root/'trace.json').write_text(json.dumps(trace,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path)
    args=parser.parse_args();verify(args.binary.resolve(),(args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-audit-'))).resolve())
