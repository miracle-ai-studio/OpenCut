#!/usr/bin/env python3
"""Bounded source-data acquisition for the explicitly reviewed V3 gaps.

Uses previously captured official directory responses and fixed commits only.
No product test is run and no previously admitted face is rescanned.
"""
import argparse
import concurrent.futures
import hashlib
import io
import json
import re
import urllib.parse
import urllib.request
from pathlib import Path
import generate as old
import generate_catalog_v3 as out
from collect_supplemental_v3 import pin_record, blob

FAMILIES={"arimamadurai":"Arima Madurai","fredokaone":"Fredoka One","gentiumbookbasic":"Gentium Book Basic","meriendaone":"Merienda One",
    "sourcesanspro":"Source Sans Pro","sourceserifpro":"Source Serif Pro","bbhsansbartle":"BBH Sans Bartle","bbhsansbogle":"BBH Sans Bogle","bbhsanshegarty":"BBH Sans Hegarty",
    "edunswactcursive":"Edu NSW ACT Cursive","edunswacthandpre":"Edu NSW ACT Hand Pre","eduqldhand":"Edu QLD Hand","edusahand":"Edu SA Hand","eduvicwanthand":"Edu VIC WA NT Hand","eduvicwanthandpre":"Edu VIC WA NT Hand Pre"}
MATERIAL_COMMIT="49d4db35df873165d6bd6ba09b063c7dafbac2f4"
MATERIAL={"MaterialIcons-Regular.ttf":"Material Icons","MaterialIconsOutlined-Regular.otf":"Material Icons Outlined","MaterialIconsRound-Regular.otf":"Material Icons Round","MaterialIconsSharp-Regular.otf":"Material Icons Sharp","MaterialIconsTwoTone-Regular.otf":"Material Icons Two Tone",
    "MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf":"Material Symbols Outlined","MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf":"Material Symbols Rounded","MaterialSymbolsSharp[FILL,GRAD,opsz,wght].ttf":"Material Symbols Sharp"}

