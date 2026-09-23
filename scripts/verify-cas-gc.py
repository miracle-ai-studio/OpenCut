#!/usr/bin/env python3
"""Public collection journey with actual failed edits, publisher SIGKILL and rendering."""
import argparse,fcntl,hashlib,http.client,json,os,socket,sqlite3,subprocess,tempfile,time,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def verify(binary,root,sdk):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';cas=root/'asset-store';checks=[];trace=[];host=None
 def cli(args,data=None,error=None,credential=None):
  p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=json.dumps(data) if data is not None else None,text=True,capture_output=True,timeout=30);r=json.loads(p.stdout);trace.append({'command':args,'response':r,'exit':p.returncode})
  if error:assert p.returncode and r['error']['code']==error,(r,p.stderr)
  else:assert not p.returncode and not r['error'],(r,p.stderr)
  return r
 def call(name,data=None,error=None,credential=None):return cli(['call',name,'--input','-'],data or {},error,credential)
 def data(name,payload=None,**kwargs):return call(name,payload,**kwargs)['data']
 def sql(s,args=()):
  with sqlite3.connect(db) as c:return c.execute(s,args).fetchall()
 def plan(key,limit=100,after=None):return data('opencut_cas_gc_plan',{'idempotency_key':key,'limit':limit,'after':after})
 def execute(p,error=None):return call('opencut_cas_gc_execute',{'plan_id':p['plan_id'],'expected_plan_hash':p['plan_hash']},error)
 def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
 def object_path(h):return cas/h[:2]/h
 def inspect():return data('opencut_project_inspect',{'project_id':'a'})
 def edit(ops,key,error=None):return call('opencut_edit_apply',{'project_id':'a','request':{'expected_revision':inspect()['revision'],'idempotency_key':key,'label':key,'operations':ops}},error)
 def generate(name,color):
  p=root/(name+'.mp4');subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i',f'color={color}:s=64x48:r=10:d=1','-c:v','libx264','-pix_fmt','yuv420p',str(p)],check=True,timeout=20);return p
 def asset(p,id):return {'id':id,'name':id,'kind':'video','uri':str(p),'duration':{'ticks':'10','timebase':10},'metadata':{}}
 def orphan(p,id):
  sql("CREATE TRIGGER fail_edit BEFORE INSERT ON audit_records WHEN NEW.action='project.applied' BEGIN SELECT RAISE(ABORT,'QA failed edit'); END")
  try:edit([{'type':'asset.bind','asset':asset(p,id)}],id,error='STORAGE_ERROR')
  finally:sql('DROP TRIGGER fail_edit')
  assert object_path(sha(p)).is_file();assert all(a['id']!=id for a in inspect()['project']['assets'])
 try:
  data('opencut_project_create',{'project':{'id':'a','name':'A','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'a'})
  red=generate('red','red');blue=generate('blue','blue');green=generate('green','green');white=generate('white','white')
  span={'start':{'ticks':'0','timebase':10},'duration':{'ticks':'10','timebase':10}}
  edit([{'type':'asset.bind','asset':asset(red,'source')},{'type':'track.create','sequence_id':'main','track':{'id':'v','name':'V','kind':'video','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}},{'type':'clip.insert','sequence_id':'main','track_id':'v','clip':{'id':'c','asset_id':'source','timeline_range':span,'source_range':span,'params':{},'channels':[]}}],'red')
  old=inspect()['revision'];edit([{'type':'asset.replace','asset':asset(blue,'source')}],'blue');head=inspect()
  orphan(green,'only-job');orphan(white,'orphan')
  job=data('opencut_job_submit',{'kind':'media.probe','idempotency_key':'job-only','input':{'input':str(object_path(sha(green)))}})
  p=plan('review');assert p['protected']==3 and len(p['items'])==1,p;assert p['items'][0]['object']['content_hash']==sha(white)
  assert plan('review')==p;call('opencut_cas_gc_plan',{'idempotency_key':'review','limit':1},'IDEMPOTENCY_KEY_REUSED')
  call('opencut_cas_gc_execute',{'plan_id':p['plan_id'],'expected_plan_hash':'0'*64},'GC_PLAN_CONFLICT');assert object_path(sha(white)).exists()
  done=execute(p)['data'];assert done['state']=='complete' and done['items'][0]['state']=='removed';assert not object_path(sha(white)).exists();assert execute(p)['data']==done
  assert inspect()==head and all(object_path(sha(x)).exists() for x in [red,blue,green]);checks.append('Real failed edit leaves collectible CAS; exact plan/replay works while old revision, current revision and job-only content remain protected')
  original_input=sql('SELECT input_json FROM jobs WHERE job_id=?',(job['job_id'],))[0][0]
  escaped=original_input.replace(sha(green),''.join('\\u%04x'%ord(c) for c in sha(green)));assert json.loads(escaped)==json.loads(original_input)
  sql('UPDATE jobs SET input_json=? WHERE job_id=?',(escaped,job['job_id']));assert plan('escaped-reference')['state']=='complete'
  sql('UPDATE jobs SET input_json=? WHERE job_id=?',(original_input,job['job_id']))
  capture=cas/('.capture-'+str(uuid.uuid4()));capture.write_bytes(b'fail-closed');review=plan('bad-roots')
  for bad,error in [('{','STORAGE_CORRUPT'),(json.dumps({'input':'x'*(8*1024*1024)}),'RESOURCE_TOO_LARGE')]:
   sql('UPDATE jobs SET input_json=? WHERE job_id=?',(bad,job['job_id']));execute(review,error);assert capture.exists()
  sql('UPDATE jobs SET input_json=? WHERE job_id=?',(original_input,job['job_id']));execute(review)
  # A legacy source path can alias a CAS object without spelling its hash.
  outside=root/'external';outside.mkdir();source_alias=outside/'source.mp4';source_alias.symlink_to(object_path(sha(green)))
  sql('UPDATE jobs SET input_json=? WHERE job_id=?',(json.dumps({'input':str(source_alias)}),job['job_id']));assert plan('aliased-native-reference')['state']=='complete'
  sql('UPDATE jobs SET input_json=? WHERE job_id=?',(original_input,job['job_id']));source_alias.unlink()
  checks.append('Decoded escaped hashes and actual legacy source inode aliases remain protected; malformed or over-budget roots refuse deletion before journaling')
  # Recovery discovery progresses even when an entire bounded page is filtered out.
  cursor=None;visible=[];empty_page=False
  while True:
   page=data('opencut_cas_gc_query',{'limit':1,'cursor':cursor});visible+=page['plans'];empty_page=empty_page or (not page['plans'] and page['next_cursor'] is not None);cursor=page['next_cursor']
   if not cursor:break
  assert not visible and empty_page
  discovery=cas/('.capture-'+str(uuid.uuid4()));discovery.write_bytes(b'discovery');pending=plan('discover-me');cursor=None;visible=[]
  while True:
   page=data('opencut_cas_gc_query',{'limit':2,'cursor':cursor});visible+=page['plans'];cursor=page['next_cursor']
   if not cursor:break
  assert [x['plan_id'] for x in visible]==[pending['plan_id']]
  cursor=data('opencut_cas_gc_query',{'limit':1})['next_cursor'];call('opencut_cas_gc_query',{'cursor':cursor,'include_complete':True},'INVALID_CURSOR');call('opencut_cas_gc_query',{'cursor':'gc1:u:foreign'},'INVALID_CURSOR');execute(pending)
  checks.append('Bounded plan discovery recovers unfinished intent, advances empty filtered pages and rejects wrong-filter or foreign anchors')
  # A planned orphan can become a referenced asset before explicit execution.
  orphan(white,'late');p=plan('late-plan');edit([{'type':'asset.bind','asset':asset(white,'late')}],'late-bind')
  done=execute(p)['data'];assert done['items'][0]['state']=='retained' and object_path(sha(white)).exists()
  checks.append('Execution rechecks references and retains an object bound after review')
  capture=cas/('.capture-'+str(uuid.uuid4()));capture.write_bytes(b'old');p=plan('changed');capture.write_bytes(b'replaced bytes');done=execute(p)['data'];assert done['items'][0]['state']=='changed' and capture.read_bytes()==b'replaced bytes';execute(plan('changed-again'));assert not capture.exists()
  # Force deletion bookkeeping to fail: before unlink no data is touched; after
  # unlink pending receipt is observable and a retry confirms durable absence.
  for state,label in [('removing','before'),('removed','after')]:
   capture=cas/('.capture-'+str(uuid.uuid4()));capture.write_bytes(b'fault');p=plan('fault-'+label)
   sql(f"CREATE TRIGGER fail_gc BEFORE INSERT ON cas_gc_events WHEN NEW.state_json='\"{state}\"' BEGIN SELECT RAISE(ABORT,'QA failed receipt'); END")
   execute(p,error='STORAGE_ERROR');sql('DROP TRIGGER fail_gc');pending=data('opencut_cas_gc_inspect',{'plan_id':p['plan_id']})
   assert capture.exists()==(label=='before');assert pending['items'][0]['state']==('planned' if label=='before' else 'removing')
   retried=execute(p)['data'];assert retried['state']=='complete' and not capture.exists();assert retried['items'][0]['state']==('removed' if label=='before' else 'already_absent')
  checks.append('Changed files are skipped; injected journal failures prove no unjournaled deletion and recover unknown post-unlink outcomes idempotently')
  gate=os.open(cas,os.O_RDONLY)
  try:
   fcntl.flock(gate,fcntl.LOCK_SH);call('opencut_cas_gc_plan',{'idempotency_key':'busy'},'STORAGE_BUSY')
  finally:os.close(gate)
  # Hold the actual SQL writer slot after native capture, before commit. The
  # publisher must keep the directory gate until its revision has committed.
  orange=generate('orange','orange');body={'project_id':'a','request':{'expected_revision':inspect()['revision'],'idempotency_key':'blocked-edit','label':'blocked','operations':[{'type':'asset.bind','asset':asset(orange,'orange')}]}}
  blocker=sqlite3.connect(db);blocker.execute('BEGIN IMMEDIATE');request=root/'blocked.json';request.write_text(json.dumps(body));writer=subprocess.Popen([str(binary),'--database',str(db),'--json','call','opencut_edit_apply','--input',str(request)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  try:
   until=time.monotonic()+4
   while not object_path(sha(orange)).exists():assert writer.poll() is None and time.monotonic()<until;time.sleep(.005)
   assert writer.poll() is None;call('opencut_cas_gc_plan',{'idempotency_key':'publication-gap'},'STORAGE_BUSY')
  finally:blocker.rollback();blocker.close()
  out,err=writer.communicate(timeout=15);assert writer.returncode==0,(out,err)
  checks.append('Cross-process gate rejects collection during actual capture-to-SQL-commit window without holding a collector SQL write transaction')
  # Kill an actual public publisher while its native capture temporary exists.
  large=root/'large.bin'
  with large.open('wb') as f:f.truncate(256*1024*1024)
  body={'project_id':'a','request':{'expected_revision':inspect()['revision'],'idempotency_key':'killed-publisher','label':'killed','operations':[{'type':'asset.bind','asset':asset(large,'killed')}]}}
  request.write_text(json.dumps(body));writer=subprocess.Popen([str(binary),'--database',str(db),'--json','call','opencut_edit_apply','--input',str(request)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  until=time.monotonic()+10
  while not list(cas.glob('.capture-*')):assert writer.poll() is None and time.monotonic()<until;time.sleep(.002)
  writer.kill();writer.wait(timeout=5);assert writer.returncode==-9;abandoned=list(cas.glob('.capture-*'));assert abandoned
  done=execute(plan('killed-publisher-cleanup'))['data'];assert done['state']=='complete' and all(not p.exists() for p in abandoned);assert all(a['id']!='killed' for a in inspect()['project']['assets']);assert large.exists()
  checks.append('Actual CLI publisher SIGKILL leaves an abandoned capture; explicit collection reclaims it without touching external source or publishing a revision')
  # Old history remains usable after actual collection, not just visible in SQL.
  restored=data('opencut_revision_restore',{'project_id':'a','request':{'expected_revision':inspect()['revision'],'target_revision':old,'idempotency_key':'restore-red','label':'restore'}})
  render=data('opencut_job_submit',{'kind':'render.project','idempotency_key':'render-old','input':{'project_id':'a','revision':restored['revision'],'sequence_id':'main','container':'mp4','video_codec':'h264'}})
  for _ in range(2):
   worker=subprocess.run([str(binary),'--database',str(db),'worker','run','--once'],capture_output=True,text=True,timeout=30)
   trace.append({'command':['worker','run','--once'],'exit':worker.returncode,'stdout':worker.stdout,'stderr':worker.stderr})
   assert worker.returncode==0,(worker.stdout,worker.stderr)
  rendered=data('opencut_job_get',{'job_id':render['job_id']});assert rendered['state']=='succeeded',rendered;artifact=Path(rendered['result']['artifact']['path']);pixel=subprocess.check_output(['ffmpeg','-v','error','-i',str(artifact),'-frames:v','1','-vf','scale=1:1','-f','rawvideo','-pix_fmt','rgb24','-']);assert pixel[0]>180 and pixel[1]<70 and pixel[2]<70,list(pixel)
  assert data('opencut_job_get',{'job_id':job['job_id']})['state']=='succeeded';checks.append('After collection, actual Worker executes job-only source and restores/renders the historical red revision; independently decoded pixels agree')
  # Privacy and permissions on both network transports share owner-only policy.
  cli(['access','set-policy','--input','-'],{'principal_id':'agent','expected_revision':'0','enabled':True,'grants':[{'project_id':'a','actions':['read','edit','run_jobs']}]});agent=root/'agent.secret';cli(['access','issue','agent','--output',str(agent)])
  for name,payload in [('opencut_cas_gc_query',{}),('opencut_cas_gc_plan',{'idempotency_key':'deny'}),('opencut_cas_gc_inspect',{'plan_id':p['plan_id']}),('opencut_cas_gc_execute',{'plan_id':p['plan_id'],'expected_plan_hash':p['plan_hash']})]:call(name,payload,'NOT_AUTHORIZED',agent)
  token=root/'owner.token'
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
  until=time.monotonic()+15
  while True:
   assert host.poll() is None
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=.2):break
   except OSError:assert time.monotonic()<until;time.sleep(.03)
  def request_http(name,body,credential=token):
   c=http.client.HTTPConnection('127.0.0.1',port,timeout=20);c.request('POST','/v1/tools/'+name,json.dumps(body),{'Content-Type':'application/json','Authorization':'Bearer '+credential.read_text().strip()});response=c.getresponse();r=json.loads(response.read());c.close();return response.status,r
  status,out=request_http('opencut_cas_gc_plan',{'idempotency_key':'network'});assert status==200 and out['data']['state']=='complete'
  status,out=request_http('opencut_cas_gc_plan',{'idempotency_key':'forbidden'},agent);assert status==403 and out['error']['code']=='NOT_AUTHORIZED';checks.append('CLI and real HTTP share owner-only collection and structured restricted refusals')
  if sdk:
   r=subprocess.run(['node',str(ROOT/'scripts/verify-cas-gc-mcp.mjs'),str(sdk),str(binary),str(db),f'http://127.0.0.1:{port}',str(agent),str(token)],capture_output=True,text=True,timeout=90);(root/'mcp.stdout').write_text(r.stdout);(root/'mcp.stderr').write_text(r.stderr);assert r.returncode==0,(r.stdout,r.stderr);checks.append('Official MCP clients plan, inspect and execute actual capture cleanup on both stdio and Streamable HTTP with scoped refusal')
  report={'status':'passed','binary_sha256':sha(binary),'checks':checks,'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
 finally:
  if host and host.poll() is None:host.terminate();host.wait(timeout=10)
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-cas-gc-'))).resolve(),a.mcp_sdk_root)
