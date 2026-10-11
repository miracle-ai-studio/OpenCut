#!/usr/bin/env python3
"""Exercise editorial plans through CLI/HTTP/MCP, atomic history and real A/V.

All media is synthetic. The stdio host runs its own managed Worker; no external
worker process is required. Reports bind to a frozen executable identity.
"""
import argparse,array,hashlib,json,math,os,selectors,shutil,socket,subprocess,tempfile,time,urllib.request,wave
from pathlib import Path

class Mcp:
    def __init__(self,binary,database,root,launch=None,trace=None):
        self.trace=trace
        self.log=(root/f'mcp-{time.time_ns()}.log').open('wb')
        self.process=subprocess.Popen(launch or [str(binary),'--database',str(database),'mcp','serve','--with-worker'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.log)
        self.selector=selectors.DefaultSelector();self.selector.register(self.process.stdout,selectors.EVENT_READ)
        self.buffer=bytearray();self.serial=0
        try:
            self.request('initialize',{'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'editorial-acceptance','version':'1'}})
            self.send({'jsonrpc':'2.0','method':'notifications/initialized'})
        except BaseException:
            self.process.kill();self.process.wait();self.close();raise
    def send(self,value):
        self.process.stdin.write((json.dumps(value)+'\n').encode());self.process.stdin.flush()
    def request(self,method,params):
        self.serial+=1;self.send({'jsonrpc':'2.0','id':self.serial,'method':method,'params':params});deadline=time.monotonic()+60
        while time.monotonic()<deadline:
            if b'\n' not in self.buffer:
                assert self.selector.select(max(0,deadline-time.monotonic())),method
                chunk=os.read(self.process.stdout.fileno(),65536);assert chunk,'MCP exited'
                self.buffer.extend(chunk)
            while b'\n' in self.buffer:
                raw,_,rest=self.buffer.partition(b'\n');self.buffer[:]=rest;reply=json.loads(raw)
                if reply.get('id')==self.serial:
                    assert 'error' not in reply,reply
                    return reply['result']
        raise AssertionError(f'MCP timed out: {method}')
    def tool(self,name,value):
        reply=self.request('tools/call',{'name':name,'arguments':value})['structuredContent']
        assert reply['error'] is None,reply
        if self.trace is not None:self.trace.append({'transport':'mcp','tool':name,'request':value,'reply':reply})
        return reply['data']
    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try: assert self.process.wait(timeout=20)==0
            except BaseException:
                self.process.kill();self.process.wait();raise
        self.selector.close();self.process.stdout.close();self.log.close()

def verify(binary,root,config_generator=None):
    root.mkdir(parents=True,exist_ok=False)
    frozen=root/'opencut';binary_sha=hashlib.sha256(binary.read_bytes()).hexdigest();shutil.copy2(binary,frozen)
    assert hashlib.sha256(frozen.read_bytes()).hexdigest()==binary_sha
    binary=frozen;database=root/'workspace.sqlite';checks=[];trace=[];mcp=None;host=None;launch=None
    if config_generator:
        import tomllib
        subprocess.run(['python3',str(config_generator),'--binary',str(binary),'--database',str(database),'--output',str(root/'agent-config')],check=True,capture_output=True,timeout=30)
        server=json.loads((root/'agent-config/claude.mcp.json').read_text())['mcpServers']['opencut']
        codex=tomllib.loads((root/'agent-config/codex.toml').read_text())['mcp_servers']['opencut']
        assert server['command']==codex['command'] and server['args']==codex['args']
        launch=[server['command'],*server['args']]
        checks.append('Delivered generator produces matching Codex/Claude configurations used for the actual MCP process')
    def call(name,value,error=None,credential=None):
        args=[str(binary),'--database',str(database),'--json']
        if credential:args+=['--credential-file',str(credential)]
        p=subprocess.run(args+['call',name,'--input','-'],input=json.dumps(value),text=True,capture_output=True,timeout=90)
        reply=json.loads(p.stdout);trace.append({'tool':name,'request':value,'reply':reply})
        if error: assert p.returncode and reply['error']['code']==error,reply
        else:assert p.returncode==0 and reply['error'] is None,(reply,p.stderr)
        return reply.get('data')
    def tick(n):return {'ticks':str(n),'timebase':10}
    def span(a,n):return {'start':tick(a),'duration':tick(n)}
    def current():return call('opencut_project_v3_inspect',{'project_id':'film'})
    def await_job(job):
        deadline=time.monotonic()+120
        while time.monotonic()<deadline:
            value=call('opencut_job_view',{'job_id':job['job_id']})
            if value['state'] in ('succeeded','failed','cancelled'):
                assert value['state']=='succeeded',value
                record=call('opencut_job_get',{'job_id':job['job_id']});return Path(record['result']['artifact']['path'])
            time.sleep(.1)
        raise AssertionError('Managed Worker did not finish job')
    def import_asset(file,asset_id):
        upload=mcp.tool('opencut_upload_stream_begin',{'project_id':'film','idempotency_key':'import-'+asset_id,'name':file.name,'size_bytes':str(file.stat().st_size)})
        put=subprocess.run([str(binary),'--json','--database',str(database),'upload','stream-put',upload['upload_id'],'--offset','0','--input',str(file)],capture_output=True,text=True,timeout=30)
        assert put.returncode==0 and json.loads(put.stdout)['data']['next_offset']==str(file.stat().st_size),put.stderr
        accepted=mcp.tool('opencut_upload_stream_complete',{'upload_id':upload['upload_id'],'content_hash':hashlib.sha256(file.read_bytes()).hexdigest()})
        deadline=time.monotonic()+120
        while True:
            job=mcp.tool('opencut_job_view',{'job_id':accepted['job_id']})
            if job['state'] in ('succeeded','failed','cancelled'):break
            assert time.monotonic()<deadline;time.sleep(.1)
        assert job['state']=='succeeded',job
        media=mcp.tool('opencut_project_media_inspect',{'project_id':'film','media_id':job['extended_result']['data']['media_id']})
        mcp.tool('opencut_upload_release',{'upload_id':upload['upload_id']})
        return {**media['asset'],'id':asset_id}
    try:
        initial=call('opencut_project_v3_create',{'project':{'id':'film','name':'Editorial acceptance','sequence_id':'main','width':160,'height':90,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})
        mcp=Mcp(binary,database,root,launch,trace)
        def base(op):return {'type':'base.apply','operation':op}
        ops=[base({'type':'track.create','sequence_id':'main','track':{'id':'video','name':'Video','kind':'video','enabled':True,'muted':True,'locked':False,'clips':[]}})]
        for i,color in enumerate(['red','blue']):
            path=root/(color+'.mp4')
            subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i',f'color={color}:s=160x90:r=10:d=6','-c:v','libx264','-pix_fmt','yuv420p',str(path)],check=True,timeout=30)
            ops.extend([base({'type':'asset.bind','asset':import_asset(path,color)}),base({'type':'clip.insert','sequence_id':'main','track_id':'video','clip':{'id':color+'-clip','asset_id':color,'timeline_range':span(i*20,20),'source_range':span(10,20),'params':{},'channels':[]}})])
        tone=root/'tone.wav';samples=array.array('h')
        for sample in range(48000*4):
            frequency=330 if sample<48000 else (440 if sample<96000 else 660)
            sample_value=round(32767*.25*math.sin(2*math.pi*frequency*sample/48000));samples.extend((sample_value,sample_value))
        if os.sys.byteorder!='little':samples.byteswap()
        with wave.open(str(tone),'wb') as wav:wav.setnchannels(2);wav.setsampwidth(2);wav.setframerate(48000);wav.writeframes(samples.tobytes())
        ops.extend([base({'type':'asset.bind','asset':import_asset(tone,'tone')}),base({'type':'track.create','sequence_id':'main','track':{'id':'audio','name':'Audio','kind':'audio','enabled':True,'muted':False,'locked':False,'clips':[]}}),base({'type':'clip.insert','sequence_id':'main','track_id':'audio','clip':{'id':'music','asset_id':'tone','timeline_range':span(0,40),'source_range':span(0,40),'params':{},'channels':[]}})])
        ops.append({'type':'mask.put','sequence_id':'main','mask':{'id':'blue-mask','target_id':'blue-clip','shape':{'kind':'rectangle','x':.5,'y':0.,'width':.5,'height':1.},'inverted':False,'feather':0.}})
        call('opencut_edit_v3_apply',{'project_id':'film','expected_revision':initial['revision'],'idempotency_key':'seed','label':'Synthetic A/V','operations':ops})
        for name in ('red.mp4','blue.mp4','tone.wav'):(root/name).unlink()
        before=current()
        checks.append('Agent streams two videos and one audio through MCP upload sessions and CLI byte transport, waits for actual imports, binds scoped immutable media and releases staging')
        listed=mcp.request('tools/list',{})['tools'];assert any(v['name']=='opencut_edit' for v in listed)
        request={'project_id':'film','expected_revision':before['revision'],'sequence_id':'main','idempotency_key':'shorten','label':'Remove middle second','command':{'kind':'ripple_delete','range':span(10,10)}}
        cli_plan=call('opencut_edit_timeline_plan',request)
        mcp_plan=mcp.tool('opencut_edit',{'action':'timeline.plan','input':request})
        assert cli_plan==mcp_plan and current()==before
        for principal,actions in [('reader',['read']),('editor',['read','edit'])]:
            policy={'principal_id':principal,'expected_revision':'0','enabled':True,'grants':[{'project_id':'film','actions':actions}]}
            result=subprocess.run([str(binary),'--database',str(database),'--json','access','set-policy','--input','-'],input=json.dumps(policy),text=True,capture_output=True,timeout=30)
            assert result.returncode==0,result.stderr
            credential=root/(principal+'.credential')
            result=subprocess.run([str(binary),'--database',str(database),'--json','access','issue',principal,'--output',str(credential)],text=True,capture_output=True,timeout=30)
            assert result.returncode==0,result.stderr
            result=call('opencut_edit_timeline_plan',request,credential=credential,error='NOT_AUTHORIZED' if principal=='reader' else None)
            if principal=='editor':assert result==cli_plan
            rejected=subprocess.run([str(binary),'--database',str(database),'--credential-file',str(credential),'--json','mcp','serve','--with-worker'],input='',text=True,capture_output=True,timeout=30)
            assert rejected.returncode==3 and json.loads(rejected.stdout)['error']['code']=='NOT_AUTHORIZED',rejected
        assert current()==before
        checks.append('Read-only planning is denied; scoped editing agrees with owner; restricted credentials cannot launch a Worker')
        with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
        token=root/'token'
        with (root/'http.log').open('wb') as log:host=subprocess.Popen([str(binary),'--database',str(database),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
        deadline=time.monotonic()+20
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):
                    if token.exists():break
            except OSError:pass
            assert time.monotonic()<deadline;time.sleep(.05)
        req=urllib.request.Request(f'http://127.0.0.1:{port}/v1/tools/opencut_edit_timeline_plan',data=json.dumps(request).encode(),headers={'Authorization':'Bearer '+token.read_text().strip(),'Content-Type':'application/json'})
        with urllib.request.urlopen(req,timeout=30) as response:http_plan=json.load(response)
        assert http_plan['error'] is None and http_plan['data']==cli_plan
        host.terminate();assert host.wait(timeout=20)==0;host=None
        checks.append('CLI, HTTP and compact MCP produce the same read-only editorial transaction')
        edit=mcp.tool('opencut_edit_v3_apply',cli_plan['transaction']);assert mcp.tool('opencut_edit_v3_apply',cli_plan['transaction'])==edit
        call('opencut_edit_timeline_plan',request,error='REVISION_CONFLICT')
        after=current();sequence=after['project']['base']['sequences'][0]
        assert [c['timeline_range'] for c in sequence['tracks'][0]['clips']]==[span(0,10),span(10,20)]
        assert len(sequence['tracks'][1]['clips'])==2
        checks.append('One idempotent commit shortens video and audio together; stale planning is rejected')
        undo={'project_id':'film','expected_revision':edit['revision'],'target_transaction_id':edit['transaction_id'],'idempotency_key':'undo'}
        assert not call('opencut_edit_v3_undo_plan',undo)['conflicts']
        undone=call('opencut_edit_v3_undo_apply',undo)
        assert current()['project']==before['project']
        redo={'project_id':'film','expected_revision':undone['transaction']['revision'],'undo_transaction_id':undone['transaction']['transaction_id'],'idempotency_key':'redo'}
        redone=call('opencut_edit_v3_redo_apply',redo);assert current()['project']==after['project']
        checks.append('Selective undo and redo recover the exact pre/post edit documents including masks')
        target={'project_id':'film','revision':redone['transaction']['revision'],'sequence_id':'main'}
        for at,point,channel in [(5,(80,45),0),(15,(120,45),2),(15,(40,45),None)]:
            job=mcp.tool('opencut_job_v3_frame',{**target,'idempotency_key':f'frame-{at}-{point[0]}','requested_time':tick(at),'alignment':'exact'})
            output=await_job(job)
            raw=subprocess.check_output(['ffmpeg','-v','error','-i',str(output),'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30)
            rgb=raw[(point[1]*160+point[0])*3:(point[1]*160+point[0])*3+3]
            if channel is None:assert max(rgb)<8,list(rgb)
            else:assert rgb[channel]>230 and all(v<15 for i,v in enumerate(rgb) if i!=channel),list(rgb)
        checks.append('Managed stdio Worker renders independent red, blue and masked black pixel oracles after ripple edit')
        output=await_job(mcp.tool('opencut_job_v3_render',{**target,'idempotency_key':'export','container':'mp4','video_codec':'h264'}))
        metadata=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_format','-of','json',str(output)],timeout=30))
        assert abs(float(metadata['format']['duration'])-3)<.12,metadata
        pcm=array.array('f');pcm.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(output),'-map','0:a:0','-f','f32le','-ac','1','-ar','48000','-'],timeout=30))
        if os.sys.byteorder!='little':pcm.byteswap()
        for at,expected in [(.2,330),(1.2,660),(2.2,660)]:
            segment=pcm[int(at*48000):int((at+.2)*48000)]
            energy=lambda hz:abs(sum(v*complex(math.cos(2*math.pi*hz*n/48000),math.sin(2*math.pi*hz*n/48000)) for n,v in enumerate(segment)))/len(segment)
            assert energy(expected)>.1 and energy(440)<.005,(at,energy(expected),energy(440))
        checks.append('Export is three seconds and independent PCM excludes the removed 440 Hz interval')
        mcp.close();mcp=None
        queued=call('opencut_job_v3_frame',{**target,'idempotency_key':'after-restart','requested_time':tick(5),'alignment':'exact'})
        assert queued['state']=='queued'
        mcp=Mcp(binary,database,root,launch,trace);await_job(queued)
        checks.append('EOF stops the managed host cleanly; a new host executes a persisted queued task')
        copies=mcp.tool('opencut_project_duplicate',{'projects':[{'source_project_id':'film','expected_revision':target['revision'],'target_project_id':'copy','name':'Copy'}],'idempotency_key':'copy'})
        upload=mcp.tool('opencut_upload_stream_begin',{'project_id':'film','idempotency_key':'unfinished-upload','name':'pending.bin','size_bytes':'4'})
        chunk=root/'chunk.bin';chunk.write_bytes(b'abcd')
        put=subprocess.run([str(binary),'--json','--database',str(database),'upload','stream-put',upload['upload_id'],'--offset','0','--input',str(chunk)],capture_output=True,text=True,timeout=30)
        assert put.returncode==0 and json.loads(put.stdout)['data']['next_offset']=='4',put.stderr
        deletion={'projects':[{'project_id':'film','expected_revision':target['revision']}],'idempotency_key':'delete-film'}
        deleted=mcp.tool('opencut_project_delete',deletion)
        assert not deleted['cleanup_pending'] and deleted['receipt']['staging_upload_ids']==[upload['upload_id']],deleted
        assert mcp.tool('opencut_project_delete',deletion)==deleted
        retained=mcp.tool('opencut_upload_retention',{'upload_id':upload['upload_id']})
        assert retained['state']=='released' and retained['reserved_bytes']=='0',retained
        call('opencut_project_v3_inspect',{'project_id':'film','revision':target['revision']},error='NOT_FOUND')
        library=mcp.tool('opencut_project_library',{'cursor':None,'limit':100})
        assert [p['project_id'] for p in library['items']]==['copy'],library
        copy_target={'project_id':'copy','revision':copies['projects'][0]['revision'],'sequence_id':'main'}
        frame=await_job(mcp.tool('opencut_job_v3_frame',{**copy_target,'idempotency_key':'copy-frame','requested_time':tick(15),'alignment':'exact'}))
        raw=subprocess.check_output(['ffmpeg','-v','error','-i',str(frame),'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-'],timeout=30)
        for x,blue in [(40,False),(120,True)]:
            rgb=raw[(45*160+x)*3:(45*160+x)*3+3]
            assert (rgb[2]>230 and rgb[0]<15 and rgb[1]<15) if blue else max(rgb)<8,list(rgb)
        copy_before=mcp.tool('opencut_project_v3_inspect',{'project_id':'copy','revision':None})
        removed=mcp.tool('opencut_project_media_remove',{'project_id':'copy','expected_revision':copy_before['revision'],'asset_ids':[a['id'] for a in copy_before['project']['base']['assets']],'idempotency_key':'remove-copy-media'})
        empty=mcp.tool('opencut_project_v3_inspect',{'project_id':'copy','revision':None})
        assert not empty['project']['base']['assets'] and not empty['project']['visual'][0]['masks'],empty
        assert all(not t['clips'] for s in empty['project']['base']['sequences'] for t in s['tracks']),empty
        undo_media={'project_id':'copy','expected_revision':removed['revision'],'target_transaction_id':removed['transaction_id'],'idempotency_key':'restore-copy-media'}
        assert not mcp.tool('opencut_edit_v3_undo_plan',undo_media)['conflicts']
        restored=mcp.tool('opencut_edit_v3_undo_apply',undo_media)
        assert mcp.tool('opencut_project_v3_inspect',{'project_id':'copy','revision':None})['project']==copy_before['project']
        copy_target['revision']=restored['transaction']['revision']
        copy_export=await_job(mcp.tool('opencut_job_v3_render',{**copy_target,'idempotency_key':'copy-export','container':'mp4','video_codec':'h264'}))
        copy_metadata=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_format','-of','json',str(copy_export)],timeout=30))
        assert abs(float(copy_metadata['format']['duration'])-3)<.12,copy_metadata
        copy_pcm=array.array('f');copy_pcm.frombytes(subprocess.check_output(['ffmpeg','-v','error','-i',str(copy_export),'-map','0:a:0','-f','f32le','-ac','1','-ar','48000','-'],timeout=30))
        if os.sys.byteorder!='little':copy_pcm.byteswap()
        for at,expected in [(.2,330),(1.2,660),(2.2,660)]:
            segment=copy_pcm[int(at*48000):int((at+.2)*48000)]
            assert energy(expected)>.1 and energy(440)<.005,(at,energy(expected),energy(440))
        checks.append('Agent duplicates the full edit, deletes its original idempotently, releases actual unfinished upload bytes and reservation, rejects old revision access and independently verifies surviving copy pixels')
        checks.append('Shared media-removal use case clears assets, clips and masks in one edit; Agent undo restores the exact document, then exports the surviving copy with independently verified duration and PCM')
        mcp.process.terminate();assert mcp.process.wait(timeout=20)==0
        mcp.close();mcp=None
        checks.append('SIGTERM stops the managed host even while the peer keeps stdin open')
        report={'status':'passed','binary_sha256':binary_sha,'checks':checks,'artifact':str(output),'artifact_sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'copy_artifact':str(copy_export),'copy_artifact_sha256':hashlib.sha256(copy_export.read_bytes()).hexdigest(),'evidence':str(root)}
        (root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
    finally:
        if mcp:mcp.close()
        if host and host.poll() is None:host.terminate();host.wait(timeout=20)
        (root/'trace.json').write_text(json.dumps(trace,indent=2)+'\n')

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path,default=Path(tempfile.gettempdir())/f'opencut-editorial-{time.time_ns()}');parser.add_argument('--agent-config-generator',type=Path);args=parser.parse_args()
    verify(args.binary.resolve(),args.artifacts_dir.resolve(),args.agent_config_generator.resolve() if args.agent_config_generator else None)
