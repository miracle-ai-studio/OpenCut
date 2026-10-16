#!/usr/bin/env python3
"""Generate only the 142 recorded RFN failures from existing fixed local cache.

No downloading, complete inventory rescanning or native acceptance is performed.
Generated outputs/proofs are implementation data, not an acceptance statement.
"""
import argparse
import concurrent.futures
import json
import multiprocessing
import urllib.parse
from pathlib import Path
import derive_renamed_v2 as d
import generate as g
from fontTools.ttLib import TTFont

def wire(proof):
    s=proof['source'];suffix={(400,'normal'):'regular',(700,'normal'):'bold',(400,'italic'):'italic',(700,'italic'):'bold-italic'}[s['weight'],s['style']]
    return dict(face=dict(id=f"google-{g.norm(s['source_family'])}-{suffix}-derived-v2",family=s['family'],weight=s['weight'],style=s['style'],derivation_id=proof['derivation_id'],content_hash=proof['output_sha256'],size_bytes=str(proof['output_size'])),source_commit=s['source_commit'],source_url=f"https://raw.githubusercontent.com/{s['source_repository']}/{s['source_commit']}/"+urllib.parse.quote(s['source_path']),source_sha256=s['source_sha256'],source_size_bytes=str(s['source_size']),license_url=f"https://raw.githubusercontent.com/{s['license_repository']}/{s['license_commit']}/"+urllib.parse.quote(s['license_path']),license_id=s['license_identifier'],license_notice=proof['license_notice'],axes=[dict(tag=k,coordinate=json.dumps(v)) for k,v in sorted(s['axes'].items())],proof_json=d.canonical(proof).decode())

