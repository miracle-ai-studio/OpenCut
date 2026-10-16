#!/usr/bin/env python3
"""Pin only the previously recorded color/bitmap exclusions, entirely offline.

No source download, broad inventory scan, font rewrite, runtime verification or
catalog publication. Variable entries are inputs for the separate V2 derivation
pipeline; this tool never presents a variable source as an admitted static face.
"""
import argparse
import hashlib
import io
import json
import urllib.parse
from pathlib import Path

import generate as g
from fontTools.ttLib import TTFont


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fixed(cache, path, tree):
    data = (cache / path).read_bytes()
    blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
    if (len(data), blob) != (tree[path]["size"], tree[path]["sha"]):
        raise ValueError(f"fixed source tree identity differs: {path}")
    return data


def profile(font):
    if any(tag in font for tag in ("CBDT", "sbix", "bdat")):
        raise ValueError("color bitmap source needs a separate rendering profile")
    if "COLR" in font:
        if "CPAL" not in font or font["COLR"].version not in (0, 1):
            raise ValueError("unsupported color table profile")
        return f"colr_v{font['COLR'].version}"
    if "SVG " in font:
        raise ValueError("SVG-only source needs a separate rendering profile")
    if "EBDT" in font or "EBLC" in font:
        if not all(tag in font for tag in ("EBDT", "EBLC")):
            raise ValueError("incomplete monochrome bitmap tables")
        if any(strike.bitmapSizeTable.bitDepth != 1 for strike in font["EBLC"].strikes):
            raise ValueError("bitmap strikes are not monochrome")
        return "monochrome_bitmap"
    raise ValueError("recorded color source has no supported color/bitmap table")


def family(font):
    # Same Unicode/English/typographic ordering as the existing native inspector.
    names = []
    for entry in font["name"].names:
        if entry.nameID in (1, 16) and entry.isUnicode():
            english = entry.platformID == 3 and entry.langID == 0x409
            value = entry.toUnicode()
            names.append(((not english, entry.nameID != 16, value), value))
    if not names:
        raise ValueError("missing Unicode family")
    return min(names)[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    document = json.loads(args.tree.read_text())
    if document.get("truncated") or document.get("sha") != g.COMMIT:
        raise ValueError("complete fixed source tree required")
    tree = {entry["path"]: entry for entry in document["tree"]}
    static = json.loads((g.OUT / "catalog_source_report.json").read_text())["failures"]
    variable = json.loads((g.OUT.parent / "font_derived_v1/catalog_source_report.json").read_text())["failures"]
    static = [row for row in static if row["reason"] == "native unsupported color or bitmap tables"]
    variable = [row for row in variable if row["reason"] == "native unsupported color or bitmap tables"]
    pins, inputs, excluded = [], [], []
    for row, is_variable in [(r, False) for r in static] + [(r, True) for r in variable]:
        path = row["source_path"] if is_variable else row["path"]
        lp = "/".join(path.split("/")[:2]) + "/OFL.txt"
        data, notice = fixed(args.cache, path, tree), fixed(args.cache, lp, tree)
        if len(data) > 32 * 1024 * 1024:
            excluded.append({**row, "reason": "source exceeds unchanged V1 font envelope"})
            continue
        license_text = notice.decode("utf-8")
        if "SIL OPEN FONT LICENSE" not in license_text or len(notice) > 65536:
            raise ValueError("original bounded OFL notice required")
        with TTFont(io.BytesIO(data), lazy=True) as font:
            render_profile = profile(font)
            native_family = family(font)
            weight = int(font["OS/2"].usWeightClass)
            style = "italic" if font["OS/2"].fsSelection & 1 or font["post"].italicAngle else "normal"
            source = dict(source_commit=g.COMMIT, source_path=path, source_sha256=sha(data), source_size=len(data), source_git_blob=tree[path]["sha"], license_path=lp, license_sha256=sha(notice), license_git_blob=tree[lp]["sha"], license_identifier="OFL-1.1", family=row["family"], weight=row.get("weight", weight), style=row.get("style", style), render_profile=render_profile)
            if is_variable:
                axes = {axis.axisTag: axis.defaultValue for axis in font["fvar"].axes}
                if "wght" in axes:
                    axes["wght"] = row["weight"]
                if "ital" in axes:
                    axes["ital"] = 1 if row["style"] == "italic" else 0
                inputs.append({**source, "axes": dict(sorted(axes.items()))})
            else:
                if "fvar" in font:
                    raise ValueError("static pin unexpectedly variable")
                suffix = {(400, "normal"): "regular", (700, "normal"): "bold", (400, "italic"): "italic", (700, "italic"): "bold-italic"}[weight, style]
                pin = dict(catalog_id=f"google-{g.norm(row['family'])}-{suffix}-color-v3", family=native_family, weight=weight, style=style, source_commit=g.COMMIT, source_url=g.BASE + urllib.parse.quote(path), content_hash=sha(data), size_bytes=len(data), license_url=g.BASE + urllib.parse.quote(lp), license_sha256=sha(notice), license_identifier="OFL-1.1", license_notice=license_text, attribution=f"Google Fonts: {row['family']} ({g.COMMIT})")
                pins.append(dict(source=pin, render_profile=render_profile, display_family=row["family"], alias_reason=None if native_family == row["family"] else "original_family_name", postscript_name=font["name"].getDebugName(6), source_path=path))
    args.output.mkdir(parents=True, exist_ok=True)
    for name, value in [("color_source_pins.json", sorted(pins, key=lambda p:p["source"]["catalog_id"])), ("color_variable_inputs.json", sorted(inputs,key=lambda p:(p["family"],p["weight"],p["style"]))), ("color_source_generation.json",dict(source_commit=g.COMMIT, static_count=len(pins), variable_input_count=len(inputs), excluded=excluded, evidence="source identities only; native admission and independent rendering remain unverified"))]:
        (args.output / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
