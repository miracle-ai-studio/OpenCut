#!/usr/bin/env python3
"""One retained-workspace package journey;no builds,fixtures,media jobs or rendering.

Uses one fixed existing source and target workspace. Native export/inspection,
original import/replay and independent graph/CAS/provenance checks are checkpointed
as coarse stages. Old transport/render evidence remains bound to its producer.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
from acceptance import Harness
from package_exchange_oracle import database, digest, full_graph, graph


def sha_file(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        while block := stream.read(1024 * 1024):
            h.update(block)
    return h.hexdigest()


def stable(workspace):
    with database(workspace) as db:
        # Report hashes only;credentials and private records never enter reports.
        return {table: digest(sorted(db.execute(f'SELECT * FROM {table}'),key=repr))
                for table in ['jobs', 'access_principals', 'access_credentials', 'access_grants',
                              'project_plugin_grants', 'project_preset_grants']}


def verify(args):
    args.report.parent.mkdir(parents=True,exist_ok=True)
    source = Harness(args.binary, args.workspace, args.name)
    target = Harness(args.binary, args.target_workspace, args.name + '-restore')
    assert source.workspace != target.workspace, 'Use the existing separate restoration workspace'
    request = json.loads(args.export_request.read_text())
    assert set(request) == {'project_id','revision','destination','scope','mode'}
    assert request['scope'] in {'saved_revision','history'}
    assert request['mode'] in {'linked','self_contained'}
    package = Path(request['destination']).resolve()
    scope = dict(candidate=source.candidate, binary=str(source.binary), source_workspace=str(source.workspace), export=request, target=str(target.workspace),
                 target_project=args.target_project, original_import_key=args.import_key)
    assert source.state.setdefault('scope', scope) == scope, 'Retain the exact candidate and original requests'
    baseline = source.stage('capture-fixed-source', lambda: {
        'snapshot':source.inspect(request['project_id'],request['revision']),
        'source_authority_jobs':stable(source.workspace),'target_authority_jobs':stable(target.workspace)})

    def export():
        # A lost response resumes by checking the already-published destination.
        if package.exists():
            value = source.call('opencut_project_package_inspect',{'package_path':str(package)})
        else:
            value = source.request('export','opencut_project_package_export',request)['package']
        assert (value['source_project_id'],value['source_revision'],value['scope'],value['mode']) == (
            request['project_id'],request['revision'],request['scope'],request['mode'])
        return value
    inspected = source.stage('export-and-checked-manifest', export)
    import_request = {'package_path':str(package),'target_project_id':args.target_project,
                      'expected_manifest_sha256':inspected['manifest_sha256'],'idempotency_key':args.import_key}
    source.state.setdefault('import_request',import_request)
    assert source.state['import_request'] == import_request
    source.save()
    receipt = source.stage('atomic-import-and-original-replay', lambda: target.request(
        'import','opencut_project_package_import',import_request))
    assert target.call('opencut_project_package_import',import_request) == receipt
    assert receipt['package']['manifest_sha256'] == inspected['manifest_sha256']

    def independent():
        manifest_bytes = (package/'manifest.json').read_bytes()
        assert hashlib.sha256(manifest_bytes).hexdigest() == inspected['manifest_sha256']
        manifest = json.loads(manifest_bytes)
        assert 'schema_version' not in manifest and 'project_schema_version' not in manifest
        archive_bytes = (package/manifest['archive_path']).read_bytes()
        assert hashlib.sha256(archive_bytes).hexdigest() == manifest['archive_sha256']
        archive = json.loads(archive_bytes)
        assert 'schema_version' not in archive
        assert all('schema_version' not in row['document'] for row in archive['revisions'])
        with database(source.workspace) as a, database(target.workspace) as b:
            if request['scope'] == 'history':
                mapping = full_graph(a,b,request['project_id'],args.target_project,inspected,args.report.parent)
            else:
                old = json.loads(a.execute('SELECT document_json FROM revisions WHERE project_id=? AND revision=?',
                    [request['project_id'],request['revision']]).fetchone()[0])
                new = graph(b,args.target_project)
                assert len(new['revisions']) == 1 and not new['transactions'] and not new['branches'] and not new['merges']
                expected = dict(old,id=args.target_project)
                assert json.loads(new['revisions'][0][3]) == expected
                assert not new['revisions'][0][1] and not new['revisions'][0][2]
                mapping = {'single_fixed_document_equal_except_project_id':True}
            policies=[]
            for table,kind in [('project_plugin_grants','wasm'),('project_preset_grants','presets')]:
                policies += [{'kind':kind,'policy':json.loads(row[0])} for row in a.execute(
                    f'SELECT grant_json FROM {table} WHERE project_id=? ORDER BY plugin_id,version',[request['project_id']])]
            for proof in manifest['derived_fonts']:
                identity=proof['face']['derivation_id']
                columns='source_json,proof_json,output_sha256,output_size'
                original=a.execute(f'SELECT {columns} FROM font_derivations WHERE derivation_id=?',[identity]).fetchone()
                restored=b.execute(f'SELECT {columns} FROM font_derivations WHERE derivation_id=?',[identity]).fetchone()
                assert original is not None and restored==original
                assert json.loads(restored[0])==proof
            # Native capture sorts by the typed capability kind,then id/version.
            assert manifest['plugin_policies'] == policies
            assert not b.execute('SELECT 1 FROM project_plugin_grants WHERE project_id=?',[args.target_project]).fetchone()
            assert not b.execute('SELECT 1 FROM project_preset_grants WHERE project_id=?',[args.target_project]).fetchone()
        total=0
        for blob in manifest['blobs']:
            expected,size=blob['content_hash'],blob['size_bytes']
            location=blob['location']
            original=package/location['path'] if location['kind']=='bundled' else Path(location['path'])
            restored=target.workspace/'asset-store'/expected[:2]/expected
            for file in [original,restored]:
                assert file.is_file() and not file.is_symlink() and file.stat().st_size==size
                assert sha_file(file)==expected
            total+=size
        expected_installs=[b['package_sha256'] for b in manifest['plugin_bundles']]
        actual_installs=[a['input']['expected_package_sha256'] for a in receipt['plugin_restore_plan'] if a['tool']=='opencut_plugin_install']
        assert actual_installs==expected_installs
        assert stable(source.workspace)==baseline['source_authority_jobs']
        assert stable(target.workspace)==baseline['target_authority_jobs']
        assert source.inspect(request['project_id'],request['revision'])==baseline['snapshot']
        # Moving the package proves successful replay performs no file access.
        held=package.with_name(package.name+'.held-for-original-replay')
        assert not held.exists()
        package.rename(held)
        try:
            assert target.call('opencut_project_package_import',import_request)==receipt
        finally:
            held.rename(package)
        return {'graph':mapping,'resource_count':len(manifest['blobs']),'stream_verified_bytes':total,
                'plugin_policies_preserved_as_proposals':len(policies),'original_replay_without_package':True,
                'authority_and_jobs_unchanged':True,'no_media_or_render_jobs':True}
    source.stage('independent-content-resources-and-replay', independent)
    args.report.write_text(json.dumps({'scope':scope,'stages':source.state['stages'],
        'implementation_verified_only_for_this_candidate_and_scope':True,
        'not_verified':['browser_restore','mixed_historical_history','render_pixels_pcm','large_source_playback_performance']},ensure_ascii=False,indent=2)+'\n')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['binary','workspace','target-workspace','export-request','report']:
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--target-project',required=True)
    parser.add_argument('--import-key',required=True)
    parser.add_argument('--name',default='package-exchange')
    verify(parser.parse_args())