def one(job):
    f,cache,evidence,paths,lock=job;path=f['source_path'];ubuntu=path.startswith('ufl/')
    supplied=f.get('font');license_input=f.get('license')
    lp=license_input['path'] if license_input else '/'.join(path.split('/')[:2])+('/LICENCE.txt' if ubuntu else '/OFL.txt')
    source_file=cache/f.get('cache_path',path);license_file=cache/f.get('license_cache_path',lp)
    source=source_file.read_bytes();notice=license_file.read_bytes()
    source_commit=supplied['commit'] if supplied else g.COMMIT
    source_repository=supplied['repository'] if supplied else 'google/fonts'
    license_commit=license_input['commit'] if license_input else g.COMMIT
    license_repository=license_input['repository'] if license_input else 'google/fonts'
    for name,data,descriptor in ((path,source,supplied),(lp,notice,license_input)):
        expected=descriptor if descriptor else paths[name]
        if len(data)!=expected['size'] or d.git_blob(data)!=expected.get('git_blob',expected.get('sha')) or ('sha256' in expected and d.sha(data)!=expected['sha256']):raise ValueError('fixed input blob/hash mismatch')
    with TTFont(source_file) as font:
        kind=f.get('recipe_kind','instantiate_renamed')
        axes={axis.axisTag:axis.defaultValue for axis in font['fvar'].axes} if kind=='instantiate_renamed' else {}
        if 'wght' in axes:axes['wght']=f['weight']
        if 'ital' in axes:axes['ital']=1 if f['style']=='italic' else 0
        profile=f.get('render_profile')
        if profile is None:
            profile=('colr_v'+str(font['COLR'].version)) if 'COLR' in font else ('monochrome_bitmap' if any(tag in font for tag in ('EBDT','bdat')) else 'static_outline')
        if 'axes' in f and f['axes']!=axes:raise ValueError('pinned input axes differ from source defaults and target face')
        original_metadata=d.static_metadata(font) if kind!='instantiate_renamed' else None
    name_profile=d.UBUNTU_REWRITE_VERSION if ubuntu else d.REWRITE_VERSION
    family,ps=d.opaque_names(d.sha(source),axes,f['weight'],f['style'],name_profile,f['family'])
    spec=dict(source_repository=source_repository,source_commit=source_commit,source_path=path,source_sha256=d.sha(source),source_size=len(source),source_git_blob=d.git_blob(source),license_repository=license_repository,license_commit=license_commit,license_path=lp,license_sha256=d.sha(notice),license_git_blob=d.git_blob(notice),license_identifier=f.get('license_identifier','Ubuntu-font-1.0' if ubuntu else 'OFL-1.1'),family=family,source_family=f['family'],weight=f['weight'],style=f['style'],axes=axes,name_rewrite_version=name_profile,output_postscript_name=ps,reserved_names=d.reserved_names(notice.decode()),rewritten_name_ids=list(d.IDENTITY_IDS),recipe_kind=kind,render_profile=profile)
    if ubuntu:spec['derivative_notice']=family+'; Modified Version: fixed static instance and derivative naming; original outlines retained.'
    if original_metadata:spec['original_metadata']=original_metadata
    stem=g.norm(f['family'])+f"-{f['weight']}-{f['style']}";record=evidence/(stem+'.json');output=evidence/(stem+'.ttf')
    if record.exists() and output.exists():
        proof=json.loads(record.read_text());data=output.read_bytes()
        if proof['tool']==lock and proof['source']==spec and d.sha(data)==proof['output_sha256'] and len(data)==proof['output_size']:return wire(proof)
    data,proof=d.derive(source,notice,spec,lock);record.write_bytes(d.canonical(proof));output.write_bytes(data)
    target=cache/'derived'/proof['output_sha256'];target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
    return wire(proof)

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--tree',type=Path,required=True);ap.add_argument('--cache',type=Path,required=True);ap.add_argument('--evidence',type=Path,required=True);ap.add_argument('--color-inputs',type=Path);ap.add_argument('--static-corrections',type=Path);ap.add_argument('--large-inputs',type=Path);ap.add_argument('--supplemental-inputs',type=Path);ap.add_argument('--official-inputs',type=Path);ap.add_argument('--workers',type=int,default=4);a=ap.parse_args()
    if not 1<=a.workers<=4:raise ValueError('at most four implementation data workers')
    if g.ROOT==a.cache or g.ROOT in a.cache.parents:raise ValueError('cache must remain outside checkout')
    tree=json.loads(a.tree.read_text());assert not tree.get('truncated');paths={e['path']:e for e in tree['tree']}
    original=json.loads((g.ROOT/'crates/editor-application/src/font_derived_v1/catalog_source_report.json').read_text())
    failures=[f for f in original['failures'] if f['reason']=='reserved font name requires reviewed renaming']
    if a.color_inputs:
        color=json.loads(a.color_inputs.read_text());entries=color if isinstance(color,list) else color['sources']
        known={(f['family'],f['weight'],f['style']) for f in failures}
        failures.extend(f for f in entries if (f['family'],f['weight'],f['style']) not in known)
    if a.static_corrections:failures.extend(json.loads(a.static_corrections.read_text()))
    if a.large_inputs:failures.extend(json.loads(a.large_inputs.read_text()))
    for input_file in (a.supplemental_inputs,a.official_inputs):
        if not input_file:continue
        for source in json.loads(input_file.read_text()):
            path=source['font']['path'];style='italic' if '-Italic[' in path else 'normal'
            for target in source.get('targets',[dict(weight=w,style=style) for w in (400,700)]):
                failures.append(dict(source,family=source['family'],source_path=path,weight=target['weight'],style=target.get('style',style),render_profile=target.get('render_profile','static_outline'),**({'axes':target['axes']} if 'axes' in target else {})))
    a.evidence.mkdir(parents=True,exist_ok=True);lock=d.implementation_identity();pins=[];rejected=[]
    with concurrent.futures.ProcessPoolExecutor(max_workers=a.workers,mp_context=multiprocessing.get_context('spawn')) as pool:
        pending={pool.submit(one,(f,a.cache,a.evidence,paths,lock)):f for f in failures}
        for future in concurrent.futures.as_completed(pending):
            f=pending[future]
            try:pins.append(future.result())
            except Exception as e:rejected.append(dict(family=f['family'],source_path=f['source_path'],weight=f['weight'],style=f['style'],reason=str(e)))
            print(f"RFN implementation data {len(pins)+len(rejected)}/{len(failures)} generated={len(pins)} refused={len(rejected)}",flush=True)
    target=g.ROOT/'crates/editor-application/src/font_derived_v2';target.mkdir(parents=True,exist_ok=True)
    (target/'catalog_pins.json').write_text(json.dumps(sorted(pins,key=lambda p:p['face']['id']),ensure_ascii=False,indent=2)+'\n')
    (target/'approved_recipes.json').write_text(json.dumps([json.loads(p['proof_json'])['source'] for p in sorted(pins,key=lambda p:p['face']['id'])],ensure_ascii=False,indent=2)+'\n')
    (target/'catalog_source_report.json').write_text(json.dumps(dict(schema_version=2,source_commit=g.COMMIT,attempted=len(failures),generated=len(pins),source_families=len({json.loads(p['proof_json'])['source']['source_family'] for p in pins}),failures=sorted(rejected,key=lambda f:(f['family'],f['weight'],f['style'])),scope='implementation_recipe_and_output_hash_generation_only; native_and_pixel_acceptance_not_run'),ensure_ascii=False,indent=2)+'\n')
    Path(__file__).with_name('derived-v2-runtime-lock.json').write_bytes(d.canonical(lock)+b'\n')
if __name__=='__main__':main()
