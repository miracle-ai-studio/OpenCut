#!/usr/bin/env python3
"""Compare a real schema-3 continuous preview with fixed-revision export/frame jobs."""
import argparse
import hashlib
import http.client
import json
from pathlib import Path
import socket
import subprocess
import time


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024*1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def raw_frame(path, at=None):
    command = ['ffmpeg','-v','error']
    if at is not None:
        command += ['-ss',at]
    command += ['-i',str(path),'-frames:v','1','-f','rawvideo','-pix_fmt','rgb24','-']
    data = subprocess.check_output(command,timeout=40)
    assert len(data) == 320*180*3, (path,len(data))
    return data


def pixel(data,x,y):
    offset=(y*320+x)*3
    return tuple(data[offset:offset+3])


def verify(binary, run):
    report=json.loads((run/'report.json').read_text())
    revision=report['revision']
    jobs={item['kind']:item for item in report['jobs']}
    export=run/(jobs['render.project.v3']['job_id']+'.mp4')
    frame=run/(jobs['preview.project_frame.v3']['job_id']+'.png')
    assert export.is_file() and frame.is_file()
    with socket.socket() as bound:
        bound.bind(('127.0.0.1',0))
        port=bound.getsockname()[1]
    token_file=run/'preview-v3-owner.token'
    db=run/'workspace.sqlite'
    with (run/'preview-v3-host.log').open('wb') as log:
        host=subprocess.Popen([str(binary),'--database',str(db),'serve',
            '--listen',f'127.0.0.1:{port}','--token-file',str(token_file)],stdout=log,stderr=log)
        try:
            deadline=time.monotonic()+25
            while not token_file.exists():
                assert host.poll() is None and time.monotonic()<deadline
                time.sleep(.05)
            token=token_file.read_text().strip()
            while True:
                assert host.poll() is None and time.monotonic()<deadline
                try:
                    with socket.create_connection(('127.0.0.1',port),timeout=.2): break
                except OSError: time.sleep(.05)

            def request(path,payload):
                connection=http.client.HTTPConnection('127.0.0.1',port,timeout=45)
                connection.request('POST',path,json.dumps(payload).encode(),
                    {'Authorization':'Bearer '+token,'Content-Type':'application/json'})
                response=connection.getresponse()
                body=response.read()
                status=response.status
                content_type=response.getheader('Content-Type','')
                connection.close()
                return status,content_type,body
            def tool(name,payload):
                status,_,raw=request('/api/tools/'+name,payload)
                value=json.loads(raw)
                assert status==200 and value['error'] is None,(name,status,value)
                return value['data']

            create={'project_id':'visual-film','revision':revision,'sequence_id':'main',
                'start_time':{'ticks':'0','timebase':10},'alignment':'exact',
                'audio_enabled':True,'idempotency_key':'preview-v3-compare'}
            ready=tool('opencut_preview_create',create)
            assert ready['revision']==revision and ready['state']=='ready',ready
            same=tool('opencut_preview_create',create)
            assert same['session_id']==ready['session_id']
            control={key:ready[key] for key in ('session_id','generation','owner_id','lease_id')}
            playing=tool('opencut_preview_control',{**control,'command':{'kind':'play'}})
            stream_input={key:playing[key] for key in ('session_id','generation','owner_id','lease_id')}
            status,mime,body=request('/api/preview/stream',stream_input)
            assert status==200 and mime.startswith('video/mp4'),(status,mime,body[:200])
            preview=run/'preview-v3-stream.mp4'
            preview.write_bytes(body)
            assert b'moof' in body and b'mdat' in body
            subprocess.run(['ffmpeg','-v','error','-i',str(preview),'-f','null','-'],
                check=True,timeout=40)
            source_frames={'export':raw_frame(export,'0.5'),'frame':raw_frame(frame),
                'preview':raw_frame(preview,'0.5')}
            samples={name:{label:pixel(data,*point) for label,point in
                {'left_effect':(30,90),'right_mask':(290,90),'center_shape':(160,90)}.items()}
                for name,data in source_frames.items()}
            for label in ('left_effect','right_mask','center_shape'):
                reference=samples['frame'][label]
                for name in ('export','preview'):
                    current=samples[name][label]
                    assert max(abs(int(a)-int(b)) for a,b in zip(reference,current))<=35, (label,name,reference,current)
            result={'binary_sha256':sha(binary),'revision':revision,
                'session_id':ready['session_id'],'preview_sha256':sha(preview),
                'source_job_report':str(run/'report.json'),'pixels':samples,
                'checks':['schema-3 fixed revision preview idempotency',
                    'continuous fMP4 decodes with video and audio',
                    'preview, frame and full export show matching effect/mask/shape pixels']}
            path=run/'preview-v3-report.json'
            path.write_text(json.dumps(result,indent=2))
            return path
        finally:
            host.terminate()
            host.wait(timeout=12)


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--binary',required=True,type=Path)
    parser.add_argument('--run-dir',required=True,type=Path)
    args=parser.parse_args()
    print(verify(args.binary.resolve(),args.run_dir.resolve()))
