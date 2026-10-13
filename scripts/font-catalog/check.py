#!/usr/bin/env python3
"""Offline consistency verification for Classic browse metadata and static pins."""
import hashlib, json, re
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'crates/editor-application/src/font_import_v1'
catalog=json.loads((BASE/'browser_catalog.json').read_text())
report=json.loads((BASE/'catalog_source_report.json').read_text())
atlas_bytes=(ROOT/'tmp/opencut-classic/apps/web/public/fonts/font-atlas.json').read_bytes()
atlas=json.loads(atlas_bytes)['fonts']
assert catalog['classic_atlas_sha256']==hashlib.sha256(atlas_bytes).hexdigest()
assert catalog['classic_family_count']==len(atlas)==1920
records={}
faces=set()
for block in (BASE/'catalog_generated.rs').read_text().split('PinnedFontSourceV1 {')[1:]:
    values={}
    for key,val in re.findall(r'^        (\w+): (.*),$',block,re.M):
        if val.startswith('"'):values[key]=json.loads(val)
        elif val.isdigit():values[key]=int(val)
        else:values[key]=val
    cid=values['catalog_id'];assert cid not in records;records[cid]=values
    face=(values['family'],values['weight'],values['style']);assert face not in faces;faces.add(face)
    assert values['weight'] in (400,700)
    assert values['size_bytes']<=32*1024*1024
    assert re.fullmatch('[0-9a-f]{64}',values['content_hash'])
    assert catalog['source_commit'] in values['source_url']
    assert catalog['source_commit'] in values['license_url']
    license_bytes=(BASE/'licenses/catalog'/f"{values['license_sha256']}.txt").read_bytes()
    assert hashlib.sha256(license_bytes).hexdigest()==values['license_sha256']
    assert len(license_bytes)<=64*1024 and b'\0' not in license_bytes
    assert values['license_identifier'] in ('OFL-1.1','Apache-2.0')
    assert license_bytes.decode('utf-8').startswith(values['attribution'])
    assert len(values['attribution'].encode('utf-8'))<=16*1024
assert len(records)==catalog['pinned_face_count']==report['pinned_faces']
assert len(catalog['families'])==len(atlas)+9
families=set();counts={}
for family in catalog['families']:
    name=family['family'];assert name not in families;families.add(name)
    if family['category']=='google':
        assert name in atlas
        expected={(int(t.rstrip('i')), 'italic' if t.endswith('i') else 'normal') for t in atlas[name]['s']}
        assert expected=={(f['weight'],f['style']) for f in family['faces']}
    for face in family['faces']:
        cid=face['catalog_id'];reason=face['unavailable_reason']
        assert bool(cid)!=(reason is not None)
        counts[reason or 'pinned_static']=counts.get(reason or 'pinned_static',0)+1
        if cid:
            r=records[cid];assert r['family']==name and r['weight']==face['weight']
            assert r['style']=='FontStyleV4::'+('Normal' if face['style']=='normal' else 'Italic')
        elif family['category']=='system':assert reason=='system_font_not_redistributable'
        elif face['weight'] not in (400,700):assert reason=='weight_not_expressed_by_text_v4'
        else:assert reason=='no_pinned_static_source'
# Native success evidence binds this exact generated catalog and inspector source.
native=json.loads((BASE/'catalog_native_report.json').read_text())
assert native['verified_current_pinned_faces']==len(records)
assert native['current_failures']==[]
assert native['catalog_generated_sha256']==hashlib.sha256((BASE/'catalog_generated.rs').read_bytes()).hexdigest()
assert native['inspector_source_sha256']==hashlib.sha256((ROOT/'crates/native-services/src/font_import_v1/mod.rs').read_bytes()).hexdigest()
# B1 remains exactly pinned.
assert records['google-lato-regular-v1']['content_hash']=='d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251'
assert records['google-mashanzheng-regular-v1']['content_hash']=='6d2546bb189c732a8ca29af9e22457b152387d158aa459e4ac2ce1e51788b7fb'
print(json.dumps(dict(families=len(families),pinned_faces=len(records),face_availability=counts),sort_keys=True))
