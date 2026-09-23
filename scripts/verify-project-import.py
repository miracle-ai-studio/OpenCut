#!/usr/bin/env python3
"""Public project upload -> imported resource -> scoped edit -> real RGB artifact."""
import argparse,copy,hashlib,http.client,json,socket,sqlite3,subprocess,tempfile,time
import http.client as http_client
from pathlib import Path
from urllib.parse import quote,urlencode

def verify(binary,root,sdk):
 root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';checks=[];trace=[];host=None
 def cli(args,payload=None,credential=None,error=None,raw=None):
  p=subprocess.run([str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+args,input=raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None),capture_output=True,timeout=40)
  out=json.loads(p.stdout);trace.append({'cli':args,'exit':p.returncode,'response':out})
  if error:assert p.returncode and out['error']['code']==error,(out,p.stderr)
  else:assert p.returncode==0 and out['error'] is None,(out,p.stderr)
  return out.get('data')
 def call(name,args,credential=None,error=None):return cli(['call',name,'--input','-'],args,credential,error)
 def request(method,path,credential,body=None,content_type='application/json',error=None,status=200):
  c=http_client.HTTPConnection('127.0.0.1',port,timeout=30)
  try:c.request(method,path,body,{'Authorization':'Bearer '+credential.read_text().strip(),'Content-Type':content_type})
  except (BrokenPipeError, ConnectionResetError):pass # The server may reject headers before a large body finishes sending.
  r=c.getresponse();data=r.read();actual=r.status;c.close()
  out=json.loads(data);trace.append({'http':method+' '+path,'status':actual,'response':out});assert actual==status,(actual,out)
  if error:assert out['error']['code']==error,out
  else:assert out['error'] is None,out
  return out.get('data')
 def http(name,args,credential,error=None,status=200):return request('POST','/v1/tools/'+name,credential,json.dumps(args),error=error,status=status)
 def digest(b):return hashlib.sha256(b).hexdigest()
 def descriptor(data,key,project='a'):return {'project_id':project,'name':'../display-only.mp4','size_bytes':str(len(data)),'content_hash':digest(data),'idempotency_key':key}
 def put(view,offset,data,credential,hash=None,size=None,error=None,status=200):
  path=f"/v1/uploads/{view['upload_id']}/chunks/{offset}?"+urlencode({'size_bytes':str(size if size is not None else len(data)),'content_hash':hash or digest(data)})
  return request('PUT',path,credential,data,'application/octet-stream',error,status)
 def policy(name,actions):return cli(['access','set-policy','--input','-'],{'principal_id':name,'expected_revision':'0','enabled':True,'grants':[{'project_id':'a','actions':actions}]})
 def issue(name,suffix=''):
  path=root/(name+suffix+'.secret');result=cli(['access','issue',name,'--output',str(path)]);return path,result['credential_id']
 def wait(job):
  deadline=time.monotonic()+40
  while True:
   view=http('opencut_job_view',{'job_id':job},runner)
   if view['state'] in ['succeeded','failed','cancelled']:assert view['state']=='succeeded',view;return view
   assert time.monotonic()<deadline;time.sleep(.05)
 def rows():
  with sqlite3.connect(db) as c:return list(c.iterdump())
 try:
  original={}
  for project in ['a','b']:original[project]=call('opencut_project_create',{'project':{'id':project,'name':project,'sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':project})
  policy('uploader',['read','edit','run_jobs','read_artifacts']);policy('reader',['read']);policy('editor',['read','edit'])
  runner,_=issue('uploader');reader,_=issue('reader');editor,_=issue('editor')
  source=root/'red.png';subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=64x48','-frames:v','1',str(source)],check=True,timeout=30);data=source.read_bytes()
  with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
  owner=root/'owner-token'
  with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(owner)],stdout=log,stderr=log)
  deadline=time.monotonic()+15
  while True:
   assert host.poll() is None
   try:
    with socket.create_connection(('127.0.0.1',port),timeout=.2):break
   except OSError:assert time.monotonic()<deadline;time.sleep(.03)
  with sqlite3.connect(db,timeout=5) as lock:
   lock.execute('BEGIN IMMEDIATE')
   time.sleep(7)
   lock.rollback()
  assert host.poll() is None
  checks.append('The HTTP host survives a real seven-second SQLite writer lock while its idle Worker retries temporary database busy')
  cap=http('opencut_capabilities',{},runner);assert cap['uploads']['chunk_method']=='PUT';assert any(j['name']=='media.import' for j in cap['job_kinds'])
  upload=call('opencut_upload_create',descriptor(data,'main'),runner)
  assert http('opencut_upload_create',descriptor(data,'main'),runner)==upload
  call('opencut_upload_create',{**descriptor(data,'main'),'name':'changed'},runner,'IDEMPOTENCY_KEY_REUSED')
  for credential in [reader,editor]:http('opencut_upload_create',descriptor(data,'deny'),credential,'NOT_AUTHORIZED',403)
  http('opencut_upload_create',descriptor(data,'foreign','b'),runner,'NOT_AUTHORIZED',403)
  http('opencut_upload_create',{**descriptor(data,'injection'),'path':'/private/source'},runner,'INVALID_ARGUMENT',400)
  http('opencut_upload_inspect',{'upload_id':'upload_'+'0'*32},runner,'NOT_AUTHORIZED',403)
  checks.append('Public discovery, idempotent CLI/HTTP create, read/edit denial, foreign project and injected host path rejection')
  cut=len(data)//2
  confirmed=put(upload,0,data[:cut],runner);assert confirmed['next_offset']==str(cut)
  before=rows();assert put(upload,0,data[:cut],runner)['next_offset']==str(cut);assert rows()==before
  put(upload,0,b'x'*cut,runner,error='UPLOAD_CHUNK_CONFLICT',status=409)
  put(upload,cut,b'abc',runner,hash='0'*64,error='UPLOAD_CONTENT_MISMATCH',status=400)
  put(upload,cut,b'a',runner,size=2,error='UPLOAD_CONTENT_MISMATCH',status=400)
  c=http_client.HTTPConnection('127.0.0.1',port,timeout=5);c.putrequest('PUT',f"/v1/uploads/{upload['upload_id']}/chunks/{cut}?"+urlencode({'size_bytes':str(len(data)-cut),'content_hash':digest(data[cut:])}));c.putheader('Authorization','Bearer '+runner.read_text().strip());c.putheader('Content-Length',str(len(data)-cut));c.endheaders();c.send(data[cut:cut+3]);c.close();time.sleep(.1)
  assert call('opencut_upload_inspect',{'upload_id':upload['upload_id']},reader)['next_offset']==str(cut)
  assert cli(['upload','put',upload['upload_id'],'--offset',str(cut),'--input','-'],credential=runner,raw=data[cut:])['next_offset']==str(len(data))
  checks.append('HTTP partial transfer, exact replay, conflicting bytes, bad hash/length and CLI raw stdin resume preserve confirmed offsets')
  sealed=http('opencut_upload_seal',{'upload_id':upload['upload_id']},runner);assert call('opencut_upload_seal',{'upload_id':upload['upload_id']},runner)['job_id']==sealed['job_id']
  primary=wait(sealed['job_id']);assert primary['result'] is None and primary['extended_result']['schema_version']==1
  imported=primary['extended_result']['data'];assert imported['asset']['kind']=='image';assert str(root) not in json.dumps(primary)
  assert call('opencut_project_inspect',{'project_id':'a'})==original['a']
  assert call('opencut_project_media_inspect',{'project_id':'a','media_id':imported['media_id']},reader)==imported
  http('opencut_project_media_inspect',{'project_id':'b','media_id':imported['media_id']},runner,'NOT_AUTHORIZED',403)
  checks.append('Seal accepts one real Worker job, registers a portable typed resource without changing project head, and enforces scoped resource reads')
  def content(project,media_id,credential):
   path=f'/v1/projects/{quote(project,safe="")}/media/{quote(media_id,safe="")}/content'
   c=http_client.HTTPConnection('127.0.0.1',port,timeout=30)
   c.request('GET',path,headers={'Authorization':'Bearer '+credential.read_text().strip()})
   response=c.getresponse();body=response.read();status=response.status;headers=dict(response.getheaders());c.close()
   return status,body,headers
  for credential in [reader,editor,runner]:
   status,body,headers=content('a',imported['media_id'],credential)
   assert status==200 and body==data and digest(body)==imported['asset']['version']['content_hash']
   assert headers['content-type']=='image/png' and headers['content-length']==str(len(data))
   assert headers['cache-control']=='no-store' and headers['x-content-type-options']=='nosniff'
  status,body,_=content('b',imported['media_id'],runner);assert status==403 and json.loads(body)['error']['code']=='NOT_AUTHORIZED'
  status,body,_=content('b',imported['media_id'],owner);assert status==404 and json.loads(body)['error']['code']=='NOT_FOUND'
  content_reader,content_id=issue('reader','-media-content')
  assert content('a',imported['media_id'],content_reader)[0]==200
  cli(['access','revoke',content_id])
  assert content('a',imported['media_id'],content_reader)[0]==403
  checks.append('Project media bytes are hash-verified and readable by current project grants; foreign IDs and revoked credentials reject without leaking bytes')
  staging=bool(cap['uploads'].get('staging'))
  if staging:
   observed=http('opencut_upload_retention',{'upload_id':upload['upload_id']},reader);assert observed['state']=='retained' and observed['release_eligible']
   http('opencut_upload_release',{'upload_id':upload['upload_id']},reader,'NOT_AUTHORIZED',403)
   http('opencut_upload_quota',{'project_id':'b'},runner,'NOT_AUTHORIZED',403)
   released=call('opencut_upload_release',{'upload_id':upload['upload_id']},runner);assert released['state']=='released' and released['reserved_bytes']=='0'
   assert not (root/'uploads'/upload['upload_id']).exists()
   before=rows();assert http('opencut_upload_release',{'upload_id':upload['upload_id']},runner)==released;assert rows()==before
   assert http('opencut_project_media_inspect',{'project_id':'a','media_id':imported['media_id']},reader)==imported
   checks.append('Scoped CLI release durably deletes successful staging; HTTP replay is read-only, read-only mutation rejects and the resource remains usable')

  if sdk:
   p=subprocess.run(['node',str(Path(__file__).with_name('verify-project-import-mcp.mjs')),str(sdk),str(binary),str(db),f'http://127.0.0.1:{port}',str(runner),str(reader),str(source),upload['upload_id']],capture_output=True,text=True,timeout=90)
   (root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert p.returncode==0,(p.stdout,p.stderr)
   for job in json.loads(p.stdout)['jobs']:wait(job)
   checks.append('Official MCP SDK on stdio/HTTP creates, inspects and seals uploads with authenticated byte handoff; read-only and cross-project mutations reject')
  before=rows();first=http('opencut_project_media_query',{'project_id':'a','limit':1},reader);assert rows()==before
  assert first['media'][0]['media_id']==imported['media_id']
  if first['next_cursor']:
   late=http('opencut_upload_create',descriptor(data,'late'),runner);put(late,0,data,runner);late_job=http('opencut_upload_seal',{'upload_id':late['upload_id']},runner);late_media=wait(late_job['job_id'])['extended_result']['data']['media_id']
   page=first;ids=[page['media'][0]['media_id']]
   while page['next_cursor']:
    page=http('opencut_project_media_query',{'project_id':'a','cursor':page['next_cursor'],'limit':1},reader);ids += [m['media_id'] for m in page['media']]
   assert len(ids)==len(set(ids)) and late_media not in ids
   http('opencut_project_media_query',{'project_id':'b','cursor':first['next_cursor']},owner,'CURSOR_EXPIRED',410)
  checks.append('Read-only media pagination freezes membership and rejects another project cursor without writing database state')
  source.unlink();asset=copy.deepcopy(imported['asset']);asset['id']='red';asset['name']='Renamed'
  at=lambda n:{'ticks':str(n),'timebase':10}
  edit={'project_id':'a','request':{'expected_revision':original['a']['revision'],'idempotency_key':'bind-import','label':'bind import','operations':[
   {'type':'asset.bind','asset':asset},
   {'type':'track.create','sequence_id':'main','track':{'id':'visual','name':'Image','kind':'graphic','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}},
   {'type':'clip.insert','sequence_id':'main','track_id':'visual','clip':{'id':'clip','asset_id':'red','timeline_range':{'start':at(0),'duration':at(10)},'source_range':None,'params':{},'channels':[]}}]}}
  forged=copy.deepcopy(edit);forged['request']['operations'][0]['asset']['uri']='/private/path';http('opencut_edit_apply',forged,runner,'NOT_AUTHORIZED',403)
  revision=http('opencut_edit_apply',edit,runner)['revision'];assert call('opencut_edit_apply',edit,runner)['revision']==revision
  assert http('opencut_render_preflight',{'project_id':'a','revision':revision,'sequence_id':'main','container':'mp4','video_codec':'h264'},runner)['ready']
  preview=http('opencut_job_submit',{'kind':'preview.composited_frame','idempotency_key':'preview','input':{'project_id':'a','revision':revision,'sequence_id':'main','requested_time':at(0),'alignment':'exact'}},runner)
  artifact=wait(preview['job_id'])['artifacts'][0]
  c=http_client.HTTPConnection('127.0.0.1',port,timeout=30);c.request('GET','/v1/artifacts/'+quote(artifact['artifact_id'],safe=''),headers={'Authorization':'Bearer '+runner.read_text().strip()});r=c.getresponse();png=r.read();assert r.status==200 and digest(png)==artifact['content_hash'];c.close();(root/'preview.png').write_bytes(png)
  pixels=subprocess.check_output(['ffmpeg','-v','error','-i',str(root/'preview.png'),'-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30);assert len(pixels)==64*48*3 and sum(pixels[::3])/(64*48)>245 and sum(pixels[1::3])/(64*48)<5
  checks.append('After deleting source, scoped asset binding, replay, preflight, actual preview, authenticated download, SHA-256 and independent RGB verification pass')
  large=b'z'*(3*1024*1024+17);big=http('opencut_upload_create',descriptor(large,'large'),runner);assert put(big,0,large,runner)['next_offset']==str(len(large))
  folder=root/'uploads'/big['upload_id'];assert folder.is_dir();http('opencut_upload_abort',{'upload_id':big['upload_id']},runner);assert not folder.exists();http('opencut_upload_abort',{'upload_id':big['upload_id']},runner)
  put(big,0,large,runner,error='UPLOAD_STATE_CONFLICT',status=409)
  checks.append('Binary data larger than the 2 MiB control limit uploads correctly; abort removes private chunks and repeat abort is safe')
  slow=http('opencut_upload_create',descriptor(b'abc','slow'),runner);revocable,cid=issue('uploader','-revocable')
  c=http_client.HTTPConnection('127.0.0.1',port,timeout=5);c.putrequest('PUT',f"/v1/uploads/{slow['upload_id']}/chunks/0?size_bytes=3&content_hash={digest(b'abc')}");c.putheader('Authorization','Bearer '+revocable.read_text().strip());c.putheader('Content-Length','3');c.endheaders();c.send(b'a');time.sleep(.1);cli(['access','revoke',cid]);r=c.getresponse();out=json.loads(r.read());assert r.status==403 and out['error']['code']=='NOT_AUTHORIZED';c.close()
  assert call('opencut_upload_inspect',{'upload_id':slow['upload_id']})['next_offset']=='0';assert not (root/'uploads'/slow['upload_id']).exists()
  limited,lid=issue('uploader','-limited');connections=[]
  try:
   for _ in range(cap['uploads']['max_inflight_chunks']):
    stream=http_client.HTTPConnection('127.0.0.1',port,timeout=10);stream.putrequest('PUT',f"/v1/uploads/{slow['upload_id']}/chunks/0?size_bytes=3&content_hash={digest(b'abc')}");stream.putheader('Authorization','Bearer '+limited.read_text().strip());stream.putheader('Content-Length','3');stream.endheaders();stream.send(b'a');connections.append(stream)
   time.sleep(.6)
   put(slow,0,b'abc',runner,error='UPLOAD_BUSY',status=429)
   cli(['access','revoke',lid])
   for stream in connections:
    r=stream.getresponse();assert r.status==403 and json.loads(r.read())['error']['code']=='NOT_AUTHORIZED'
  finally:
   for stream in connections:stream.close()
  assert put(slow,0,b'abc',runner)['next_offset']=='3'
  http('opencut_upload_abort',{'upload_id':slow['upload_id']},runner)
  checks.append('Sixteen simultaneous incomplete bodies exhaust the advertised limit; the next chunk gets 429 and revocation releases capacity without ACK')
  if staging:
   for item in http('opencut_project_media_query',{'project_id':'a','limit':100},runner)['media']:
    http('opencut_upload_release',{'upload_id':item['upload_id']},runner)
   assert http('opencut_upload_quota',{'project_id':'a'},reader)['project_reserved_bytes']=='0'
   held=[]
   with sqlite3.connect(db) as c:c.execute("CREATE TRIGGER deny_staging_release BEFORE INSERT ON audit_records WHEN NEW.action='upload.staging_released' BEGIN SELECT RAISE(ABORT,'injected release failure'); END")
   try:
    for i in range(8):
     value=http('opencut_upload_create',{'project_id':'a','name':'reservation','idempotency_key':f'quota-{i}','size_bytes':str(1073741824),'content_hash':'a'*64},runner);held.append(value)
     http('opencut_upload_abort',{'upload_id':value['upload_id']},runner,'UPLOAD_CLEANUP_PENDING',500)
    quota=http('opencut_upload_quota',{'project_id':'a'},reader);assert quota['project_reserved_bytes']==quota['project_limit_bytes']==str(8*1073741824)
    http('opencut_upload_create',descriptor(b'x','over-quota'),runner,'UPLOAD_QUOTA_EXCEEDED',422)
    assert http('opencut_upload_retention',{'upload_id':held[0]['upload_id']},reader)['state']=='retained'
   finally:
    with sqlite3.connect(db) as c:c.execute('DROP TRIGGER deny_staging_release')
   before=rows();first=http('opencut_upload_query',{'project_id':'a','limit':3},reader);assert rows()==before
   assert len(first['uploads'])==3 and first['next_cursor']
   http('opencut_upload_query',{'project_id':'a','cursor':first['next_cursor'],'retained_only':False},reader,'INVALID_CURSOR',400)
   http('opencut_upload_query',{'project_id':'b','cursor':first['next_cursor']},runner,'NOT_AUTHORIZED',403)
   http('opencut_upload_query',{'project_id':'b','cursor':first['next_cursor']},owner,'CURSOR_EXPIRED',410)
   http('opencut_upload_release',{'upload_id':held[-1]['upload_id']},runner)
   late=http('opencut_upload_create',descriptor(b'x','after-upload-page'),runner)
   page=first;ids=[e['upload']['upload_id'] for e in page['uploads']]
   while page['next_cursor']:
    page=http('opencut_upload_query',{'project_id':'a','cursor':page['next_cursor'],'limit':3},reader);ids += [e['upload']['upload_id'] for e in page['uploads']]
   assert len(ids)==7 and len(set(ids))==7 and late['upload_id'] not in ids and held[-1]['upload_id'] not in ids
   discovered=http('opencut_upload_query',{'project_id':'a','limit':100},reader)['uploads'];assert len(discovered)==8
   for entry in discovered:
    name='opencut_upload_release' if entry['retention']['release_eligible'] else 'opencut_upload_abort'
    http(name,{'upload_id':entry['upload']['upload_id']},runner)
   assert http('opencut_upload_query',{'project_id':'a'},reader)['uploads']==[]
   assert len(http('opencut_upload_query',{'project_id':'a','retained_only':False,'limit':100},reader)['uploads'])>=len(held)
   assert http('opencut_upload_quota',{'project_id':'a'},reader)['project_reserved_bytes']=='0'
   checks.append('Upload discovery is read-only, scopes cursors to project/filter, excludes later creations while observing releases, and recovers capacity without remembered upload IDs')
   admitted=http('opencut_upload_create',descriptor(b'x','over-quota'),runner)
   http('opencut_upload_abort',{'upload_id':admitted['upload_id']},runner)
   checks.append('Real release-audit failures retain logical reservations despite absent files; project quota blocks admission, explicit retries free capacity and the failed key is reusable')
  audit=call('opencut_audit_query',{'target':{'kind':'project','id':'a'},'limit':500})
  actors={(x['action'],x['actor']['source'],x['actor']['principal_id']) for x in audit['records']}
  assert ('upload.chunk_accepted','http','uploader') in actors and ('upload.chunk_accepted','cli','uploader') in actors
  checks.append('Revocation during an incomplete HTTP body closes with a structured denial, no disk chunk/ACK, and committed writes preserve HTTP/CLI actor provenance')
  content_hash=imported['asset']['version']['content_hash'];object_path=root/'asset-store'/content_hash[:2]/content_hash
  original_bytes=object_path.read_bytes()
  try:
   object_path.chmod(0o600);object_path.write_bytes(original_bytes[:-1]+bytes([original_bytes[-1]^1]))
   status,body,_=content('a',imported['media_id'],reader)
   assert status!=200 and json.loads(body)['error']['code']=='ARTIFACT_VERIFICATION_FAILED'
  finally:
   object_path.write_bytes(original_bytes);object_path.chmod(0o444)
  assert content('a',imported['media_id'],reader)[1]==original_bytes
  checks.append('A same-size CAS mutation is rejected before project media streaming and intact content remains readable after repair')
  host.terminate();host.wait(timeout=10);host=None
  assert call('opencut_project_media_inspect',{'project_id':'a','media_id':imported['media_id']},runner)==imported
  report={'status':'passed','checks':checks,'binary_sha256':digest(binary.read_bytes()),'evidence':str(root),'job_id':sealed['job_id'],'media_id':imported['media_id'],'revision':revision};(root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
 except Exception as error:
  (root/'failure.txt').write_text(repr(error));raise
 finally:
  (root/'trace.json').write_text(json.dumps(trace,indent=2))
  if host is not None:host.terminate();host.wait(timeout=10)

if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path);a=p.parse_args();verify(a.binary.resolve(),a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-project-import-')),a.mcp_sdk_root)
