#!/usr/bin/env python3
"""Legacy base project cover through existing native ports and the actual Web UI."""
import argparse,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
REPO=Path(__file__).resolve().parents[1]
def verify(binary,web,root,resume=False):
 root.mkdir(parents=True,exist_ok=resume);frozen=root/'opencut'
 if frozen.exists():assert resume and hashlib.sha256(frozen.read_bytes()).digest()==hashlib.sha256(binary.read_bytes()).digest()
 else:shutil.copy2(binary,frozen)
 if resume and (root/'trace.json').exists():
  trial=root/f'trial-trace-{time.time_ns()}.json';shutil.copy2(root/'trace.json',trial)
 db=root/'workspace.sqlite';service=root/'service';trace=[];state=None
 def lifecycle(action):
  p=subprocess.run([str(frozen),'--json','--database',str(db),'server','--directory',str(service),action,*(['--listen','127.0.0.1:0','--web-root',str(web)] if action=='start' else [])],capture_output=True,text=True,timeout=60)
  r=json.loads(p.stdout);assert p.returncode==0 and r['error'] is None,r;return r['data']
 def call(name,value):
  deadline=time.monotonic()+5
  while True:
   p=subprocess.run([str(frozen),'--json','--database',str(db),'call',name,'--input','-'],input=json.dumps(value),capture_output=True,text=True,timeout=60);r=json.loads(p.stdout)
   trace.append({'tool':name,'input':value,'reply':r})
   if r['error'] and r['error']['code']=='STORAGE_BUDGET_BUSY' and time.monotonic()<deadline:time.sleep(.1);continue
   assert p.returncode==0 and r['error'] is None,r;return r['data']
 try:
  state=lifecycle('start');assert state['ready']
  created=call('opencut_project_create',{'project':{'id':'legacy-base-cover','name':'Legacy base cover','sequence_id':'main','width':160,'height':90,'fps':{'numerator':24,'denominator':1}},'idempotency_key':'base-create'})
  source=root/'lime.mp4'
  if not source.exists():subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=lime:s=160x90:r=24:d=2','-c:v','libx264','-pix_fmt','yuv420p',str(source)],check=True,timeout=30)
  upload=call('opencut_upload_begin',{'project_id':'legacy-base-cover','idempotency_key':'lime-upload','name':'lime.mp4','size_bytes':str(source.stat().st_size)})
  if upload['state']=='receiving':
   assert upload['next_offset']=='0',upload
   transfer=subprocess.run([str(frozen),'--json','--database',str(db),'upload','put',upload['upload_id'],'--offset','0','--input',str(source)],capture_output=True,text=True,timeout=60)
   received=json.loads(transfer.stdout);trace.append({'transport':'cli put','upload_id':upload['upload_id'],'reply':received})
   assert transfer.returncode==0 and received['error'] is None,received
  job=call('opencut_upload_complete',{'upload_id':upload['upload_id'],'content_hash':hashlib.sha256(source.read_bytes()).hexdigest()});job=call('opencut_job_view',{'job_id':job['job_id']});deadline=time.monotonic()+120
  while job['state'] in ['queued','running'] and time.monotonic()<deadline:time.sleep(.2);job=call('opencut_job_view',{'job_id':job['job_id']})
  assert job['state']=='succeeded',job
  asset=call('opencut_project_media_inspect',{'project_id':'legacy-base-cover','media_id':job['extended_result']['data']['media_id']})['asset'];span={'start':{'ticks':'0','timebase':1},'duration':asset['duration']}
  result=call('opencut_edit_apply',{'project_id':'legacy-base-cover','request':{'expected_revision':created['revision'],'idempotency_key':'base-bind','label':'Base cover media','operations':[{'type':'asset.bind','asset':asset},{'type':'track.create','sequence_id':'main','track':{'id':'v','name':'V','kind':'video','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}},{'type':'clip.insert','sequence_id':'main','track_id':'v','clip':{'id':'c','asset_id':asset['id'],'timeline_range':span,'source_range':span,'params':{},'channels':[]}}]}})
  source.unlink();before=call('opencut_project_inspect',{'project_id':'legacy-base-cover'})
  env={**os.environ,'OPENCUT_WEB_URL':state['endpoint'],'OPENCUT_WEB_WORKSPACE':str(root),'OPENCUT_BASE_REVISION':result['revision']}
  subprocess.run(['node',str(REPO/'apps/web/scripts/verify-base-cover.mjs')],cwd=REPO,env=env,check=True,timeout=240)
  after=call('opencut_project_inspect',{'project_id':'legacy-base-cover'});assert before==after
  report=json.loads((root/'base-cover-browser-report.json').read_text());report['binary_sha256']=hashlib.sha256(frozen.read_bytes()).hexdigest();report['web_files']={str(f.relative_to(web)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(web.rglob('*')) if f.is_file()};report['source_removed']=True;report['content_revision_unchanged']=True
  (root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
 finally:
  (root/'trace.json').write_text(json.dumps(trace,indent=2)+'\n')
  if state:lifecycle('stop')
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--binary',type=Path,required=True);p.add_argument('--web-root',type=Path,required=True);p.add_argument('--artifacts-dir',type=Path,required=True);p.add_argument('--resume',action='store_true');a=p.parse_args();verify(a.binary.resolve(),a.web_root.resolve(),a.artifacts_dir.resolve(),a.resume)
