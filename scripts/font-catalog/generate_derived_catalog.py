#!/usr/bin/env python3
"""Resume fixed-source variable derivations; failures remain explicit evidence."""
import argparse, concurrent.futures, json, urllib.parse, threading, tempfile, multiprocessing, collections
from pathlib import Path
import generate as g
import derive_static as d
from fontTools.ttLib import TTFont

def publish_cache(cache, data, proof):
    """Verified output only, addressed by pinned output SHA; never a caller path."""
    if len(data)!=proof['output_size'] or d.sha(data)!=proof['output_sha256']:
        raise ValueError('output cache identity mismatch')
    target=cache/'derived'/proof['output_sha256'];target.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=target.parent,delete=False) as output:
        temporary=Path(output.name);output.write(data)
    temporary.replace(target)

def wire(proof):
    s=proof['source'];style=s['style'];suffix={(400,'normal'):'regular',(700,'normal'):'bold',(400,'italic'):'italic',(700,'italic'):'bold-italic'}[s['weight'],style]
    return dict(face=dict(id=f"google-{g.norm(s['family'])}-{suffix}-derived-v1",family=s['family'],weight=s['weight'],style=style,derivation_id=proof['derivation_id'],content_hash=proof['output_sha256'],size_bytes=str(proof['output_size'])),source_commit=s['source_commit'],source_url=g.BASE+urllib.parse.quote(s['source_path']),source_sha256=s['source_sha256'],source_size_bytes=str(s['source_size']),license_url=g.BASE+urllib.parse.quote(s['license_path']),license_id=s['license_identifier'],license_notice=proof['license_notice'],axes=[dict(tag=k,coordinate=json.dumps(v)) for k,v in sorted(s['axes'].items())],proof_json=d.canonical(proof).decode())

def coverage_report(report, pins, atlas, present, catalog_bytes):
    report=dict(report)
    derived={(r['face']['family'],r['face']['weight'],r['face']['style']) for r in pins}
    covered=present|derived
    applicable={(family,weight,style) for family,metadata in atlas.items() for weight in (400,700) for style in ('normal','italic') if str(weight)+('i' if style=='italic' else '') in metadata['s']}
    reasons={(f['family'],f['weight'],f['style']):f['reason'] for f in report['failures']}
    missing=sorted(applicable-covered)
    report.update(classic_families=len(atlas),classic_face_declarations=sum(len(v['s']) for v in atlas.values()),unexpressed_classic_face_declarations=sum(len(v['s']) for v in atlas.values())-len(applicable),expressed_weights=[400,700],expressed_styles=['normal','italic'],catalog_sha256=d.sha(catalog_bytes),static_pinned_faces=len(present),derived_pinned_faces=len(derived),applicable_classic_faces=len(applicable),combined_pinned_applicable_faces=len(applicable&covered),missing_applicable_faces=[dict(family=f,weight=w,style=s,reason=reasons.get((f,w,s),'no_reviewed_pinned_recipe')) for f,w,s in missing],covered_400_normal_families=len({f for f,w,s in covered if w==400 and s=='normal'}),remaining_400_normal_families=sorted(set(atlas)-{f for f,w,s in covered if w==400 and s=='normal'}),rejection_counts=dict(sorted(collections.Counter(f['reason'] for f in report['failures']).items())),admission_scope='deterministic_static_metadata_only; native_inventory_and_bounded_runtime_execution_are_separate_checks')
    return report

