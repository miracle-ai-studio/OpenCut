#!/usr/bin/env python3
"""Real local ASR and Agent caption editing. Synthetic speech only; requires macOS say.

The explicit model is never downloaded by this verifier. A configured model and
native executable are host dependencies, not paths supplied to editing tools.
"""
import argparse, array, hashlib, http.client, json, math, os, shutil, subprocess, time, urllib.request, wave
from pathlib import Path


def verify(binary, model, web, root, verify_browser=False):
    root.mkdir(parents=True, exist_ok=False)
    frozen = root / 'opencut'; shutil.copy2(binary, frozen)
    binary_sha = hashlib.sha256(frozen.read_bytes()).hexdigest()
    model_sha = hashlib.sha256(model.read_bytes()).hexdigest()
    database = root / 'workspace.sqlite'; service = root / 'service'; service.mkdir(mode=0o700)
    configuration = service / 'config.json'
    configuration.write_text(json.dumps({'listen':'127.0.0.1:0','web_root':str(web),
        'transcription':{'model':str(model),'executable':shutil.which('whisper-cli')}}))
    trace, checks, serial, session, state = [], [], 0, None, None
    def lifecycle(*args):
        p = subprocess.run([str(frozen),'--json','--database',str(database),'server','--directory',str(service),*args],capture_output=True,text=True,timeout=60)
        result=json.loads(p.stdout); assert p.returncode==0,(result,p.stderr)
        return result['data']
    def cli(name,value,error=None):
        p=subprocess.run([str(frozen),'--json','--database',str(database),'call',name,'--input','-'],input=json.dumps(value),capture_output=True,text=True,timeout=60)
        reply=json.loads(p.stdout)
        trace.append({'transport':'cli','tool':name,'input':value,'reply':reply})
        if error:assert reply['error']['code']==error,reply
        else:assert reply['error'] is None,reply
        return reply.get('data')
    def rpc(method,params):
        nonlocal serial,session
        serial+=1
        headers={'Accept':'application/json, text/event-stream','Content-Type':'application/json',
            'Authorization':'Bearer '+(service/'api-token').read_text().strip()}
        if session:headers.update({'Mcp-Session-Id':session,'Mcp-Protocol-Version':'2025-11-25'})
        conn=http.client.HTTPConnection('127.0.0.1',int(state['endpoint'].rsplit(':',1)[1]),timeout=60)
        conn.request('POST','/mcp',json.dumps({'jsonrpc':'2.0','id':serial,'method':method,'params':params}),headers)
        response=conn.getresponse();assert response.status==200,(response.status,response.read())
        session=response.getheader('Mcp-Session-Id') or session
        raw=response.read();conn.close()
        if response.getheader('Content-Type','').startswith('text/event-stream'):
            messages=[json.loads(line[6:]) for line in raw.decode().splitlines() if line.startswith('data: ') and line[6:].strip()]
            reply=next(message for message in messages if message.get('id')==serial)
        else:reply=json.loads(raw)
        assert 'error' not in reply,reply
        return reply['result']
    def call(name,value):
        if name.startswith('opencut_analysis_'):domain='media';action=name[8:].replace('_','.')
        elif name.startswith('opencut_subtitle_'):domain='edit';action=name[8:].replace('_','.')
        elif name.startswith('opencut_project_media_') or name.startswith('opencut_upload_'):domain='media';action=name[8:].replace('_','.')
        elif name.startswith('opencut_project_'):domain='project';action=name[len('opencut_project_'):].replace('_','.')
        elif name.startswith('opencut_edit_'):domain='edit';action=name[len('opencut_edit_'):].replace('_','.')
        elif name.startswith('opencut_job_'):domain='task';action=name[len('opencut_job_'):].replace('_','.')
        else:raise AssertionError(name)
        reply=rpc('tools/call',{'name':'opencut_'+domain,'arguments':{'action':action,'input':value}})['structuredContent']
        trace.append({'transport':'mcp-http','tool':name,'input':value,'reply':reply})
        assert reply['error'] is None,reply
        return reply['data']
    def await_job(job):
        deadline=time.monotonic()+900
        while time.monotonic()<deadline:
            job=call('opencut_job_view',{'job_id':job['job_id']})
            if job['state'] in ['succeeded','failed','cancelled']:
                assert job['state']=='succeeded',job
                return job
            time.sleep(.5)
        raise AssertionError('Local transcription timed out')
    def tick(n):return {'ticks':str(n),'timebase':48000}
    def span(a,n):return {'start':tick(a),'duration':tick(n)}
    def inspect():return call('opencut_project_inspect',{'project_id':'speech','revision':None})
    def download(job,filename):
        item=job['artifacts'][0];url=state['endpoint']+'/api/artifacts/'+urllib.parse.quote(item['artifact_id'],safe='')
        data=urllib.request.urlopen(urllib.request.Request(url,headers={'Authorization':'Bearer '+(service/'api-token').read_text().strip()}),timeout=60).read()
        assert hashlib.sha256(data).hexdigest()==item['content_hash']
        destination=root/filename;destination.write_bytes(data);return destination
    try:
        state=lifecycle('start');assert state['ready'];assert lifecycle('start')['instance']==state['instance']
        assert lifecycle('status')['ready']
        checks.append('Independent service starts from config including local transcription paths; repeated start and CLI status agree')
        rpc('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'caption-agent-acceptance','version':'1'}})
        # MCP initialized notification has no RPC reply.
        conn=http.client.HTTPConnection('127.0.0.1',int(state['endpoint'].rsplit(':',1)[1]),timeout=30)
        conn.request('POST','/mcp',json.dumps({'jsonrpc':'2.0','method':'notifications/initialized'}),{
            'Accept':'application/json, text/event-stream','Content-Type':'application/json',
            'Authorization':'Bearer '+(service/'api-token').read_text().strip(),'Mcp-Session-Id':session,'Mcp-Protocol-Version':'2025-11-25'})
        response=conn.getresponse();assert response.status==202;response.read();conn.close()
        created=call('opencut_project_create',{'project':{'id':'speech','name':'Speech acceptance','sequence_id':'main','width':640,'height':360,'fps':{'numerator':25,'denominator':1}},'idempotency_key':'create'})
        spoken=root/'spoken.aiff';source=root/'spoken.wav'
        phrase='Welcome to the video editor. We cut the scene and add clear captions.'
        subprocess.run(['say','-v','Samantha','-r','150','-o',str(spoken),phrase],check=True,timeout=60)
        subprocess.run(['ffmpeg','-v','error','-i',str(spoken),'-ar','48000','-ac','2',str(source)],check=True,timeout=30)
        with wave.open(str(source),'rb') as wav:
            assert wav.getsampwidth()==2 and wav.getnchannels()==2 and wav.getframerate()==48000
            frames=wav.getnframes();raw=wav.readframes(frames)
        upload=call('opencut_upload_begin',{'project_id':'speech','idempotency_key':'upload','name':'speech.wav','size_bytes':str(source.stat().st_size)})
        subprocess.run([str(frozen),'--json','--database',str(database),'upload','put',upload['upload_id'],'--offset','0','--input',str(source)],check=True,capture_output=True,timeout=60)
        imported=await_job(call('opencut_upload_complete',{'upload_id':upload['upload_id'],'content_hash':hashlib.sha256(source.read_bytes()).hexdigest()}))
        asset=call('opencut_project_media_inspect',{'project_id':'speech','media_id':imported['extended_result']['data']['media_id']})['asset'];asset['id']='voice'
        duration=asset['duration'];frames=min(frames,int(duration['ticks'])*48000//duration['timebase'])//1920*1920
        raw=raw[:frames*4]
        call('opencut_upload_release',{'upload_id':upload['upload_id']})
        def base(op):return {'type':'base.apply','operation':op}
        operations=[base({'type':'asset.bind','asset':asset})]
        for track_id,enabled,muted in [('dialogue',True,False),('muted',True,True),('disabled',False,False)]:
            operations.extend([base({'type':'track.create','sequence_id':'main','track':{'id':track_id,'name':track_id,'kind':'audio','enabled':enabled,'muted':muted,'locked':False,'clips':[],'params':{}}}),
                base({'type':'clip.insert','sequence_id':'main','track_id':track_id,'clip':{'id':track_id+'-clip','asset_id':'voice','timeline_range':span(48000,frames),'source_range':span(0,frames),'params':{'volume':.5},'channels':[]}})])
        call('opencut_edit_apply',{'project_id':'speech','expected_revision':created['revision'],'idempotency_key':'mix','label':'Local speech scene','operations':operations})
        before=inspect();source.unlink();spoken.unlink()
        checks.append('Agent imports and edits managed speech through MCP; source files are deleted before analysis, preview and export')
        request={'project_id':'speech','revision':before['revision'],'sequence_id':'main','language':'en','idempotency_key':'transcribe'}
        accepted=call('opencut_analysis_sequence_transcribe',request)
        assert call('opencut_analysis_sequence_transcribe',request)['job_id']==accepted['job_id']
        cli('opencut_analysis_sequence_transcribe',{**request,'language':'zh'},'IDEMPOTENCY_KEY_REUSED')
        analysis=await_job(accepted);result=analysis['extended_result']['data']
        transcript=' '.join(segment['text'] for segment in result['transcription']['segments']).lower()
        assert 'video editor' in transcript and 'captions' in transcript,transcript
        assert result['transcription']['model_sha256']==model_sha,result
        assert result['asset_lock']['assets'][0]['content_hash']==asset['version']['content_hash']
        assert inspect()==before
        checks.append('Real whisper.cpp inference recognizes expected synthetic speech with exact model hash and fixed source identity; same-key replay and mismatch rejection hold')
        args={'project_id':'speech','expected_revision':before['revision'],'sequence_id':'main','analysis_job_id':analysis['job_id'],'idempotency_key':'captions','words_per_caption':3,'style':None}
        receipt=call('opencut_subtitle_generate',args);after=inspect()
        assert call('opencut_subtitle_generate',args)==receipt
        cli('opencut_subtitle_generate',{**args,'words_per_caption':5},'IDEMPOTENCY_KEY_REUSED')
        cli('opencut_subtitle_generate',{**args,'idempotency_key':'stale'},'REVISION_CONFLICT')
        cues=after['project']['base']['sequences'][0]['caption_cues']
        assert len(cues)==receipt['cue_count'] and len(cues)>=3
        assert all(len(cue['text'].split())<=3 for cue in cues)
        assert any('captions' in cue['text'].lower() for cue in cues)
        assert all(int(cue['timeline_range']['start']['ticks'])>=0 for cue in cues)
        exported=call('opencut_subtitle_export',{'project_id':'speech','revision':after['revision'],'sequence_id':'main','format':'srt'})
        assert exported['cue_count']==len(cues) and 'captions' in exported['content'].lower()
        checks.append('One atomic generated-caption transaction supports same-key replay, conflict rejection and SRT export with bounded word chunks')
        undo={'project_id':'speech','expected_revision':after['revision'],'target_transaction_id':receipt['transaction']['transaction_id'],'idempotency_key':'undo'}
        undone=call('opencut_edit_undo_apply',undo)
        assert inspect()['project']==before['project']
        call('opencut_edit_redo_apply',{'project_id':'speech','expected_revision':undone['transaction']['revision'],
            'undo_transaction_id':undone['transaction']['transaction_id'],'idempotency_key':'redo'})
        final=inspect();assert final['project']==after['project']
        checks.append('Selective undo restores the exact original project; redo restores all generated captions')
        first=cues[0]['timeline_range']['start'];seconds=int(first['ticks'])/first['timebase']
        frame=int((seconds+.2)*25)
        preview=await_job(call('opencut_job_frame',{'project_id':'speech','revision':final['revision'],'sequence_id':'main','requested_time':{'ticks':str(frame),'timebase':25},'alignment':'exact','idempotency_key':'frame'}))
        png=download(preview,'caption.png')
        pixels=subprocess.check_output(['ffmpeg','-v','error','-i',str(png),'-f','rawvideo','-pix_fmt','rgb24','-frames:v','1','-'],timeout=30)
        assert len(pixels)==640*360*3
        bright=sum(1 for i in range((360-100)*640*3,len(pixels),3) if min(pixels[i:i+3])>180)
        assert bright>100 and max(pixels[:640*3*50])==0,bright
        render=await_job(call('opencut_job_render',{'project_id':'speech','revision':final['revision'],'sequence_id':'main','container':'mp4','video_codec':'h264','idempotency_key':'render'}))
        movie=download(render,'captioned.mp4')
        metadata=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(movie)],timeout=30))
        assert len(metadata['streams'])==2 and abs(float(metadata['format']['duration'])-(frames/48000+1))<.05
        observed=array.array('h',subprocess.check_output(['ffmpeg','-v','error','-i',str(movie),'-f','s16le','-ar','48000','-ac','2','-'],timeout=30))
        original=array.array('h',raw)
        # Independent PCM oracle: a one-second delay, one half-gain source;
        # muted/disabled duplicates must not contribute. AAC tolerance is explicit.
        a=[sample/32768 for sample in observed[48000*2:48000*2+len(original)]]
        b=[sample/32768*.5 for sample in original[:len(a)]]
        rms=math.sqrt(sum((x-y)**2 for x,y in zip(a,b))/len(a))
        assert rms<.008,rms
        assert max(abs(x) for x in observed[:int(.9*48000)*2])<100
        exported_pixels=subprocess.check_output(['ffmpeg','-v','error','-i',str(movie),'-vf',f"select=eq(n\\,{frame})",'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30)
        assert sum(1 for i in range((360-100)*640*3,len(exported_pixels),3) if min(exported_pixels[i:i+3])>150)>100
        checks.append('Native preview and verified movie contain visible white caption glyphs on independently expected black canvas; independent PCM verifies delay, half gain and mute/disabled exclusion')
        assert not list((root/'media').rglob('scene-audio.wav')) and not list((root/'media').rglob('*.model.bin'))
        checks.append('Private mixed audio and model snapshots are removed after successful analysis')
        report={'status':'passed','whole_classic_migration_complete':False,'binary_sha256':binary_sha,'model_sha256':model_sha,
            'checks':checks,'transcript':transcript,'cue_count':len(cues),'pcm_rmse':rms,'service_instance':state['instance'],
            'project_revision':final['revision'],'movie_sha256':hashlib.sha256(movie.read_bytes()).hexdigest(),'workspace':str(root)}
        if verify_browser:
            repository = Path(__file__).resolve().parents[1]
            env = {**os.environ, 'OPENCUT_WEB_URL':state['endpoint'], 'OPENCUT_WEB_WORKSPACE':str(root)}
            subprocess.run(['node',str(repository/'apps/web/scripts/verify-auto-captions.mjs')],
                           env=env,cwd=repository,check=True,timeout=900)
            browser_report = root/'caption-browser-report.json'
            browser = json.loads(browser_report.read_text())
            assert browser['status']=='passed' and len(browser['checks'])==9
            browser['binary_sha256']=binary_sha
            browser['web_files']={str(p.relative_to(web)):hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in sorted(web.rglob('*')) if p.is_file()}
            browser_report.write_text(json.dumps(browser,indent=2)+'\n')
            report['browser_report']=str(browser_report)
            report['browser_checks']=len(browser['checks'])
        (root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2))
    finally:
        (root/'trace.json').write_text(json.dumps(trace,indent=2)+'\n')
        if state:lifecycle('stop')


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/release/opencut'))
    parser.add_argument('--model',type=Path,required=True);parser.add_argument('--web-root',type=Path,required=True)
    parser.add_argument('--artifacts-dir',type=Path,required=True)
    parser.add_argument('--verify-browser',action='store_true',help='Also verify built caption and viewport UI against this real ASR service')
    args=parser.parse_args()
    verify(args.binary.resolve(),args.model.resolve(),args.web_root.resolve(),args.artifacts_dir.resolve(),args.verify_browser)
