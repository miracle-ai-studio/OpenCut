#!/usr/bin/env python3
"""Actual small-volume ENOSPC, transactional recovery, and pinned-reader WAL maintenance."""
import argparse, errno, hashlib, json, os, platform, socket, sqlite3, subprocess, tempfile, time
from contextlib import closing
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]

def verify(binary,root):
 root.mkdir(parents=True,exist_ok=True);mount=root/'volume';mount.mkdir();image=root/'disk.dmg';trace=[];checks=[];reader=None;attached=False
 assert platform.system()=='Darwin','This physical-capacity fixture currently requires macOS hdiutil; never fill the host filesystem'
 def command(args):
  p=subprocess.run(args,capture_output=True,text=True,timeout=45);assert p.returncode==0,(args,p.stdout,p.stderr);return p.stdout
 def call(name,data=None,error=None,credential=None):
  args=[str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+['call',name,'--input','-']
  p=subprocess.run(args,input=json.dumps(data or {}),text=True,capture_output=True,timeout=30);r=json.loads(p.stdout);trace.append({'tool':name,'response':r,'exit':p.returncode})
  if error:assert p.returncode and r['error']['code']==error,(r,p.stderr)
  else:assert not p.returncode and not r['error'],(r,p.stderr)
  return r['data']
 def dump():
  with closing(sqlite3.connect(f'file:{db}?mode=ro',uri=True)) as c:return list(c.iterdump())
 try:
  command(['hdiutil','create','-size','64m','-fs','HFS+','-volname','OpenCutCapacityQA',str(image)])
  command(['hdiutil','attach','-nobrowse','-mountpoint',str(mount),str(image)]);attached=True
  volume=os.statvfs(mount);assert mount.stat().st_dev!=root.stat().st_dev and 0<volume.f_blocks*volume.f_frsize<=128<<20
  db=mount/'workspace.sqlite'
  initial=call('opencut_project_create',{'project':{'id':'capacity','name':'Capacity','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
  source=root/'source.png';command(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=64x48','-frames:v','1',str(source)])
  job=call('opencut_job_submit',{'kind':'preview.frame','idempotency_key':'queued','input':{'input':str(source),'requested_time':{'ticks':'0','timebase':10},'alignment':'exact'}})
  before=call('opencut_database_inspect');assert before['persistent'] and before['journal_mode']=='wal' and not before['enforces_total_storage_limit']
  assert int(before['logical_database_bytes'])==int(before['page_count'])*before['page_size_bytes']
  reader=sqlite3.connect(db);reader.execute('BEGIN');reader.execute('SELECT * FROM projects').fetchall()
  baseline=dump();filler=mount/'fixture-filler';filled=0
  with filler.open('wb',buffering=0) as f:
   for block in [1<<20,4096,512]:
    while True:
     try:
      n=f.write(b'X'*block);assert n;filled+=n
     except OSError as e:
      assert e.errno==errno.ENOSPC,e;break
   try:os.fsync(f.fileno())
   except OSError as e:assert e.errno==errno.ENOSPC
  exhausted=os.statvfs(mount);assert exhausted.f_bavail*exhausted.f_frsize<4096
  track={'id':'v','name':'T'*(512<<10),'kind':'video','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}
  request={'project_id':'capacity','request':{'expected_revision':initial['revision'],'idempotency_key':'capacity-edit','label':'capacity','operations':[{'type':'track.create','sequence_id':'main','track':track}]}}
  call('opencut_edit_apply',request,'DATABASE_CAPACITY_EXHAUSTED');assert dump()==baseline
  call('opencut_job_request_cancel',{'job_id':job['job_id']},'DATABASE_CAPACITY_EXHAUSTED');assert dump()==baseline
  assert call('opencut_project_inspect',{'project_id':'capacity'})==initial
  assert call('opencut_job_view',{'job_id':job['job_id']})['state']=='queued'
  checks.append('Actual independent 64 MiB filesystem filled to ENOSPC: edit and cancellation report precise capacity failure, all SQL rows and project head remain unchanged, queued job is not falsely cancelled')
  filler.unlink()
  committed=call('opencut_edit_apply',request);assert call('opencut_edit_apply',request)==committed
  cancelled=call('opencut_job_request_cancel',{'job_id':job['job_id']});assert cancelled['state']=='cancelled';assert call('opencut_job_request_cancel',{'job_id':job['job_id']})['state']=='cancelled'
  with closing(sqlite3.connect(db)) as c:
   assert c.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
   assert c.execute("SELECT count(*) FROM audit_records WHERE job_id=? AND action='job.cancelled'",(job['job_id'],)).fetchone()[0]==1
  checks.append('After releasing only fixture filler, original edit key commits and replays exactly once; explicit cancellation succeeds once and actual SQLite integrity_check passes')
  expanded=call('opencut_database_inspect');assert int(expanded['wal_file_bytes'])>0
  logical=dump();passive=call('opencut_database_checkpoint',{'mode':'passive'});assert passive['outcome']=='blocked' and passive['checkpointed_frames']<passive['log_frames'],passive
  start=time.monotonic();blocked=call('opencut_database_checkpoint',{'mode':'truncate','busy_timeout_ms':25});elapsed=time.monotonic()-start
  assert blocked['outcome']=='blocked' and elapsed<5 and int(blocked['observed_wal_file_bytes'])>0,blocked
  assert reader.execute('SELECT count(*) FROM revisions').fetchone()[0]==1;assert dump()==logical
  call('opencut_database_checkpoint',{'mode':'truncate','busy_timeout_ms':5001},'INVALID_ARGUMENT')
  reader.rollback();reader.close();reader=None
  done=call('opencut_database_checkpoint',{'mode':'truncate'});assert done['outcome']=='complete' and done['observed_wal_file_bytes']=='0',done
  assert dump()==logical and call('opencut_project_inspect',{'project_id':'capacity'})['revision']==committed['revision']
  again=call('opencut_database_checkpoint',{'mode':'truncate'});assert again['outcome']=='complete'
  checks.append('Real pinned reader blocks passive/truncate completion without being killed; bounded lock waiting reports blocked honestly; release then explicit truncate reclaims WAL without changing any SQL row')
  report={'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'volume_capacity_bytes':volume.f_blocks*volume.f_frsize,'filler_bytes':filled,'available_at_failure_bytes':exhausted.f_bavail*exhausted.f_frsize,'wal_before_checkpoint_bytes':expanded['wal_file_bytes'],'blocked_wait_seconds':elapsed,'evidence':str(root)}
  (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
 finally:
  if reader:reader.close()
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
  if attached:
   for _ in range(10):
    p=subprocess.run(['hdiutil','detach',str(mount)],capture_output=True,text=True,timeout=30)
    if p.returncode==0 or '资源忙' not in p.stderr and 'busy' not in p.stderr.lower():break
    time.sleep(.3)
   (root/'detach.json').write_text(json.dumps({'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr},indent=2));assert p.returncode==0,(p.stdout,p.stderr)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-database-recovery-'))).resolve())
