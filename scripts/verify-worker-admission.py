#!/usr/bin/env python3
"""Real Worker output limits, caption bytes, attempt locks and orphan recovery."""
import argparse, hashlib, json, os, shutil, signal, sqlite3, subprocess, tarfile, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]

def verify(binary,root):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';trace=[];checks=[];workers=[];children=set();ffmpeg=shutil.which('ffmpeg');assert ffmpeg
 def call(name,data=None,error=None,database=db,exe=binary):
  p=subprocess.run([str(exe),'--database',str(database),'--json','call',name,'--input','-'],input=json.dumps(data or {}),text=True,capture_output=True,timeout=40);r=json.loads(p.stdout);trace.append({'tool':name,'input':data,'response':r,'exit':p.returncode})
  if error:assert p.returncode and r['error']['code']==error,(r,p.stderr)
  else:assert not p.returncode and r['error'] is None,(r,p.stderr)
  return r['data']
 def state():return call('opencut_storage_budget_inspect')
 def configure(key,ceiling=None,total=1<<30):
  before=state();data={'expected_revision':before['policy']['revision'],'idempotency_key':key,'payload_limit_bytes':str(total),'minimum_available_bytes':'0'}
  if ceiling is not None:data['media_output_limit_bytes']=str(ceiling)
  change=call('opencut_storage_budget_configure',data);assert call('opencut_storage_budget_configure',data)==change
  assert set(change['policy'])=={'revision','payload_limit_bytes','minimum_available_bytes'}
  assert change['media_output_limit_bytes']==str(ceiling) if ceiling is not None else change['media_output_limit_bytes']==before['media_output_limit_bytes']
  return change
 def worker(label,env=None):
  with (root/f'{label}.log').open('w') as log:p=subprocess.Popen([str(binary),'--database',str(db),'worker','run','--once'],env=env,stdout=log,stderr=log)
  workers.append(p);return p
 def view(job):return call('opencut_job_view',{'job_id':job['job_id']})
 def run(job):
  p=worker('run-'+job['job_id']);assert p.wait(timeout=90)==0
  result=view(job);assert result['state'] in ['succeeded','failed','cancelled'],result;return result
 def submit(kind,key,input):return call('opencut_job_submit',{'kind':kind,'idempotency_key':key,'input':input})
 def settled():
  deadline=time.monotonic()+15
  while True:
   s=state()
   if s['reserved_bytes']=='0':return s
   assert time.monotonic()<deadline,s;time.sleep(.03)
 def live(job,attempt,parent):
  deadline=time.monotonic()+20
  while True:
   s=state();matches=[r for r in s['reservations'] if r['category']=='media_output' and r.get('writer')=={'job_id':job['job_id'],'attempt':attempt}]
   if matches:
    for line in subprocess.check_output(['ps','-axo','pid,ppid,command'],text=True).splitlines()[1:]:
     parts=line.strip().split(None,2)
     if len(parts)==3 and int(parts[1])==parent.pid and parts[2].startswith(ffmpeg+' ') and '-progress' in parts[2] and job['job_id'] in parts[2]:
      pid=int(parts[0]);os.kill(pid,signal.SIGSTOP);children.add(pid);return pid,s,matches[0]
   assert parent.poll() is None and time.monotonic()<deadline,(parent.poll(),s);time.sleep(.01)
 def t(n):return {'ticks':str(n),'timebase':10}
 try:
  project=call('opencut_project_create',{'project':{'id':'admission','name':'Output admission','sequence_id':'main','width':192,'height':108,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
  assert state()['media_output_limit_bytes']==str(1<<30);assert {'media_output','media_caption'}<=set(state()['coverage']);assert not state()['enforces_all_workspace_writes']
  previous=ROOT/'dist/local/opencut-0.1.0-dev-4398ef6e2a9e-x86_64-apple-darwin.tar.gz'
  if previous.is_file():
   assert hashlib.sha256(previous.read_bytes()).hexdigest()==Path(str(previous)+'.sha256').read_text().split()[0]
   with tarfile.open(previous) as tar:
    manifest=json.load(tar.extractfile(next(m for m in tar if m.name.endswith('/manifest.json'))));oldbytes=tar.extractfile(next(m for m in tar if m.name.endswith('/bin/opencut'))).read()
   assert manifest['database_schema']==14 and hashlib.sha256(oldbytes).hexdigest()==manifest['binary_sha256'];old=root/'old-opencut';old.write_bytes(oldbytes);old.chmod(0o755)
   with sqlite3.connect(db) as c:before=list(c.iterdump())
   call('opencut_project_list',error='UNSUPPORTED_DATABASE_VERSION',exe=old)
   with sqlite3.connect(db) as c:assert list(c.iterdump())==before
   legacy=root/'legacy';legacy.mkdir();legacydb=legacy/'workspace.sqlite';oldrequest={'expected_revision':0,'idempotency_key':'legacy-policy','payload_limit_bytes':str(1<<30),'minimum_available_bytes':'0'}
   oldchange=call('opencut_storage_budget_configure',oldrequest,database=legacydb,exe=old);upgraded=call('opencut_storage_budget_inspect',database=legacydb);assert upgraded['last_policy_change']==oldchange and upgraded['media_output_limit_bytes']==str(1<<30);assert call('opencut_storage_budget_configure',oldrequest,database=legacydb)==oldchange
   checks.append('Actual released schema-14 host refuses schema-15; upgrading its real old ledger preserves original policy receipt while deriving the new default ceiling')
  configure('tiny',64);configure('legacy-omit');assert state()['media_output_limit_bytes']=='64'
  oldstate=state();call('opencut_storage_budget_configure',{'expected_revision':oldstate['policy']['revision'],'idempotency_key':'zero','payload_limit_bytes':str(1<<30),'minimum_available_bytes':'0','media_output_limit_bytes':'0'},'INVALID_ARGUMENT');assert state()['policy']==oldstate['policy']
  source=root/'red.png';subprocess.run([ffmpeg,'-v','error','-f','lavfi','-i','color=red:s=192x108','-frames:v','1','-threads','1',str(source)],check=True,capture_output=True)
  frame={'input':str(source),'requested_time':t(0),'alignment':'exact'}
  failed=run(submit('preview.frame','limit',frame));assert failed['state']=='failed' and failed['failure']['code']=='MEDIA_OUTPUT_LIMIT_EXCEEDED' and not failed['artifacts'],failed;settled()
  configure('frame-room',2<<20);done=run(submit('preview.frame','frame-retry',frame));assert done['state']=='succeeded',done;artifact=call('opencut_job_get',{'job_id':done['job_id']})['result']['artifact'];rgb=subprocess.check_output([ffmpeg,'-v','error','-i',artifact['path'],'-f','rawvideo','-pix_fmt','rgb24','-']);assert rgb[0]>200 and rgb[1]<40 and rgb[2]<40;settled()
  checks.append('Output ceiling is explicit, old configure input preserves it, zero is rejected atomically, capped real FFmpeg fails without artifact and fresh task succeeds with decoded red pixels')
  ops=[{'type':'asset.bind','asset':{'id':'red','kind':'image','name':'red','uri':str(source),'duration':None,'metadata':{}}},{'type':'track.create','sequence_id':'main','track':{'id':'v','name':'V','kind':'graphic','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}},{'type':'clip.insert','sequence_id':'main','track_id':'v','clip':{'id':'clip','asset_id':'red','timeline_range':{'start':t(0),'duration':t(20)},'source_range':None,'params':{},'channels':[]}},{'type':'caption.insert','sequence_id':'main','cue':{'id':'caption','timeline_range':{'start':t(0),'duration':t(20)},'text':'CAPTION','source_anchor':None,'style':{'font_family':'Arial','font_size':18,'color':'white','background_color':None}}}]
  edited=call('opencut_edit_apply',{'project_id':'admission','request':{'expected_revision':project['revision'],'idempotency_key':'edit','label':'caption','operations':ops}});fixed={'project_id':'admission','revision':edited['revision'],'sequence_id':'main'}
  configure('caption-output-deny',64);failed=run(submit('preview.composited_frame','caption-limit',{**fixed,'requested_time':t(0),'alignment':'exact'}));assert failed['state']=='failed' and failed['failure']['code']=='MEDIA_OUTPUT_LIMIT_EXCEEDED',failed;assert not list((root/'artifacts').rglob('*.caption-*.png'));settled()
  configure('caption-room',2<<20)
  for kind,input in [('preview.project_frame',{**fixed,'requested_time':t(0),'alignment':'exact'}),('preview.composited_frame',{**fixed,'requested_time':t(0),'alignment':'exact'}),('preview.project_range',{**fixed,'requested_range':{'start':t(0),'duration':t(10)},'alignment':'exact'}),('render.project',{**fixed,'container':'mp4','video_codec':'h264'})]:
   done=run(submit(kind,'success-'+kind,input));assert done['state']=='succeeded',done
   artifact=call('opencut_job_get',{'job_id':done['job_id']})['result']['artifact'];assert Path(artifact['path']).stat().st_size < 2<<20
   rgb=subprocess.check_output([ffmpeg,'-v','error','-i',artifact['path'],'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-']);assert rgb[0]>200 and rgb[1]<40 and rgb[2]<40
   assert sum(min(rgb[i:i+3])>180 for i in range(0,len(rgb),3))>10
   assert not list((root/'artifacts').rglob('*.caption-*.png'));settled()
  checks.append('Direct/composited frame, range and project export use the same admission port; actual caption intermediates clean on failure/success and final images decode correctly')
  # The wrapper only adds real-time input pacing; it execs the installed FFmpeg
  # in place, retaining exactly the descriptors installed by the native host.
  slow=root/'slow-bin';slow.mkdir();wrapper=slow/'ffmpeg';wrapper.write_text('#!/usr/bin/env python3\nimport os,sys\na=sys.argv[1:]\nif "-progress" in a and a[-1]!="-":a=["-re",*a]\nos.execv('+repr(ffmpeg)+',['+repr(ffmpeg)+',*a])\n');wrapper.chmod(0o755);env={**os.environ,'PATH':str(slow)+os.pathsep+os.environ['PATH']}
  video=root/'long.mp4';subprocess.run([ffmpeg,'-v','error','-f','lavfi','-i','testsrc2=size=320x180:rate=10:duration=60','-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p',str(video)],check=True,capture_output=True,timeout=60)
  configure('busy-heartbeat-room',16<<20)
  busy_job=submit('render.export','busy-heartbeat',{'segments':[{'input':str(video),'source_start':t(0),'source_duration':t(600)}],'container':'mp4','video':{'width':320,'height':180,'frame_rate':{'numerator':10,'denominator':1},'aspect_mode':'fit','codec':'h264'},'audio':{'mode':'drop'}})
  busy_worker=worker('busy-heartbeat',env);deadline=time.monotonic()+20
  while view(busy_job)['state']!='running':assert busy_worker.poll() is None and time.monotonic()<deadline;time.sleep(.05)
  lease_before=call('opencut_job_get',{'job_id':busy_job['job_id']})['lease_until']
  with sqlite3.connect(db,timeout=5) as lock:
   lock.execute('BEGIN IMMEDIATE')
   time.sleep(7)
   lock.rollback()
  assert view(busy_job)['state']=='running' and not view(busy_job)['cancel_requested']
  deadline=time.monotonic()+12
  while call('opencut_job_get',{'job_id':busy_job['job_id']})['lease_until']<=lease_before:
   assert time.monotonic()<deadline;time.sleep(.1)
  call('opencut_job_request_cancel',{'job_id':busy_job['job_id']});assert busy_worker.wait(timeout=30)==0
  assert view(busy_job)['state']=='cancelled' and 'heartbeat failed' not in (root/'busy-heartbeat.log').read_text()
  settled();checks.append('A real seven-second SQLite write lock outlasts one busy timeout; the Worker retains and renews its lease instead of cancelling on transient database busy')
  configure('orphan-limit',256<<10);job=submit('render.export','orphan',{'segments':[{'input':str(video),'source_start':t(0),'source_duration':t(600)}],'container':'mp4','video':{'width':320,'height':180,'frame_rate':{'numerator':10,'denominator':1},'aspect_mode':'fit','codec':'h264'},'audio':{'mode':'drop'}})
  first=worker('orphan-first',env);pid1,_,reservation=live(job,1,first);assert reservation['reserved_bytes']==str(256<<10);attempt1=root/'artifacts'/job['job_id']/'attempt-1';assert attempt1.is_dir();first.kill();first.wait(timeout=15)
  recovered=call('opencut_storage_budget_recover');assert any(r.get('writer')=={'job_id':job['job_id'],'attempt':1} for r in recovered['reservations'])
  record=call('opencut_job_get',{'job_id':job['job_id']});deadline=time.monotonic()+45
  while int(time.time()*1000)<=record['lease_until']+100:assert time.monotonic()<deadline;time.sleep(.1)
  second=worker('orphan-second',env);pid2,_,_=live(job,2,second);assert attempt1.is_dir();current=view(job);assert call('opencut_job_get',{'job_id':job['job_id']})['attempt']==2 and current['state']=='running',current
  live_state=call('opencut_storage_budget_recover');assert {r['writer']['attempt'] for r in live_state['reservations'] if r.get('writer',{}).get('job_id')==job['job_id']}=={1,2},live_state
  call('opencut_job_request_cancel',{'job_id':job['job_id']});assert second.wait(timeout=30)==0;children.discard(pid2);cancelled=view(job);assert cancelled['state']=='cancelled' and not cancelled['artifacts'],cancelled
  still=call('opencut_storage_budget_recover');assert any(r.get('writer',{}).get('attempt')==1 for r in still['reservations']);assert attempt1.is_dir()
  os.kill(pid1,signal.SIGCONT);deadline=time.monotonic()+90
  while call('opencut_storage_budget_recover')['reserved_bytes']!='0':assert time.monotonic()<deadline;time.sleep(.05)
  children.discard(pid1)
  files=[p for p in attempt1.rglob('*') if p.is_file()];orphan_sizes=[p.stat().st_size for p in files];assert orphan_sizes and max(orphan_sizes)>0 and all(n<=256<<10 for n in orphan_sizes)
  cleanup=worker('orphan-cleanup');assert cleanup.wait(timeout=30)==0;assert not attempt1.exists();settled()
  checks.append('Actual Worker SIGKILL retains orphan FFmpeg output and attempt locks; natural lease expiry starts attempt 2 without deleting live attempt 1, cancellation stops only attempt 2, and later orphan exit permits cleanup')
  # A misplaced artifact-root alias must not let startup cleanup touch its target.
  artifact_root=root/'artifacts';preserved=root/'preserved-artifacts';outside=root/'outside';outside.mkdir();sentinel=outside/'sentinel';sentinel.write_text('preserve')
  artifact_root.rename(preserved)
  try:
   artifact_root.symlink_to(outside,target_is_directory=True)
   refused=worker('aliased-cleanup');exit_code=refused.wait(timeout=30);log=(root/'aliased-cleanup.log').read_text()
   assert (exit_code==0 and 'PRIVATE_INPUT_CLEANUP_FAILED' in log) or (exit_code!=0 and 'STORAGE_ERROR' in log),(exit_code,log)
   assert sentinel.read_text()=='preserve' and list(outside.iterdir())==[sentinel]
  finally:
   artifact_root.unlink();preserved.rename(artifact_root)
  resumed=run(submit('preview.frame','after-alias-restore',frame));assert resumed['state']=='succeeded',resumed
  checks.append('Aliased artifact root fails closed at Worker startup or cleanup without touching outside files; restored root runs a real frame job')
  assert call('opencut_project_inspect',{'project_id':'admission'})['revision']==edited['revision']
  report={'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'orphan_output_bytes':orphan_sizes,'orphan_output_ceiling':256<<10,'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
 finally:
  for pid in children:
   try:os.kill(pid,signal.SIGKILL)
   except ProcessLookupError:pass
  for p in workers:
   if p.poll() is None:p.kill()
   p.wait(timeout=15)
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-worker-admission-'))).resolve())
