#!/usr/bin/env python3
"""Accept an actual delivery archive outside the repository, including two installed SDKs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import wave

ROOT = Path(__file__).resolve().parents[1]


def unpack(archive, destination):
    with tarfile.open(archive) as tar:
        names = set()
        for member in tar.getmembers():
            path = (destination / member.name).resolve()
            assert path.is_relative_to(destination.resolve()) and member.name not in names
            assert member.isfile() or member.isdir(), member.name
            names.add(member.name)
        tar.extractall(destination, filter='data')


def verify(archive, mcp_sdk_root=None, component=None):
    evidence = Path(tempfile.mkdtemp(prefix='opencut-package-acceptance-')).resolve()
    logs, checks = [], []
    host = None
    host_log = None
    def run(args, cwd=evidence, env=None):
        value = subprocess.run([str(x) for x in args], cwd=cwd, env=env, text=True, capture_output=True, timeout=600)
        logs.append({'command': [str(x) for x in args], 'cwd': str(cwd), 'exit_code': value.returncode, 'stdout': value.stdout, 'stderr': value.stderr})
        assert value.returncode == 0, (args, value.stdout, value.stderr)
        return value.stdout
    try:
        expected = Path(str(archive)+'.sha256').read_text().split()[0]
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected
        unpack(archive, evidence / 'unpacked')
        bundle, = (evidence / 'unpacked').iterdir()
        manifest = json.loads((bundle / 'manifest.json').read_text())
        doctor = json.loads(run(['python3', bundle / 'doctor.py']))
        assert doctor['status'] == 'passed'
        binary = bundle / 'bin/opencut'
        assert hashlib.sha256(binary.read_bytes()).hexdigest() == manifest['binary_sha256']
        for contract in ['project-plugin-contract.md', 'denial-audit-contract.md',
                         'storage-governance-contract.md', 'cas-gc-contract.md',
                         'storage-admission-contract.md', 'native-child-write-contract.md']:
            assert (bundle / contract).is_file(), f'Missing packaged contract: {contract}'
        schema = json.loads(run([binary, '--json', 'system', 'schema']))['data']
        assert schema == json.loads((bundle / 'contracts/tools.generated.json').read_text())
        domains = json.loads(run([binary, '--json', 'system', 'domains']))['data']
        assert manifest['operation_count'] == len(schema) and manifest['domain_count'] == len(domains)
        paths = json.loads((bundle / 'contracts/openapi.generated.json').read_text())['paths']
        expected_paths = {*(f"/v1/tools/{tool['name']}" for tool in schema),
                          *(f"/v1/tools/opencut_{domain['domain']}" for domain in domains),
                          '/v1/domains', '/v1/domains/{domain}',
                          '/v1/uploads/{upload_id}/chunks/{offset}',
                          '/v1/projects/{project}/media/{media_id}/content',
                          '/v1/artifacts/{artifact}', '/v1/changes/stream'}
        assert set(paths) == expected_paths
        assert paths['/v1/uploads/{upload_id}/chunks/{offset}']['put']['operationId'] == 'opencut_upload_chunk'
        assert paths['/v1/projects/{project}/media/{media_id}/content']['get']['operationId'] == 'opencut_project_media_content'
        assert paths['/v1/artifacts/{artifact}']['get']['operationId'] == 'opencut_artifact_download'
        assert paths['/v1/changes/stream']['post']['responses']['200']['content']['text/event-stream']['x-opencut-data-schema'] == {'$ref': '#/components/schemas/OpenCutEventStreamRecord'}
        capabilities = json.loads(run([binary, '--json', 'system', 'capabilities']))['data']
        if 'streamable-http' in capabilities.get('mcp', {}).get('transports', []):
            http_report = json.loads(run(['python3', ROOT / 'scripts/verify-mcp-http.py', '--binary', binary,
                '--artifacts-dir', evidence / 'mcp-http']))
            assert http_report['status'] == 'passed'
            checks.append('Delivered Streamable HTTP lifecycle, authenticated sessions and bounded replay')
            if mcp_sdk_root:
                interop = json.loads(run(['node', ROOT / 'scripts/verify-mcp-client.mjs',
                    '--binary', binary, '--sdk-root', mcp_sdk_root]))
                assert interop['status'] == 'passed'
                (evidence / 'mcp-sdk-interop.json').write_text(json.dumps(interop, indent=2))
                checks.append('Official MCP SDK independently discovers calls subscribes resumes and terminates the delivered host session')
        # A corrupted delivered file must be detected before a caller trusts this directory.
        sample = bundle / 'contracts/edit-journey.json'
        original = sample.read_bytes()
        try:
            sample.write_bytes(original + b' ')
            rejected = subprocess.run(['python3', str(bundle / 'doctor.py')], text=True, capture_output=True, timeout=30)
            assert rejected.returncode == 1 and json.loads(rejected.stdout)['status'] == 'failed'
        finally:
            sample.write_bytes(original)
        checks += ['archive digest and every delivered file verified outside repository', 'diagnostic reports a tampered contract file and accepts restored bytes']
        database, token = evidence / 'workspace/workspace.sqlite', evidence / 'workspace/token'
        with socket.socket() as s:
            s.bind(('127.0.0.1', 0))
            port = s.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        host_log = (evidence / 'host.log').open('w')
        host = subprocess.Popen([str(binary), '--database', str(database), 'serve', '--listen', f'127.0.0.1:{port}', '--token-file', str(token)], cwd=evidence, stdout=host_log, stderr=host_log)
        deadline = time.monotonic()+20
        while True:
            assert host.poll() is None, 'Delivered host exited during startup'
            try:
                urllib.request.urlopen(base+'/v1/capabilities', timeout=1)
                raise AssertionError('Delivered HTTP host accepted an unauthenticated request')
            except urllib.error.HTTPError as e:
                assert e.code == 403
                break
            except urllib.error.URLError:
                assert time.monotonic() < deadline
                time.sleep(.05)
        with sqlite3.connect(database) as connection:
            actual_schema = int(connection.execute("SELECT value FROM metadata WHERE key='schema_version'").fetchone()[0])
        assert manifest['database_schema'] == actual_schema
        checks.append('delivered manifest matches executable database version, domain registry and OpenAPI routes')
        probe = evidence / 'tone.wav'
        with wave.open(str(probe), 'wb') as f:
            f.setnchannels(1); f.setsampwidth(2); f.setframerate(48000); f.writeframes(b'\0\0'*4800)
        consumer = evidence / 'consumer'
        consumer.mkdir()
        (consumer / 'package.json').write_text('{"private":true,"type":"module"}')
        npm_package, = (bundle / 'sdk').glob('*.tgz')
        run(['npm', 'install', '--ignore-scripts', '--no-audit', '--no-fund', npm_package], consumer)
        (consumer / 'accept.mjs').write_text('''import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import {OpenCutClient} from '@opencut/sdk';
const [base,token,fixture,probe,report]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
const f=JSON.parse(fs.readFileSync(fixture,'utf8'));
const initial=await c.call('opencut_project_create',f.create);
assert.deepEqual(await c.callDomain('project','inspect',{project_id:initial.project.id}),initial);
const request={project_id:initial.project.id,request:{...f.request,expected_revision:initial.revision,operations:f.operations}};
await c.callDomain('edit','plan',request);
assert.equal((await c.call('opencut_project_inspect',{project_id:initial.project.id})).revision,initial.revision);
const transaction=await c.callDomain('edit','apply',request);
assert.deepEqual(await c.call('opencut_edit_apply',request),transaction);
const job=await c.submitJob({kind:'media.probe',idempotency_key:'package-probe',input:{input:probe}});
const done=await c.waitForPublicJob(job.job_id,{timeoutMs:30000});
assert.equal(done.state,'succeeded');assert.equal(done.result.kind,'media.probe');
assert.equal(done.result.data.streams[0].sample_rate,48000);
const bytes=fs.readFileSync(probe);
const contentHash=createHash('sha256').update(bytes).digest('hex');
const upload=await c.call('opencut_upload_create',{project_id:initial.project.id,idempotency_key:'package-cross-sdk-upload',name:'tone.wav',size_bytes:String(bytes.length),content_hash:contentHash});
const cut=Math.floor(bytes.length/2);
const partial=await c.uploadChunk(upload.upload_id,'0',bytes.subarray(0,cut));
assert.equal(partial.next_offset,String(cut));
assert.equal((await c.call('opencut_upload_inspect',{upload_id:upload.upload_id})).next_offset,String(cut));
fs.writeFileSync(report,JSON.stringify({request,transaction,job:done,upload:{id:upload.upload_id,cut,contentHash,size:bytes.length}},null,2));
''')
        ts_report = evidence / 'ts.json'
        run(['node', consumer / 'accept.mjs', base, token, sample, probe, ts_report], consumer)
        checks += ['npm tarball installs independently and plans commits retries through delivered HTTP host', 'installed TS SDK submits and observes a real FFprobe Worker task']
        rust_archive, = (bundle / 'sdk').glob('*.tar.gz')
        unpack(rust_archive, evidence / 'rust-sdk')
        vendor, = (evidence / 'rust-sdk').iterdir()
        rust_consumer = evidence / 'rust-consumer'
        (rust_consumer / 'src').mkdir(parents=True)
        (rust_consumer / 'Cargo.toml').write_text('[package]\nname="delivered-sdk-consumer"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nopencut-sdk={path="../rust-sdk/opencut-rust-sdk/crates/sdk-rust"}\nserde_json="1"\n')
        shutil.copy2(vendor / 'Cargo.lock', rust_consumer / 'Cargo.lock')
        (rust_consumer / 'src/main.rs').write_text('''use opencut_sdk::{Client,api::{self,tools}};
use serde_json::{Value,json};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let a:Vec<_>=std::env::args().skip(1).collect();
 let client=Client::new(&a[0],std::fs::read_to_string(&a[1])?.trim())?;
 let ts:Value=serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
 let mut request:api::EditInput=serde_json::from_value(ts["request"].clone())?;
 let replayed=client.call::<tools::EditApply>(&request)?;
 assert_eq!(serde_json::to_value(&replayed)?,ts["transaction"]);
 request.request.expected_revision=replayed.revision;
 request.request.idempotency_key="rust-after-ts".into();
 request.request.operations=vec![opencut_sdk::project::Operation::UpdateSequenceSettings {
 sequence_id:"main".into(),name:Some("Edited from delivered Rust SDK".into()),width:None,height:None,fps:None}];
 let applied=client.call::<tools::EditApply>(&request)?;
 let observed=client.call::<tools::ProjectInspect>(&api::ProjectInput{project_id:request.project_id})?;
 assert_eq!(applied.revision,observed.revision);
 let upload_id=ts["upload"]["id"].as_str().expect("TypeScript upload identity");
 let cut=ts["upload"]["cut"].as_u64().expect("confirmed TypeScript offset") as usize;
 let bytes=std::fs::read(&a[3])?;
 assert_eq!(bytes.len() as u64,ts["upload"]["size"].as_u64().unwrap());
 let continued=client.upload_chunk(upload_id,cut as u64,&bytes[cut..])?;
 assert_eq!(continued.next_offset,bytes.len().to_string());
 let inspected=client.call::<tools::UploadInspect>(&api::uploads::UploadInput{upload_id:upload_id.into()})?;
 assert_eq!(inspected.next_offset,continued.next_offset);
 let sealed=client.call::<tools::UploadSeal>(&api::uploads::UploadInput{upload_id:upload_id.into()})?;
 assert_eq!(sealed.state,api::uploads::UploadState::Sealed);
 let import_job_id=sealed.job_id.expect("sealed upload must accept import job");
 let done=client.wait_for_public_job(&import_job_id,opencut_sdk::WaitOptions::default())?;
 let imported=opencut_sdk::extended_job_result::<opencut_sdk::jobs::kinds::MediaImport>(&done)?;
 let media_url=client.project_media_url(&imported)?;
 println!("{}",json!({"transaction":applied,"snapshot":observed,"upload_id":upload_id,"import_job_id":import_job_id,"media_id":imported.media_id,"media_url":media_url}));Ok(())
}
''')
        env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target/local-package-consumer'))
        run(['cargo', 'build', '--offline'], rust_consumer, env)
        metadata = json.loads(run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1'], rust_consumer, env))
        forbidden = {'opencut-editor-application','opencut-sqlite-store','opencut-media','wasmtime','rusqlite','opencut-job-runtime'}
        assert not forbidden.intersection(p['name'] for p in metadata['packages'])
        for p in metadata['packages']:
            if p['name'].startswith('opencut-'):
                assert Path(p['manifest_path']).resolve().is_relative_to(evidence)
        executable = ROOT / 'target/local-package-consumer/debug/delivered-sdk-consumer'
        rust_result = json.loads(run([executable, base, token, ts_report, probe], rust_consumer))
        input_file = evidence / 'inspect.json'
        input_file.write_text('{"project_id":"sdk-acceptance"}')
        readback = json.loads(run([binary, '--database', database, '--json', 'call', 'opencut_project_inspect', '--input', input_file]))
        assert readback['data'] == rust_result['snapshot']
        checks += ['Rust SDK source archive builds outside repo with portable dependencies only', 'Rust SDK replays the TS transaction then commits a new revision', 'delivered CLI independently reads the exact Rust SDK snapshot']
        (consumer / 'finish-upload.mjs').write_text('''import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import {OpenCutClient,publicJobResult} from '@opencut/sdk';
const [base,token,tsReport,rustReport,output]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
const ts=JSON.parse(fs.readFileSync(tsReport,'utf8'));
const rust=JSON.parse(fs.readFileSync(rustReport,'utf8'));
assert.equal(rust.upload_id,ts.upload.id);
const job=await c.waitForPublicJob(rust.import_job_id,{timeoutMs:60000});
const imported=publicJobResult(job,'media.import');
assert.equal(imported.project_id,'sdk-acceptance');
assert.equal(imported.upload_id,ts.upload.id);
assert.equal(imported.asset.kind,'audio');
assert.equal(imported.asset.version.content_hash,ts.upload.contentHash);
assert.deepEqual(await c.call('opencut_project_media_inspect',{project_id:'sdk-acceptance',media_id:imported.media_id}),imported);
assert.equal(rust.media_id,imported.media_id);
assert.equal(rust.media_url,c.projectMediaUrl(imported));
const source=new Uint8Array(await (await c.downloadProjectMedia(imported)).arrayBuffer());
assert.equal(source.byteLength,ts.upload.size);
assert.equal(createHash('sha256').update(source).digest('hex'),ts.upload.contentHash);
const released=await c.call('opencut_upload_release',{upload_id:ts.upload.id});
assert.equal(released.state,'released');
assert.equal(released.reserved_bytes,'0');
fs.writeFileSync(output,JSON.stringify({job,imported,released},null,2));
''')
        rust_report = evidence / 'rust.json'
        rust_report.write_text(json.dumps(rust_result, indent=2))
        run(['node', consumer / 'finish-upload.mjs', base, token, ts_report, rust_report,
             evidence / 'cross-sdk-upload.json'], consumer)
        checks.append('installed TS and Rust SDKs resume one raw upload, verify scoped imported bytes and release staging across the same delivered host')
        history = json.loads(run(['python3', ROOT / 'scripts/verify-history-compatibility.py', '--binary', binary]))
        history_detail = json.loads(Path(history['report']).read_text())
        assert history['status'] == 'passed' and history_detail['binary_sha256'] == manifest['binary_sha256']
        checks.append(f"delivered binary passes {history['matrix_cells']}-cell frozen historical format matrix")
        available_tools = {tool['name'] for tool in schema}
        independent_journeys = [
            ('opencut_access_inspect', 'verify-access.py', 'scoped CLI and stdio MCP authorization, real artifacts and live revocation'),
            ('opencut_audit_query', 'verify-audit.py', 'actor and Worker artifact provenance with atomic audit rollback'),
            ('opencut_access_audit_query', 'verify-access-audit.py', 'owner administration history across CLI HTTP and both MCP transports'),
            ('opencut_render_preflight', 'verify-scoped-preflight.py', 'scoped CLI/HTTP/MCP preflight and decoded preview of pinned historical content'),
            ('opencut_maintenance_query', 'verify-maintenance.py', 'media publication journal, unknown outcomes and locked reconciliation'),
            ('opencut_denial_query', 'verify-denials.py', 'bounded refusal audit, degraded logging and stream revocation'),
            ('opencut_storage_inspect', 'verify-storage.py', 'bounded workspace observation and scoped refusal under directory replacement'),
            ('opencut_storage_budget_inspect', 'verify-budget.py', 'real write admission, failed import recovery and SIGKILL reservation recovery'),
            ('opencut_cas_gc_plan', 'verify-cas-gc.py', 'reviewed CAS collection, historical render preservation and crash recovery'),
        ]
        if sys.platform == 'darwin' and capabilities.get('database'):
            independent_journeys.append(
                ('opencut_database_checkpoint', 'verify-database-recovery.py',
                 'actual disk-full SQL rollback and pinned-reader WAL checkpoint recovery')
            )
        for tool_name, script_name, description in independent_journeys:
            if tool_name not in available_tools:
                continue
            script_args = ['python3', ROOT / 'scripts' / script_name, '--binary', binary,
                           '--artifacts-dir', evidence / script_name.removesuffix('.py')]
            if mcp_sdk_root and script_name in ('verify-scoped-preflight.py', 'verify-denials.py', 'verify-storage.py', 'verify-budget.py', 'verify-cas-gc.py'):
                script_args += ['--mcp-sdk-root', mcp_sdk_root]
            journey = json.loads(run(script_args))
            assert journey['status'] == 'passed' and journey['binary_sha256'] == manifest['binary_sha256']
            checks.append(f'delivered binary passes {description}')
        worker = json.loads(run(['python3', ROOT / 'scripts/verify-worker-admission.py',
                                 '--binary', binary, '--artifacts-dir', evidence / 'worker-admission']))
        assert worker['status'] == 'passed' and worker['binary_sha256'] == manifest['binary_sha256']
        checks.append('delivered Worker enforces output ceilings, orphan recovery and safe cleanup on real FFmpeg tasks')
        component_sha256 = None
        if component:
            component_sha256 = hashlib.sha256(component.read_bytes()).hexdigest()
            plugin_scripts = [
                ('verify-plugin.py', 'real WASM planner validation, immutable plan and transactional apply'),
                ('verify-plugin-publication.py', 'plugin package and grant publication, reconciliation and MCP access'),
                ('verify-project-plugins.py', 'project-scoped WASM grants, concurrency and CLI/HTTP/MCP planning'),
                ('verify-budget-writers.py', 'upload, media repair, plugin install and live download write admission'),
            ]
            for script_name, description in plugin_scripts:
                script_args = ['python3', ROOT / 'scripts' / script_name, '--binary', binary,
                               '--component', component]
                if script_name != 'verify-plugin.py':
                    script_args += ['--artifacts-dir', evidence / script_name.removesuffix('.py')]
                if mcp_sdk_root and script_name in ('verify-plugin-publication.py', 'verify-project-plugins.py'):
                    script_args += ['--mcp-sdk-root', mcp_sdk_root]
                plugin = json.loads(run(script_args))
                assert plugin['status'] == 'passed' and plugin['binary_sha256'] == manifest['binary_sha256']
                if 'component_sha256' in plugin:
                    assert plugin['component_sha256'] == component_sha256
                if script_name == 'verify-plugin.py':
                    (evidence / 'verify-plugin.json').write_text(json.dumps(plugin, indent=2))
                checks.append(f'delivered binary passes {description}')
        if 'http' in capabilities.get('access', {}).get('scoped_transports', []):
            network = json.loads(run(['python3', ROOT / 'scripts/verify-network-access.py',
                                      '--binary', binary, '--artifacts-dir', evidence / 'network-access']))
            assert network['status'] == 'passed' and network['binary_sha256'] == manifest['binary_sha256']
            checks.append('delivered scoped HTTP/MCP enforces SSE resume, verified artifact bytes and live stream revocation')
        if capabilities.get('uploads'):
            import_args = ['python3', ROOT / 'scripts/verify-project-import.py', '--binary', binary,
                           '--artifacts-dir', evidence / 'project-import']
            if mcp_sdk_root:
                import_args += ['--mcp-sdk-root', mcp_sdk_root]
            imported = json.loads(run(import_args))
            assert imported['status'] == 'passed'
            checks.append('delivered project import passes scoped CLI/HTTP/MCP byte transfer, publication, binding and independent media decode')
            recovery = json.loads(run(['python3', ROOT / 'scripts/verify-project-import-recovery.py',
                                       '--binary', binary, '--artifacts-dir', evidence / 'import-recovery']))
            assert recovery['status'] == 'passed' and recovery['attempt'] == 2
            checks.append('delivered import Worker survives SIGKILL, natural lease recovery and abandoned input cleanup')
        run(['python3', ROOT / 'scripts/verify-product-journey.py', '--binary', binary, '--artifacts-dir', evidence / 'media-journey'])
        checks.append('delivered binary passes real upload preview export repair and decoded RGB/PCM journey')
        host.terminate(); host.wait(timeout=10); host = None
        readback_after = json.loads(run([binary, '--database', database, '--json', 'call', 'opencut_project_inspect', '--input', input_file]))
        assert readback_after['data'] == rust_result['snapshot']
        checks.append('service stop and standalone delivered CLI reopen preserve final project')
        report = {'status':'passed','archive':str(archive),'archive_sha256':expected,'binary_sha256':manifest['binary_sha256'],
                  'target':manifest['target'],'checks':checks,'evidence':str(evidence),'doctor':doctor,
                  'component_sha256':component_sha256,
                  'scope':'local development package acceptance on current host; not full R0-R7 or cross-platform release'}
        (evidence / 'report.json').write_text(json.dumps(report,indent=2))
        Path(str(archive)+'.acceptance.json').write_text(json.dumps(report,indent=2))
        Path(str(archive)+'.acceptance-commands.json').write_text(json.dumps(logs,indent=2))
        print(json.dumps({'status':'passed','checks':len(checks),'report':str(evidence / 'report.json')},indent=2))
    except Exception as e:
        (evidence / 'failure.json').write_text(json.dumps({'error':repr(e),'checks':checks},indent=2))
        raise
    finally:
        if host and host.poll() is None:
            host.terminate()
            try:host.wait(timeout=10)
            except subprocess.TimeoutExpired:host.kill();host.wait()
        if host_log:host_log.close()
        (evidence / 'commands.json').write_text(json.dumps(logs,indent=2))


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('archive',type=Path)
    parser.add_argument('--mcp-sdk-root',type=Path,help='External npm prefix containing @modelcontextprotocol/sdk for independent interop')
    parser.add_argument('--component',type=Path,help='Built wasm32-wasip2 planner component for plugin acceptance')
    args=parser.parse_args()
    verify(args.archive.resolve(), args.mcp_sdk_root.resolve() if args.mcp_sdk_root else None,
           args.component.resolve() if args.component else None)
