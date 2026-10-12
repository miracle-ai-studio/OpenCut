#!/usr/bin/env python3
"""Public task projections, snapshot pagination, durable cursors and content-bound downloads."""
import hashlib
import json
import argparse
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,help='Reuse this built CLI without invoking Cargo')
arguments=parser.parse_args()
BIN=arguments.binary.resolve() if arguments.binary else ROOT/'target/debug/opencut'
run=Path(tempfile.mkdtemp(prefix='opencut-job-views-')).resolve()
db=run/'workspace.sqlite'; token_file=run/'token'; logs=(run/'host.log').open('wb')
trace=[];checks=[];host=None;token=None
print(f'Public Job acceptance evidence: {run}',flush=True)

def command(args,**kwargs):return subprocess.check_output([str(a) for a in args],stderr=subprocess.PIPE,**kwargs)
def cli(name,args,database=db,ok=True):
    path=run/'cli-input.json';path.write_text(json.dumps(args))
    p=subprocess.run([str(BIN),'--database',str(database),'--json','call',name,'--input',str(path)],capture_output=True)
    response=json.loads(p.stdout)
    if ok:assert p.returncode==0 and response['error'] is None,response
    return response['data'] if ok else response

def private_free(value):
    assert str(run) not in json.dumps(value),value
    blocked={'input','output','path','source','lease_owner','lease_until','attempt','payload','worker'}
    def visit(v):
        if isinstance(v,dict):
            assert not blocked.intersection(v),v
            for child in v.values():visit(child)
        elif isinstance(v,list):
            for child in v:visit(child)
    visit(value)

def http(path,body=None,raw=False,auth=True):
    headers={'Authorization':f'Bearer {token}'} if auth else {}
    if body is not None and not isinstance(body,bytes):body=json.dumps(body).encode();headers['Content-Type']='application/json'
    req=urllib.request.Request(base+path,data=body,headers=headers)
    try:
        with urllib.request.urlopen(req,timeout=20) as response:status,payload=response.status,response.read()
    except urllib.error.HTTPError as error:status,payload=error.code,error.read()
    if raw:return status,payload
    value=json.loads(payload);trace.append({'path':path,'status':status,'response':value});return value

def call(name,args):
    response=http('/v1/tools/'+name,args);assert response['error'] is None,response;return response['data']
def wait(job):
    seen={job['state']};deadline=time.monotonic()+60
    while job['state'] not in ['succeeded','failed','cancelled']:
        assert time.monotonic()<deadline,job
        time.sleep(.03);job=call('opencut_job_view',{'job_id':job['job_id']});seen.add(job['state']);private_free(job)
    return job,sorted(seen)
def start():
    global host,token
    host=subprocess.Popen([str(BIN),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token_file)],stdout=logs,stderr=logs)
    deadline=time.monotonic()+15
    while True:
        assert host.poll() is None,'Host exited during startup'
        try:
            token=token_file.read_text().strip();call('opencut_capabilities',{});return
        except (FileNotFoundError,urllib.error.URLError):
            assert time.monotonic()<deadline;time.sleep(.05)
def stop():
    if host and host.poll() is None:host.send_signal(signal.SIGINT);host.wait(timeout=15)
