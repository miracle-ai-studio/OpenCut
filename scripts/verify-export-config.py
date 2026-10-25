#!/usr/bin/env python3
"""Complete missing export configuration cells in one retained workspace.

No product implementation, source fixture copy, build or successful-job replay
replacement. Original requests, jobs, and exact encoder observations are durable.
"""
import argparse
from array import array
import importlib.util
import json
import math
from pathlib import Path
import re
import shlex
import sqlite3
import subprocess
import time
from acceptance import Harness, sha


def verify(a):
    h=Harness(a.binary,a.workspace,'agent-export-config-v2')
    ui=json.loads(a.sound_report.read_text()); project=ui['project_id']; revision=ui['revision']
    before=h.inspect(project); assert before['revision']==revision
    status=h.cli(['server','--directory',str(h.workspace/'service'),'status']); assert status['ready'] and sha(Path(status['executable']).read_bytes())==h.candidate
    root=Path(__file__).resolve().parents[1]; helper=root/'apps/web/scripts/verify-export-config.mjs'
    def scan_encoders():
        raw=subprocess.check_output(['/bin/ps','-ww','-axo','command='],text=True)
        for line in raw.splitlines():
            if '/artifacts/job_' not in line or ' -b:v ' not in line: continue
            args=shlex.split(line)
            if not args or Path(args[0]).name!='ffmpeg':continue
            match=re.search(r'/artifacts/(job_[a-z0-9]+)/attempt-',line)
            if not match: continue
            seen=h.state.setdefault('encoder_processes',{})
            if match[1] not in seen:seen[match[1]]=args;h.save()
    def browser(phase,input_path,report):
        log=a.report.with_name('export-config-'+phase+'.log')
        command=['node',str(helper),'--phase',phase,'--workspace',str(h.workspace),'--endpoint',status['endpoint'],'--input',str(input_path),'--report',str(report),'--credential',status['token_file']]
        with log.open('a') as output:
            child=subprocess.Popen(command,cwd=root/'apps/web',stdout=output,stderr=subprocess.STDOUT)
            while child.poll() is None:
                scan_encoders();time.sleep(.1)
            assert child.returncode==0,{'browser_phase':phase,'log':str(log)}
        return json.loads(report.read_text())
    def get_public(job_id):
        view=h.call('opencut_job_view',{'job_id':job_id});assert view['state']=='succeeded' and view['result_unavailable'] is None
        assert view['kind'] in ['render.video','render.video_range'] and view['result']['kind']==view['kind']
        return view,view['result']['data']
    def wait(job_id):
        deadline=time.monotonic()+240
        while True:
            scan_encoders();view=h.call('opencut_job_view',{'job_id':job_id})
            if view['state'] in ['succeeded','cancelled','failed']:
                assert view['state']=='succeeded',view;return view
            assert time.monotonic()<deadline,{'observation_timeout':job_id,'note':'Retain the original job and inspect its current state; do not resubmit a replacement.'}
            time.sleep(.1)
    def rates(options,width=320,height=180):
        factor={'low':.6,'medium':1,'high':2,'very_high':4}[options['quality']]
        efficiency=1 if options['format']=='mp4' else .6
        video=math.ceil(3000000*((width*height)/(1920*1080))**.95*efficiency*factor/1000)*1000
        if not options['include_audio']:audio=None
        elif options['format']=='mp4':audio=min([96000,128000,160000,192000],key=lambda n:abs(n-128000*factor))
        else:audio=math.floor(64000*factor/1000+.5)*1000
        return {'video_bitrate':video,'audio_bitrate':audio}
    def record(job_id):
        view,data=get_public(job_id);identity=data['identity'];options=identity['options'];encoder=data['execution']['encoder']
        assert encoder['rates']==rates(options,identity['width'],identity['height'])
        path=h.artifact(view);assert data['artifact']['content_hash']==view['artifacts'][0]['content_hash']
        return {'job_id':job_id,'options':options,'identity':identity,'encoder':encoder,'artifact':view['artifacts'][0],'path':path,'view':view,'data':data}
    baseline=h.stage('existing-high-mp4',lambda:{'job_id':ui['job_id'],'producer_report':str(a.sound_report)})
    old_webm=h.stage('existing-very-high-webm',lambda:{'job_id':'job_b48be3956c2f48099e21ffefa0770b47','producer_evidence':'a0_integrated_20261017.native_scene; independent531 PNG samples and8 video times already qualified, retain original5abc3e8a producer.'})
    existing=[record(baseline['job_id']),record(old_webm['job_id'])]
    source_pcm=array('f');source_pcm.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(existing[0]['path'].parent/'video-audio.wav'),'-f','f32le','-ar','48000','-ac','2','pipe:1']))
    input_path=a.report.with_name('export-config-ui-input.json');input_path.write_text(json.dumps({'project_id':project,'revision':revision})+'\n')
    ui_result=h.stage('actual-ui-low-mp4-original-recovery',lambda:browser('submit',input_path,a.report.with_name('export-config-ui.json')))
    assert h.call('opencut_render_submit',{'kind':'video','request':ui_result['request']})['job_id']==ui_result['accepted_job_id']
    h.stage('complete-ui-low-mp4',lambda:wait(ui_result['accepted_job_id']))
    cases=[('mp4','medium',False),('mp4','very_high',False),('webm','low',False),('webm','medium',True),('webm','high',True)]
    new=[record(ui_result['accepted_job_id'])]
    for fmt,quality,audio in cases:
        name=fmt+'-'+quality+('-audio' if audio else '-silent')
        request={'project_id':project,'revision':revision,'sequence_id':'sound-scene','idempotency_key':h.key(name),'options':{'format':fmt,'quality':quality,'include_audio':audio}}
        accepted=h.stage(name+'-accepted',lambda: h.request(name,'opencut_render_submit',{'kind':'video','request':request}))
        h.stage(name+'-complete',lambda:wait(accepted['job_id']))
        row=record(accepted['job_id']);assert row['options']==request['options'];new.append(row)
    summaries=[]
    def measure(row):
        path=row['path'];options=row['options'];view=row['view'];data=row['data']
        assert data['identity']['project_id']==project and data['identity']['revision']==revision
        assert data['identity']['frame_count']=='30' and data['identity']['width']==320 and data['identity']['height']==180
        probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-count_frames','-show_streams','-show_format','-of','json',str(path)]));streams=probe['streams'];video=next(s for s in streams if s['codec_type']=='video')
        assert options['format'] in probe['format']['format_name'].split(',')
        assert video['codec_name']==('h264' if options['format']=='mp4' else 'vp9') and video['nb_read_frames']=='30' and video['r_frame_rate']=='30/1'
        assert len(streams)==(2 if options['include_audio'] else 1)
        rgb=subprocess.check_output(['ffmpeg','-v','error','-i',str(path),'-an','-f','rawvideo','-pix_fmt','rgb24','pipe:1']);assert len(rgb)==30*320*180*3 and max(rgb)<=2
        result={'job_id':row['job_id'],'options':options,'rates':row['encoder']['rates'],'black_max_channel':max(rgb),'streams':len(streams),'artifact':row['artifact']}
        args=h.state.get('encoder_processes',{}).get(row['job_id'])
        at=lambda key:args[args.index(key)+1]
        if args:
            assert at('-b:v')==str(row['encoder']['rates']['video_bitrate']) and at('-c:v')==('libx264' if options['format']=='mp4' else 'libvpx-vp9') and '-crf' not in args
        if options['format']=='mp4':
            # x264 records the actual ABR target in its encoder information SEI.
            targets=re.findall(rb'(?<!max_)bitrate=(\d+)',path.read_bytes())
            assert str(row['encoder']['rates']['video_bitrate']//1000).encode() in targets,targets
            result['h264_bitstream_abr_kbps']=row['encoder']['rates']['video_bitrate']//1000
        if options['include_audio']:
            if args: assert at('-b:a')==str(row['encoder']['rates']['audio_bitrate']) and at('-c:a')==('aac' if options['format']=='mp4' else 'libopus')
            audio=next(s for s in streams if s['codec_type']=='audio');assert audio['codec_name']==('aac' if options['format']=='mp4' else 'opus') and audio['sample_rate']=='48000' and audio['channels']==2
            mixed=array('f');mixed.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(path.parent/'video-audio.wav'),'-f','f32le','-ar','48000','-ac','2','pipe:1']))
            assert len(mixed)==len(source_pcm);error=max(abs(a-b) for a,b in zip(mixed,source_pcm));assert error==0
            encoded=array('f');encoded.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(path),'-vn','-f','f32le','-ar','48000','-ac','2','pipe:1']));assert len(encoded)>=len(source_pcm)
            energy=sum(x*x for x in source_pcm);encoded_energy=sum(encoded[i]**2 for i in range(len(source_pcm)))
            correlation=sum(encoded[i]*x for i,x in enumerate(source_pcm))/math.sqrt(energy*encoded_energy);ratio=encoded_energy/energy
            assert correlation>.5 and .3<=ratio<=1.5,(correlation,ratio)
            result.update(unencoded_pcm_max_absolute_error=error,encoded_audio_correlation=correlation,encoded_audio_energy_ratio=ratio)
        else:
            if args: assert '-an' in args and '-c:a' not in args and '-b:a' not in args and args.count('-i')==1
            assert data['execution']['audio'] is None and data['identity']['audio_frame_count'] is None and not (path.parent/'video-audio.wav').exists()
        result['actual_encoder_arguments_captured']=args is not None;return result
    for row in new:summaries.append(h.stage('measured-'+row['job_id'],lambda:measure(row)))
    all_rows=existing+new;assert {(r['options']['format'],r['options']['quality']) for r in all_rows}=={(f,q) for f in ['mp4','webm'] for q in ['low','medium','high','very_high']}
    for fmt in ['mp4','webm']:
        chosen=[r for r in all_rows if r['options']['format']==fmt];assert len({r['encoder']['rates']['video_bitrate'] for r in chosen})==4
        assert {r['options']['include_audio'] for r in chosen}=={True,False}
    def refusals():
        with sqlite3.connect(f'file:{h.workspace / "workspace.sqlite"}?mode=ro',uri=True) as db:count=db.execute('select count(*) from jobs').fetchone()[0]
        for option in [{'format':'gif','quality':'high','include_audio':True},{'format':'mp4','quality':'invalid','include_audio':False}]:
            request={**ui_result['request'],'idempotency_key':h.key('invalid-'+option['format']+'-'+option['quality']),'options':option}
            p=subprocess.run([str(h.binary),'--json','--database',str(h.workspace/'workspace.sqlite'),'call','opencut_job_export_video','--input','-'],input=json.dumps(request),text=True,capture_output=True);body=json.loads(p.stdout);assert p.returncode and body['error'] and body['data'] is None
        with sqlite3.connect(f'file:{h.workspace / "workspace.sqlite"}?mode=ro',uri=True) as db:assert db.execute('select count(*) from jobs').fetchone()[0]==count
        assert h.inspect(project)==before;return {'invalid_format_and_quality_refused_before_job':True,'project_unchanged':True}
    negative=h.stage('invalid-combinations',refusals)
    spec=importlib.util.spec_from_file_location('export_mcp',root/'scripts/verify-editorial.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    mcp=module.Mcp(h.binary,h.workspace/'workspace.sqlite',a.report.parent,[str(h.binary),'--database',str(h.workspace/'workspace.sqlite'),'mcp','serve'])
    try:
        for row in new:
            original=ui_result['request'] if row['job_id']==ui_result['accepted_job_id'] else h.state['requests'][row['options']['format']+'-'+row['options']['quality']+('-audio' if row['options']['include_audio'] else '-silent')]['input']['request']
            assert mcp.tool('opencut_task',{'action':'render.submit','input':{'kind':'video','request':original}})['job_id']==row['job_id']
    finally:mcp.close()
    download_input=a.report.with_name('export-config-download-input.json');download_input.write_text(json.dumps({'jobs':[{'job_id':r['job_id'],'artifact':r['artifact'],'play':r['job_id'] in [ui_result['accepted_job_id'],new[-2]['job_id']]} for r in new]},indent=2)+'\n')
    downloads=h.stage('public-download-and-browser-play',lambda:browser('download-play',download_input,a.report.with_name('export-config-downloads.json')))
    assert h.inspect(project)==before
    result={'status':'passed_export_configuration_matrix','binary_sha256':h.candidate,'project_id':project,'revision':revision,'ui':ui_result,'reused_outputs':[baseline,old_webm],'new_outputs':summaries,'negative':negative,'downloads':downloads,'format_quality_pairs':8,'new_jobs':6,'new_media_imports':0,'new_product_builds':0,'scope':'F53 two formats/four distinct quality targets, format-specific audio presence/absence, exact expected encoder receipt targets, H264 bitstream ABR values and any captured actual argv, independently expected black pixels and source-qualified PCM reuse, lossy audio decoding, real UI original submission recovery, MCP replay, SDK download and actual MP4/WebM browser playback. Existing complex WebM independent pixels and high MP4 independent source PCM keep original producers. F52 cancellation windows and whole migration remain open.'}
    a.report.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'status':result['status'],'format_quality_pairs':8,'new_jobs':6}))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['binary','workspace','sound-report','report']:p.add_argument('--'+name,type=Path,required=True)
    verify(p.parse_args())
