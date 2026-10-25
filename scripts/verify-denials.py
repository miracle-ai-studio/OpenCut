#!/usr/bin/env python3
"""Public host refusal audit: actual CLI/HTTP/MCP, revocation, failure, retention and privacy."""
import argparse, hashlib, http.client, json, socket, sqlite3, subprocess, tempfile, time, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def verify(binary,root,sdk):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';host=None;trace=[];checks=[];marker='DO_NOT_LOG_SECRET_BODY_URL_OR_ACTOR'
 def cli(args,data=None,credential=None,error=None):
  p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=json.dumps(data) if data is not None else None,text=True,capture_output=True,timeout=20);out=json.loads(p.stdout);trace.append({'cli':args,'response':out,'exit':p.returncode})
  if error:assert p.returncode and out['error']['code']==error,(out,p.stderr)
  else:assert p.returncode==0 and out['error'] is None,(out,p.stderr)
  return out
 def call(name,data,credential=None,error=None):return cli(['call',name,'--input','-'],data,credential,error)
 def query(target=None,**kw):return call('opencut_denial_query',{'target':target or {'kind':'workspace'},**kw})['data']
 def by_request(response):return query({'kind':'request','id':response['request_id']})['records']
 def sql(query,args=()):
  with sqlite3.connect(db) as c:return c.execute(query,args).fetchall()
 def issue(who):
  file=root/(who+'-'+uuid.uuid4().hex+'.secret');r=cli(['access','issue',who,'--output',str(file)]);return file,r['data']['credential_id']
 try:
  assert query()['records']==[]
  for name in ['a','b']:call('opencut_project_create',{'project':{'id':name,'name':name,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':name})
  policy={'principal_id':'agent','expected_revision':'0','enabled':True,'grants':[{'project_id':'a','actions':['read','edit','run_jobs']}]};cli(['access','set-policy','--input','-'],policy);agent,cid=issue('agent')
  a=call('opencut_project_inspect',{'project_id':'a'},agent)['data'];before=query();assert before['records']==[]
  denied=call('opencut_plugin_list',{'actor':marker,'token':marker,'request_id':marker},agent,'NOT_AUTHORIZED');r=by_request(denied);assert len(r)==1 and r[0]['principal_id']=='agent' and r[0]['credential_id']==cid and r[0]['source']=='cli' and r[0]['operation']=={'kind':'tool','name':'opencut_plugin_list'}
  unknown=call(marker,{},agent,'NOT_AUTHORIZED');assert by_request(unknown)[0]['operation']=={'kind':'unknown_tool'}
  foreign=call('opencut_project_inspect',{'project_id':'b'},agent,'NOT_AUTHORIZED');assert len(by_request(foreign))==1
  forbidden=call('opencut_denial_query',{'target':{'kind':'workspace'}},agent,'NOT_AUTHORIZED');assert len(by_request(forbidden))==1
  admin=cli(['access','set-policy','--input','-'],policy,agent,'NOT_AUTHORIZED');assert by_request(admin)[0]['operation']['kind']=='administration'
  bad=root/'bad.secret';bad.write_text(marker);auth=cli(['project','list'],credential=bad,error='NOT_AUTHORIZED');assert by_request(auth)[0]['principal_id'] is None and by_request(auth)[0]['reason']=='authentication_rejected'
  assert call('opencut_project_inspect',{'project_id':'a'})['data']==a
  checks.append('CLI tool, unknown action, foreign project, owner audit query, administration and anonymous authentication refusals persist once without project changes')
  token=root/'owner.token'
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
  deadline=time.monotonic()+15
  while True:
   assert host.poll() is None
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=.2):break
   except OSError:assert time.monotonic()<deadline;time.sleep(.03)
  def raw(method,path,data=None,credential=agent,extra=None):
   headers={'Content-Type':'application/json'}
   if credential:headers['Authorization']='Bearer '+credential.read_text().strip()
   headers.update(extra or {});c=http.client.HTTPConnection('127.0.0.1',port,timeout=15);c.request(method,path,json.dumps(data) if data is not None else None,headers);r=c.getresponse();out=json.loads(r.read());h=dict(r.getheaders());c.close();trace.append({'http':method,'status':r.status,'response':out});return r.status,out,{k.lower():v for k,v in h.items()}
  status,out,h=raw('POST','/api/tools/opencut_plugin_list',{'secret':marker},extra={'X-OpenCut-Request-Id':marker});assert status==403 and h['x-opencut-request-id']==out['request_id']!=marker;assert len(by_request(out))==1 and by_request(out)[0]['source']=='http'
  for credential,extra,reason in [(None,None,'authentication_rejected'),(bad,None,'authentication_rejected'),(agent,{'Origin':'https://'+marker+'.invalid'},'origin_rejected')]:
   status,out,h=raw('GET','/api/capabilities',credential=credential,extra=extra);assert status==403 and h['x-opencut-request-id']==out['request_id'];r=by_request(out);assert len(r)==1 and r[0]['principal_id'] is None and r[0]['reason']==reason
  status,out,h=raw('GET','/api/media/'+marker);assert status==403 and by_request(out)[0]['operation']['kind']=='http_request'
  status,out,h=raw('POST','/mcp',{'jsonrpc':'2.0','id':1,'method':'initialize','params':{}},extra={'Host':'attacker.invalid'});assert status==403;r=query({'kind':'request','id':h['x-opencut-request-id']})['records'];assert len(r)==1 and r[0]['source']=='mcp_http'
  checks.append('HTTP tool refusal deduplicates across adapters; server request header ignores client claims; missing/bad bearer, Origin, global media and MCP Host refusals are recorded')
  # Audit failure must remain a refusal and must be visible, without recursive logging.
  sql("CREATE TRIGGER deny_audit BEFORE INSERT ON denial_records BEGIN SELECT RAISE(ABORT,'QA denied log'); END")
  count=sql('SELECT COUNT(*) FROM denial_records');fail=call('opencut_plugin_list',{},agent,'NOT_AUTHORIZED');assert fail['warnings']==['DENIAL_AUDIT_UNAVAILABLE'];assert sql('SELECT COUNT(*) FROM denial_records')==count
  status,out,h=raw('POST','/api/tools/opencut_plugin_list',{});assert status==403 and h['x-opencut-audit-warning']=='DENIAL_AUDIT_UNAVAILABLE';assert out['warnings']==['DENIAL_AUDIT_UNAVAILABLE'];assert sql('SELECT COUNT(*) FROM denial_records')==count
  sql('DROP TRIGGER deny_audit');assert len(by_request(call('opencut_plugin_list',{},agent,'NOT_AUTHORIZED')))==1
  checks.append('Injected audit persistence failure preserves 403/refusal and domain state; envelope/header expose degraded logging, with no recursive or phantom rows')
  if sdk:
   p=subprocess.run(['node',str(ROOT/'scripts/verify-denials-mcp.mjs'),str(sdk),str(binary),str(db),f'http://127.0.0.1:{port}',str(agent),str(token)],text=True,capture_output=True,timeout=90);(root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert p.returncode==0,(p.stdout,p.stderr);checks.append('Official MCP clients on stdio/HTTP correlate tool and resource denials by server request ID with accurate transport source')
  # Use an independent credential to prove idle SSE revalidation closes and logs once.
  stream_credential,stream_id=issue('agent');c=http.client.HTTPConnection('127.0.0.1',port,timeout=5);c.request('POST','/api/changes/stream',json.dumps({'scope':{'kind':'projects','project_id':'a'},'limit':100}),{'Authorization':'Bearer '+stream_credential.read_text().strip(),'Content-Type':'application/json'});response=c.getresponse();assert response.status==200;stream_request=response.getheader('X-OpenCut-Request-Id');assert stream_request
  # Consume ready and the already committed project event before revocation.
  seen=[]
  while True:
   line=response.readline();assert line
   if line.startswith(b'data:'):seen.append(json.loads(line[5:]))
   if len(seen)>=2 and not line.strip():break
  cli(['access','revoke',stream_id]);assert response.read()==b'';c.close();records=query({'kind':'request','id':stream_request})['records'];assert len(records)==1 and records[0]['reason']=='policy_changed' and records[0]['operation']['kind']=='stream' and records[0]['credential_id']==stream_id
  checks.append('Idle public SSE credential revocation closes actual stream and records its original response request ID exactly once')
  all_records=query(limit=200)['records'];paged=[];cursor='0'
  while True:
   page=query(after_sequence=cursor,limit=2);paged+=page['records'];cursor=page['last_sequence']
   if not page['has_more']:break
  assert paged==all_records
  assert all(r['principal_id']=='agent' for r in query({'kind':'principal','id':'agent'})['records']);assert query()['coverage']=='bounded_host_denials_since_schema11'
  assert marker not in json.dumps(sql('SELECT * FROM denial_records'))
  for secret in [agent.read_text().strip(),token.read_text().strip(),bad.read_text().strip()]:assert secret not in json.dumps(sql('SELECT * FROM denial_records'))
  checks.append('Owner filtering/pagination is lossless; raw bodies, spoofed actor/request IDs, tokens, original paths and URLs never enter denial rows')
  # Independently seed a retention boundary, then use a public denial to trigger bounded pruning.
  oldest=all_records[0]['sequence'];row=sql('SELECT source_json,operation_json,reason_json FROM denial_records LIMIT 1')[0]
  with sqlite3.connect(db) as conn:
   conn.executemany('INSERT INTO denial_records(request_id,source_json,operation_json,reason_json) VALUES (?,?,?,?)',[(str(uuid.uuid4()),*row) for _ in range(10000)])
  last=call('opencut_plugin_list',{},agent,'NOT_AUTHORIZED');assert sql('SELECT COUNT(*) FROM denial_records')==[(10000,)];assert by_request(last)
  call('opencut_denial_query',{'target':{'kind':'workspace'},'after_sequence':oldest},error='CURSOR_EXPIRED');assert query()['oldest_available_sequence']!=oldest
  checks.append('A real public denial prunes independently seeded overflow to 10000 retained rows; stale cursors explicitly expire and restart advertises oldest availability')
  host.terminate();host.wait(timeout=10);host=None;assert by_request(last) and call('opencut_project_inspect',{'project_id':'a'})['data']==a
  report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
 finally:
  if host and host.poll() is None:host.terminate();host.wait(timeout=10)
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-denials-'))).resolve(),a.mcp_sdk_root)
