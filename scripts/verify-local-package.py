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
import uuid
import wave
from acceptance_runner import AcceptanceRunner, write_json

ROOT = Path(__file__).resolve().parents[1]

SELECTABLE_CHECKS = {
    'artifact-gc-download': ('verify-artifact-gc-download.py', 6, False),
    'artifact-gc-full-volume': ('verify-artifact-gc-full-volume.py', 4, True),
    'artifact-gc-permissions': ('verify-artifact-gc-permissions.py', 4, True),
    'artifact-gc-mark-replacement': ('verify-artifact-gc-mark-replacement.py', 4, True),
    'cas-gc': ('verify-cas-gc.py', None, False),
    'artifact-publication-fence': ('verify-artifact-publication-fence.py', None, False),
}


def unpack(archive, destination):
    with tarfile.open(archive) as tar:
        names = set()
        for member in tar.getmembers():
            path = (destination / member.name).resolve()
            assert path.is_relative_to(destination.resolve()) and member.name not in names
            assert member.isfile() or member.isdir(), member.name
            names.add(member.name)
        tar.extractall(destination, filter='data')


def file_sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def verify_selected(archive, names):
    """Run named independent checks against a verified delivery, never full acceptance."""
    evidence = Path(tempfile.mkdtemp(prefix='opencut-package-check-')).resolve()
    runner = AcceptanceRunner(evidence)
    try:
        expected = Path(str(archive) + '.sha256').read_text().split()[0]
        assert file_sha256(archive) == expected, 'Delivery archive SHA-256 mismatch'
        unpack(archive, evidence / 'unpacked')
        bundle, = (evidence / 'unpacked').iterdir()
        manifest = json.loads((bundle / 'manifest.json').read_text())
        doctor = json.loads(runner.run(['python3', bundle / 'doctor.py']))
        assert doctor['status'] == 'passed', doctor
        binary = bundle / 'bin/opencut'
        assert file_sha256(binary) == manifest['binary_sha256'], 'Delivered binary SHA-256 mismatch'
        runner.checkpoint('archive, doctor and delivered binary identity verified',
                          archive_sha256=expected, binary_sha256=manifest['binary_sha256'])
        results = {}
        for name in names:
            script, expected_count, darwin_only = SELECTABLE_CHECKS[name]
            assert not darwin_only or sys.platform == 'darwin', f'{name} requires macOS'
            artifacts = evidence / name
            command = ['python3', ROOT / 'scripts' / script, '--binary', binary]
            if script != 'verify-artifact-publication-fence.py':
                command += ['--artifacts-dir', artifacts]
            output = json.loads(runner.run(command, name=name))
            assert output['status'] == 'passed', output
            if expected_count is not None:
                assert output['checks'] == expected_count, output
            report_path = Path(output['report']) if 'report' in output else artifacts / 'report.json'
            detail = json.loads(report_path.read_text())
            assert detail['status'] == 'passed', detail
            assert detail['binary_sha256'] == manifest['binary_sha256'], name
            local_report = evidence / f'{name}-report.json'
            write_json(local_report, detail)
            count = output.get('checks')
            results[name] = {'checks': len(count) if isinstance(count, list) else count,
                             'report': str(local_report)}
            runner.checkpoint(f'{name} report verified', binary_sha256=manifest['binary_sha256'])
        report = {'status': 'passed', 'mode': 'selected-checks', 'selected_checks': names,
                  'results': results, 'archive': str(archive), 'archive_sha256': expected,
                  'binary_sha256': manifest['binary_sha256'], 'evidence': str(evidence),
                  'scope': 'Named checks only; not full local package acceptance'}
        write_json(evidence / 'selected-report.json', report)
        print(json.dumps({'status': 'passed', 'mode': 'selected-checks',
                          'checks': names, 'report': str(evidence / 'selected-report.json')}, indent=2))
    except Exception as error:
        runner.failure(error)
        write_json(evidence / 'failure.json', {'mode': 'selected-checks',
                                               'selected_checks': names,
                                               'error': repr(error), 'evidence': str(evidence)})
        raise
    finally:
        runner.flush_commands()


