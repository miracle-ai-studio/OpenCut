#!/usr/bin/env python3
"""Real CLI/HTTP/MCP plugin installation, WASM invocation and failure recovery."""
import argparse, hashlib, http.client, json, shutil, socket, sqlite3, subprocess, tempfile, time
from pathlib import Path

def fixture(package,component):
    package.mkdir(parents=True);shutil.copyfile(component,package/'planner.wasm')
    (package/'input.json').write_text(json.dumps({'type':'object'}));(package/'output.json').write_text(json.dumps({'type':'array'}))
    permissions=['project.snapshot.read','edit.plan.propose']
    manifest={'manifest_version':1,'id':'org.example.publication','version':'1.0.0','display_name':'Publication acceptance','plugin_api':'>=1.0.0, <2.0.0','runtime':{'kind':'wasm-component','entry':'planner.wasm','world':'opencut:planner/planner@1.0.0'},'targets':['native'],'permissions':permissions,'contributes':{'operations':[{'name':'org.example.publication.plan','version':1,'kind':'job','input_schema':'input.json','output_schema':'output.json'}]},'limits':{'memory_mib':32,'wall_time_ms':2000,'output_bytes':65536,'fuel':10000000}}
    (package/'manifest.json').write_text(json.dumps(manifest));return manifest,permissions

def digest(package):
    h=hashlib.sha256()
    for path in sorted(p for p in package.rglob('*') if p.is_file() and p.relative_to(package).as_posix() not in ['.opencut-grants.json','.opencut-grants.json.tmp']):
        h.update(path.relative_to(package).as_posix().encode());h.update(b'\0');h.update(path.read_bytes())
    return h.hexdigest()