def acquire(entry,repo,commit,cache):
    path=entry["path"];limit=32*1024*1024
    if entry["type"] not in ("file","blob") or not 0<entry["size"]<=limit:raise ValueError("Invalid fixed source envelope")
    target=cache/path if repo=="google/fonts" else cache/"official"/repo.replace("/","-")/commit/path
    url=f"https://raw.githubusercontent.com/{repo}/{commit}/"+urllib.parse.quote(path)
    if target.exists():data=target.read_bytes()
    else:
        with urllib.request.urlopen(url,timeout=30) as response:data=response.read(limit+1)
        target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
    if len(data)!=entry["size"] or blob(data)!=entry["sha"]:raise ValueError("Fixed source differs: "+path)
    return data,dict(repository=repo,commit=commit,path=path,url=url,git_blob=entry["sha"],sha256=old.digest(data),size=len(data))

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--metadata",type=Path,required=True);parser.add_argument("--cache",type=Path,required=True);parser.add_argument("--tree",type=Path,required=True)
    args=parser.parse_args();commits=json.loads((args.metadata/"historical_commits.json").read_text())
    jobs=[]
    for folder,commit in commits.items():
        entries=json.loads((args.metadata/(folder+"-contents.json")).read_text());license_entry=next(x for x in entries if x["name"]=="OFL.txt")
        for entry in entries:
            if entry["name"].endswith(".ttf") and ("[" in entry["name"] or folder=="gentiumbookbasic" or re.search(r"-(Regular|Bold|Italic|BoldItalic)\.ttf$",entry["name"])):
                jobs.append((FAMILIES[folder],entry,"google/fonts",commit,license_entry,"OFL-1.1",None))
    material_license=json.loads((args.metadata/"material-license.json").read_text())
    for group in ["material-font","material-variable"]:
        for entry in json.loads((args.metadata/(group+"-contents.json")).read_text()):
            if entry["name"] in MATERIAL:jobs.append((MATERIAL[entry["name"]],entry,"google/material-design-icons",MATERIAL_COMMIT,material_license,"Apache-2.0",None))
    tree=json.loads(args.tree.read_text());entries={e["path"]:e for e in tree["tree"]}
    for family,folder,license_path in [("Kumar One Outline","kumaroneoutline","ofl/kumarone/OFL.txt"),("M PLUS Rounded 1c","mplusrounded1c","ofl/roundedmplus1c/OFL.txt")]:
        license_entry=entries[license_path]
        for path,entry in entries.items():
            if path.startswith("ofl/"+folder+"/") and re.search(r"-(Regular|Bold)\.ttf$",path):
                jobs.append((family,entry,"google/fonts",tree["sha"],license_entry,"OFL-1.1","Full original project license is preserved from the sibling family directory in the same fixed Google source tree."))
    external_licenses={"playwritenzbasicguides":("Playwrite NZ Basic Guides","playwrite-license","TypeTogether/Playwrite","fbc8d9790feeba3a5d0c0b4ccdcf246d3c41526e"),"tinos":("Tinos","tinos-license","googlefonts/tinos","3b4482a99b80ea5fc75f187b1be3120a3f5905b3")}
    for folder,(family,name,repo,commit) in external_licenses.items():
        license_entry=json.loads((args.metadata/(name+".json")).read_text());license_entry=dict(license_entry,license_repository=repo,license_commit=commit)
        for path,entry in entries.items():
            if path.startswith("ofl/"+folder+"/") and path.endswith(".ttf"):
                jobs.append((family,entry,"google/fonts",tree["sha"],license_entry,"OFL-1.1","Full original license is pinned to the official upstream commit identified by this Google source metadata."))
    # De-duplicate downloads before parallel acquisition to avoid concurrent writes.
    requests={}
    for _,entry,repo,commit,license_entry,_,_ in jobs:
        requests[(repo,commit,entry["path"])]=entry
        lr=license_entry.get("license_repository",repo);lc=license_entry.get("license_commit",commit)
        requests[(lr,lc,license_entry["path"])]=license_entry
    def get(item):
        (repo,commit,path),entry=item
        result=acquire(entry,repo,commit,args.cache);print(path,flush=True);return (repo,commit,path),result
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:acquired=dict(executor.map(get,requests.items()))
    target=out.OUT/"supplemental_source_pins.json";records={r["source"]["catalog_id"]:r for r in json.loads(target.read_text())}
    variables=[];rejected=[]
    for family,entry,repo,commit,license_entry,identifier,explanation in jobs:
        data,font=acquired[(repo,commit,entry["path"])];lr=license_entry.get("license_repository",repo);lc=license_entry.get("license_commit",commit)
        notice,license_record=acquired[(lr,lc,license_entry["path"])];license_record=dict(license_record,notice=notice.decode())
        if "[" in entry["path"]:
            variables.append(dict(family=family,font=font,license=license_record,license_identifier=identifier));continue
        try:
            profile="static_outline"
            if family=="Material Icons Two Tone":
                from fontTools.ttLib import TTFont
                from generate_color_sources_v3 import family as actual_family, profile as actual_profile
                with TTFont(io.BytesIO(data),lazy=True) as parsed:
                    native=actual_family(parsed);weight=parsed["OS/2"].usWeightClass
                    style="italic" if parsed["OS/2"].fsSelection & 1 or parsed["post"].italicAngle else "normal"
                    profile=actual_profile(parsed)
                if profile!="colr_v0":raise ValueError("Two Tone requires its reviewed COLR v0 profile")
            else:native,weight,style=old.face(data)
            if family=="M PLUS Rounded 1c" and (native,weight,style) in [("Rounded Mplus 1c",400,"normal"),("Rounded Mplus 1c Bold",700,"normal")]:
                explanation=f'Classic labels this family "M PLUS Rounded 1c"; original English Unicode name declares "{native}". Original font and same-project complete OFL are retained unchanged.'
            elif native!=family:raise ValueError(f"Native family differs: {native}")
            if weight not in (400,700):raise ValueError(f"Native weight differs: {weight}")
            record=pin_record(family,native,weight,style,font,license_record,identifier,re.split(r"\r?\n\r?\n",notice.decode())[0].rstrip() if identifier=="OFL-1.1" else "",explanation)
            record["render_profile"]=profile
            records[record["source"]["catalog_id"]]=record
        except Exception as error:rejected.append(dict(family=family,font=font,license=license_record,reason=str(error)))
    values=sorted(records.values(),key=lambda r:r["source"]["catalog_id"]);target.write_text(json.dumps(values,ensure_ascii=False,indent=2)+"\n");out.emit(values,out.OUT/"supplemental_generated.rs")
    for item in variables:
        for key,target_key in [("font","cache_path"),("license","license_cache_path")]:
            record=item[key];item[target_key]=record["path"] if record["repository"]=="google/fonts" else "official/"+record["repository"].replace("/","-")+"/"+record["commit"]+"/"+record["path"]
    (out.OUT/"official_variable_inputs.json").write_text(json.dumps(variables,ensure_ascii=False,indent=2)+"\n")
    (out.OUT/"official_source_rejections.json").write_text(json.dumps(rejected,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps(dict(supplemental_static_pins=len(values),new_variable_sources=len(variables),static_rejections=len(rejected))),flush=True)

if __name__=="__main__": main()
