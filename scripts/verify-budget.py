#!/usr/bin/env python3
"""Storage admission through real edits, upload bytes, Worker execution and SIGKILL."""
import argparse,hashlib,http.client,json,os,socket,subprocess,tempfile,time,wave
from pathlib import Path
from urllib.parse import urlencode
ROOT=Path(__file__).resolve().parents[1]
def verify(binary,root,sdk):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';checks=[];trace=[];host=None;writer=None
 def cli(args,data=None,credential=None,error=None):
  p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=json.dumps(data) if data is not None else None,text=True,capture_output=True,timeout=30);r=json.loads(p.stdout);trace.append({'command':args,'response':r,'exit':p.returncode})
  if error:assert p.returncode and r['error']['code']==error,(r,p.stderr)
  else:assert not p.returncode and not r['error'],(r,p.stderr)
  return r['data']
 def call(name,data=None,**kwargs):return cli(['call',name,'--input','-'],data or {},**kwargs)
 def inspect():return call('opencut_storage_budget_inspect')
 def configure(limit,key,minimum=0):
  old=inspect()['policy'];request={'expected_revision':old['revision'],'idempotency_key':key,'payload_limit_bytes':str(limit),'minimum_available_bytes':str(minimum)}
  result=call('opencut_storage_budget_configure',request);assert result['actor']['source']=='cli';assert inspect()['last_policy_change']==result;assert call('opencut_storage_budget_configure',request)==result;return request,result
 def head():return call('opencut_project_inspect',{'project_id':'a'})
 def edit(ops,key,error=None):return call('opencut_edit_apply',{'project_id':'a','request':{'expected_revision':head()['revision'],'idempotency_key':key,'label':key,'operations':ops}},error=error)
 def worker(job):
  p=subprocess.run([str(binary),'--database',str(db),'worker','run','--once'],capture_output=True,text=True,timeout=30);assert p.returncode==0,p.stderr
  # The HTTP host can win the lease first. An empty one-shot run is not completion.
  deadline=time.monotonic()+45
  while True:
   view=call('opencut_job_view',{'job_id':job['job_id']})
   if view['state'] in ['succeeded','failed','cancelled']:return view
   assert time.monotonic()<deadline,view;time.sleep(.05)
 def raw(method,path,body,credential,error=None,status=200):
  c=http.client.HTTPConnection('127.0.0.1',port,timeout=30);c.request(method,path,body,{'Authorization':'Bearer '+credential.read_text().strip(),'Content-Type':'application/octet-stream' if method=='PUT' else 'application/json'});response=c.getresponse();r=json.loads(response.read());actual=response.status;c.close();trace.append({'http':method+' '+path,'status':actual,'response':r});assert actual==status,(actual,r)
  if error:assert r['error']['code']==error,r
  else:assert not r['error'],r
  return r['data']
 def put(upload,data,error=None,status=200):return raw('PUT',f"/api/uploads/{upload['upload_id']}/chunks/0?"+urlencode({'size_bytes':str(len(data)),'content_hash':hashlib.sha256(data).hexdigest()}),data,agent,error,status)
 try:
  call('opencut_project_create',{'project':{'id':'a','name':'A','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'a'})
  initial_budget=inspect();assert initial_budget['reserved_bytes']=='0' and initial_budget['control']['database_name']==db.name and (root/'.storage-budget').is_dir()
  initial=head();request,policy=configure(32,'tiny');call('opencut_storage_budget_configure',{**request,'idempotency_key':'stale'},error='STORAGE_POLICY_CONFLICT');call('opencut_storage_budget_configure',{**request,'payload_limit_bytes':'33'},error='IDEMPOTENCY_KEY_REUSED');call('opencut_storage_budget_configure',{**request,'payload_limit_bytes':'032'},error='INVALID_ARGUMENT')
  source=root/'source.png';subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=64x48','-frames:v','1','-threads','1',str(source)],check=True,capture_output=True)
  asset={'id':'source','name':'Source','kind':'image','uri':str(source),'duration':None,'metadata':{}}
  edit([{'type':'asset.bind','asset':asset}],'denied',error='STORAGE_BUDGET_EXCEEDED');assert head()==initial and not list((root/'asset-store').glob('.capture-*'))
  configure(1<<20,'bind-room');at=lambda n:{'ticks':str(n),'timebase':10}
  edit([{'type':'asset.bind','asset':asset},{'type':'track.create','sequence_id':'main','track':{'id':'v','name':'V','kind':'graphic','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}},{'type':'clip.insert','sequence_id':'main','track_id':'v','clip':{'id':'c','asset_id':'source','timeline_range':{'start':at(0),'duration':at(10)},'source_range':None,'params':{},'channels':[]}}],'bind')
  state=inspect();assert int(state['observed_payload_bytes'])==source.stat().st_size and state['reserved_bytes']=='0' and not state['enforces_all_workspace_writes'];checks.append('Policy revision/replay and canonical bytes work; denied real edit leaves head untouched, accepted CAS capture settles into independently measured usage')
  capacity_source=root/'capacity.wav'
  with wave.open(str(capacity_source),'wb') as w:
   w.setnchannels(1);w.setsampwidth(2);w.setframerate(8000)
   for _ in range(96):w.writeframes(b'\0'*(1<<20))
  available=os.statvfs(root).f_bavail*os.statvfs(root).f_frsize
  assert available>160<<20
  # Retain enough physical space for managed SQLite control operations while
  # making this valid audio asset too large for the remaining payload headroom.
  configure(1<<30,'volume-denial',available-(64<<20))
  copy={**asset,'id':'capacity','kind':'audio','uri':str(capacity_source)}
  edit([{'type':'asset.bind','asset':copy}],'capacity',error='STORAGE_CAPACITY_LOW')
  capacity_source.unlink()
  configure(int(state['observed_payload_bytes']),'copy-denial')
  def submit(key):return call('opencut_job_submit',{'kind':'preview.composited_frame','idempotency_key':key,'input':{'project_id':'a','revision':head()['revision'],'sequence_id':'main','requested_time':at(0),'alignment':'exact'}})
  failed=worker(submit('no-room'));assert failed['state']=='failed' and failed['failure']['code']=='STORAGE_BUDGET_EXCEEDED',failed;assert inspect()['reserved_bytes']=='0'
  configure(1<<20,'preview-room');done=worker(submit('room'));assert done['state']=='succeeded',done
  result=call('opencut_job_get',{'job_id':done['job_id']});frame=Path(result['result']['artifact']['path']);rgb=subprocess.check_output(['ffmpeg','-v','error','-i',str(frame),'-f','rawvideo','-pix_fmt','rgb24','-']);assert rgb[0]>200 and rgb[1]<40 and rgb[2]<40;checks.append('Low volume margin and private-input admission reject before uncontrolled copy; fresh task after policy recovery produces independently decoded red pixels')
  cli(['access','set-policy','--input','-'],{'principal_id':'agent','expected_revision':'0','enabled':True,'grants':[{'project_id':'a','actions':['read','edit','run_jobs']}]});agent=root/'agent.secret';cli(['access','issue','agent','--output',str(agent)])
  for tool in ['opencut_storage_budget_inspect','opencut_storage_budget_recover']:call(tool,credential=agent,error='NOT_AUTHORIZED')
  call('opencut_storage_budget_configure',request,credential=agent,error='NOT_AUTHORIZED')
  owner=root/'owner.token'
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(owner)],stdout=log,stderr=log)
  deadline=time.monotonic()+15
  while True:
   assert host.poll() is None
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=.2):break
   except OSError:assert time.monotonic()<deadline;time.sleep(.03)
  used=int(inspect()['observed_payload_bytes']);configure(used,'chunk-denial')
  wav=root/'tone.wav'
  with wave.open(str(wav),'wb') as w:w.setnchannels(1);w.setsampwidth(2);w.setframerate(8000);w.writeframes(b'\0\0'*800)
  data=wav.read_bytes();descriptor={'project_id':'a','name':'tone.wav','size_bytes':str(len(data)),'content_hash':hashlib.sha256(data).hexdigest(),'idempotency_key':'upload'}
  upload=call('opencut_upload_begin',descriptor,credential=agent);put(upload,data,'STORAGE_BUDGET_EXCEEDED',422);assert call('opencut_upload_inspect',{'upload_id':upload['upload_id']})['next_offset']=='0'
  configure(used+len(data),'chunk-only');put(upload,data);sealed=call('opencut_upload_complete', {'upload_id':upload['upload_id'], 'content_hash':upload['content_hash']},credential=agent);failed=worker(sealed);assert failed['state']=='failed' and failed['failure']['code']=='STORAGE_BUDGET_EXCEEDED',failed
  assert call('opencut_upload_retention',{'upload_id':upload['upload_id']})['state']=='retained';call('opencut_upload_release',{'upload_id':upload['upload_id']},credential=agent);assert inspect()['reserved_bytes']=='0'
  configure(1<<20,'import-room');upload=call('opencut_upload_begin',{**descriptor,'idempotency_key':'upload-retry'},credential=agent);put(upload,data);sealed=call('opencut_upload_complete', {'upload_id':upload['upload_id'], 'content_hash':upload['content_hash']},credential=agent);done=worker(sealed);assert done['state']=='succeeded',done;assert done['extended_result']['data']['asset']['kind']=='audio';call('opencut_upload_release',{'upload_id':upload['upload_id']},credential=agent);assert inspect()['reserved_bytes']=='0';checks.append('Scoped HTTP chunk refuses without advancing offset; insufficient assembly headroom fails a real import, preserves releasable staging, and fresh upload succeeds after policy increase')
  # The original process is killed while its actual capture file is growing.
  configure(2<<30,'crash-room');large=root/'large.bin'
  with large.open('wb') as f:f.truncate(512<<20)
  before=head();body={'project_id':'a','request':{'expected_revision':before['revision'],'idempotency_key':'crash','label':'crash','operations':[{'type':'asset.bind','asset':{**asset,'id':'large','uri':str(large)}}]}}
  request_path=root/'crash.json';request_path.write_text(json.dumps(body));writer=subprocess.Popen([str(binary),'--database',str(db),'--json','call','opencut_edit_apply','--input',str(request_path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
  deadline=time.monotonic()+10
  while not list((root/'asset-store').glob('.capture-*')):assert writer.poll() is None and time.monotonic()<deadline;time.sleep(.001)
  live=call('opencut_storage_budget_recover');assert live['recovered_reservations']==0 and int(live['reserved_bytes'])>=512<<20;assert writer.poll() is None;writer.kill();writer.communicate(timeout=10);writer=None
  pending=inspect();assert pending['reservations'];files=list((root/'asset-store').glob('.capture-*'));assert files;hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in files};recovered=call('opencut_storage_budget_recover');assert recovered['recovered_reservations']==1 and recovered['reserved_bytes']=='0';assert hashes=={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in files};assert head()==before
  plan=call('opencut_cas_gc_plan',{'idempotency_key':'crash-orphan'});call('opencut_cas_gc_execute',{'plan_id':plan['plan_id'],'expected_plan_hash':plan['plan_hash']});assert all(not p.exists() for p in files);assert large.stat().st_size==512<<20;checks.append('Actual CLI capture SIGKILL leaves a durable reservation; recovery retains live writer, later releases only dead reservation, preserves bytes, and separate reviewed GC removes the orphan')
  raw('POST','/api/tools/opencut_storage_budget_inspect',b'{}',agent,'NOT_AUTHORIZED',403);assert raw('POST','/api/tools/opencut_storage_budget_inspect',b'{}',owner)['policy']==inspect()['policy'];checks.append('Owner CLI/HTTP observe the same durable policy and restricted callers cannot inspect, configure or recover global admission')
  if sdk:
   p=subprocess.run(['node',str(ROOT/'scripts/verify-budget-mcp.mjs'),str(sdk),str(binary),str(db),f'http://127.0.0.1:{port}',str(agent),str(owner)],capture_output=True,text=True,timeout=90);(root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert not p.returncode,(p.stdout,p.stderr);checks.append('Official MCP stdio and HTTP consumers discover read/write semantics, configure with exact replay, inspect and recover; restricted operations reject')
  report={'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
 finally:
  if writer and writer.poll() is None:writer.kill();writer.communicate(timeout=10)
  if host and host.poll() is None:host.terminate();host.wait(timeout=10)
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-budget-'))).resolve(),a.mcp_sdk_root)
