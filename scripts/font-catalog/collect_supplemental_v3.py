#!/usr/bin/env python3
"""Generate only explicitly selected additional official source pins.

No network discovery or old-catalog rewrite is performed here. Acquisition
records contain a full commit, exact repository path, Git blob, SHA256, size,
license URL/hash/complete bytes, and a reviewed Classic/native name pair.
"""
import argparse
import hashlib
import json
from pathlib import Path
import generate as old
import generate_catalog_v3 as out

def blob(data): return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()
def pin_record(family, native, weight, style, font, license_record, identifier, attribution, explanation=None):
    suffix={(400,"normal"):"regular",(700,"normal"):"bold",(400,"italic"):"italic",(700,"italic"):"bold-italic"}[(weight,style)]
    return dict(display_family=family,alias_reason=explanation,render_profile="static_outline",source_path=font["path"],
        source=dict(catalog_id=f"google-{old.norm(family)}-{suffix}-source-v3",family=native,weight=weight,style=style,
            source_commit=font["commit"],source_url=font["url"],content_hash=font["sha256"],size_bytes=font["size"],
            license_url=license_record["url"],license_sha256=license_record["sha256"],license_identifier=identifier,
            license_notice=license_record["notice"],attribution=attribution),
        provenance=dict(font=font,license={k:v for k,v in license_record.items() if k!="notice"}))

def fixed(path, cache, tree, commit):
    data=(cache/path).read_bytes();entry=tree[path]
    if len(data)!=entry["size"] or blob(data)!=entry["sha"]: raise ValueError("Fixed source Git blob differs: "+path)
    return data,dict(repository="google/fonts",commit=commit,path=path,url=f"https://raw.githubusercontent.com/google/fonts/{commit}/{path}",git_blob=entry["sha"],sha256=old.digest(data),size=len(data))

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--cache",type=Path,required=True);parser.add_argument("--tree",type=Path,required=True)
    args=parser.parse_args();tree=json.loads(args.tree.read_text())
    if tree.get("truncated") or tree["sha"]!=old.COMMIT:raise ValueError("Expected complete original fixed tree")
    entries={e["path"]:e for e in tree["tree"]}
    target=out.OUT/"supplemental_source_pins.json"
    records=json.loads(target.read_text()) if target.exists() else []
    records={r["source"]["catalog_id"]:r for r in records}
    variable=[]
    for family,folder in [("Ubuntu","ubuntu"),("Ubuntu Condensed","ubuntucondensed"),("Ubuntu Mono","ubuntumono"),("Ubuntu Sans","ubuntusans"),("Ubuntu Sans Mono","ubuntusansmono")]:
        prefix=f"ufl/{folder}/";lp=prefix+("UFL.txt" if prefix+"UFL.txt" in entries else "LICENCE.txt")
        notice,license_record=fixed(lp,args.cache,entries,tree["sha"]);license_record["notice"]=notice.decode()
        if "UBUNTU FONT LICENCE Version 1.0" not in license_record["notice"]:raise ValueError("Unexpected UFL source")
        for path in sorted(entries):
            if not path.startswith(prefix) or not path.endswith(".ttf") or not (args.cache/path).is_file():continue
            data,font=fixed(path,args.cache,entries,tree["sha"])
            if "[" in path:
                variable.append(dict(family=family,font=font,license=license_record,license_identifier="Ubuntu-font-1.0",cache_path=path,license_cache_path=lp));continue
            native,weight,style=old.face(data)
            if weight not in (400,700):continue
            if family!=native:raise ValueError("Ubuntu source family differs")
            record=pin_record(family,native,weight,style,font,license_record,"Ubuntu-font-1.0","Original Ubuntu font, unchanged bytes; complete Ubuntu Font Licence retained.")
            records[record["source"]["catalog_id"]]=record
    records=sorted(records.values(),key=lambda r:r["source"]["catalog_id"])
    target.write_text(json.dumps(records,ensure_ascii=False,indent=2)+"\n")
    (out.OUT/"supplemental_variable_inputs.json").write_text(json.dumps(variable,ensure_ascii=False,indent=2)+"\n")
    out.emit(records,out.OUT/"supplemental_generated.rs")
    print(json.dumps(dict(static_pins=len(records),variable_sources=len(variable))))

if __name__=="__main__": main()
