#!/usr/bin/env python3
"""Real-process storage observations against independently measured native files."""
import argparse, hashlib, http.client, json, os, shutil, socket, sqlite3, stat, subprocess, tempfile, threading, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]

def verify(binary,root,sdk):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';checks=[];trace=[];host=None
 def cli(args,data=None,credential=None,error=None):
  p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=json.dumps(data) if data is not None else None,text=True,capture_output=True,timeout=20)
  out=json.loads(p.stdout);trace.append({'command':args,'response':out,'exit':p.returncode})
  if error:assert out['error']['code']==error and p.returncode,(out,p.stderr)
  else:assert not out['error'] and p.returncode==0,(out,p.stderr)
  return out
 def call(data=None,credential=None,error=None):return cli(['call','opencut_storage_inspect','--input','-'],data or {},credential,error)
 def state():
  with sqlite3.connect(db) as c:return list(c.iterdump())
 def cats(r):return {x['category']:x for x in r['categories']}
 try:
  cli(['call','opencut_project_create','--input','-'],{'project':{'id':'a','name':'A','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'a'})
  schema=cli(['system','schema'])['data'];shape=next(t for t in schema if t['name']=='opencut_storage_inspect')['input_schema']['properties']['max_entries'];assert shape['minimum']==1 and shape['maximum']==100000
  before=state();before_dirs={p.name for p in root.iterdir() if p.is_dir()};r=call()['data'];assert r['complete'] and r['volume_observed'];assert int(r['volume_capacity_bytes'])==os.statvfs(root).f_blocks*os.statvfs(root).f_frsize
  assert cats(r)['database']['regular_files']>=1
  assert all(not x['present'] and x['logical_bytes']=='0' for x in r['categories'][1:]);assert state()==before;assert {p.name for p in root.iterdir() if p.is_dir()}==before_dirs
  for data in [{'max_entries':0},{'max_entries':100001},{'path':'/private'},{'actor':'owner'}]:call(data,error='INVALID_ARGUMENT')
  checks.append('Storage observation creates no additional directories or business records; managed VFS budget metadata predates it, volume capacity matches statvfs and untrusted path/budget inputs reject')
  mapping=[('asset_store','asset-store'),('media','media'),('uploads','uploads'),('artifacts','artifacts'),('plugins','plugins')]
  for i,(category,name) in enumerate(mapping):
   folder=root/name;folder.mkdir();(folder/'entry').write_bytes(bytes([i+1])*(i+1)*2048)
  # A sparse file exposes the distinction between apparent and allocated bytes.
  sparse=root/'artifacts/sparse'
  with sparse.open('wb') as f:f.seek(8*1024*1024);f.write(b'x')
  os.link(root/'asset-store/entry',root/'media/alias')
  seen=set();expected={}
  for category,name in mapping:
   logical=allocated=count=aliases=0
   for p in [root/name,*sorted((root/name).iterdir())]:
    s=p.lstat();key=(s.st_dev,s.st_ino)
    if key in seen:aliases+=1;continue
    seen.add(key);allocated+=s.st_blocks*512
    if stat.S_ISREG(s.st_mode):logical+=s.st_size;count+=1
   expected[category]=(logical,allocated,count,aliases)
  before=state();r=call()['data'];assert r['complete'];assert state()==before
  for category,(logical,allocated,count,aliases) in expected.items():
   c=cats(r)[category];assert (int(c['logical_bytes']),int(c['allocated_bytes']),c['regular_files'],c['hardlink_aliases'])==(logical,allocated,count,aliases),(category,c,expected)
  assert sum(int(x['logical_bytes']) for x in r['categories'])==int(r['logical_bytes']);assert sum(int(x['allocated_bytes']) for x in r['categories'])==int(r['allocated_bytes'])
  assert root.as_posix() not in json.dumps(r) and 'entry' not in json.dumps(r)
  checks.append('Five managed categories match independent stat lengths and allocated blocks, sparse file accounting, cross-category hardlink dedup and additive totals without native paths')
  outside=root/'outside';outside.mkdir();(outside/'SENSITIVE').write_bytes(b'x'*1000000)
  (root/'uploads/link').symlink_to(outside,target_is_directory=True);os.mkfifo(root/'uploads/pipe')
  r=call()['data'];u=cats(r)['uploads'];assert not r['complete'] and not u['complete'] and u['omissions']['symlinks']==1 and u['omissions']['other_file_types']==1 and int(u['logical_bytes'])==expected['uploads'][0]
  (root/'uploads/link').unlink();(root/'uploads/pipe').unlink()
  # Depth budget must stop before an inaccessible-to-budget leaf.
  p=root/'plugins'
  for i in range(65):p=p/'d';p.mkdir()
  (p/'leaf').write_bytes(b'x')
  r=call()['data'];assert not r['complete'] and cats(r)['plugins']['omissions']['depth_limited']==1
  r=call({'max_entries':2})['data'];assert not r['complete'] and r['stop_reason']=='entry_limit' and r['visited_entries']==2 and all(not x['complete'] for x in r['categories'])
  checks.append('Symlink/FIFO/depth omissions and global entry exhaustion produce explicit partial results; linked outside bytes are never counted')
  # The write-admission scanner must fail closed on the deliberately excessive
  # depth. Remove only that fixture before testing later write-bearing flows.
  shutil.rmtree(root/'plugins/d')
  policy={'principal_id':'agent','expected_revision':'0','enabled':True,'grants':[{'project_id':'a','actions':['read','edit','run_jobs']}]}
  cli(['access','set-policy','--input','-'],policy);agent=root/'agent.secret';cli(['access','issue','agent','--output',str(agent)])
  denial=call(credential=agent,error='NOT_AUTHORIZED');audit=cli(['call','opencut_denial_query','--input','-'],{'target':{'kind':'request','id':denial['request_id']}})['data'];assert len(audit['records'])==1 and audit['records'][0]['operation']['name']=='opencut_storage_inspect'
  token=root/'owner.token'
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
  deadline=time.monotonic()+15
  while True:
   assert host.poll() is None
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=.2):break
   except OSError:assert time.monotonic()<deadline;time.sleep(.03)
  def request(data,credential=token):
   c=http.client.HTTPConnection('127.0.0.1',port,timeout=15);c.request('POST','/v1/tools/opencut_storage_inspect',json.dumps(data),{'Content-Type':'application/json','Authorization':'Bearer '+credential.read_text().strip()});response=c.getresponse();out=json.loads(response.read());c.close();trace.append({'http':response.status,'response':out});return response.status,out
  code,r=request({'max_entries':2});assert code==200 and r['data']['stop_reason']=='entry_limit';code,r=request({},agent);assert code==403 and r['error']['code']=='NOT_AUTHORIZED'
  checks.append('CLI/HTTP owner inspection and scoped principal refusal share contract; denied inspection is correlated in existing audit')
  # Race an actual directory with a link to external sensitive bytes while the server scans.
  moving=root/'media/moving';moving.mkdir();(moving/'local').write_bytes(b'ok');park=root/'media/park';stop=threading.Event();failures=[]
  def swap():
   try:
    while not stop.is_set():
     moving.rename(park);moving.symlink_to(outside,target_is_directory=True);moving.unlink();park.rename(moving)
   except BaseException as e:failures.append(repr(e))
  worker=threading.Thread(target=swap);worker.start()
  try:
   for _ in range(40):
    code,out=request({});assert code==200;assert int(cats(out['data'])['media']['logical_bytes'])<=expected['media'][0]+2,out
  finally:stop.set();worker.join(timeout=5)
  assert not worker.is_alive() and not failures;assert (outside/'SENSITIVE').stat().st_size==1000000
  checks.append('Forty real HTTP observations during directory/symlink replacement never traverse external bytes or hang on special files')
  if sdk:
   p=subprocess.run(['node',str(ROOT/'scripts/verify-storage-mcp.mjs'),str(sdk),str(binary),str(db),f'http://127.0.0.1:{port}',str(agent),str(token)],text=True,capture_output=True,timeout=90);(root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert p.returncode==0,(p.stdout,p.stderr);checks.append('Official MCP SDK validates typed owner observations and restricted refusals through stdio and Streamable HTTP')
  host.terminate();host.wait(timeout=10);host=None
  report={'status':'passed','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks,'evidence':str(root)};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as e:(root/'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2));raise
 finally:
  if host and host.poll() is None:host.terminate();host.wait(timeout=10)
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=ROOT/'target/debug/opencut');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-storage-'))).resolve(),a.mcp_sdk_root)