def verify(binary,component,root,mcp_sdk_root=None):
    root.mkdir(parents=True,exist_ok=True);db=root/'workspace.sqlite';token=root/'token';host=None;trace=[];checks=[]
    manifest,permissions=fixture(root/'package',component);package=root/'package';identity=digest(package)
    obj={'kind':'plugin_package','plugin_id':manifest['id'],'version':manifest['version'],'package_sha256':identity}
    grants={**obj,'kind':'plugin_grants'};install={'package_root':str(package),'granted_permissions':permissions}
    def cli(name,args,error=None,credential=None):
        command=[str(binary),'--database',str(db),'--json']+(['--credential-file',str(credential)] if credential else [])+['call',name,'--input','-']
        p=subprocess.run(command,input=json.dumps(args),capture_output=True,text=True,timeout=30);out=json.loads(p.stdout);trace.append({'cli':name,'response':out,'exit':p.returncode})
        if error:assert p.returncode and out['error']['code']==error,(p.stderr,out)
        else:assert p.returncode==0 and out['error'] is None,(p.stderr,out)
        return out
    def http_call(name,args,error=None):
        c=http.client.HTTPConnection('127.0.0.1',port,timeout=30);c.request('POST','/v1/tools/'+name,json.dumps(args),{'Authorization':'Bearer '+token.read_text().strip(),'Content-Type':'application/json'});r=c.getresponse();out=json.loads(r.read());c.close();trace.append({'http':name,'response':out,'status':r.status})
        if error:assert out['error']['code']==error and r.status>=400,out
        else:assert r.status==200 and out['error'] is None,out
        return out
    def query(target=None,**kwargs):return cli('opencut_plugin_publication_query',{'target':target or {'kind':'workspace'},**kwargs})['data']
    def sql(statement,args=()):
        with sqlite3.connect(db) as c:return c.execute(statement,args).fetchall()
    def count():return [sql('SELECT count(*) FROM '+t)[0][0] for t in ['plugin_events','plugin_publications']]
    def reconcile(object):return cli('opencut_plugin_publication_reconcile',{'object':object})
    def request(out):return query({'kind':'request','id':out['request_id']})['events']
    def trigger(phase,kind=None):
        predicate="json_extract(NEW.event_json,'$.phase')='"+phase+"'"
        if kind:predicate+=" AND json_extract(NEW.event_json,'$.intent.object.kind')='"+kind+"'"
        sql("CREATE TRIGGER reject_publication BEFORE INSERT ON plugin_events WHEN "+predicate+" BEGIN SELECT RAISE(ABORT,'publication QA failure'); END")
    try:
        assert query()['events']==[]
        with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
        with (root/'host.log').open('w') as log:host=subprocess.Popen([str(binary),'--database',str(db),'serve','--listen',f'127.0.0.1:{port}','--token-file',str(token)],stdout=log,stderr=log)
        deadline=time.monotonic()+15
        while True:
            assert host.poll() is None
            try:
                with socket.create_connection(('127.0.0.1',port),timeout=.2):break
            except OSError:assert time.monotonic()<deadline;time.sleep(.03)
        trigger('prepared');before=count();cli('opencut_plugin_install',install,'STORAGE_ERROR');assert count()==before;assert cli('opencut_plugin_list',{})['data']==[];sql('DROP TRIGGER reject_publication')
        checks.append('Prepare persistence failure prevents package publication and rolls back both journal tables')
        installed=http_call('opencut_plugin_install',install);path=Path(installed['data']['install_path']);assert digest(path)==identity==installed['data']['package_sha256']
        events=request(installed);assert [e['phase'] for e in events]==['prepared','completed','prepared','completed'];assert all(e['actor']['source']=='http' and e['origin_request_id']==installed['request_id'] for e in events)
        assert events[-1]['intent']['after_hash']==hashlib.sha256((path/'.opencut-grants.json').read_bytes()).hexdigest()
        before=count();cli('opencut_plugin_install',install);assert count()==before
        project=cli('opencut_project_create',{'project':{'id':'scope','name':'scope','sequence_id':'main','width':64,'height':48,'fps':{'numerator':10,'denominator':1}},'idempotency_key':'create'})['data']
        plan={'id':manifest['id'],'version':manifest['version'],'operation_name':'org.example.publication.plan','project_id':'scope','revision':project['revision'],'parameters':{'track_id':'generated'}}
        assert cli('opencut_plugin_plan',plan)['data']['operations'][0]['track']['id']=='generated'
        revoked=cli('opencut_plugin_install',{**install,'granted_permissions':[]});assert len(request(revoked))==2;assert digest(path)==identity
        cli('opencut_plugin_plan',plan,'PERMISSION_DENIED');assert cli('opencut_project_inspect',{'project_id':'scope'})['data']==project
        cli('opencut_plugin_install',install);assert cli('opencut_plugin_plan',plan)['data']['operations'][0]['track']['id']=='generated'
        checks.append('HTTP install records package/grants under actual request identity; independent fingerprints, no-op retry, real WASM plan, grant revocation and restoration all agree')
        trigger('prepared','plugin_grants');before=count();cli('opencut_plugin_install',{**install,'granted_permissions':[]},'STORAGE_ERROR');assert count()==before;assert set(json.loads((path/'.opencut-grants.json').read_text()))==set(permissions);sql('DROP TRIGGER reject_publication')
        trigger('completed','plugin_grants');unknown=cli('opencut_plugin_install',{**install,'granted_permissions':[]},'PUBLICATION_OUTCOME_UNKNOWN');assert json.loads((path/'.opencut-grants.json').read_text())==[]
        publication=unknown['error']['details']['publication_id'];assert unknown['error']['details']['next_operation']=='opencut_plugin_publication_reconcile';assert sql('SELECT pending FROM plugin_publications WHERE publication_id=?',(publication,))==[(1,)]
        sql('DROP TRIGGER reject_publication');recovered=reconcile(grants);assert recovered['data']['resolved_publication_ids']==[publication]
        events=query({'kind':'publication','id':publication})['events'];assert events[-1]['phase']=='observed_applied';assert events[-1]['origin_request_id']==unknown['request_id'] and events[-1]['request_id']==recovered['request_id'];assert reconcile(grants)['data']['resolved_publication_ids']==[]
        checks.append('Grant prepare failure preserves permissions; completion failure exposes unknown outcome and actual revocation, with repeatable locked reconciliation and origin linkage')
        trigger('completed','plugin_grants');unknown=cli('opencut_plugin_install',install,'PUBLICATION_OUTCOME_UNKNOWN');publication=unknown['error']['details']['publication_id'];sql('DROP TRIGGER reject_publication')
        (path/'.opencut-grants.json').write_text('["project.snapshot.read"]');actual=hashlib.sha256((path/'.opencut-grants.json').read_bytes()).hexdigest();assert reconcile(grants)['data']['observed_hash']==actual;assert query({'kind':'publication','id':publication})['events'][-1]['phase']=='observed_diverged';assert json.loads((path/'.opencut-grants.json').read_text())==['project.snapshot.read']
        cli('opencut_plugin_install',install)
        original=(path/'planner.wasm').read_bytes();(path/'planner.wasm').write_bytes(original+b'bad');before=count();cli('opencut_plugin_install',install,'PLUGIN_INTEGRITY');assert count()==before and (path/'planner.wasm').read_bytes()==original+b'bad';(path/'planner.wasm').write_bytes(original)
        checks.append('Externally changed grants reconcile as diverged without overwrite; corrupt installed bytes cannot be silently reused or replaced')
        manifest['version']='2.0.0';(package/'manifest.json').write_text(json.dumps(manifest));new_hash=digest(package);new_obj={**obj,'version':'2.0.0','package_sha256':new_hash}
        trigger('completed','plugin_package');unknown=http_call('opencut_plugin_install',install,'PUBLICATION_OUTCOME_UNKNOWN');sql('DROP TRIGGER reject_publication');publication=unknown['error']['details']['publication_id']
        new_path=path.parent.parent/'2.0.0'/new_hash;assert digest(new_path)==new_hash and not (new_path/'.opencut-grants.json').exists();cli('opencut_plugin_inspect',{'id':manifest['id'],'version':'2.0.0'},'PLUGIN_INSTALL_INCOMPLETE');assert len(cli('opencut_plugin_list',{})['data'])==1
        assert reconcile(new_obj)['data']['resolved_publication_ids']==[publication];assert query({'kind':'publication','id':publication})['events'][-1]['phase']=='observed_applied';cli('opencut_plugin_install',install);assert len(cli('opencut_plugin_list',{})['data'])==2
        checks.append('Package completion failure leaves an explicit incomplete, non-discoverable installation; observed reconciliation plus install retry finishes grants without republishing bytes')
        assert cli('opencut_maintenance_query',{'target':{'kind':'workspace'}})['data']['events']==[]
        complete=query()['events'];cursor='0';paged=[]
        while True:
            page=query(after_sequence=cursor,limit=1);paged+=page['events'];cursor=page['last_sequence']
            if not page['has_more']:break
        assert paged==complete and query()['coverage']=='recorded_plugin_publications_since_schema7'
        cli('opencut_plugin_publication_query',{'target':{'kind':'workspace'},'after_sequence':'01'},'INVALID_ARGUMENT');cli('opencut_plugin_publication_reconcile',{'object':{**obj,'plugin_id':'../escape'}},'INVALID_ARGUMENT')
        for args,payload in [(['access','set-policy','--input','-'],{'principal_id':'reader','expected_revision':'0','enabled':True,'grants':[{'project_id':'scope','actions':['read','read_audit']}]}),(['access','issue','reader','--output',str(root/'reader.secret')],None)]:
            p=subprocess.run([str(binary),'--database',str(db),'--json']+args,input=json.dumps(payload) if payload else None,text=True,capture_output=True,timeout=30);assert p.returncode==0,p.stdout
        for name,args in [('opencut_plugin_publication_query',{'target':{'kind':'workspace'}}),('opencut_plugin_publication_reconcile',{'object':obj}),('opencut_plugin_install',install)]:cli(name,args,'NOT_AUTHORIZED',root/'reader.secret')
        checks.append('Plugin and media histories stay isolated; decimal pagination is lossless; malformed object identities and restricted callers fail closed')
        if mcp_sdk_root:
            expected=root/'expected-events.json';expected.write_text(json.dumps(complete))
            p=subprocess.run(['node',str(Path(__file__).with_name('verify-maintenance-mcp.mjs')),str(mcp_sdk_root),str(binary),str(db),f'http://127.0.0.1:{port}',str(token),str(root/'reader.secret'),json.dumps(grants),str(expected),'plugin'],capture_output=True,text=True,timeout=60)
            (root/'mcp.stdout').write_text(p.stdout);(root/'mcp.stderr').write_text(p.stderr);assert p.returncode==0,(p.stdout,p.stderr)
            checks.append('Official MCP SDK reads identical plugin history and owner reconciliation on stdio/HTTP; restricted credentials are denied on both')
        host.terminate();host.wait(timeout=10);host=None;assert query()['events']==complete
        checks.append('Host shutdown and standalone reopen preserve every publication transition')
        report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'component_sha256':hashlib.sha256(component.read_bytes()).hexdigest(),'evidence':str(root)}
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
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,default=Path('target/debug/opencut'));p.add_argument('--component',type=Path,default=Path(__file__).resolve().parents[1]/'crates/plugin-host/examples/planner-guest/target/wasm32-wasip2/release/opencut_example_planner_guest.wasm');p.add_argument('--artifacts-dir',type=Path);p.add_argument('--mcp-sdk-root',type=Path)
    a=p.parse_args();verify(a.binary.resolve(),a.component.resolve(),(a.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-plugin-publication-'))).resolve(),a.mcp_sdk_root)
