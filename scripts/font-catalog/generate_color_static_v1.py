#!/usr/bin/env python3
"""Generate separately versioned static COLR pins from existing fixed caches.

Only the seventeen confirmed incomplete V2 color instances are selected. Their
original recipes, bytes and catalog remain unchanged; nothing is downloaded.
"""
import argparse
import copy
import json
from pathlib import Path
import derive_color_static_v1 as color

ROOT = Path(__file__).resolve().parents[2]
FAMILIES = ('bitcountgriddoubleink', 'bitcountgridsingleink', 'bitcountink',
            'bitcountpropdoubleink', 'bitcountpropsingleink', 'bitcountsingleink',
            'foldit', 'kalniaglaze')
IDS = {f'google-{family}-{style}-derived-v2'
       for family in FAMILIES for style in ('bold', 'regular')}
IDS.add('google-sixtyfourconvergence-regular-derived-v2')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    if args.cache.resolve().is_relative_to(ROOT):
        raise ValueError('Font source/output cache must remain outside repository')
    original = ROOT / 'crates/editor-application/src/font_named/catalog_pins.json'
    original_hash = color.sha(original.read_bytes())
    selected = [source for source in json.loads(original.read_text())
                if source['face']['id'] in IDS]
    if len(selected) != 17:
        raise ValueError('Fixed V2 color source set differs')
    tool = color.implementation_identity()
    pins, report = [], []
    for source in selected:
        base_proof = json.loads(source['proof_json'])
        spec = base_proof['source']
        data, proof = color.derive(
            (args.cache / spec['source_path']).read_bytes(),
            source['license_notice'].encode(), source, tool,
            (args.cache / 'derived' / source['face']['content_hash']).read_bytes())
        pin = copy.deepcopy(source)
        pin['face'].update(id=source['face']['id'].removesuffix('-derived-v2') + '-color-static-v1',
            family=proof['source']['family'], derivation_id=proof['derivation_id'],
            content_hash=proof['output_sha256'], size_bytes=str(len(data)))
        pin['proof_json'] = color.canonical(proof).decode()
        target = args.cache / 'derived' / proof['output_sha256']
        if target.exists():
            if target.read_bytes() != data:
                raise ValueError('Existing static output cache identity conflict')
        else:
            target.write_bytes(data)
        pins.append(pin)
        report.append(dict(id=pin['face']['id'], output_bytes=len(data),
                           color_instantiation=proof['color_instantiation']))
    output = ROOT / 'crates/editor-application/src/font_color_static'
    output.mkdir(exist_ok=True)
    (output / 'catalog_pins.json').write_text(json.dumps(pins, ensure_ascii=False, indent=2) + '\n')
    (ROOT / 'scripts/font-catalog/color-static-runtime-lock.json').write_text(
        json.dumps(tool, indent=2, sort_keys=True) + '\n')
    if color.sha(original.read_bytes()) != original_hash:
        raise ValueError('Frozen original V2 pins changed')
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(dict(status='generated_not_yet_native_or_pixel_qualified',
        faces=len(pins), original_v2_catalog_unchanged=True, records=report), indent=2) + '\n')
    print(json.dumps(dict(generated_faces=len(pins), status='generated_not_yet_qualified')))


if __name__ == '__main__':
    main()