def instantiate(source, notice, spec, lock):
    return d.derive(source,notice,spec,lock)

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--tree',type=Path,required=True);ap.add_argument('--cache',type=Path,required=True);ap.add_argument('--evidence',type=Path,required=True);ap.add_argument('--workers',type=int,default=16);ap.add_argument('--derive-workers',type=int,default=4);a=ap.parse_args();assert 1<=a.derive_workers<=8 and 1<=a.workers<=48
    tree=json.loads(a.tree.read_text());assert not tree.get('truncated');paths={e['path']:e for e in tree['tree']};atlas=json.loads((g.ROOT/'tmp/opencut-classic/apps/web/public/fonts/font-atlas.json').read_text())['fonts'];families={g.norm(f):f for f in atlas};static=json.loads((g.OUT/'browser_catalog.json').read_text());present={(f['family'],x['weight'],x['style']) for f in static['families'] for x in f['faces'] if x['catalog_id']};jobs=[]
    for p in sorted(paths):
        parts=p.split('/')
        if len(parts)!=3 or parts[0] not in ('ofl','apache') or parts[1] not in families or '[' not in p or not p.endswith('.ttf'):continue
        family=families[parts[1]];style='italic' if '-Italic[' in p else 'normal';lp='/'.join(parts[:2])+('/OFL.txt' if parts[0]=='ofl' else '/LICENSE.txt')
        if lp not in paths:continue
        for weight in (400,700):
            token=str(weight)+('i' if style=='italic' else '')
            if token in atlas[family]['s'] and (family,weight,style) not in present:jobs.append((family,p,lp,weight,style))
    a.evidence.mkdir(parents=True,exist_ok=True);lock=d.implementation_identity();source_locks={j[1]:threading.Lock() for j in jobs}
    def derive(job):
        family,p,lp,weight,style=job;key=g.norm(family)+f'-{weight}-{style}';record=a.evidence/(key+'.json');output=a.evidence/(key+'.ttf')
        if record.exists() and output.exists():
            proof=json.loads(record.read_text());data=output.read_bytes()
            if proof['tool']==lock and d.sha(data)==proof['output_sha256'] and len(data)==proof['output_size']:
                publish_cache(a.cache,data,proof);return wire(proof)
        if paths[p]['size']>d.MAX_BYTES:raise ValueError('source exceeds font envelope')
        with source_locks[p]:
            source=g.get(p,a.cache);notice=g.get(lp,a.cache)
        for path,data in [(p,source),(lp,notice)]:
            if len(data)!=paths[path]['size'] or d.git_blob(data)!=paths[path]['sha']:raise ValueError('fixed tree blob mismatch')
        with TTFont(a.cache/p) as font:
            axes={axis.axisTag:axis.defaultValue for axis in font['fvar'].axes}
            if 'wght' in axes:axes['wght']=weight
            if 'ital' in axes:axes['ital']=1 if style=='italic' else 0
        spec=dict(source_commit=g.COMMIT,source_path=p,source_sha256=d.sha(source),source_size=len(source),source_git_blob=d.git_blob(source),license_path=lp,license_sha256=d.sha(notice),license_git_blob=d.git_blob(notice),license_identifier='OFL-1.1' if lp.endswith('OFL.txt') else 'Apache-2.0',family=family,weight=weight,style=style,axes=axes)
        data,proof=instances.submit(instantiate,source,notice,spec,lock).result();output.write_bytes(data);record.write_bytes(d.canonical(proof));publish_cache(a.cache,data,proof);return wire(proof)
    results=[];failures=[]
    with concurrent.futures.ProcessPoolExecutor(max_workers=a.derive_workers,mp_context=multiprocessing.get_context('spawn')) as instances, concurrent.futures.ThreadPoolExecutor(max_workers=a.workers) as pool:
        futures={pool.submit(derive,j):j for j in jobs}
        for i,f in enumerate(concurrent.futures.as_completed(futures),1):
            try:results.append(f.result())
            except Exception as e:failures.append(dict(family=futures[f][0],source_path=futures[f][1],weight=futures[f][3],style=futures[f][4],reason=str(e)))
            if i%25==0:
                progress=dict(source_commit=g.COMMIT,attempted=len(jobs),completed=i,derived=len(results),failures=failures)
                temporary=a.evidence/'progress.tmp.json';temporary.write_text(json.dumps(progress,indent=2)+'\n');temporary.replace(a.evidence/'progress.json')
                print(f'{i}/{len(jobs)} derived={len(results)} failed={len(failures)}',flush=True)
    results.sort(key=lambda r:r['face']['id']);unique={r['face']['id']:r for r in reversed(results)}
    target=g.ROOT/'crates/editor-application/src/font_derived/catalog_pins.json';target.parent.mkdir(parents=True,exist_ok=True);target.write_text(json.dumps(list(sorted(unique.values(),key=lambda r:r['face']['id'])),ensure_ascii=False,indent=2)+'\n')
    report=dict(source_commit=g.COMMIT,attempted=len(jobs),derived=len(unique),families=len({r['face']['family'] for r in unique.values()}),failures=sorted(failures,key=lambda r:(r['family'],r['weight'],r['style'])))
    report=coverage_report(report,list(unique.values()),atlas,present,target.read_bytes())
    (target.parent/'catalog_source_report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    (a.evidence/'source-report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='failures'}))
if __name__=='__main__':main()
