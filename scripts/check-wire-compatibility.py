#!/usr/bin/env python3
"""Keep the captured pre-public-view v1 tool inputs and outputs unchanged."""
import argparse
import json
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--skip-build',action='store_true')
parser.add_argument('--binary',type=Path,help='Check an already-built CLI instead of rebuilding the default target')
args=parser.parse_args()
if args.binary is None and not args.skip_build: subprocess.run(['cargo','build','-p','opencut-cli'],cwd=root,check=True)
binary=args.binary or root/'target/debug/opencut'
legacy=json.loads((root/'packages/contract-fixtures/legacy-v1-tools.json').read_text())
current=json.loads(subprocess.check_output([str(binary),'system','schema','--json'],text=True))['data']
by_name={tool['name']:tool for tool in current}
for old in legacy:
    assert old['name'] in by_name, f"Legacy tool removed: {old['name']}"
    new=by_name[old['name']]
    for field in ('input_schema','output_schema','read_only'):
        assert old[field]==new[field], f"Legacy contract changed: {old['name']}.{field}"
print(json.dumps({'status':'passed','unchanged_legacy_tools':len(legacy),'current_tools':len(current)},indent=2))