def verify(archive, mcp_sdk_root=None, component=None):
    evidence = Path(tempfile.mkdtemp(prefix='opencut-package-acceptance-')).resolve()
    runner = AcceptanceRunner(evidence)
    logs, checks = runner.commands, []
    host = None
    host_log = None
    def run(args, cwd=evidence, env=None):
        return runner.run(args, cwd=cwd, env=env)
    try:
        expected = Path(str(archive)+'.sha256').read_text().split()[0]
        assert file_sha256(archive) == expected
        unpack(archive, evidence / 'unpacked')
        bundle, = (evidence / 'unpacked').iterdir()
        manifest = json.loads((bundle / 'manifest.json').read_text())
        doctor = json.loads(run(['python3', bundle / 'doctor.py']))
        assert doctor['status'] == 'passed'
        binary = bundle / 'bin/opencut'
        assert file_sha256(binary) == manifest['binary_sha256']
        runner.checkpoint('archive, doctor and delivered binary identity verified',
                          archive_sha256=expected, binary_sha256=manifest['binary_sha256'])
        for contract in ['project-plugin-contract.md', 'denial-audit-contract.md',
                         'storage-governance-contract.md', 'cas-gc-contract.md',
                         'storage-admission-contract.md', 'database-control-admission.md',
                         'native-child-write-contract.md', 'artifact-retention-contract.md']:
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
            except (urllib.error.URLError, TimeoutError):
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
 if a.first().is_some_and(|mode| mode=="gc") {
  let client=Client::new(&a[1],std::fs::read_to_string(&a[2])?.trim())?;
  let reviewed:api::gc::Plan=serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
  let input=api::gc::ExecuteInput{plan_id:reviewed.plan_id.clone(),expected_plan_hash:reviewed.plan_hash.clone()};
  let wrong=api::gc::ExecuteInput{expected_plan_hash:"0".repeat(64),..input.clone()};
  let rejected=client.call::<tools::CasGcExecute>(&wrong).expect_err("wrong reviewed hash must reject");
  assert!(matches!(rejected,opencut_sdk::Error::Api(ref failure) if failure.error.code=="GC_PLAN_CONFLICT"));
  let executed=client.call::<tools::CasGcExecute>(&input)?;
  assert_eq!(executed.plan_id,reviewed.plan_id);
  assert_eq!(executed.plan_hash,reviewed.plan_hash);
  assert!(matches!(executed.state,api::gc::PlanState::Complete));
  println!("{}",serde_json::to_string(&executed)?);
  return Ok(());
 }
 if a.first().is_some_and(|mode| mode=="staging") {
  let client=Client::new(&a[1],std::fs::read_to_string(&a[2])?.trim())?;
  let prepared:Value=serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
  let project_id=prepared["project_id"].as_str().expect("typed project id");
  let upload_id=prepared["upload_id"].as_str().expect("typed upload id");
  let input=api::uploads::UploadInput{upload_id:upload_id.into()};
  let page=client.call::<tools::UploadQuery>(&api::uploads::UploadQuery{project_id:project_id.into(),cursor:None,limit:50,retained_only:true})?;
  assert!(page.uploads.iter().any(|entry| entry.upload.upload_id==upload_id && entry.retention.release_eligible));
  let before=client.call::<tools::UploadRetention>(&input)?;
  assert_eq!(before.state,api::uploads::UploadRetentionState::Retained);
  assert!(before.release_eligible);
  let quota_before=client.call::<tools::UploadQuota>(&api::ProjectInput{project_id:project_id.into()})?;
  assert_eq!(quota_before.project_reserved_bytes,prepared["size_bytes"].as_str().expect("typed size"));
  let released=client.call::<tools::UploadRelease>(&input)?;
  assert_eq!(released.state,api::uploads::UploadRetentionState::Released);
  let quota_after=client.call::<tools::UploadQuota>(&api::ProjectInput{project_id:project_id.into()})?;
  assert_eq!(quota_after.project_reserved_bytes,"0");
  println!("{}",json!({"before":before,"quota_before":quota_before,"released":released,"quota_after":quota_after}));
  return Ok(());
 }
 if a.first().is_some_and(|mode| mode=="preflight") {
  let runner=Client::new(&a[1],std::fs::read_to_string(&a[2])?.trim())?;
  let reader=Client::new(&a[1],std::fs::read_to_string(&a[3])?.trim())?;
  let prepared:Value=serde_json::from_str(&std::fs::read_to_string(&a[4])?)?;
  let input:api::render::RenderPreflightInput=serde_json::from_value(prepared["request"].clone())?;
  let expected=prepared["report"].clone();
  let actual=runner.call::<tools::RenderPreflight>(&input)?;
  assert_eq!(serde_json::to_value(&actual)?,expected);
  assert!(actual.ready && actual.issues.is_empty());
  let denied=reader.call::<tools::RenderPreflight>(&input).expect_err("read-only principal must not preflight");
  assert!(matches!(denied,opencut_sdk::Error::Api(ref failure) if failure.error.code=="NOT_AUTHORIZED"));
  let observed=runner.call::<tools::ProjectInspect>(&api::ProjectInput{project_id:input.project_id.clone()})?;
  assert_eq!(observed.revision,input.revision);
  println!("{}",json!({"report":actual,"revision":observed.revision}));
  return Ok(());
 }
 if a.first().is_some_and(|mode| mode=="warning") {
  let client=Client::new(&a[1],std::fs::read_to_string(&a[2])?.trim())?;
  let denied=client.call::<tools::PluginList>(&api::EmptyInput{}).expect_err("scoped principal must not list owner plugins");
  let opencut_sdk::Error::Api(failure)=denied else {panic!("expected structured API refusal")};
  assert_eq!(failure.error.code,"NOT_AUTHORIZED");
  assert_eq!(failure.warnings,vec!["DENIAL_AUDIT_UNAVAILABLE".to_string()]);
  println!("{}",json!({"code":failure.error.code,"warnings":failure.warnings,"request_id":failure.request_id}));
  return Ok(());
 }
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
        # Review and execution must be two explicit SDK actions against the
        # delivered host. An incorrect hash must leave the reviewed object intact.
        capture_id = str(uuid.uuid4())
        capture = database.parent / 'asset-store' / f'.capture-{capture_id}'
        capture.write_bytes(b'cross-sdk-reviewed-capture')
        (consumer / 'review-gc.mjs').write_text('''import assert from 'node:assert/strict';
import fs from 'node:fs';
import {OpenCutClient} from '@opencut/sdk';
const [base,token,captureId,report]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
const plan=await c.call('opencut_cas_gc_plan',{idempotency_key:'cross-sdk-reviewed-capture',limit:100});
assert.equal(plan.state,'planned');
assert.match(plan.plan_hash,/^[0-9a-f]{64}$/);
assert(plan.items.some(item=>item.object.kind==='capture' && item.object.capture_id===captureId && item.state==='planned'));
assert.deepEqual(await c.call('opencut_cas_gc_inspect',{plan_id:plan.plan_id}),plan);
fs.writeFileSync(report,JSON.stringify(plan,null,2));
''')
        reviewed_plan = evidence / 'cross-sdk-gc-reviewed.json'
        run(['node', consumer / 'review-gc.mjs', base, token, capture_id, reviewed_plan], consumer)
        assert capture.read_bytes() == b'cross-sdk-reviewed-capture'
        executed_plan = json.loads(run([executable, 'gc', base, token, reviewed_plan], rust_consumer))
        assert executed_plan['plan_hash'] == json.loads(reviewed_plan.read_text())['plan_hash']
        assert executed_plan['state'] == 'complete' and not capture.exists()
        (consumer / 'confirm-gc.mjs').write_text('''import assert from 'node:assert/strict';
import fs from 'node:fs';
import {OpenCutClient} from '@opencut/sdk';
const [base,token,reviewed,executed,captureId]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
const before=JSON.parse(fs.readFileSync(reviewed,'utf8'));
const after=JSON.parse(fs.readFileSync(executed,'utf8'));
assert.equal(before.plan_id,after.plan_id);
assert.equal(before.plan_hash,after.plan_hash);
const observed=await c.call('opencut_cas_gc_inspect',{plan_id:before.plan_id});
assert.deepEqual(observed,after);
assert(observed.items.some(item=>item.object.kind==='capture' && item.object.capture_id===captureId && item.state==='removed'));
''')
        executed_file = evidence / 'cross-sdk-gc-executed.json'
        executed_file.write_text(json.dumps(executed_plan, indent=2))
        run(['node', consumer / 'confirm-gc.mjs', base, token, reviewed_plan, executed_file, capture_id], consumer)
        checks.append('installed TS SDK reviews an exact CAS plan and Rust SDK rejects a wrong hash before executing it; TS confirms the same durable receipt and removed bytes')
        policy_file = evidence / 'scoped-policy.json'
        policy_file.write_text(json.dumps({'principal_id':'sdk-runner','expected_revision':'0','enabled':True,
            'grants':[{'project_id':'sdk-acceptance','actions':['read','edit','run_jobs','read_artifacts']}]}))
        policy = json.loads(run([binary, '--database', database, '--json', 'access', 'set-policy', '--input', policy_file]))
        assert policy['error'] is None
        scoped_token = evidence / 'sdk-runner.secret'
        issued = json.loads(run([binary, '--database', database, '--json', 'access', 'issue', 'sdk-runner', '--output', scoped_token]))
        assert issued['error'] is None and scoped_token.read_text().strip()
        (consumer / 'stage-upload.mjs').write_text('''import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import {OpenCutClient} from '@opencut/sdk';
const [base,token,source,report]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
const bytes=fs.readFileSync(source), project_id='sdk-acceptance';
const created=await c.call('opencut_upload_create',{project_id,idempotency_key:'cross-sdk-staging',name:'staging-tone.wav',size_bytes:String(bytes.length),content_hash:createHash('sha256').update(bytes).digest('hex')});
const chunk=await c.uploadChunk(created.upload_id,'0',bytes);
assert.equal(chunk.next_offset,String(bytes.length));
const sealed=await c.call('opencut_upload_seal',{upload_id:created.upload_id});
assert.equal(sealed.state,'sealed');
const job=await c.waitForPublicJob(sealed.job_id,{timeoutMs:60000});
assert.equal(job.state,'succeeded');
const retention=await c.call('opencut_upload_retention',{upload_id:created.upload_id});
const quota=await c.call('opencut_upload_quota',{project_id});
const page=await c.call('opencut_upload_query',{project_id,limit:50,retained_only:true});
assert.equal(retention.state,'retained');assert(retention.release_eligible);
assert.equal(quota.project_reserved_bytes,String(bytes.length));
assert(page.uploads.some(entry=>entry.upload.upload_id===created.upload_id));
fs.writeFileSync(report,JSON.stringify({project_id,upload_id:created.upload_id,size_bytes:String(bytes.length),retention,quota},null,2));
''')
        stage_report = evidence / 'cross-sdk-staging-before.json'
        run(['node', consumer / 'stage-upload.mjs', base, scoped_token, probe, stage_report], consumer)
        staging_result = json.loads(run([executable, 'staging', base, scoped_token, stage_report], rust_consumer))
        assert staging_result['before'] == json.loads(stage_report.read_text())['retention']
        assert staging_result['quota_before'] == json.loads(stage_report.read_text())['quota']
        (consumer / 'confirm-staging.mjs').write_text('''import assert from 'node:assert/strict';
import fs from 'node:fs';
import {OpenCutClient} from '@opencut/sdk';
const [base,token,before,rust]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
const prepared=JSON.parse(fs.readFileSync(before,'utf8'));
const result=JSON.parse(fs.readFileSync(rust,'utf8'));
assert.deepEqual(await c.call('opencut_upload_retention',{upload_id:prepared.upload_id}),result.released);
assert.deepEqual(await c.call('opencut_upload_quota',{project_id:prepared.project_id}),result.quota_after);
assert.equal((await c.call('opencut_upload_query',{project_id:prepared.project_id,limit:50,retained_only:true})).uploads.some(entry=>entry.upload.upload_id===prepared.upload_id),false);
''')
        staging_file = evidence / 'cross-sdk-staging-after.json'
        staging_file.write_text(json.dumps(staging_result, indent=2))
        run(['node', consumer / 'confirm-staging.mjs', base, scoped_token, stage_report, staging_file], consumer)
        checks.append('installed scoped TS SDK discovers a terminal imported upload; Rust SDK reads typed retention/quota, releases it, and TS confirms freed staging')
        reader_policy_file = evidence / 'reader-policy.json'
        reader_policy_file.write_text(json.dumps({'principal_id':'sdk-reader','expected_revision':'0','enabled':True,
            'grants':[{'project_id':'sdk-acceptance','actions':['read']}]}))
        reader_policy = json.loads(run([binary, '--database', database, '--json', 'access', 'set-policy', '--input', reader_policy_file]))
        assert reader_policy['error'] is None
        reader_token = evidence / 'sdk-reader.secret'
        reader_issue = json.loads(run([binary, '--database', database, '--json', 'access', 'issue', 'sdk-reader', '--output', reader_token]))
        assert reader_issue['error'] is None and reader_token.read_text().strip()
        (consumer / 'review-preflight.mjs').write_text('''import assert from 'node:assert/strict';
import fs from 'node:fs';
import {OpenCutClient} from '@opencut/sdk';
const [base,runnerToken,readerToken,importedFile,output]=process.argv.slice(2);
const runner=new OpenCutClient({baseUrl:base,token:fs.readFileSync(runnerToken,'utf8').trim()});
const reader=new OpenCutClient({baseUrl:base,token:fs.readFileSync(readerToken,'utf8').trim()});
const imported=JSON.parse(fs.readFileSync(importedFile,'utf8')).imported;
const project_id='sdk-acceptance', current=await runner.call('opencut_project_inspect',{project_id});
const span={start:{ticks:'0',timebase:1000000},duration:imported.asset.duration};
const applied=await runner.call('opencut_edit_apply',{project_id,request:{expected_revision:current.revision,idempotency_key:'cross-sdk-preflight-media',label:'Bind imported media for preflight',operations:[
 {type:'asset.bind',asset:imported.asset},
 {type:'track.create',sequence_id:'main',track:{id:'scoped-audio',name:'Scoped audio',kind:'audio',enabled:true,muted:false,locked:false,clips:[],params:{}}},
 {type:'clip.insert',sequence_id:'main',track_id:'scoped-audio',clip:{id:'scoped-tone',asset_id:imported.asset.id,timeline_range:span,source_range:span,params:{},channels:[]}}
]}});
const request={project_id,revision:applied.revision,sequence_id:'main',container:'mp4',video_codec:'h264'};
const report=await runner.call('opencut_render_preflight',request);
assert.equal(report.ready,true);assert.deepEqual(report.issues,[]);
await assert.rejects(()=>reader.call('opencut_render_preflight',request),e=>e.code==='NOT_AUTHORIZED');
assert.equal((await runner.call('opencut_project_inspect',{project_id})).revision,applied.revision);
fs.writeFileSync(output,JSON.stringify({request,report,asset:imported.asset},null,2));
''')
        preflight_file = evidence / 'cross-sdk-preflight-ts.json'
        run(['node', consumer / 'review-preflight.mjs', base, scoped_token, reader_token,
             evidence / 'cross-sdk-upload.json', preflight_file], consumer)
        preflight_result = json.loads(run([executable, 'preflight', base, scoped_token, reader_token, preflight_file], rust_consumer))
        assert preflight_result['report'] == json.loads(preflight_file.read_text())['report']
        checks.append('installed restricted TS and Rust SDKs agree on typed immutable media preflight; read-only credentials reject without changing the revision')
        (consumer / 'audit-warning.mjs').write_text('''import assert from 'node:assert/strict';
import fs from 'node:fs';
import {OpenCutClient,OpenCutApiError} from '@opencut/sdk';
const [base,token,report]=process.argv.slice(2);
const c=new OpenCutClient({baseUrl:base,token:fs.readFileSync(token,'utf8').trim()});
let failure;
try { await c.call('opencut_plugin_list',{}); }
catch (error) { failure=error; }
assert(failure instanceof OpenCutApiError);
assert.equal(failure.code,'NOT_AUTHORIZED');
assert.deepEqual(failure.warnings,['DENIAL_AUDIT_UNAVAILABLE']);
fs.writeFileSync(report,JSON.stringify({code:failure.code,warnings:failure.warnings,request_id:failure.requestId},null,2));
''')
        with sqlite3.connect(database) as connection:
            denials_before = connection.execute('SELECT COUNT(*) FROM denial_records').fetchone()[0]
            connection.execute("CREATE TRIGGER cross_sdk_denial_fault BEFORE INSERT ON denial_records BEGIN SELECT RAISE(ABORT,'QA denial journal fault'); END")
        try:
            warning_ts_file = evidence / 'cross-sdk-warning-ts.json'
            run(['node', consumer / 'audit-warning.mjs', base, reader_token, warning_ts_file], consumer)
            warning_rust = json.loads(run([executable, 'warning', base, reader_token], rust_consumer))
        finally:
            with sqlite3.connect(database) as connection:
                connection.execute('DROP TRIGGER cross_sdk_denial_fault')
        warning_ts = json.loads(warning_ts_file.read_text())
        assert warning_ts['warnings'] == warning_rust['warnings'] == ['DENIAL_AUDIT_UNAVAILABLE']
        assert warning_ts['request_id'] != warning_rust['request_id']
        with sqlite3.connect(database) as connection:
            assert connection.execute('SELECT COUNT(*) FROM denial_records').fetchone()[0] == denials_before
        checks.append('installed TS and Rust SDKs retain the denial-audit failure warning on separate real restricted refusals while failed journal writes add no records')
        history = json.loads(run(['python3', ROOT / 'scripts/verify-history-compatibility.py', '--binary', binary]))
        history_detail = json.loads(Path(history['report']).read_text())
        assert history['status'] == 'passed' and history_detail['binary_sha256'] == manifest['binary_sha256']
        checks.append(f"delivered binary passes {history['matrix_cells']}-cell frozen historical format matrix")
        publication = json.loads(run(['python3', ROOT / 'scripts/verify-artifact-publication-fence.py', '--binary', binary]))
        publication_detail = json.loads(Path(publication['report']).read_text())
        assert publication['status'] == 'passed' and publication_detail['binary_sha256'] == manifest['binary_sha256']
        checks.append('delivered CLI rejects new project references while a crash-persistent artifact removal intent is unresolved, then retries cleanly')
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
            ('opencut_storage_budget_inspect', 'verify-storage-recovery-cli.py', 'local owner policy recovery through a stranded WAL and 64 MiB bootstrap without SQLite access'),
            ('opencut_cas_gc_plan', 'verify-cas-gc.py', 'reviewed CAS collection, historical render preservation and crash recovery'),
            ('opencut_database_inspect', 'verify-control-vfs.py', 'managed SQLite WAL write admission, exact-key retry and independent integrity'),
        ]
        if os.name == 'posix':
            independent_journeys.append(
                ('opencut_storage_budget_inspect', 'verify-worker-capacity-quiesce.py',
                 'live Worker capacity-fault quiescence, owner repair cancellation and audit')
            )
        if sys.platform == 'darwin' and capabilities.get('database'):
            independent_journeys.append(
                ('opencut_database_checkpoint', 'verify-database-recovery.py',
                 'actual disk-full SQL rollback and pinned-reader WAL checkpoint recovery')
            )
            independent_journeys.append(
                ('opencut_storage_budget_inspect', 'verify-repair-reserve.py',
                 'physical repair reserve and fenced running/queued cancellation on a full 64 MiB volume')
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
        gc_download = json.loads(run(['python3', ROOT / 'scripts/verify-artifact-gc-download.py',
            '--binary', binary, '--artifacts-dir', evidence / 'artifact-gc-download']))
        assert gc_download['status'] == 'passed' and gc_download['checks'] == 6
        checks.append('delivered HTTP download lease excludes collection; file, directory and hardlink replacements remain untouched')
        if sys.platform == 'darwin':
            gc_full = json.loads(run(['python3', ROOT / 'scripts/verify-artifact-gc-full-volume.py',
                '--binary', binary, '--artifacts-dir', evidence / 'artifact-gc-full-volume']))
            assert gc_full['status'] == 'passed' and gc_full['checks'] == 4
            checks.append('delivered collector survives real full-volume failure before intent and after unlink, then resumes the exact plans')
            gc_permissions = json.loads(run(['python3', ROOT / 'scripts/verify-artifact-gc-permissions.py',
                '--binary', binary, '--artifacts-dir', evidence / 'artifact-gc-permissions']))
            assert gc_permissions['status'] == 'passed' and gc_permissions['checks'] == 4
            checks.append('delivered collector preserves bytes through revoked attempt and file permissions, then resumes reviewed plans')
            gc_replacement = json.loads(run(['python3', ROOT / 'scripts/verify-artifact-gc-mark-replacement.py',
                '--binary', binary, '--artifacts-dir', evidence / 'artifact-gc-mark-replacement']))
            assert gc_replacement['status'] == 'passed' and gc_replacement['checks'] == 4
            checks.append('delivered collector rechecks file identity after durable intent and preserves new file or symlink names')
        final_before = json.loads(run([binary, '--database', database, '--json', 'call', 'opencut_project_inspect', '--input', input_file]))['data']
        assert final_before['revision'] == preflight_result['revision']
        host.terminate(); host.wait(timeout=10); host = None
        readback_after = json.loads(run([binary, '--database', database, '--json', 'call', 'opencut_project_inspect', '--input', input_file]))
        assert readback_after['data'] == final_before
        checks.append('service stop and standalone delivered CLI reopen preserve final project')
        report = {'status':'passed','archive':str(archive),'archive_sha256':expected,'binary_sha256':manifest['binary_sha256'],
                  'target':manifest['target'],'checks':checks,'evidence':str(evidence),'doctor':doctor,
                  'component_sha256':component_sha256,
                  'scope':'local development package acceptance on current host; not full R0-R7 or cross-platform release'}
        runner.checkpoint('full local package acceptance completed', checks=len(checks))
        write_json(evidence / 'report.json', report)
        write_json(Path(str(archive)+'.acceptance.json'), report)
        write_json(Path(str(archive)+'.acceptance-commands.json'), logs)
        print(json.dumps({'status':'passed','checks':len(checks),'report':str(evidence / 'report.json')},indent=2))
    except Exception as e:
        runner.failure(e)
        write_json(evidence / 'failure.json', {'error':repr(e),'checks':checks,
                                               'evidence':str(evidence)})
        raise
    finally:
        if host and host.poll() is None:
            host.terminate()
            try:host.wait(timeout=10)
            except subprocess.TimeoutExpired:host.kill();host.wait()
        if host_log:host_log.close()
        runner.flush_commands()


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('archive',type=Path,nargs='?')
    parser.add_argument('--list-checks',action='store_true',help='List named independent package checks')
    parser.add_argument('--check',action='append',choices=sorted(SELECTABLE_CHECKS),
                        help='Run only this named check; repeat for multiple checks')
    parser.add_argument('--mcp-sdk-root',type=Path,help='External npm prefix containing @modelcontextprotocol/sdk for independent interop')
    parser.add_argument('--component',type=Path,help='Built wasm32-wasip2 planner component for plugin acceptance')
    args=parser.parse_args()
    if args.list_checks:
        if args.archive or args.check or args.component or args.mcp_sdk_root:
            parser.error('--list-checks cannot be combined with an archive or other options')
        print(json.dumps({'checks': {name: {'script': script, 'macos_only': darwin_only}
                                     for name, (script, _, darwin_only) in SELECTABLE_CHECKS.items()}}, indent=2))
    else:
        if not args.archive:
            parser.error('archive is required unless --list-checks is used')
        if args.check:
            if args.component or args.mcp_sdk_root:
                parser.error('--check cannot be combined with --component or --mcp-sdk-root')
            verify_selected(args.archive.resolve(), list(dict.fromkeys(args.check)))
        else:
            verify(args.archive.resolve(), args.mcp_sdk_root.resolve() if args.mcp_sdk_root else None,
                   args.component.resolve() if args.component else None)
