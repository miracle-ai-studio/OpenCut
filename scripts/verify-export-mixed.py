#!/usr/bin/env python3
"""F52 actual mixed-content output and UI task lifecycle in the retained workspace."""
import argparse
from array import array
import copy
import json
import math
from pathlib import Path
import struct
import subprocess
import wave
import zlib
from acceptance import Harness,at,sha


def verify(a):
 h=Harness(a.binary,a.workspace,'agent-export-mixed-v4');root=Path(__file__).resolve().parents[1]
 source_project='b2101b49-480b-4fd8-a111-66b4d9a0bad1';source_revision='rev_7b145449be3c3736cba2b587bd27a0a1db67e665127d6ed524e4bba5b256e1d9';project='agent-export-mixed-v4-acceptance'
 h.stage('copy-existing-complex-source',lambda:h.request('copy','opencut_project_copy',{'source_project_id':source_project,'source_revision':source_revision,'project_id':project,'name':'F52 complete mixed export acceptance','idempotency_key':h.key('copy')}))
 snap=h.inspect(project);scene=snap['project']['default_sequence_id']
 colors=[(210,40,30),(30,190,50),(40,60,210),(220,180,30)]
 def make_png():
  # One new deterministic picture fills the prior workspace's missing image cell.
  # No screenshot/product pixels or extra copy of an existing source is used.
  def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
  raw=b''.join(b'\0'+b''.join(bytes(colors[(y>=90)*2+(x>=160)]) for x in range(320)) for y in range(180))
  return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',320,180,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
 image=h.workspace/'f52-source-quadrants.png';payload=make_png()
 if image.exists():assert image.read_bytes()==payload
 else:image.write_bytes(payload)
 media=h.stage('one-required-image-import',lambda:h.import_media('picture',project,image))
 def placement():
  s=h.inspect(project);q=s['project']['sequences'][0];assets=s['project']['assets'];video=next(v for v in assets if v['kind']=='video');audio=next(v for v in assets if v['kind']=='audio')
  timing={'kind':'pinned_timeline','source_start':at(0),'duration':at(4),'rate':{'numerator':1,'denominator':1},'maintain_pitch':False}
  request={'project_id':project,'expected_revision':s['revision'],'sequence_id':scene,'idempotency_key':h.key('media-placement'),'label':'F52 existing video/audio plus one independent image','ripple':False,
    'bindings':[{'media_id':media['media_id'],'version':media['asset']['version'],'asset_id':'f52-image','name':'F52 independent quadrants','mode':'bind'}],
    'new_tracks':[{'id':'f52-image-track','name':'F52 picture','kind':'video'},{'id':'f52-audio-track','name':'F52 sound','kind':'audio'}],
    'placements':[{'element_id':'f52-video','name':'F52 existing source picture','track_id':q['main_track_id'],'asset_id':video['id'],'start':at(0),'content':{'kind':'video','timing':timing,'source_audio_enabled':False}},
     {'element_id':'f52-audio','name':'F52 existing source sound','track_id':'f52-audio-track','asset_id':audio['id'],'start':at(0),'content':{'kind':'audio','timing':timing}},
     {'element_id':'f52-image-element','name':'F52 picture','track_id':'f52-image-track','asset_id':'f52-image','start':at(0),'content':{'kind':'image','duration':at(4)}}]}
  p=h.request('placement-plan','opencut_media_import_plan',request);return h.apply('placement',p['transaction'])
 h.stage('media-placement',placement)
 def add_text():
  s=h.inspect(project);font=next(f for f in s['project']['fonts'] if f['family']=='Lato' and f['weight']==400 and f['style']=='normal')
  initial=h.call('opencut_text_plan',{'project_id':project,'expected_revision':s['revision'],'sequence_id':scene,'idempotency_key':h.key('text-defaults'),'command':{'kind':'defaults','font_id':font['id'],'start':{'kind':'exact','time':at(0)}}})['initial'];text=initial['text'];text['content']='F52';text['font_size']={'unit':'canvas_pixels','value':20};text['color']={'encoding':'srgb','red':1,'green':0,'blue':1,'alpha':1};text['background']['enabled']=False
  p=h.request('title-plan','opencut_text_plan',{'project_id':project,'expected_revision':s['revision'],'sequence_id':scene,'idempotency_key':h.key('title'),'command':{'kind':'create','element_id':'f52-title','track':{'kind':'new','track_id':'f52-title-track'},'values':{'name':'F52 ordinary text','text':text,'start':{'kind':'exact','time':at(0)},'duration':{'kind':'exact','time':at(4)}}}});return h.apply('title',p['transaction'])
 h.stage('title',add_text)
 def finish_graph():
  s=h.inspect(project);q=s['project']['sequences'][0];operations=[]
  for tid,eid,x,y,scale in [(q['main_track_id'],'f52-video',-130,30,.15),('f52-image-track','f52-image-element',-100,-50,.25),('f52-title-track','f52-title',0,-45,1)]:
   e=copy.deepcopy(next(e for t in q['tracks'] for e in t['elements'] if e['id']==eid));e['visual']['transform']={'position_x_px':x,'position_y_px':y,'scale_x':scale,'scale_y':scale,'rotation_degrees':0};operations.append({'type':'element_put','sequence_id':scene,'track_id':tid,'element':e})
  track={'id':'f52-caption-track','name':'F52 pinned captions','kind':'caption','enabled':True,'muted':False,'locked':False,'opacity':1,'gain':{'unit':'decibels','value':0},'elements':[],'channels':[]}
  font=next(f for f in s['project']['fonts'] if f['family']=='Lato' and f['weight']==400 and f['style']=='normal')
  caption={'id':'f52-caption','track_id':track['id'],'timeline_range':{'start':at(0),'duration':at(4)},'text':'F52 subtitle','source_anchor':None,'style':{'font':{'kind':'locked','font_id':font['id']},'font_size':{'unit':'canvas_pixels','value':16},'color':{'encoding':'srgb','red':0,'green':1,'blue':1,'alpha':1},'background_color':None,'max_width_px':260,'line_height_px':20,'text_align':'center','layout':'pinned_caption_v4'}}
  operations.extend([{'type':'track_put','sequence_id':scene,'track':track},{'type':'caption_put','sequence_id':scene,'caption':caption}])
  return h.apply('mixed-layout',{'project_id':project,'expected_revision':s['revision'],'idempotency_key':h.key('mixed-layout'),'label':'F52 complete visible type mix','operations':operations})
 h.stage('complete-visible-type-graph',finish_graph);snap=h.inspect(project);q=snap['project']['sequences'][0]
 kinds={e['content']['kind'] for t in q['tracks'] for e in t['elements']};assert kinds=={'video','audio','image','graphic','text','effect_layer'} and len(q['captions'])==1
 status=h.cli(['server','--directory',str(h.workspace/'service'),'status']);assert status['ready'] and sha(Path(status['executable']).read_bytes())==h.candidate
 refused=[json.loads((a.report.parent/name).read_text())['job_id'] for name in ['task-cancel-queue.json','task-cancel-render.json','task-cancel-before-publication.json']]+['job_dad442a54f874a42a3eb290494b8f162']
 input_file=a.report.with_name('task-mixed-ui-input.json');ui_report=a.report.with_name('task-mixed-ui.json');input_file.write_text(json.dumps({'project_id':project,'revision':snap['revision'],'refused_jobs':refused},indent=2)+'\n')
 def ui():
  with a.report.with_name('task-mixed-ui.log').open('a') as log:
   p=subprocess.run(['node',str(root/'apps/web/scripts/verify-export-tasks.mjs'),'--workspace',str(h.workspace),'--endpoint',status['endpoint'],'--credential',status['token_file'],'--input',str(input_file),'--report',str(ui_report)],cwd=root/'apps/web',stdout=log,stderr=subprocess.STDOUT)
  assert p.returncode==0,str(a.report.with_name('task-mixed-ui.log'));return json.loads(ui_report.read_text())
 ui_result=h.stage('actual-mixed-export-ui',ui);job=ui_result['completed'];path=h.artifact(job);data=job['extended_result']['data'];assert data['identity']['visual_profile']=='scene_v1' and data['identity']['frame_count']=='120';assert h.inspect(project)==snap
 rgb=subprocess.check_output(['ffmpeg','-v','error','-i',str(path),'-an','-f','rawvideo','-pix_fmt','rgb24','pipe:1']);assert len(rgb)==120*320*180*3
 def pixel(frame,x,y):return list(rgb[((frame*180+y)*320+x)*3:((frame*180+y)*320+x)*3+3])
 # Match the existing H264 source-color gate from verify-agent-long-source.py;
 # the separate retained complex-geometry gate remains6. VP9 sample tolerance
 # is not the H264 source-color contract. Never alter source colors or rerender.
 source_color_threshold=8
 samples=[];maximum=0
 for frame in [8,38,68,98]:
  for x,y,expected in [(40,30,colors[0]),(80,30,colors[1]),(40,50,colors[2]),(80,50,colors[3])]:
   actual=pixel(frame,x,y);error=max(abs(a-b) for a,b in zip(actual,expected));maximum=max(maximum,error);assert error<=source_color_threshold,(frame,x,y,actual,expected);samples.append({'frame':frame,'x':x,'y':y,'expected':expected,'actual':actual})
 # Reuse independently authored expected samples, never a product frame, away
 # from the added source/text/picture/caption rectangles and their blur halo.
 old=json.loads((h.workspace/'d2c-scene-stack-checkpoint.json').read_text());mask_points=0;mask_error=0
 for o in old['jobs']['webm']['observations']:
  assert o['passed'];frame=math.floor(o['seconds']*30+.5)
  for p in o['samples']:
   if p['x']>=120 and 80<=p['y']<=130 and max(p['expected'][:3])-min(p['expected'][:3])<=1 and (max(abs(v-223) for v in p['expected'][:3])<=1 or max(abs(v-31) for v in p['expected'][:3])<=1):
    e=max(abs(a-b) for a,b in zip(pixel(frame,p['x'],p['y']),p['expected'][:3]));assert e<=6,(o['case'],p,e);mask_error=max(mask_error,e);mask_points+=1
 assert mask_points>=16
 title_pixels=caption_pixels=0
 for y in range(15,80):
  for x in range(110,210):
   r,g,b=pixel(8,x,y)
   if r>180 and b>180 and g<80:title_pixels+=1
 for y in range(135,180):
  for x in range(50,270):
   r,g,b=pixel(8,x,y)
   if g>180 and b>180 and r<80:caption_pixels+=1
 assert title_pixels>=10 and caption_pixels>=10,(title_pixels,caption_pixels)
 def resource(asset):
  digest=asset['version']['content_hash'];p=h.workspace/'asset-store'/digest[:2]/digest;assert sha(p.read_bytes())==digest;return p
 video=next(v for v in snap['project']['assets'] if v['kind']=='video');source_rgb=subprocess.check_output(['ffmpeg','-v','error','-ss','0.75','-i',str(resource(video)),'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','pipe:1'])
 # Actual source dimensions come from the independent container decoder.
 probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-of','json',str(resource(video))]));stream=next(v for v in probe['streams'] if v['codec_type']=='video');sx,sy=stream['width']//2,stream['height']//2;expected=list(source_rgb[(sy*stream['width']+sx)*3:(sy*stream['width']+sx)*3+3]);video_error=max(abs(a-b) for a,b in zip(pixel(23,30,120),expected));assert video_error<=6,(pixel(23,30,120),expected)
 audio=next(v for v in snap['project']['assets'] if v['kind']=='audio');mixed=array('f');mixed.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(path.parent/'video-audio.wav'),'-f','f32le','-ar','48000','-ac','2','pipe:1']))
 with wave.open(str(resource(audio))) as wav:
  assert (wav.getnchannels(),wav.getsampwidth(),wav.getframerate())==(1,2,48000);source=array('h');source.frombytes(wav.readframes(192000))
 assert len(mixed)==384000 and len(source)==192000;pcm_error=max(abs(mixed[i*2+c]-v/(32767 if v>=0 else 32768)) for i,v in enumerate(source) for c in [0,1]);assert pcm_error<=1e-7,pcm_error
 result={'status':'passed_scoped_mixed_output_and_task_lifecycle','binary_sha256':h.candidate,'project_id':project,'revision':snap['revision'],'job_id':job['job_id'],'types':sorted(kinds|{'caption'}),'source_scene_copy':{'project_id':source_project,'revision':source_revision},'new_media_sources':1,'source_image_sha256':sha(payload),'reused_source_assets':2,'image_samples':samples,'image_max_channel_error':maximum,'image_h264_source_color_threshold':source_color_threshold,'independent_existing_mask_samples':mask_points,'mask_max_channel_error':mask_error,'source_video_max_channel_error':video_error,'title_magenta_pixels':title_pixels,'caption_cyan_pixels':caption_pixels,'independent_pcm_max_absolute_error':pcm_error,'pcm_threshold':1e-7,'ui_report':str(ui_report),'ui':ui_result,'scope':'Actual full120-frame mixed scene with video/audio/picture/text/pinned caption/graphics/complex masks/blur effect track, public UI durable input-frame progress/reload, download and postpublication cancel; source picture colors/video decode/signed PCM and previously authored independent complex geometry expectations. Not all type/property combinations, full S1, physical audio or whole migration.'}
 a.report.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'status':result['status'],'job_id':job['job_id'],'pcm_error':pcm_error,'independent_mask_samples':mask_points}))


if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__)
 for name in ['binary','workspace','report']:p.add_argument('--'+name,type=Path,required=True)
 verify(p.parse_args())
