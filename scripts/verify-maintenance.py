#!/usr/bin/env python3
"""Real HTTP upload, metadata repair and journal failures with independent file checks."""
import argparse, hashlib, http.client, io, json, socket, sqlite3, subprocess, tempfile, time, wave
from pathlib import Path


def verify(binary,root):
    root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';token_file=root/'token';host=None;trace=[];checks=[]
    def cli(name,args,error=None,credential=None):
        command=[str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+['call',name,'--input','-']
        p=subprocess.run(command,input=json.dumps(args),capture_output=True,text=True,timeout=30);out=json.loads(p.stdout);trace.append({'cli':name,'response':out,'exit':p.returncode})
        if error:assert p.returncode!=0 and out['error']['code']==error,(p.stderr,out)
        else:assert p.returncode==0 and out['error'] is None,(p.stderr,out)
        return out
    def journal(target=None,**kwargs):return cli('opencut_maintenance_query',{'target':target or {'kind':'workspace'},**kwargs})['data']
    def sql(statement,args=()):
        with sqlite3.connect(db) as c:return c.execute(statement,args).fetchall()
    def upload(data,error=None):
        c=http.client.HTTPConnection('127.0.0.1',port,timeout=30);c.request('POST','/api/media/upload?name=audio.wav',data,{'Authorization':'Bearer '+token_file.read_text().strip(),'Content-Type':'application/octet-stream'});r=c.getresponse();out=json.loads(r.read());c.close();trace.append({'upload':hashlib.sha256(data).hexdigest(),'response':out,'status':r.status})
        if error:assert r.status==500 and out['error']['code']==error,out
        else:assert r.status==200 and out['error'] is None,out
        return out
    def sample(n):
        f=io.BytesIO()
        with wave.open(f,'wb') as w:w.setnchannels(1);w.setsampwidth(2);w.setframerate(48000);w.writeframes((n.to_bytes(2,'little')+b'\x00\x00')*2400)
        return f.getvalue()
    def reconcile(kind,hash):return cli('opencut_maintenance_reconcile',{'object':{'kind':kind,'content_hash':hash}})
    def by_request(out):return journal({'kind':'request','id':out['request_id']})['events']
    def counts():return [sql('SELECT count(*) FROM '+table)[0][0] for table in ['maintenance_publications','maintenance_events']]
    try:
        assert journal()['events']==[]
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token_file)],stdout=log,stderr=log)
        deadline=time.monotonic()+15
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):break
            except OSError:assert time.monotonic()<deadline;time.sleep(.03)
        data=sample(1);hash=hashlib.sha256(data).hexdigest();first=upload(data);events=by_request(first)
        assert [e['phase'] for e in events]==['prepared','completed','prepared','completed']
        assert [e['intent']['object']['kind'] for e in events]==['media_content']*2+['media_metadata']*2
        assert all(e['actor']=={'principal_id':'local-owner','source':'http','worker_id':None} and e['origin_request_id']==first['request_id'] for e in events)
        assert (root/'media'/hash).read_bytes()==data
        metadata=root/'media'/(hash+'.json');assert hashlib.sha256(metadata.read_bytes()).hexdigest()==events[-1]['intent']['after_hash']
        old=journal()['events'];upload(data);assert journal()['events']==old
        checks.append('Real WAV upload journals content and metadata separately under the response request/actor; bytes and metadata SHA-256 agree; identical reupload is a no-op')
        forged=json.loads(metadata.read_text());forged['kind']='image';forged['metadata']['mime_type']='image/png';metadata.write_text(json.dumps(forged))
        inspected=cli('opencut_media_inspect',{'content_hash':hash})['data']
        repaired=cli('opencut_media_repair',{'content_hash':hash,'expected_metadata_hash':inspected['metadata_hash']})
        assert repaired['data']['asset']['kind']=='audio' and repaired['data']['asset']['metadata']['mime_type']=='audio/wav'
        events=by_request(repaired);assert [e['phase'] for e in events]==['prepared','completed']
        assert events[0]['intent']['before_hash']==inspected['metadata_hash'] and events[1]['intent']['after_hash']==repaired['data']['metadata_hash']
        assert all(e['actor']['source']=='cli' for e in events)
        with wave.open(str(root/'media'/hash),'rb') as w:assert w.getframerate()==48000 and w.readframes(w.getnframes())==(b'\x01\x00\x00\x00')*2400
        checks.append('Real FFprobe metadata repair records before/after identity and CLI provenance; independent PCM decoding proves content preservation')
        before=counts();blocked=sample(2);blocked_hash=hashlib.sha256(blocked).hexdigest()
        sql("CREATE TRIGGER reject_prepare BEFORE INSERT ON maintenance_events WHEN json_extract(NEW.event_json,'$.phase')='prepared' BEGIN SELECT RAISE(ABORT,'prepare journal failure'); END")
        upload(blocked,'STORAGE_ERROR');assert counts()==before and not (root/'media'/blocked_hash).exists()
        sql('DROP TRIGGER reject_prepare')
        checks.append('Prepare-record insertion failure rolls back both journal tables and prevents content publication')
        sql("CREATE TRIGGER reject_complete BEFORE INSERT ON maintenance_events WHEN json_extract(NEW.event_json,'$.phase')='completed' BEGIN SELECT RAISE(ABORT,'completion journal failure'); END")
        uncertain=upload(blocked,'PUBLICATION_OUTCOME_UNKNOWN');publication=uncertain['error']['details']['publication_id']
        assert (root/'media'/blocked_hash).read_bytes()==blocked and not (root/'media'/(blocked_hash+'.json')).exists()
        events=journal({'kind':'publication','id':publication})['events'];assert len(events)==1 and events[0]['phase']=='prepared'
        assert sql('SELECT pending FROM maintenance_publications WHERE publication_id=?',(publication,))==[(1,)]
        sql('DROP TRIGGER reject_complete')
        recovered=reconcile('media_content',blocked_hash);assert recovered['data']['resolved_publication_ids']==[publication]
        events=journal({'kind':'publication','id':publication})['events'];assert events[-1]['phase']=='observed_applied'
        assert events[-1]['origin_request_id']==uncertain['request_id'] and events[-1]['request_id']==recovered['request_id']
        assert reconcile('media_content',blocked_hash)['data']['resolved_publication_ids']==[]
        upload(blocked)
        checks.append('Completion-record failure returns explicit unknown outcome after real rename; locked reconciliation records observed_applied with original and recovery request IDs and is repeatable')
        sql("CREATE TRIGGER reject_metadata_complete BEFORE INSERT ON maintenance_events WHEN json_extract(NEW.event_json,'$.phase')='completed' AND json_extract(NEW.event_json,'$.intent.object.kind')='media_metadata' BEGIN SELECT RAISE(ABORT,'metadata completion failure'); END")
        third=sample(3);third_hash=hashlib.sha256(third).hexdigest();uncertain=upload(third,'PUBLICATION_OUTCOME_UNKNOWN');publication=uncertain['error']['details']['publication_id']
        third_meta=root/'media'/(third_hash+'.json');assert third_meta.exists()
        third_meta.write_text('{"unexpected":"external mutation"}')
        actual=hashlib.sha256(third_meta.read_bytes()).hexdigest();sql('DROP TRIGGER reject_metadata_complete')
        reconciled=reconcile('media_metadata',third_hash);assert reconciled['data']['observed_hash']==actual
        assert journal({'kind':'publication','id':publication})['events'][-1]['phase']=='observed_diverged'
        assert third_meta.read_text()=='{"unexpected":"external mutation"}'
        repaired=cli('opencut_media_repair',{'content_hash':third_hash,'expected_metadata_hash':actual});assert repaired['data']['asset']['kind']=='audio'
        checks.append('Changed on-disk metadata is recorded as observed_diverged without fabricating success or overwriting evidence; explicit repair uses the current fingerprint')
        # Existing bytes must be checked before deduplication can return success.
        target=root/'media'/hash;damaged=bytearray(target.read_bytes());damaged[-1]^=1;target.write_bytes(damaged)
        before=counts();upload(data,'ASSET_INTEGRITY_ERROR');assert target.read_bytes()==damaged and counts()==before
        target.write_bytes(data);upload(data)
        checks.append('Corrupted existing content cannot be silently reused or replaced by a same-hash upload; failed verification leaves both bytes and journal unchanged')
        complete=journal()['events'];after='0';paged=[]
        while True:
            page=journal(after_sequence=after,limit=1);paged+=page['events'];after=page['last_sequence']
            if not page['has_more']:break
        assert paged==complete and journal(after_sequence=after)['events']==[]
        cli('opencut_maintenance_query',{'target':{'kind':'workspace'},'after_sequence':'01'},'INVALID_ARGUMENT')
        cli('opencut_maintenance_reconcile',{'object':{'kind':'media_content','content_hash':'../escape'}},'INVALID_ARGUMENT')
        create={'project':{'id':'scope','name':'scope','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'scope'};cli('opencut_project_create',create)
        for args,payload in [(['access','set-policy','--input','-'],{'principal_id':'reader','expected_revision':'0','enabled':True,'grants':[{'project_id':'scope','actions':['read','read_audit']}]}),(['access','issue','reader','--output',str(root/'reader.secret')],None)]:
            p=subprocess.run([str(binary),'--database',str(db),'--json']+args,input=json.dumps(payload) if payload else None,text=True,capture_output=True,timeout=30);assert p.returncode==0,p.stdout
        cli('opencut_maintenance_query',{'target':{'kind':'workspace'}},'NOT_AUTHORIZED',root/'reader.secret')
        cli('opencut_maintenance_reconcile',{'object':{'kind':'media_metadata','content_hash':hash}},'NOT_AUTHORIZED',root/'reader.secret')
        host.terminate();host.wait(timeout=10);host=None
        assert journal()['events']==complete
        checks.append('Separate decimal cursor pages every transition exactly once; invalid paths and restricted project audit credentials reject; standalone reopen preserves all events')
        report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'evidence':str(root),'scope':'workspace media content/metadata publication, not plugin installation or an atomic SQLite/filesystem transaction'}
        (root/'report.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
    except Exception as e:
        (root/'failure.json').write_text(json.dumps({'error':repr(e),'completed_checks':checks},indent=2));raise
    finally:
        if host and host.poll() is None:
            host.terminate()
            try:host.wait(timeout=8)
            except subprocess.TimeoutExpired:host.kill();host.wait()
        (root/'trace.json').write_text(json.dumps(trace,indent=2))

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));parser.add_argument('--artifacts-dir',type=Path)
    a=parser.parse_args();verify(a.binary.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-maintenance-'))).resolve())
