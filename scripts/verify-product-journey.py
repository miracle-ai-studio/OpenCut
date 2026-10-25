#!/usr/bin/env python3
"""Public product journey; synthetic media is not real interview or browser acceptance."""
import argparse,copy,json,subprocess,socket,time,urllib.request,urllib.error,hashlib,signal,os,sqlite3
from http.client import HTTPConnection
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from array import array
parser=argparse.ArgumentParser(description="Real HTTP/CLI product journey and R1 integrity acceptance")
parser.add_argument('--binary',type=Path,default=Path(__file__).resolve().parents[1]/'target/debug/opencut')
parser.add_argument('--artifacts-dir',type=Path,required=True)
args=parser.parse_args()
ROOT=args.artifacts_dir.resolve(); ROOT.mkdir(parents=True,exist_ok=True)
run=ROOT/('journey-'+str(time.time_ns())); run.mkdir()
BIN=args.binary.resolve(); db=run/'workspace.sqlite'; trace=[]; checks=[]
def t(n,b=10): return {'ticks':str(n),'timebase':b}
def span(s,d): return {'start':t(s),'duration':t(d)}
def cmd(args): return subprocess.check_output(args,stderr=subprocess.PIPE,timeout=90)
source=run/'source.mp4'
cmd(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=160x90:r=10:d=2','-f','lavfi','-i','color=blue:s=160x90:r=10:d=2','-f','lavfi','-i','sine=frequency=600:sample_rate=48000:duration=4','-filter_complex','[0:v][1:v]concat=n=2:v=1:a=0[v]','-map','[v]','-map','2:a','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac',str(source)])
with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
base=f'http://127.0.0.1:{port}'; logfile=(run/'service.log').open('wb'); process=None; token=None

def http(path,body=None,raw=False):
 headers={'Authorization':f'Bearer {token}'}
 if body is not None and not isinstance(body,bytes):body=json.dumps(body).encode();headers['Content-Type']='application/json'
 try:
  with urllib.request.urlopen(urllib.request.Request(base+path,data=body,headers=headers),timeout=25) as r:data=r.read(); status=r.status
 except urllib.error.HTTPError as e:data=e.read();status=e.code
 if raw:return data
 value=json.loads(data); trace.append({'path':path,'status':status,'response':value});return value

def upload_chunked(path,payload):
 connection=HTTPConnection('127.0.0.1',port,timeout=60)
 connection.putrequest('POST',path)
 connection.putheader('Authorization',f'Bearer {token}')
 connection.putheader('Content-Type','application/octet-stream')
 connection.putheader('Transfer-Encoding','chunked')
 connection.endheaders()
 for offset in range(0,len(payload),65536):
  block=payload[offset:offset+65536]
  connection.send(f'{len(block):X}\r\n'.encode()+block+b'\r\n')
 connection.send(b'0\r\n\r\n')
 response=connection.getresponse();data=response.read();connection.close()
 value=json.loads(data);trace.append({'path':path,'status':response.status,'response':value})
 assert response.status==200 and value['error'] is None,(response.status,value)
 return value['data']

def call(path,body=None):
 deadline=time.monotonic()+20
 while True:
  v=http(path,body)
  if v.get('error') is None:return v['data']
  assert v['error']['code']=='DATABASE_BUSY' and time.monotonic()<deadline,v
  time.sleep(.1)
def start():
 global process,token
 process=subprocess.Popen([str(BIN),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(run/'token')],stdout=logfile,stderr=logfile)
 deadline=time.monotonic()+15
 while True:
  try:
   token=(run/'token').read_text().strip();return call('/api/capabilities')
  except (FileNotFoundError,urllib.error.URLError):
   assert time.monotonic()<deadline and process.poll() is None;time.sleep(.05)
def stop():
 if process and process.poll() is None:process.send_signal(signal.SIGINT);process.wait(timeout=15)
def snapshot():return call('/api/projects/journey/snapshot')
def edit(rev,key,ops):return {'expected_revision':rev,'idempotency_key':key,'label':key,'operations':ops}
def apply(body):return call('/api/projects/journey/branches/main/transactions',body)
def wait(job):
 end=time.monotonic()+90
 while job['state'] not in ('succeeded','failed','cancelled'):
  assert time.monotonic()<end;time.sleep(.1);job=call('/api/jobs/'+job['job_id'])
 assert job['state']=='succeeded',job;return job

def artifact(job,name):
 a=job['result']['artifact']; data=http('/api/jobs/'+job['job_id']+'/artifact',raw=True)
 assert hashlib.sha256(data).hexdigest()==a['content_hash'];p=run/name;p.write_bytes(data);return p

def rgb(p,at):return list(cmd(['ffmpeg','-v','error','-ss',str(at),'-i',str(p),'-frames:v','1','-vf','crop=2:2:0:0,scale=1:1','-pix_fmt','rgb24','-f','rawvideo','-']))
def duration(p):return float(cmd(['ffprobe','-v','error','-show_entries','format=duration','-of','default=nw=1:nk=1',str(p)]))
try:
 caps=start(); kinds={x['name'] for x in caps['job_kinds']}
 assert {'preview.project_range','render.project'}<=kinds
 created=call('/api/projects',{'project':{'id':'journey','name':'QA story','sequence_id':'main','width':160,'height':90,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
 upload=call('/api/media/upload?name=source.mp4',source.read_bytes())
 track={'id':'vtrack','name':'Video','kind':'video','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}
 clip={'id':'left','asset_id':upload['asset']['id'],'timeline_range':span(0,40),'source_range':span(0,40),'params':{},'channels':[]}
 bind=edit(created['revision'],'bind',[{'type':'asset.bind','asset':upload['asset']},{'type':'track.create','sequence_id':'main','track':track},{'type':'clip.insert','sequence_id':'main','track_id':'vtrack','clip':clip}])
 plan=call('/api/projects/journey/branches/main/edit-plans',bind);assert snapshot()['revision']==created['revision']
 bound=apply(bind);assert apply(bind)==bound
 checks.append('HTTP import and read-only plan; atomic apply and exact retry return same transaction')
 split=apply(edit(bound['revision'],'split',[{'type':'clip.split','sequence_id':'main','track_id':'vtrack','clip_id':'left','at':t(20),'right_clip_id':'right'}]))
 cue={'id':'cue','timeline_range':{'start':t(1,2),'duration':t(1,2)},'text':'QA STORY','source_anchor':None,'style':{'font_family':'Arial','font_size':14,'color':'#ffffff','background_color':None}}
 human=apply(edit(split['revision'],'human-caption',[{'type':'caption.insert','sequence_id':'main','cue':cue}]))
 retry=edit(split['revision'],'human-caption',[{'type':'caption.insert','sequence_id':'main','cue':copy.deepcopy(cue)}])
 retry['operations'][0]['cue']['timeline_range']={'start':t(2,4),'duration':t(2,4)}
 result=subprocess.run([str(BIN),'--database',str(db),'--json','call','opencut_edit_apply','--input','-'],input=json.dumps({'project_id':'journey','request':retry}).encode(),capture_output=True,timeout=15)
 assert result.returncode==0,(result.stdout,result.stderr)
 assert json.loads(result.stdout)['data']==human
 retry['operations'][0]['cue']['timeline_range']['duration']=t(3,4)
 rejected=http('/api/tools/opencut_edit_apply',{'project_id':'journey','request':retry})
 assert rejected['error']['code']=='IDEMPOTENCY_KEY_REUSED',rejected
 checks.append('HTTP write and separate CLI equivalent-time retry share transaction; changed duration rejects')
 stale=http('/api/projects/journey/branches/main/transactions',edit(split['revision'],'stale-agent',[{'type':'clip.delete','sequence_id':'main','track_id':'vtrack','clip_id':'right'}]))
 assert stale['error']['code']=='REVISION_CONFLICT';assert snapshot()['project']['sequences'][0]['caption_cues'][0]['text']=='QA STORY'
 checks.append('Split followed by independent human caption; stale Agent edit rejected without losing caption')
 preflight=call('/api/render/preflight',{'project_id':'journey','revision':human['revision'],'sequence_id':'main','container':'mp4','video_codec':'h264'});assert preflight['ready'],preflight
 preview=wait(call('/api/jobs',{'kind':'preview.project_range','idempotency_key':'preview','input':{'project_id':'journey','revision':human['revision'],'sequence_id':'main','requested_range':span(10,20),'alignment':'exact'}}))
 preview_path=artifact(preview,'preview.mp4');r=rgb(preview_path,.2);b=rgb(preview_path,1.5)
 assert r[0]>200 and r[2]<20 and b[2]>200 and b[0]<20,(r,b)
 assert abs(duration(preview_path)-2)<.05
 samples=array('h');samples.frombytes(cmd(['ffmpeg','-v','error','-i',str(preview_path),'-vn','-ac','1','-ar','48000','-f','s16le','-']))
 rms=(sum(v*v for v in samples)/len(samples))**.5;assert rms>100
 checks.append('Nonzero range preview independently decoded red then blue, two seconds and audible PCM')
 corrected=apply(edit(human['revision'],'correct-opacity',[{'type':'parameter.set_clip','sequence_id':'main','track_id':'vtrack','clip_id':'right','params':{'opacity':.5}}]))
 current=apply(edit(corrected['revision'],'later-delete',[{'type':'clip.delete','sequence_id':'main','track_id':'vtrack','clip_id':'right'}]))
 exported=wait(call('/api/jobs',{'kind':'render.project','idempotency_key':'historical-export','input':{'project_id':'journey','revision':corrected['revision'],'sequence_id':'main','container':'mp4','video_codec':'h264'}}))
 output=artifact(exported,'export.mp4');blue=rgb(output,3.0)
 assert 105<blue[2]<150 and blue[0]<20 and abs(duration(output)-4)<.05,(blue,duration(output))
 probe=json.loads(cmd(['ffprobe','-v','error','-show_streams','-of','json',str(output)]));assert any(s['codec_type']=='audio' for s in probe['streams'])
 checks.append('Correct opacity then edit head; old fixed revision export preserves four seconds and half-intensity blue')
 stop();start();reopened=snapshot();assert reopened['revision']==current['revision'];assert len(reopened['project']['sequences'][0]['tracks'][0]['clips'])==1
 independent=json.loads(cmd([str(BIN),'--database',str(db),'--json','project','inspect','journey']));assert independent['data']==reopened
 checks.append('Real service restart and separate CLI reopen preserve latest head and clip count')
 restored=call('/api/projects/journey/branches/main/restore',{'expected_revision':current['revision'],'target_revision':corrected['revision'],'idempotency_key':'restore','label':'Restore full project'})
 assert restored['revision']!=corrected['revision'];assert len(snapshot()['project']['sequences'][0]['tracks'][0]['clips'])==2
 checks.append('Whole-project restore creates a new revision and restores two editable clips')
 renamed=call('/api/media/upload?name=same-video.png',source.read_bytes())
 assert renamed['asset']['id']==upload['asset']['id'] and renamed['asset']['kind']=='video'
 assert renamed['asset']['metadata']['mime_type']=='video/mp4'
 assert renamed['asset']['name']=='same-video.png'
 def content_type(path):
  with urllib.request.urlopen(urllib.request.Request(base+path,headers={'Authorization':f'Bearer {token}'}),timeout=15) as response:
   assert response.read()==source.read_bytes()
   return response.headers['Content-Type']
 assert content_type(upload['media_url'])=='video/mp4'
 mp4_probe=wait(call('/api/jobs',{'kind':'media.probe','idempotency_key':'probe-mp4-brand','input':{'input':str(source)}}))
 assert mp4_probe['kind']=='media.probe' and mp4_probe['result']['format_brand']=='isom',mp4_probe
 mov=run/'quicktime.mov'
 cmd(['ffmpeg','-v','error','-f','lavfi','-i','color=green:s=160x90:r=10:d=0.5','-an','-c:v','libx264','-f','mov',str(mov)])
 quicktime=call('/api/media/upload?name=quicktime.mp4',mov.read_bytes())
 assert quicktime['asset']['kind']=='video' and quicktime['asset']['metadata']['mime_type']=='video/quicktime',quicktime
 mov_probe=wait(call('/api/jobs',{'kind':'media.probe','idempotency_key':'probe-quicktime-brand','input':{'input':str(mov)}}))
 assert mov_probe['kind']=='media.probe' and mov_probe['result']['format_brand']=='qt',mov_probe
 checks.append('MP4 and QuickTime MOV share FFprobe aliases; major brand keeps MIME distinct through upload and public probe Job despite opposite filename extension')
 with ThreadPoolExecutor(max_workers=4) as executor:
  duplicates=list(executor.map(lambda i:call('/api/media/upload?name=duplicate-'+str(i)+'.png',source.read_bytes()),range(8)))
 assert all(item['asset']['id']==upload['asset']['id'] and item['asset']['kind']=='video' for item in duplicates)
 checks.append('Renamed and concurrent duplicate uploads preserve video identity and original MIME')
 chunked_source=run/'chunked.wav'
 cmd(['ffmpeg','-v','error','-f','lavfi','-i','sine=frequency=440:sample_rate=48000:duration=60','-ac','1','-c:a','pcm_s16le',str(chunked_source)])
 chunked_bytes=chunked_source.read_bytes();assert len(chunked_bytes)>4*1024*1024
 chunked=upload_chunked('/api/media/upload?name=chunked.wav',chunked_bytes)['asset']
 chunked_download=http(chunked['metadata']['media_url'],raw=True)
 assert chunked['kind']=='audio' and chunked['metadata']['mime_type']=='audio/wav'
 assert chunked['metadata']['content_hash']==hashlib.sha256(chunked_bytes).hexdigest()
 assert chunked_download==chunked_bytes
 checks.append('Chunked HTTP upload without Content-Length crosses the 4 MiB write batch and downloads identical verified bytes')
 png=run/'image.png'
 cmd(['ffmpeg','-v','error','-i',str(source),'-frames:v','1',str(png)])
 image=call('/api/media/upload?name=picture.mp4',png.read_bytes())
 assert image['asset']['kind']=='image' and image['asset']['metadata']['mime_type']=='image/png',image
 avi=run/'mjpeg.avi'
 cmd(['ffmpeg','-v','error','-i',str(source),'-t','0.2','-an','-c:v','mjpeg',str(avi)])
 movie=call('/api/media/upload?name=picture.jpg',avi.read_bytes())
 assert movie['asset']['kind']=='video' and movie['asset']['metadata']['mime_type']=='video/x-msvideo',movie
 bad=http('/api/media/upload?name=invalid.mp4',b'not a movie')
 assert bad['error'] is not None,bad
 truncated=run/'truncated.mp4'
 truncated.write_bytes(source.read_bytes()[:len(source.read_bytes())//2])
 partial=http('/api/media/upload?name=truncated.mp4',truncated.read_bytes())
 assert partial['error'] is not None and partial['error'].get('code'),partial
 checks.append('PNG content stays image despite movie extension; MJPEG movie stays video; invalid and truncated containers return structured rejection')
 a=exported['result']['artifact']; artifact_path=Path(a['path']); original=artifact_path.read_bytes()
 try:
  for corrupted in (b'corrupt',bytes([original[0]^1])+original[1:]):
   artifact_path.write_bytes(corrupted)
   failure=http('/api/jobs/'+exported['job_id']+'/artifact')
   assert failure['error']['code']=='ARTIFACT_VERIFICATION_FAILED',failure
   unchanged=call('/api/jobs/'+exported['job_id'])
   assert unchanged['state']=='succeeded' and unchanged['result']['artifact']==a
 finally:
  artifact_path.write_bytes(original)
 backup=artifact_path.with_name(artifact_path.name+'.saved')
 artifact_path.rename(backup)
 artifact_path.symlink_to(backup.name)
 try:
  failure=http('/api/jobs/'+exported['job_id']+'/artifact')
  assert failure['error']['code']=='ARTIFACT_VERIFICATION_FAILED',failure
 finally:
  artifact_path.unlink();backup.rename(artifact_path)
 linked=run/'linked-artifact.mp4'
 os.link(artifact_path,linked)
 try:
  failure=http('/api/jobs/'+exported['job_id']+'/artifact')
  assert failure['error']['code']=='ARTIFACT_VERIFICATION_FAILED',failure
 finally:
  linked.unlink()
 assert http('/api/jobs/'+exported['job_id']+'/artifact',raw=True)==original
 checks.append('Truncated, same-size corrupted, symlink and hardlinked artifacts rejected; original successful record preserved; restored file downloads')
 artifact_id='artifact:'+exported['job_id']+':'+a['content_hash']
 preview_artifact=preview['result']['artifact']
 preview_id='artifact:'+preview['job_id']+':'+preview_artifact['content_hash']
 retained=call('/api/tools/opencut_artifact_retention_inspect',{'artifact_id':artifact_id})
 assert retained['state']=='retained' and retained['artifact_id']==artifact_id and retained['size_bytes']==str(a['size']),retained
 references=call('/api/tools/opencut_artifact_references_inspect',{'artifact_id':artifact_id})
 assert references['default_retained'] and references['root_count']=='0' and references['artifact_id']==artifact_id,references
 wrong_id='artifact:'+exported['job_id']+':'+'0'*64
 assert http('/api/tools/opencut_artifact_retention_inspect',{'artifact_id':wrong_id})['error']['code']=='NOT_FOUND'
 with sqlite3.connect(db,timeout=30) as connection:
  connection.execute("CREATE TRIGGER fail_artifact_release BEFORE INSERT ON audit_records WHEN NEW.action='artifact.released' BEGIN SELECT RAISE(ABORT,'forced audit failure'); END")
 rolled_back=http('/api/tools/opencut_artifact_retention_release',{'artifact_id':artifact_id,'idempotency_key':'release-export'})
 assert rolled_back['error'] and call('/api/tools/opencut_artifact_retention_inspect',{'artifact_id':artifact_id})['state']=='retained',rolled_back
 with sqlite3.connect(db,timeout=30) as connection:
  assert connection.execute('SELECT count(*) FROM artifact_retention').fetchone()[0]==0
  connection.execute('DROP TRIGGER fail_artifact_release')
 released=call('/api/tools/opencut_artifact_retention_release',{'artifact_id':artifact_id,'idempotency_key':'release-export'})
 assert released['state']=='released' and released['release_request_id'] and released['released_at'],released
 replay=call('/api/tools/opencut_artifact_retention_release',{'artifact_id':artifact_id,'idempotency_key':'release-export'})
 assert replay==released
 changed=http('/api/tools/opencut_artifact_retention_release',{'artifact_id':preview_id,'idempotency_key':'release-export'})
 assert changed['error']['code']=='IDEMPOTENCY_KEY_REUSED',changed
 duplicate=http('/api/tools/opencut_artifact_retention_release',{'artifact_id':artifact_id,'idempotency_key':'different-release-key'})
 assert duplicate['error']['code']=='ARTIFACT_ALREADY_RELEASED',duplicate
 assert call('/api/tools/opencut_artifact_retention_inspect',{'artifact_id':preview_id})['state']=='retained'
 cli_retention=subprocess.run([str(BIN),'--database',str(db),'--json','call','opencut_artifact_retention_inspect','--input','-'],input=json.dumps({'artifact_id':artifact_id}).encode(),capture_output=True,timeout=15)
 assert cli_retention.returncode==0,(cli_retention.stdout,cli_retention.stderr)
 assert json.loads(cli_retention.stdout)['data']==released
 with sqlite3.connect(db,timeout=30) as connection:
  assert connection.execute("SELECT count(*) FROM audit_records WHERE action='artifact.released' AND job_id=?",(exported['job_id'],)).fetchone()[0]==1
 assert http('/api/jobs/'+exported['job_id']+'/artifact',raw=True)==original
 stop();start()
 assert call('/api/tools/opencut_artifact_retention_inspect',{'artifact_id':artifact_id})==released
 assert http('/api/jobs/'+exported['job_id']+'/artifact',raw=True)==original
 checks.append('Owner release is durable and exactly replayable across HTTP, CLI and restart; audit commits once, other artifact stays retained and release does not delete bytes')
 consumer=call('/api/projects',{'project':{'id':'artifact-consumer','name':'Artifact consumer','sequence_id':'main','width':160,'height':90,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'artifact-consumer-create'})
 dependent={'id':'rendered','name':'Rendered source','kind':'video','uri':str(artifact_path),'duration':t(40),'metadata':{}}
 bound=call('/api/projects/artifact-consumer/branches/main/transactions',edit(consumer['revision'],'bind-artifact-source',[{'type':'asset.bind','asset':dependent}]))
 queued=call('/api/jobs',{'kind':'media.probe','idempotency_key':'consume-released-artifact','input':{'input':str(artifact_path)}})
 assert queued['state']=='queued',queued
 references=call('/api/tools/opencut_artifact_references_inspect',{'artifact_id':artifact_id})
 assert not references['default_retained'] and int(references['root_count'])>=2,references
 roots={(root['kind'],root['resource_id']) for root in references['roots']}
 assert ('revision',bound['revision']) in roots and ('job_input',queued['job_id']) in roots,references
 assert http('/api/jobs/'+exported['job_id']+'/artifact',raw=True)==original
 checks.append('After release, a real new project revision and queued Job referencing the published bytes appear as distinct protection roots; the artifact remains downloadable')
 alias_path=run/'artifact alias.mp4'
 os.symlink(artifact_path,alias_path)
 alias_job=call('/api/jobs',{'kind':'media.probe','idempotency_key':'consume-artifact-alias','input':{'input':alias_path.as_uri()}})
 assert alias_job['state']=='queued',alias_job
 with sqlite3.connect(db,timeout=30) as connection:
  connection.execute("CREATE TRIGGER fail_artifact_plan BEFORE INSERT ON audit_records WHEN NEW.action='artifact.gc.planned' BEGIN SELECT RAISE(ABORT,'forced plan audit failure'); END")
 connection.close()
 failed_plan=http('/api/tools/opencut_artifact_gc_plan',{'artifact_id':artifact_id,'idempotency_key':'review-artifact'})
 assert failed_plan['error'] is not None,failed_plan
 with sqlite3.connect(db,timeout=30) as connection:
  assert connection.execute('SELECT count(*) FROM artifact_gc_plans').fetchone()[0]==0
  connection.execute('DROP TRIGGER fail_artifact_plan')
 connection.close()
 plan_input={'artifact_id':artifact_id,'idempotency_key':'review-artifact'}
 for attempt in range(10):
  planned_response=http('/api/tools/opencut_artifact_gc_plan',plan_input)
  if planned_response['error'] is None:
   planned=planned_response['data'];break
  assert planned_response['error']['code']=='DATABASE_BUSY',planned_response
  time.sleep(.1)
 else:raise AssertionError('artifact plan stayed busy after exact-key retries')
 assert planned['artifact_id']==artifact_id and not planned['default_retained'] and planned['execution_available'],planned
 assert len(planned['plan_hash'])==64 and planned['size_bytes']==str(a['size']),planned
 plan_roots={(root['kind'],root['resource_id']) for root in planned['roots']}
 assert ('revision',bound['revision']) in plan_roots and ('job_input',queued['job_id']) in plan_roots and ('job_input',alias_job['job_id']) in plan_roots,planned
 assert int(planned['native_alias_root_count'])>=1 and str(artifact_path) not in json.dumps(planned),planned
 assert call('/api/tools/opencut_artifact_gc_plan',plan_input)==planned
 assert http('/api/tools/opencut_artifact_gc_plan',{'artifact_id':preview_id,'idempotency_key':'review-artifact'})['error']['code']=='IDEMPOTENCY_KEY_REUSED'
 retained_plan=call('/api/tools/opencut_artifact_gc_plan',{'artifact_id':preview_id,'idempotency_key':'review-preview'})
 assert retained_plan['default_retained'] and retained_plan['execution_available'],retained_plan
 for protected in (planned,retained_plan):
  refused=http('/api/tools/opencut_artifact_gc_execute',{'plan_id':protected['plan_id'],'expected_plan_hash':protected['plan_hash']})
  assert refused['error']['code']=='ARTIFACT_PLAN_PROTECTED',refused
 first_page=call('/api/tools/opencut_artifact_gc_query',{'limit':1})
 assert first_page['plans'][0]['plan_id']==planned['plan_id'] and first_page['next_cursor'],first_page
 second_page=call('/api/tools/opencut_artifact_gc_query',{'limit':1,'cursor':first_page['next_cursor']})
 assert second_page['plans'][0]['plan_id']==retained_plan['plan_id'] and second_page['next_cursor'] is None,second_page
 assert http('/api/tools/opencut_artifact_gc_query',{'limit':1,'cursor':'agc1:missing'})['error']['code']=='INVALID_CURSOR'
 stop();start()
 assert call('/api/tools/opencut_artifact_gc_inspect',{'plan_id':planned['plan_id']})==planned
 assert call('/api/tools/opencut_artifact_gc_plan',plan_input)==planned
 assert call('/api/tools/opencut_artifact_gc_query',{'limit':1})==first_page
 assert http('/api/jobs/'+exported['job_id']+'/artifact',raw=True)==original
 checks.append('Durable artifact review plan captures file identity privately and percent-encoded file-URI symlink alias; audit rollback, exact replay, bounded discovery and restart preserve plans without deleting bytes')
 preview_release=call('/api/tools/opencut_artifact_retention_release',{'artifact_id':preview_id,'idempotency_key':'release-preview-for-collection'})
 assert preview_release['state']=='released',preview_release
 recover_plan=call('/api/tools/opencut_artifact_gc_plan',{'artifact_id':preview_id,'idempotency_key':'collect-preview-after-release'})
 assert recover_plan['root_count']=='0' and not recover_plan['default_retained'] and recover_plan['execution_available'],recover_plan
 recovery_input={'plan_id':recover_plan['plan_id'],'expected_plan_hash':recover_plan['plan_hash']}
 assert http('/api/tools/opencut_artifact_gc_execute',{'plan_id':recover_plan['plan_id'],'expected_plan_hash':'0'*64})['error']['code']=='PLAN_HASH_MISMATCH'
 recovery_path=Path(preview_artifact['path'])
 assert recovery_path.is_file() and http('/api/jobs/'+preview['job_id']+'/artifact',raw=True)==preview_path.read_bytes()
 with sqlite3.connect(db,timeout=30) as connection:
  connection.execute("CREATE TRIGGER fail_gc_mark BEFORE INSERT ON audit_records WHEN NEW.action='artifact.gc.removing' BEGIN SELECT RAISE(ABORT,'forced removal intent audit failure'); END")
 refused=http('/api/tools/opencut_artifact_gc_execute',recovery_input)
 assert refused['error'] and recovery_path.is_file(),refused
 with sqlite3.connect(db,timeout=30) as connection:
  assert connection.execute('SELECT count(*) FROM artifact_gc_removals WHERE plan_id=?',(recover_plan['plan_id'],)).fetchone()[0]==0
  connection.execute('DROP TRIGGER fail_gc_mark')
 checks.append('Wrong plan hash and failed removal-intent audit leave reviewed bytes and durable state unchanged')
 with sqlite3.connect(db,timeout=30) as connection:
  connection.execute("CREATE TRIGGER fail_gc_result BEFORE INSERT ON audit_records WHEN NEW.action='artifact.gc.removed' BEGIN SELECT RAISE(ABORT,'forced removal result audit failure'); END")
 failed_result=http('/api/tools/opencut_artifact_gc_execute',recovery_input)
 assert failed_result['error'] and not recovery_path.exists(),failed_result
 pending=call('/api/tools/opencut_artifact_gc_execution_inspect',{'plan_id':recover_plan['plan_id']})
 assert pending['state']=='removing' and pending['durable'],pending
 blocked_project={'project':{'id':'gc-fenced','name':'GC fenced','sequence_id':'main','width':160,'height':90,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'gc-fenced-create'}
 blocked=http('/api/projects',blocked_project)
 assert blocked['error']['code']=='ARTIFACT_COLLECTION_RECOVERY_REQUIRED',blocked
 with sqlite3.connect(db,timeout=30) as connection:
  assert connection.execute("SELECT count(*) FROM projects WHERE project_id='gc-fenced'").fetchone()[0]==0
  connection.execute('DROP TRIGGER fail_gc_result')
 stop();start()
 recovered=call('/api/tools/opencut_artifact_gc_execute',recovery_input)
 assert recovered['state']=='absent' and recovered['durable'],recovered
 assert call('/api/tools/opencut_artifact_gc_execution_inspect',{'plan_id':recover_plan['plan_id']})==recovered
 assert call('/api/tools/opencut_artifact_gc_execute',recovery_input)==recovered
 assert call('/api/projects',blocked_project)['project']['id']=='gc-fenced'
 with sqlite3.connect(db,timeout=30) as connection:
  assert connection.execute("SELECT count(*) FROM audit_records WHERE action='artifact.gc.removing'").fetchone()[0]==1
  assert connection.execute("SELECT count(*) FROM audit_records WHERE action='artifact.gc.absent'").fetchone()[0]==1
  assert connection.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
 checks.append('Failed final audit after durable unlink leaves removing fence; restart and exact retry record absent once, then blocked project publish resumes')
 second=wait(call('/api/jobs',{'kind':'preview.project_range','idempotency_key':'preview-for-direct-collection','input':{'project_id':'journey','revision':human['revision'],'sequence_id':'main','requested_range':span(10,20),'alignment':'exact'}}))
 second_artifact=second['result']['artifact'];second_path=Path(second_artifact['path'])
 second_id='artifact:'+second['job_id']+':'+second_artifact['content_hash']
 assert second_path.is_file()
 call('/api/tools/opencut_artifact_retention_release',{'artifact_id':second_id,'idempotency_key':'release-second-preview'})
 direct_plan=call('/api/tools/opencut_artifact_gc_plan',{'artifact_id':second_id,'idempotency_key':'collect-second-preview'})
 assert direct_plan['root_count']=='0' and not direct_plan['default_retained'],direct_plan
 direct_input={'plan_id':direct_plan['plan_id'],'expected_plan_hash':direct_plan['plan_hash']}
 direct_cli=subprocess.run([str(BIN),'--database',str(db),'--json','call','opencut_artifact_gc_execute','--input','-'],input=json.dumps(direct_input).encode(),capture_output=True,timeout=30)
 assert direct_cli.returncode==0,(direct_cli.stdout,direct_cli.stderr)
 removed=json.loads(direct_cli.stdout)['data']
 assert removed['state']=='removed' and removed['durable'] and not second_path.exists(),removed
 assert call('/api/tools/opencut_artifact_gc_execute',direct_input)==removed
 assert call('/api/tools/opencut_artifact_gc_execution_inspect',{'plan_id':direct_plan['plan_id']})==removed
 assert call('/api/jobs/'+second['job_id'])['state']=='succeeded'
 assert http('/api/jobs/'+second['job_id']+'/artifact')['error']['code']=='ARTIFACT_VERIFICATION_FAILED'
 stop();start()
 assert call('/api/tools/opencut_artifact_gc_execute',direct_input)==removed
 checks.append('Unprotected published preview is unlinked through CLI under exact reviewed hash, terminal receipt replays through HTTP across restart, and historical Job success remains')
 legacy_id='agc_'+'1'*32
 with sqlite3.connect(db,timeout=30) as connection:
  stored=json.loads(connection.execute('SELECT plan_json FROM artifact_gc_plans WHERE plan_id=?',(direct_plan['plan_id'],)).fetchone()[0])
  stored['view']['plan_id']=legacy_id
  stored['view']['execution_available']=False
  stored['view']['plan_hash']=''
  stored['observation'].pop('directory_identity')
  digest=json.dumps([stored['view'],stored['observation'],stored['all_roots'],stored['alias_roots']],ensure_ascii=False,separators=(',',':')).encode()
  stored['view']['plan_hash']=hashlib.sha256(digest).hexdigest()
  connection.execute('INSERT INTO artifact_gc_plans(plan_id,idempotency_key,input_json,plan_json) VALUES (?,?,?,?)',(legacy_id,'legacy-review-plan',json.dumps({'artifact_id':second_id,'idempotency_key':'legacy-review-plan'},separators=(',',':')),json.dumps(stored,ensure_ascii=False,separators=(',',':'))))
 legacy=call('/api/tools/opencut_artifact_gc_inspect',{'plan_id':legacy_id})
 assert not legacy['execution_available'] and legacy['plan_hash']==stored['view']['plan_hash'],legacy
 assert http('/api/tools/opencut_artifact_gc_execute',{'plan_id':legacy_id,'expected_plan_hash':legacy['plan_hash']})['error']['code']=='CAPABILITY_UNAVAILABLE'
 checks.append('Legacy schema-18 review plan remains readable but cannot authorize deletion without a private directory identity')
 findings={'renamed_video':{'same_asset_id':renamed['asset']['id']==upload['asset']['id'],'original_kind':upload['asset']['kind'],'renamed_kind':renamed['asset']['kind'],'renamed_mime':renamed['asset']['metadata'].get('mime_type')},'missing_journey_jobs':sorted({'analysis.transcribe','analysis.shots','project.package'}-kinds)}
 report={'status':'passed_with_remaining_scope','scope':'real HTTP/CLI workflow with synthetic audiovisual media; no browser or interview understanding validation','checks':checks,'findings':findings,'evidence':{'preview_rgb':[r,b],'export_blue':blue,'export_duration':duration(output),'preview_pcm_rms':rms,'job_id':exported['job_id']},'directory':str(run),'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()}
 (run/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
except Exception as e:
 (run/'report.json').write_text(json.dumps({'status':'failed','error':repr(e),'directory':str(run),'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()},indent=2));raise
finally:
 stop();logfile.close();(run/'trace.json').write_text(json.dumps(trace,indent=2));(ROOT/'latest-journey.txt').write_text(str(run))
