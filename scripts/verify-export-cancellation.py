#!/usr/bin/env python3
"""Owned retained-workspace cancellation windows; no media copy or clock/DB edits.

Run queue/render/before-publication with the detached service stopped, so the
single captured manual worker is the only owner. Instrumented ffprobe delays
only an already completed real output probe, records that runtime variation,
and never substitutes its bytes or weakens a product guard.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time
from acceptance import Harness


def verify(a):
    h=Harness(a.binary,a.workspace,'agent-export-cancellation-v4')
    spec=importlib.util.spec_from_file_location('worker_processes',Path(__file__).with_name('verify-agent-worker-recovery.py'));mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
    p=subprocess.run([str(h.binary),'--json','--database',str(h.workspace/'workspace.sqlite'),'server','--directory',str(h.workspace/'service'),'status'],capture_output=True,text=True)
    status=json.loads(p.stdout);assert status['error'] is None and status['data']['state']=='stopped' and not status['data']['ready'],'Stop only the idle owned service before exclusive worker acceptance'
    name=a.phase+('-pipe-v1' if a.phase=='before-publication' else '')
    project='agent-real-interview-v4-acceptance' if name=='render' else 'agent-sound-catalog-v4-acceptance'
    before=h.inspect(project)
    original=h.state['requests'].get(name)
    request=original['input'] if original else {'kind':'video','request':{'project_id':project,'revision':before['revision'],'sequence_id':before['project']['default_sequence_id'],'idempotency_key':h.key(name),'options':{'format':'mp4','quality':'high','include_audio':True}}}
    accepted=h.stage(name+'-accepted',lambda:h.request(name,'opencut_render_submit',request));job_id=accepted['job_id'];view=lambda:h.call('opencut_job_view',{'job_id':job_id})
    def private():
        with sqlite3.connect(f'file:{h.workspace/"workspace.sqlite"}?mode=ro',uri=True) as db:r=db.execute('select state,attempt,progress_json,result_json,cancel_requested from jobs where job_id=?',(job_id,)).fetchone()
        return {'state':r[0],'attempt':r[1],'progress':json.loads(r[2]) if r[2] else None,'has_result':r[3] is not None,'cancel_requested':bool(r[4])}
    def cancel():
        v=h.call('opencut_job_request_cancel',{'job_id':job_id});assert v['cancel_requested'] or v['state']=='cancelled';return {'job_id':job_id,'at':private()}
    if name=='queue':
        def queued_cancel():
            prior=private();assert prior['state']=='queued' and prior['attempt']==0 and not prior['has_result'];result=cancel();assert view()['state']=='cancelled';return {'before':prior,'request':result}
        h.stage(name+'-cancel-requested',queued_cancel)
    else:
        env=dict(os.environ);gate=None;marker=None
        if a.phase=='before-publication':
            directory=h.workspace/('f52-probe-'+job_id);directory.mkdir(exist_ok=True)
            probe=Path(shutil.which('ffprobe')).resolve();marker=directory/'probe-completed.fifo';gate=directory/'release';wrapper=directory/'ffprobe'
            if not marker.exists():os.mkfifo(marker,0o600)
            pipe_fd=os.open(marker,os.O_RDONLY|os.O_NONBLOCK);pipe_bytes=b''
            script='''#!/usr/bin/env python3
import json,pathlib,subprocess,sys,time,hashlib,os
real=REAL_PROBE
marker=pathlib.Path(MARKER)
gate=pathlib.Path(GATE)
p=subprocess.run([real,*sys.argv[1:]],capture_output=True)
if p.returncode==0 and any(JOB_ID in v and v.endswith('video.mp4') for v in sys.argv[1:]):
    output=pathlib.Path(sys.argv[-1]);metadata=json.loads(p.stdout)
    record={'real_probe_succeeded':True,'output':str(output),'bytes':output.stat().st_size,'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'streams':[{'codec_type':s['codec_type'],'codec_name':s['codec_name']} for s in metadata['streams']]}
    fd=os.open(marker,os.O_WRONLY|os.O_NONBLOCK)
    try:os.write(fd,(json.dumps(record)+chr(10)).encode())
    finally:os.close(fd)
    deadline=time.monotonic()+15
    while not gate.exists() and time.monotonic()<deadline:time.sleep(.01)
sys.stdout.buffer.write(p.stdout);sys.stderr.buffer.write(p.stderr);raise SystemExit(p.returncode)
'''
            for key,value in [('REAL_PROBE',str(probe)),('MARKER',str(marker)),('GATE',str(gate)),('JOB_ID',job_id)]:script=script.replace(key,repr(value))
            if wrapper.exists():assert wrapper.read_text()==script
            else:wrapper.write_text(script);wrapper.chmod(0o700)
            env['PATH']=str(directory)+os.pathsep+env['PATH']
            h.state.setdefault('probe_runtimes',{}).setdefault(name,{'scope':'Fault-instrumented output-probe completion delay only; not a production output identity','wrapper':str(wrapper),'wrapper_sha256':hashlib.sha256(wrapper.read_bytes()).hexdigest(),'real_ffprobe':str(probe),'real_ffprobe_sha256':hashlib.sha256(probe.read_bytes()).hexdigest()});h.save()
        phase=h.state.setdefault('workers',{}).get(name);child=None
        if not phase:
            assert private()['state']=='queued'
            with (h.workspace/('f52-'+name+'-worker.log')).open('ab') as log:child=subprocess.Popen([str(h.binary),'--database',str(h.workspace/'workspace.sqlite'),'worker','run','--once'],env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log,start_new_session=True)
            rows=mod.processes();phase={'worker':rows[child.pid],'observed_children':[]};h.state['workers'][name]=phase;h.save()
        if name+'-cancel-requested' not in h.state['stages']:
            deadline=time.monotonic()+120
            while True:
                info=private();rows=mod.processes();assert mod.alive(phase['worker'],rows),{'worker_exited_before_window':info}
                children=mod.descendants(rows,phase['worker']['pid']);known={v['pid'] for v in phase['observed_children']};new=[v for v in children if v['pid'] not in known]
                if new:phase['observed_children']+=new;h.save()
                progress=info['progress'];seen=name=='render' and progress and progress['stage']=='encoding' and progress['fraction'] is not None and 0<progress['fraction']<1
                if a.phase=='before-publication':
                    try:pipe_bytes+=os.read(pipe_fd,8192)
                    except BlockingIOError:pass
                    if b'\n' in pipe_bytes:
                        proof=json.loads(pipe_bytes.split(b'\n',1)[0]);assert proof['real_probe_succeeded'] and proof['bytes']>0 and {s['codec_type'] for s in proof['streams']}=={'video','audio'};phase['tentative_output']=proof;h.save();seen=True
                if seen:
                    assert info['state']=='running' and not info['has_result'] and not view()['artifacts'];phase['observed_window']=info;h.save();h.stage(name+'-cancel-requested',cancel)
                    if gate:gate.write_text('release after durable cancellation\n')
                    break
                assert info['state'] not in ['failed','cancelled','succeeded'],{'missed_window':info,'note':'Retain original job; do not create a replacement'}
                assert time.monotonic()<deadline,{'observation_timeout':job_id,'worker':phase['worker'],'note':'Keep original worker/job and inspect current state'}
                time.sleep(.03)
        def completed():
            deadline=time.monotonic()+30
            while True:
                v=view();rows=mod.processes();live=[x for x in [phase['worker'],*phase['observed_children']] if mod.alive(x,rows)]
                if v['state']=='cancelled' and not live:break
                assert v['state'] not in ['failed','succeeded'],v
                assert time.monotonic()<deadline,{'job_state':v['state'],'live_owned_pids':[x['pid'] for x in live]};time.sleep(.1)
            if child:assert child.wait(timeout=1)==0
            return {'terminal_state':v['state'],'owned_processes_terminal':True,'observed_processes':len(phase['observed_children'])+1,'window':phase['observed_window']}
        h.stage(name+'-worker-completed',completed)
    final=view();assert final['state']=='cancelled' and not final['artifacts'] and final['extended_result'] is None and final['failure'] is None
    assert not private()['has_result'] and h.inspect(project)==before
    assert h.call('opencut_render_submit',request)['job_id']==job_id
    final2=view();assert final2['state']=='cancelled' and not final2['artifacts']
    result={'status':'passed_cancelled_without_publication','binary_sha256':h.candidate,'phase':a.phase,'episode':name,'job_id':job_id,'request':request,'project_unchanged':True,'original_replay_same_cancelled_job':True,'public_artifacts':0,'private_result_absent':True,'observed_window':h.state.get('workers',{}).get(name,{}).get('observed_window'),'tentative_output':h.state.get('workers',{}).get(name,{}).get('tentative_output'),'probe_runtime':h.state.get('probe_runtimes',{}).get(name) if a.phase=='before-publication' else None,'scope':'One real public cancellation in the declared phase. No forced process kill, database/clock mutation or replacement task. Output-probe variant is explicitly recorded; no normal-runtime output equivalence claim.'}
    if a.phase=='before-publication':os.close(pipe_fd)
    a.report.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'phase':name,'job_id':job_id,'status':result['status']}))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['binary','workspace','report']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--phase',choices=['queue','render','before-publication'],required=True);verify(p.parse_args())
