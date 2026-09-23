#!/usr/bin/env python3
"""Public product journey; synthetic media is not real interview or browser acceptance."""
import argparse,copy,json,subprocess,socket,time,urllib.request,urllib.error,hashlib,signal
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

def call(path,body=None):
 v=http(path,body);assert v.get('error') is None,v;return v['data']
def start():
 global process,token
 process=subprocess.Popen([str(BIN),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(run/'token')],stdout=logfile,stderr=logfile)
 deadline=time.monotonic()+15
 while True:
  try:
   token=(run/'token').read_text().strip();return call('/v1/capabilities')
  except (FileNotFoundError,urllib.error.URLError):
   assert time.monotonic()<deadline and process.poll() is None;time.sleep(.05)
def stop():
 if process and process.poll() is None:process.send_signal(signal.SIGINT);process.wait(timeout=15)
def snapshot():return call('/v1/projects/journey/snapshot')
def edit(rev,key,ops):return {'expected_revision':rev,'idempotency_key':key,'label':key,'operations':ops}
def apply(body):return call('/v1/projects/journey/branches/main/transactions',body)
def wait(job):
 end=time.monotonic()+90
 while job['state'] not in ('succeeded','failed','cancelled'):
  assert time.monotonic()<end;time.sleep(.1);job=call('/v1/jobs/'+job['job_id'])
 assert job['state']=='succeeded',job;return job

def artifact(job,name):
 a=job['result']['artifact']; data=http('/v1/jobs/'+job['job_id']+'/artifact',raw=True)
 assert hashlib.sha256(data).hexdigest()==a['content_hash'];p=run/name;p.write_bytes(data);return p