def t(n,b=10):return {'ticks':str(n),'timebase':b}
def span(s,d):return {'start':t(s),'duration':t(d)}
try:
    if arguments.binary is None:
        command(['cargo','build','-p','opencut-cli'],cwd=ROOT)
    video=run/'private-source.mp4'
    command(['ffmpeg','-v','error','-f','lavfi','-i','color=red:s=160x90:r=10:d=1','-f','lavfi','-i','sine=frequency=440:sample_rate=48000:duration=1','-c:v','libx264','-pix_fmt','yuv420p','-c:a','aac',video])
    seeds=[]
    for i in range(3):
        seed=cli('opencut_job_submit',{'kind':'media.probe','idempotency_key':f'seed-{i}','input':{'input':str(video)}})
        assert seed['state']=='queued';private_free(seed)
        cancelled=cli('opencut_job_request_cancel',{'job_id':seed['job_id']});assert cancelled['state']=='cancelled';private_free(cancelled);seeds.append(seed['job_id'])
    first=cli('opencut_job_query',{'limit':1});assert len(first['jobs'])==1 and first['next_cursor'];private_free(first)
    checks.append('CLI public submission and queued cancellation disclose no prepared input or lease')
    with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
    base=f'http://127.0.0.1:{port}';start()
    created=call('opencut_project_create',{'project':{'id':'public-jobs','name':'Public Jobs','sequence_id':'main','width':160,'height':90,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'project'})
    uploaded=http('/v1/media/upload?name=source.mp4',video.read_bytes());assert uploaded['error'] is None,uploaded
    asset=uploaded['data']['asset']
    track={'id':'v','name':'Video','kind':'video','enabled':True,'muted':False,'locked':False,'clips':[]}
    clip={'id':'clip','asset_id':asset['id'],'timeline_range':span(0,10),'source_range':span(0,10),'params':{},'channels':[]}
    edited=call('opencut_edit_apply',{'project_id':'public-jobs','request':{'expected_revision':created['revision'],'idempotency_key':'bind','label':'bind','operations':[
        {'type':'asset.bind','asset':asset},{'type':'track.create','sequence_id':'main','track':track},{'type':'clip.insert','sequence_id':'main','track_id':'v','clip':clip},
        {'type':'caption.insert','sequence_id':'main','cue':{'id':'cue','timeline_range':span(0,10),'text':'PUBLIC','source_anchor':None,'style':{'font_family':'Arial','font_size':14,'color':'white','background_color':None}}}
    ]}})
    fixed={'project_id':'public-jobs','revision':edited['revision'],'sequence_id':'main'}
    requests=[
        ('media.probe',{'input':str(video)}),
        ('preview.frame',{'input':str(video),'requested_time':t(2),'alignment':'exact'}),
        ('preview.project_frame',dict(fixed,requested_time=t(2),alignment='exact')),
        ('preview.composited_frame',dict(fixed,requested_time=t(2),alignment='exact')),
        ('preview.project_range',dict(fixed,requested_range=span(2,5),alignment='exact')),
        ('render.export',{'segments':[{'input':str(video),'source_start':t(0),'source_duration':t(10)}],'container':'mp4','video':{'width':160,'height':90,'frame_rate':{'numerator':10,'denominator':1},'aspect_mode':'fit','codec':'h264'},'audio':{'mode':'preserve','gain_db':0}}),
        ('render.project',dict(fixed,container='mp4',video_codec='h264')),
        ('artifact.verify',{'input':str(video),'expected_width':160,'expected_height':90}),
    ]
    outputs=[];observed={}
    for index,(kind,request) in enumerate(requests):
        body={'kind':kind,'idempotency_key':f'public-{index}','input':request}
        accepted=call('opencut_job_submit',body);private_free(accepted)
        if kind in ('preview.project_frame','preview.composited_frame','preview.project_range','render.project'):assert accepted['target']==fixed,accepted
        else:assert accepted['target'] is None,accepted
        job,states=wait(accepted);assert job['state']=='succeeded' and job['result'] and job['result_unavailable'] is None,job
        assert job['result']['kind']==kind;private_free(job);outputs.append(job);observed[kind]=states
        assert call('opencut_job_submit',body)['job_id']==job['job_id']
        legacy=call('opencut_job_start',body);assert legacy['job_id']==job['job_id'] and 'input' in legacy
        assert cli('opencut_job_view',{'job_id':job['job_id']})==job
        for ref in job['artifacts']:
            assert ref['availability']=='unchecked' and str(int(ref['size_bytes']))==ref['size_bytes']
            uri='/v1/artifacts/'+urllib.parse.quote(ref['artifact_id'],safe='')
            status,data=http(uri,raw=True);assert status==200 and hashlib.sha256(data).hexdigest()==ref['content_hash']
            target=run/f'{index}.media';target.write_bytes(data)
            decoded=command(['ffmpeg','-v','error','-i',target,'-frames:v','1','-vf','crop=2:2:0:0,scale=1:1','-pix_fmt','rgb24','-f','rawvideo','-'])
            assert decoded[0]>200 and decoded[2]<20,list(decoded)
    checks.append('all eight real worker result kinds preserve typed media/time/verification data and hide host paths; CLI agrees')
    assert any(o['result']['data'].get('text_layouts') for o in outputs),outputs
    checks.append('real font layout reports preserve shaping facts while omitting font source paths')
    checks.append('new submit and legacy start share stable idempotency identity without duplicate tasks')
    checks.append('six content-bound artifact downloads verify SHA-256 and independently decode expected red media')
    page=first;member_ids=[]
    while True:
        private_free(page);member_ids.extend(j['job_id'] for j in page['jobs'])
        if not page['next_cursor']:break
        page=call('opencut_job_query',{'limit':1,'cursor':page['next_cursor']})
    assert member_ids==seeds,member_ids
    checks.append('membership-frozen pagination excludes eight jobs created after the first page without skipping original jobs')
    paginated=call('opencut_job_changes',{'job_id':outputs[0]['job_id'],'limit':1});events=[]
    while paginated['events']:
        private_free(paginated);events.extend(paginated['events']);cursor=paginated['next_cursor']
        paginated=call('opencut_job_changes',{'job_id':outputs[0]['job_id'],'limit':1,'cursor':cursor})
    assert len({e['event_id'] for e in events})==len(events)
    assert {'queued','started','succeeded'}<={e['event_type'] for e in events},events
    bad=http('/v1/tools/opencut_job_changes',{'cursor':cursor,'job_id':outputs[1]['job_id']});assert bad['error']['code']=='INVALID_CURSOR',bad
    foreign=cli('opencut_job_changes',{'cursor':cursor},database=run/'foreign.sqlite',ok=False);assert foreign['error']['code']=='INVALID_CURSOR',foreign
    foreign=cli('opencut_job_query',{'cursor':first['next_cursor']},database=run/'foreign.sqlite',ok=False);assert foreign['error']['code']=='CURSOR_EXPIRED',foreign
    all_cursor=call('opencut_job_changes',{'limit':1})['next_cursor']
    foreign=cli('opencut_job_changes',{'cursor':all_cursor},database=run/'foreign.sqlite',ok=False);assert foreign['error']['code']=='CURSOR_EXPIRED',foreign
    checks.append('durable public changes include started/progress/terminal events without raw payload; wrong filter and foreign page cursors reject')
    stop();start()
    resumed=call('opencut_job_changes',{'job_id':outputs[0]['job_id'],'cursor':cursor});assert resumed['events']==[] and resumed['next_cursor']==cursor
    checks.append('real service restart preserves cursor anchors and replay position')
    failed,_=wait(call('opencut_job_submit',{'kind':'media.probe','idempotency_key':'private-failure','input':{'input':str(run/'private-secret-missing.mp4')}}))
    assert failed['state']=='failed' and failed['failure'];private_free(failed)
    raw=cli('opencut_job_get',{'job_id':failed['job_id']});assert str(run) in json.dumps(raw['error']),raw
    checks.append('actual FFmpeg failure preserves actionable public classification without exposing path-bearing diagnostic text')
    output=outputs[6];ref=output['artifacts'][0];uri='/v1/artifacts/'+urllib.parse.quote(ref['artifact_id'],safe='')
    status,_=http(uri,raw=True,auth=False);assert status==403
    forged=ref['artifact_id'][:-64]+('0'*64)
    status,_=http('/v1/artifacts/'+urllib.parse.quote(forged,safe=''),raw=True);assert status==404
    raw=cli('opencut_job_get',{'job_id':output['job_id']});path=Path(raw['result']['artifact']['path']);original=path.read_bytes()
    modified=bytearray(original);modified[len(modified)//2]^=1;path.write_bytes(modified)
    failure=http(uri);assert failure['error']['code']=='ARTIFACT_VERIFICATION_FAILED',failure
    assert call('opencut_job_view',{'job_id':output['job_id']})==output
    path.write_bytes(original);path.rename(path.with_suffix('.hidden'))
    status,_=http(uri,raw=True);assert status==404
    assert call('opencut_job_view',{'job_id':output['job_id']})==output
    path.with_suffix('.hidden').rename(path)
    checks.append('artifact auth, wrong hash, same-size corruption and missing bytes reject while historical successful JobView remains unchanged')
    (run/'public-jobs.json').write_text(json.dumps(outputs,indent=2))
    sdk=ROOT/'packages/sdk-ts'
    command(['npm','run','build'],cwd=sdk)
    package=json.loads(command(['npm','pack','--ignore-scripts','--json','--pack-destination',run],cwd=sdk))[0]['filename']
    (run/'package.json').write_text(json.dumps({'private':True,'type':'module'}))
    command(['npm','install','--ignore-scripts','--no-audit','--no-fund',run/package],cwd=run)
    (run/'consumer.mjs').write_text((sdk/'tests/public-job-consumer.mjs').read_text())
    (run/'sdk.log').write_bytes(command(['node',run/'consumer.mjs',base,token_file,video,run/'public-jobs.json'],cwd=run))
    checks.append('independently installed TS SDK uses public job helpers, precise result narrowing, authenticated artifact streams and structured download errors')
    (run/'report.json').write_text(json.dumps({'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest(),'observed_states':observed,'artifact':ref,'jobs':[o['job_id'] for o in outputs]},indent=2))
    print(json.dumps({'status':'passed','checks':len(checks),'report':str(run/'report.json')},indent=2))
except Exception as error:
    (run/'failure.txt').write_text(repr(error)+'\n'+str(getattr(error,'stderr',b'')));raise
finally:
    stop();logs.close();(run/'trace.json').write_text(json.dumps(trace,indent=2))
