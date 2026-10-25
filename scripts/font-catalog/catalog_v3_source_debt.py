#!/usr/bin/env python3
"""Reconcile compiled pin metadata only; never open or revalidate font bytes."""
import argparse
import json
import re
from pathlib import Path
import generate as old
import generate_catalog_v3 as output

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--tree",type=Path);args=parser.parse_args()
    root=old.OUT.parent;atlas=json.loads((old.ROOT/"tmp/opencut-classic/apps/web/public/fonts/font-atlas.json").read_text())["fonts"]
    browser=json.loads((old.OUT/"browser_catalog.json").read_text())
    covered={(family["family"],face["weight"],face["style"]) for family in browser["families"] for face in family["faces"] if face["catalog_id"]}
    counts={"static_v1":len(covered)}
    for version in (1,2):
        pins=json.loads((root/f"font_derived_v{version}/catalog_pins.json").read_text());counts[f"derived_v{version}"]=len(pins)
        for pin in pins:
            family=pin["face"]["family"] if version==1 else json.loads(pin["proof_json"])["source"]["source_family"]
            covered.add((family,pin["face"]["weight"],pin["face"]["style"]))
    for filename in ["static_source_pins.json","color_source_pins.json","supplemental_source_pins.json","distribution_source_pins.json"]:
        records=json.loads((output.OUT/filename).read_text());counts[filename]=len(records)
        covered.update((pin["display_family"],pin["source"]["weight"],pin["source"]["style"]) for pin in records)
    tree=json.loads(args.tree.read_text()) if args.tree else {"sha":old.COMMIT};pending={}
    for filename in ["official_variable_inputs.json","supplemental_variable_inputs.json"]:
        for pin in json.loads((output.OUT/filename).read_text()):pending[pin["family"]]=filename
    rows=[]
    for family in sorted(set(atlas)-{f for f,w,s in covered if w==400 and s=="normal"}):
        normalized=re.sub("[^a-z0-9]","",family.lower())
        if "tree" not in tree: raise ValueError("A remaining regular-family source needs the original full --tree input")
        paths=[e["path"] for e in tree["tree"] if len(e["path"].split("/"))>=3 and e["path"].split("/")[1]==normalized]
        if family in pending:category="fixed_variable_source_acquired_awaiting_derived_pin"
        elif family=="Material Symbols":category="ambiguous_generic_family_no_same_name_in_official_icons_source"
        elif family=="Pushster":category="fixed_source_bytes_are_lobster_not_a_classic_label_alias"
        else:category="explicit_source_derivation_or_metadata_admission_pending"
        rows.append(dict(family=family,category=category,fixed_tree_paths=paths,pending_input=pending.get(family)))
    applicable={(f,w,s) for f,v in atlas.items() for w in (400,700) for s in ("normal","italic") if str(w)+("i" if s=="italic" else "") in v["s"]}
    missing=[]
    absent_bold_italic={"Bona Nova","Bona Nova SC","Cardo","Gudea","Judson","Lekton","Libertinus Sans","Neuton","Old Standard TT","Trochut","Zain"}
    for family,weight,style in sorted(applicable-covered):
        reason="pending_explicit_source_admission"
        if (weight,style)==(700,"italic") and family in absent_bold_italic:reason="no_bold_italic_or_variable_source_in_fixed_tree_browser_synthesis_is_not_a_source_face"
        elif family=="Material Symbols":reason="ambiguous_generic_family_no_same_name_in_official_icons_source"
        elif family=="Pushster":reason="fixed_source_bytes_are_lobster_not_a_classic_label_alias"
        elif family in pending:reason="fixed_variable_source_acquired_awaiting_derived_pin"
        missing.append(dict(family=family,weight=weight,style=style,reason=reason))
    document=dict(scope="source_data_only_not_native_or_product_acceptance",source_commit=tree["sha"],counts=counts,classic_family_count=len(atlas),
        applicable_face_count=len(applicable),covered_applicable_faces=len(applicable & covered),missing_applicable_faces=missing,
        missing_regular_family_count=len(rows),families=rows,
        historical_source_evidence="supplemental_source_pins.json: fixed official commit and exact font/license blobs; no substitution with successor families",
        classic_icon_behavior="Classic google-fonts.ts loads every atlas family through css2 and document.fonts.load; no special generic Material Symbols character mapping exists")
    (output.OUT/"source_debt.json").write_text(json.dumps(document,ensure_ascii=False,indent=2)+"\n")
    print(json.dumps({k:document[k] for k in ["counts","covered_applicable_faces","missing_regular_family_count"]}))

if __name__=="__main__":main()