def rgb(p,at):return list(cmd(['ffmpeg','-v','error','-ss',str(at),'-i',str(p),'-frames:v','1','-vf','crop=2:2:0:0,scale=1:1','-pix_fmt','rgb24','-f','rawvideo','-']))
def duration(p):return float(cmd(['ffprobe','-v','error','-show_entries','format=duration','-of','default=nw=1:nk=1',str(p)]))
try:
 caps=start(); kinds={x['name'] for x in caps['job_kinds']}
 assert {'preview.project_range','render.project'}<=kinds
 created=call('/v1/projects',{'project':{'id':'journey','name':'QA story','sequence_id':'main','width':160,'height':90,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
 upload=call('/v1/media/upload?name=source.mp4',source.read_bytes())
 track={'id':'vtrack','name':'Video','kind':'video','enabled':True,'muted':False,'locked':False,'clips':[],'params':{}}
 clip={'id':'left','asset_id':upload['asset']['id'],'timeline_range':span(0,40),'source_range':span(0,40),'params':{},'channels':[]}
 bind=edit(created['revision'],'bind',[{'type':'asset.bind','asset':upload['asset']},{'type':'track.create','sequence_id':'main','track':track},{'type':'clip.insert','sequence_id':'main','track_id':'vtrack','clip':clip}])
 plan=call('/v1/projects/journey/branches/main/edit-plans',bind);assert snapshot()['revision']==created['revision']
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
 rejected=http('/v1/tools/opencut_edit_apply',{'project_id':'journey','request':retry})
 assert rejected['error']['code']=='IDEMPOTENCY_KEY_REUSED',rejected
 checks.append('HTTP write and separate CLI equivalent-time retry share transaction; changed duration rejects')
 stale=http('/v1/projects/journey/branches/main/transactions',edit(split['revision'],'stale-agent',[{'type':'clip.delete','sequence_id':'main','track_id':'vtrack','clip_id':'right'}]))
 assert stale['error']['code']=='REVISION_CONFLICT';assert snapshot()['project']['sequences'][0]['caption_cues'][0]['text']=='QA STORY'
 checks.append('Split followed by independent human caption; stale Agent edit rejected without losing caption')
 preflight=call('/v1/render/preflight',{'project_id':'journey','revision':human['revision'],'sequence_id':'main','container':'mp4','video_codec':'h264'});assert preflight['ready'],preflight
 preview=wait(call('/v1/jobs',{'kind':'preview.project_range','idempotency_key':'preview','input':{'project_id':'journey','revision':human['revision'],'sequence_id':'main','requested_range':span(10,20),'alignment':'exact'}}))
 preview_path=artifact(preview,'preview.mp4');r=rgb(preview_path,.2);b=rgb(preview_path,1.5)
 assert r[0]>200 and r[2]<20 and b[2]>200 and b[0]<20,(r,b)
 assert abs(duration(preview_path)-2)<.05
 samples=array('h');samples.frombytes(cmd(['ffmpeg','-v','error','-i',str(preview_path),'-vn','-ac','1','-ar','48000','-f','s16le','-']))
 rms=(sum(v*v for v in samples)/len(samples))**.5;assert rms>100
 checks.append('Nonzero range preview independently decoded red then blue, two seconds and audible PCM')
 corrected=apply(edit(human['revision'],'correct-opacity',[{'type':'parameter.set_clip','sequence_id':'main','track_id':'vtrack','clip_id':'right','params':{'opacity':.5}}]))
 current=apply(edit(corrected['revision'],'later-delete',[{'type':'clip.delete','sequence_id':'main','track_id':'vtrack','clip_id':'right'}]))
 exported=wait(call('/v1/jobs',{'kind':'render.project','idempotency_key':'historical-export','input':{'project_id':'journey','revision':corrected['revision'],'sequence_id':'main','container':'mp4','video_codec':'h264'}}))
 output=artifact(exported,'export.mp4');blue=rgb(output,3.0)
 assert 105<blue[2]<150 and blue[0]<20 and abs(duration(output)-4)<.05,(blue,duration(output))
 probe=json.loads(cmd(['ffprobe','-v','error','-show_streams','-of','json',str(output)]));assert any(s['codec_type']=='audio' for s in probe['streams'])
 checks.append('Correct opacity then edit head; old fixed revision export preserves four seconds and half-intensity blue')
 stop();start();reopened=snapshot();assert reopened['revision']==current['revision'];assert len(reopened['project']['sequences'][0]['tracks'][0]['clips'])==1
 independent=json.loads(cmd([str(BIN),'--database',str(db),'--json','project','inspect','journey']));assert independent['data']==reopened
 checks.append('Real service restart and separate CLI reopen preserve latest head and clip count')
 restored=call('/v1/projects/journey/branches/main/restore',{'expected_revision':current['revision'],'target_revision':corrected['revision'],'idempotency_key':'restore','label':'Restore full project'})
 assert restored['revision']!=corrected['revision'];assert len(snapshot()['project']['sequences'][0]['tracks'][0]['clips'])==2
 checks.append('Whole-project restore creates a new revision and restores two editable clips')
 renamed=call('/v1/media/upload?name=same-video.png',source.read_bytes())
 assert renamed['asset']['id']==upload['asset']['id'] and renamed['asset']['kind']=='video'
 assert renamed['asset']['metadata']['mime_type']=='video/mp4'
 assert renamed['asset']['name']=='same-video.png'
 def content_type(path):
  with urllib.request.urlopen(urllib.request.Request(base+path,headers={'Authorization':f'Bearer {token}'}),timeout=15) as response:
   assert response.read()==source.read_bytes()
   return response.headers['Content-Type']
 assert content_type(upload['media_url'])=='video/mp4'
 with ThreadPoolExecutor(max_workers=4) as executor:
  duplicates=list(executor.map(lambda i:call('/v1/media/upload?name=duplicate-'+str(i)+'.png',source.read_bytes()),range(8)))
 assert all(item['asset']['id']==upload['asset']['id'] and item['asset']['kind']=='video' for item in duplicates)
 checks.append('Renamed and concurrent duplicate uploads preserve video identity and original MIME')
 png=run/'image.png'
 cmd(['ffmpeg','-v','error','-i',str(source),'-frames:v','1',str(png)])
 image=call('/v1/media/upload?name=picture.mp4',png.read_bytes())
 assert image['asset']['kind']=='image' and image['asset']['metadata']['mime_type']=='image/png',image
 avi=run/'mjpeg.avi'
 cmd(['ffmpeg','-v','error','-i',str(source),'-t','0.2','-an','-c:v','mjpeg',str(avi)])
 movie=call('/v1/media/upload?name=picture.jpg',avi.read_bytes())
 assert movie['asset']['kind']=='video' and movie['asset']['metadata']['mime_type']=='video/x-msvideo',movie
 bad=http('/v1/media/upload?name=invalid.mp4',b'not a movie')
 assert bad['error'] is not None,bad
 checks.append('PNG content stays image despite movie extension; MJPEG movie stays video; invalid bytes rejected')
 a=exported['result']['artifact']; artifact_path=Path(a['path']); original=artifact_path.read_bytes()
 try:
  for corrupted in (b'corrupt',bytes([original[0]^1])+original[1:]):
   artifact_path.write_bytes(corrupted)
   failure=http('/v1/jobs/'+exported['job_id']+'/artifact')
   assert failure['error']['code']=='ARTIFACT_VERIFICATION_FAILED',failure
   unchanged=call('/v1/jobs/'+exported['job_id'])
   assert unchanged['state']=='succeeded' and unchanged['result']['artifact']==a
 finally:
  artifact_path.write_bytes(original)
 assert http('/v1/jobs/'+exported['job_id']+'/artifact',raw=True)==original
 checks.append('Truncated and same-size corrupted artifact rejected; original successful record preserved; restored file downloads')
 findings={'renamed_video':{'same_asset_id':renamed['asset']['id']==upload['asset']['id'],'original_kind':upload['asset']['kind'],'renamed_kind':renamed['asset']['kind'],'renamed_mime':renamed['asset']['metadata'].get('mime_type')},'missing_journey_jobs':sorted({'analysis.transcribe','analysis.shots','project.package'}-kinds)}
 report={'status':'passed_with_remaining_scope','scope':'real HTTP/CLI workflow with synthetic audiovisual media; no browser or interview understanding validation','checks':checks,'findings':findings,'evidence':{'preview_rgb':[r,b],'export_blue':blue,'export_duration':duration(output),'preview_pcm_rms':rms,'job_id':exported['job_id']},'directory':str(run),'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()}
 (run/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
except Exception as e:
 (run/'report.json').write_text(json.dumps({'status':'failed','error':repr(e),'directory':str(run),'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest()},indent=2));raise
finally:
 stop();logfile.close();(run/'trace.json').write_text(json.dumps(trace,indent=2));(ROOT/'latest-journey.txt').write_text(str(run))
