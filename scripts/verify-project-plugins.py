#!/usr/bin/env python3
"""Real WASM + public CLI/HTTP/MCP project plugin journey and native-lock revocation."""
import argparse, copy, fcntl, hashlib, http.client, importlib.util, json, socket, sqlite3, subprocess, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('publication',ROOT/'scripts/verify-plugin-publication.py');fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)
def verify(binary,component,root,sdk):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';trace=[];checks=[];host=None
 def command(args,payload=None,credential=None,error=None):
  p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=json.dumps(payload) if payload is not None else None,capture_output=True,text=True,timeout=45)
  out=json.loads(p.stdout);trace.append({'entry':args,'response':out,'exit':p.returncode})
  if error:assert out['error']['code']==error and p.returncode,(out,p.stderr)
  else:assert out['error'] is None and p.returncode==0,(out,p.stderr)
  return out['data'] if not error else out
 def call(name,args,credential=None,error=None):return command(['call',name,'--input','-'],args,credential,error)
 def sql(query,args=()):
  with sqlite3.connect(db) as c:return c.execute(query,args).fetchall()
 def policy(who,actions,revision='0',enabled=True):return command(['access','set-policy','--input','-'],{'principal_id':who,'expected_revision':revision,'enabled':enabled,'grants':[{'project_id':'a','actions':actions}]})
 def issue(who):
  file=root/(who+'.secret');command(['access','issue',who,'--output',str(file)]);return file
 try:
  projects={}
  for name in ['a','b']:projects[name]=call('opencut_project_create',{'project':{'id':name,'name':name,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':name})
  manifest,permissions=fixture.fixture(root/'package',component);install={'package_root':str(root/'package'),'granted_permissions':permissions};installed=call('opencut_plugin_install',install)
  target={'project_id':'a','id':manifest['id'],'version':manifest['version']};op=manifest['contributes']['operations'][0]['name'];inspection={'target':target,'operation_name':op}
  plan={'id':target['id'],'version':target['version'],'operation_name':op,'project_id':'a','revision':projects['a']['revision'],'parameters':{'track_id':'plugin-track'}}
  grant={'target':target,'package_sha256':installed['package_sha256'],'operations':[op],'enabled':True,'expected_revision':'0','idempotency_key':'allow'}
  policy('agent',['read','edit','run_jobs','read_audit']);agent=issue('agent');policy('reader',['read']);reader=issue('reader');policy('runner',['read','run_jobs']);runner=issue('runner')
  for name,args in [('opencut_plugin_list',{}),('opencut_project_plugin_grant',grant),('opencut_project_plugin_plan',plan)]:call(name,args,agent,'NOT_AUTHORIZED')
  stored=call('opencut_project_plugin_grant',grant);assert stored['revision']=='1';assert call('opencut_project_plugin_grant',grant)==stored
  call('opencut_project_plugin_grant',{**grant,'enabled':False},error='IDEMPOTENCY_KEY_REUSED');call('opencut_project_plugin_grant',{**grant,'idempotency_key':'stale'},error='REVISION_CONFLICT')
  assert call('opencut_project_plugin_list',{'project_id':'a'},reader)==[stored];call('opencut_project_plugin_list',{'project_id':'b'},agent,'NOT_AUTHORIZED')
  for credential in [reader,runner]:call('opencut_project_plugin_plan',plan,credential,'NOT_AUTHORIZED')
  call('opencut_project_plugin_inspect',{**inspection,'operation_name':op+'.other'},agent,'NOT_AUTHORIZED')
  seen=call('opencut_project_plugin_inspect',inspection,reader);assert seen['input_schema']=={'type':'object'} and seen['output_schema']=={'type':'array'};assert seen['limits']['wall_time_ms']==1000 and seen['limits']['fuel']==5000000
  assert 'install_path' not in json.dumps(seen) and str(root) not in json.dumps(seen)
  planned=call('opencut_project_plugin_plan',plan,agent);assert planned['grant']==stored and planned['result']['operations'][0]['track']['id']=='plugin-track';assert call('opencut_project_inspect',{'project_id':'a'})==projects['a']
  checks.append('Owner grants exact package; scoped/read-only discovery is portable; planning requires read+edit+run_jobs and does not change head; replay and CAS agree')
  for fault in ['invalid_json','invalid_operation','missing_sequence']:
   error=call('opencut_project_plugin_plan',{**plan,'parameters':{'track_id':'bad','fault':fault}},agent,'INVALID_PLUGIN');assert str(root) not in json.dumps(error) and 'wasm backtrace' not in json.dumps(error)
  call('opencut_project_plugin_plan',{**plan,'parameters':{'loop':True}},agent,'PLUGIN_RUNTIME')
  call('opencut_project_plugin_plan',{**plan,'parameters':{'large':'x'*262144}},agent,'INVALID_ARGUMENT')
  call('opencut_project_plugin_plan',{**plan,'project_id':'b','revision':projects['b']['revision']},agent,'NOT_AUTHORIZED')
  call('opencut_project_plugin_plan',{**plan,'revision':projects['b']['revision']},agent,'NOT_FOUND')
  checks.append('Real WASM invalid JSON/operation/domain output and infinite loop fail; parameter ceiling, foreign project and foreign revision are enforced')
  # Global package policy is independently required.
  call('opencut_plugin_install',{**install,'granted_permissions':[]});call('opencut_project_plugin_plan',plan,agent,'PERMISSION_DENIED');call('opencut_project_plugin_inspect',inspection,reader,'PERMISSION_DENIED');call('opencut_plugin_install',install)
  changed={**grant,'package_sha256':'0'*64,'expected_revision':'1','idempotency_key':'wrong-package'};assert call('opencut_project_plugin_grant',changed)['revision']=='2';call('opencut_project_plugin_plan',plan,agent,'NOT_AUTHORIZED')
  restored={**grant,'expected_revision':'2','idempotency_key':'restore'};assert call('opencut_project_plugin_grant',restored)['revision']=='3'
  # Failed audit must roll back policy, receipt and outbox, preserving invocation.
  sql("CREATE TRIGGER fail_plugin_audit BEFORE INSERT ON audit_records WHEN NEW.action='project.plugin_grant.updated' BEGIN SELECT RAISE(ABORT,'QA'); END")
  revoke={**grant,'enabled':False,'expected_revision':'3','idempotency_key':'revoke'};before=sql("SELECT count(*) FROM outbox WHERE event_type='project.plugin_grant.updated'")
  call('opencut_project_plugin_grant',revoke,error='STORAGE_ERROR');assert sql("SELECT count(*) FROM outbox WHERE event_type='project.plugin_grant.updated'")==before;assert call('opencut_project_plugin_list',{'project_id':'a'})[0]['revision']=='3';sql('DROP TRIGGER fail_plugin_audit')
  checks.append('Global revoke, exact-hash mismatch and audit-write rollback reject or preserve authority without mutating project history')
  # Observe the child's open version-lock fd, then revoke while it is blocked.
  lock=Path(installed['install_path']).parents[2]/'.install-locks'/f"{target['id']}@{target['version']}.lock"
  assert lock.exists(),lock
  with lock.open('r') as f:
   fcntl.flock(f,fcntl.LOCK_EX)
   child=subprocess.Popen([str(binary),'--database',str(db),'--json','--credential-file',str(agent),'call','opencut_project_plugin_plan','--input','-'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   try:
    child.stdin.write(json.dumps(plan));child.stdin.close();child.stdin=None;deadline=time.monotonic()+15
    while str(lock) not in subprocess.run(['lsof','-p',str(child.pid),'-Fn'],capture_output=True,text=True).stdout:
     assert child.poll() is None and time.monotonic()<deadline;time.sleep(.03)
    assert call('opencut_project_plugin_grant',revoke)['revision']=='4'
    fcntl.flock(f,fcntl.LOCK_UN);out,err=child.communicate(timeout=20);out=json.loads(out);trace.append({'locked_plan':out});assert out['error']['code']=='NOT_AUTHORIZED',out
   finally:
    if child.poll() is None:child.kill();child.wait()
  assert call('opencut_project_plugin_grant',{**grant,'expected_revision':'4','idempotency_key':'again'})['revision']=='5'
  checks.append('Real OS version-lock wait observes project grant revocation before any WASM call; no SQLite write lock blocks revocation')
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  token=root/'owner.token'
  with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
  deadline=time.monotonic()+15
  while True:
   assert host.poll() is None
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=.2):break
   except OSError:assert time.monotonic()<deadline;time.sleep(.03)
  def http_call(name,args,credential=agent,error=None,status=200):
   c=http.client.HTTPConnection('127.0.0.1',port,timeout=30);c.request('POST','/v1/tools/'+name,json.dumps(args),{'Authorization':'Bearer '+credential.read_text().strip(),'Content-Type':'application/json'});r=c.getresponse();out=json.loads(r.read());c.close();trace.append({'http':name,'status':r.status,'response':out});assert r.status==status,out
   if error:assert out['error']['code']==error,out
   else:assert out['error'] is None,out
   return out['data']
  assert http_call('opencut_project_plugin_inspect',inspection)['input_schema']==seen['input_schema'];assert http_call('opencut_project_plugin_plan',plan)['result']==planned['result']
  http_call('opencut_project_plugin_plan',plan,reader,'NOT_AUTHORIZED',403)
  changed=http_call('opencut_edit_apply',{'project_id':'a','request':{'expected_revision':plan['revision'],'idempotency_key':'apply-plugin','label':'Explicit scoped plugin edit','operations':planned['result']['operations']}})
  actual=call('opencut_project_inspect',{'project_id':'a'},agent);assert actual['revision']==changed['revision'];assert actual['project']['sequences'][0]['tracks'][0]['id']=='plugin-track'
  assert call('opencut_project_plugin_plan',plan,agent)['result']==planned['result'] # fixed historical revision stays valid
  http_call('opencut_edit_apply',{'project_id':'a','request':{'expected_revision':plan['revision'],'idempotency_key':'stale-apply','label':'stale','operations':planned['result']['operations']}},agent,'REVISION_CONFLICT',409)
  checks.append('HTTP scoped schema/plan match CLI; explicit edit commits and reopens; historical planning stays fixed while stale apply fails')
  if sdk:
   input_file=root/'mcp-input.json';input_file.write_text(json.dumps({'inspection':inspection,'plan':plan}))
   p=subprocess.run(['node',str(ROOT/'scripts/verify-project-plugins-mcp.mjs'),str(sdk),str(binary),str(db),f'http://127.0.0.1:{port}',str(agent),str(reader),str(input_file)],capture_output=True,text=True,timeout=90);(root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert p.returncode==0,(p.stdout,p.stderr);checks.append('Official MCP SDK discovers and invokes the same project plugin tools over stdio and HTTP, including read-only denial')
  assert sql('PRAGMA integrity_check')==[('ok',)];assert int(sql("SELECT value FROM metadata WHERE key='schema_version'")[0][0])>=10
  host.terminate();host.wait(timeout=10);host=None;assert call('opencut_project_inspect',{'project_id':'a'},agent)==actual
  report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'component_sha256':hashlib.sha256(component.read_bytes()).hexdigest(),'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'completed_checks':checks},indent=2));raise
 finally:
  if host and host.poll() is None:host.terminate();host.wait(timeout=10)
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--component',type=Path,default=ROOT/'crates/plugin-host/examples/planner-guest/target/wasm32-wasip2/release/opencut_example_planner_guest.wasm');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),a.component.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-project-plugins-'))).resolve(),a.mcp_sdk_root)
