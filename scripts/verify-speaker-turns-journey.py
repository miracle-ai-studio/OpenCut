#!/usr/bin/env python3
"""Exercise fixed-asset tinydiarize through public CLI tool, durable worker and JobView."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


def sha(path):
    digest=hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda:source.read(1024*1024),b''): digest.update(chunk)
    return digest.hexdigest()


APOLLO_30S_SHA256='c73036a6beb0f7b391c09094ccfc298b213030d0cb703b789f719fad9111a08d'
REFERENCE_TURNS_MS=[3800,6200,11320,13820]
REFERENCE_TOLERANCE_MS=50


def verify(binary,audio,model,artifacts):
    audio_sha256=sha(audio)
    assert audio_sha256==APOLLO_30S_SHA256, ('unexpected fixed speaker-turn fixture', audio_sha256)
    run=artifacts.resolve()/f'speaker-turns-{time.time_ns()}'
    run.mkdir(parents=True)
    source=run/'english-voices.wav'
    shutil.copyfile(audio,source)
    db=run/'workspace.sqlite'
    trace=[]
    def call(name,body,error=None):
        proc=subprocess.run([str(binary),'--database',str(db),'--json','call',name,'--input','-'],
            input=json.dumps(body),text=True,capture_output=True,timeout=45)
        response=json.loads(proc.stdout)
        trace.append({'tool':name,'response':response})
        if error:
            assert proc.returncode!=0 and response['error']['code']==error,(response,proc.stderr)
        else:
            assert proc.returncode==0 and response['error'] is None,(response,proc.stderr)
        return response.get('data')
    created=call('opencut_project_create',{'project':{'id':'speaker-fixture','name':'Speaker turns fixture',
        'sequence_id':'main','width':640,'height':360,'fps':{'numerator':25,'denominator':1}},
        'idempotency_key':'speaker-create'})
    bound=call('opencut_edit_apply',{'project_id':'speaker-fixture','request':{
        'expected_revision':created['revision'],'idempotency_key':'speaker-bind','label':'bind English audio',
        'operations':[{'type':'asset.bind','asset':{'id':'voices','name':'Voices','kind':'audio',
            'uri':str(source),'duration':None,'metadata':{}}}]}})
    revision=bound['revision']
    request={'idempotency_key':'speaker-analyze','project_id':'speaker-fixture','revision':revision,
        'asset_id':'voices','language':'en'}
    queued=call('opencut_analysis_speaker_turns',request)
    assert queued['kind']=='analysis.speaker_turns' and queued['state']=='queued',queued
    assert call('opencut_analysis_speaker_turns',request)['job_id']==queued['job_id']
    call('opencut_analysis_speaker_turns',{**request,'model_path':str(model)},error='INVALID_ARGUMENT')
    source.unlink()
    environment=os.environ.copy()
    environment['OPENCUT_WHISPER_TDRZ_MODEL']=str(model)
    environment['OPENCUT_WHISPER_CLI']='/usr/local/bin/whisper-cli'
    worker=subprocess.run([str(binary),'--database',str(db),'worker','run','--once'],
        capture_output=True,text=True,env=environment,timeout=240)
    assert worker.returncode==0,(worker.stderr,worker.stdout)
    done=call('opencut_job_view',{'job_id':queued['job_id']})
    assert done['state']=='succeeded' and done['result'] is None,done
    result=done['extended_result']['data']['analysis']
    assert result['turns'], 'the pinned positive-dialogue fixture must produce at least one speaker-turn boundary'
    assert result['language']=='en' and result['provider']=='whisper.cpp'
    assert result['method']=='tinydiarize_speaker_turn_next'
    assert result['model']=='ggml-small.en-tdrz.bin'
    assert result['model_sha256']==sha(model)
    assert result['confidence_known'] is False and result['speaker_identity_status']=='unavailable'
    assert all(int(turn['at']['ticks'])>0 for turn in result['turns'])
    observed_turns_ms=[int(turn['at']['ticks']) for turn in result['turns']]
    assert all(any(abs(actual-expected)<=REFERENCE_TOLERANCE_MS for actual in observed_turns_ms)
        for expected in REFERENCE_TURNS_MS), (observed_turns_ms,REFERENCE_TURNS_MS)
    assert call('opencut_analysis_speaker_turns',request)['job_id']==queued['job_id']
    evidence=call('opencut_analysis_evidence_query',{'job_id':queued['job_id'],'limit':100})
    assert evidence['analysis_kind']=='analysis.speaker_turns'
    assert evidence['provenance']['model_sha256']==sha(model)
    assert evidence['provenance']['speaker_identity_status']=='unavailable'
    assert all(item['kind']=='speaker_turn' for item in evidence['evidence'])
    assert len(evidence['evidence'])==len(result['turns'])
    report={'binary_sha256':sha(binary),'audio_sha256':audio_sha256,'model_sha256':sha(model),
        'revision':revision,'job_id':queued['job_id'],'result':result,
        'speaker_turn_count':len(result['turns']),
        'reference_evaluation':{'source':'upstream whisper.cpp README a13.wav example output; not an independent human transcript','expected_turns_ms':REFERENCE_TURNS_MS,
            'observed_turns_ms':observed_turns_ms,'tolerance_ms':REFERENCE_TOLERANCE_MS},
        'checks':['public schema rejects host model path','durable fixed-asset job survives source deletion',
            'actual local model inference and public JobView provenance','replay preserves durable receipt','unified evidence query exposes only source turn times and pinned model provenance',
            'turn boundaries only; no speaker identity or confidence fabricated'],
        'positive_turn_detection_proven':bool(result['turns']),'trace':trace}
    path=run/'report.json'
    path.write_text(json.dumps(report,indent=2))
    return path


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--binary',required=True,type=Path)
    parser.add_argument('--audio',required=True,type=Path)
    parser.add_argument('--model',required=True,type=Path)
    parser.add_argument('--artifacts-dir',required=True,type=Path)
    args=parser.parse_args()
    print(verify(args.binary.resolve(),args.audio.resolve(),args.model.resolve(),args.artifacts_dir))
