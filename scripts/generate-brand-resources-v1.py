#!/usr/bin/env python3
"""Compile the reviewed Simple Icons release, never fetch sources at runtime.

Inputs are a locally retained fixed official archive and pinned SPDX license texts.
No dependency installation, upstream code execution, extraction or product tests.
Only admitted SVG bytes are redistributed; all 3466 metadata records stay searchable.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import xml.etree.ElementTree as ET
from urllib.parse import urlsplit

RELEASE = "16.35.0"
COMMIT = "ed9a5192b24ad2f169bf1009931da0df27d5a49a"
TAG_OBJECT = "37ada29537474766bd71595af691abfe295798ef"
ARCHIVE_SHA256 = "dc719873cd6c44bfefc5d82f3c0eef99bc355c89a959e576119d83765b305c5c"
SPDX_COMMIT = "d46e94e2c78ceede1cfc63cfa0396472d2798d4c"
LICENSE_HASHES = {
    "CC0-1.0": "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499",
    "CC-BY-4.0": "d557539df68e771cc1eedcc91d13f70fca930e508d11eedcafa4b15db49e3744",
    "CC-BY-3.0": "e6bc9e9c474700b708f568bac9e5a8a9bcb2b1dad53442f5ba449fcb848b8e76",
    "CC-BY-2.5": "82d2eddedf451177a12c9753e518352f7f90fd4ca3542111c499be981e9a9600",
    "CC-BY-SA-4.0": "cde7883b9050a1104f4ac19a1572aafd6e5d7323b68351aaf51fbf4beba54966",
    "CC-BY-SA-3.0": "3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f",
    "CC-BY-SA-2.5": "dbe7fadf81c75e0cb15a0cb338a6f42a7b34dced43958a4921ca056ca1b87a93",
    "CC-BY-SA-2.0": "6d6b3bb586e5f4ae7e6a955556442fbfcc2a6cedb89be5e4e0492b2983e40760",
}
LICENSE_PATHS = {"CC0-1.0": "publicdomain/zero/1.0", **{
    "CC-" + family.upper() + "-" + version: "licenses/" + family + "/" + version
    for family, versions in [("by", ["4.0", "3.0", "2.5"]), ("by-sa", ["4.0", "3.0", "2.5", "2.0"])]
    for version in versions}}

def digest(data): return hashlib.sha256(data).hexdigest()
def encoded(value): return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()
def strings(value):
    if isinstance(value, str): return [value]
    if isinstance(value, list): return [s for v in value for s in strings(v)]
    if isinstance(value, dict): return [s for v in value.values() for s in strings(v)]
    return []

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--licenses", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1] / "crates/editor-application/src/brand_resources_v1")
    args = parser.parse_args()
    raw = args.archive.read_bytes()
    if len(raw) != 2419402 or digest(raw) != ARCHIVE_SHA256: raise ValueError("Fixed Simple Icons archive differs")
    files = {}
    with tarfile.open(args.archive) as archive:
        for member in archive.getmembers():
            p = PurePosixPath(member.name)
            if p.is_absolute() or ".." in p.parts: raise ValueError("Unsafe archive member")
            name = "/".join(p.parts[1:])
            # Never extract or follow unrelated source-tree entries (.nvmrc is
            # an upstream symlink). Only the exact data subset is consumed.
            if not (name.startswith("icons/") or name in ["data/simple-icons.json", "LICENSE.md", "DISCLAIMER.md", "package.json"]): continue
            if member.issym() or member.islnk(): raise ValueError("Linked catalog source")
            if member.isdir(): continue
            if not member.isfile() or member.size > 8 * 1024 * 1024: raise ValueError("Unexpected archive entry")
            if name in files: raise ValueError("Duplicate archive entry")
            if name.startswith("icons/") or name in ["data/simple-icons.json", "LICENSE.md", "DISCLAIMER.md", "package.json"]:
                files[name] = archive.extractfile(member).read()
    if json.loads(files["package.json"])["version"] != RELEASE: raise ValueError("Wrong release")
    metadata = json.loads(files["data/simple-icons.json"])
    if len(metadata) != 3466: raise ValueError("Incomplete catalog")
    icons = {}
    for path, svg in files.items():
        if not path.startswith("icons/"): continue
        root = ET.fromstring(svg)
        ns = "{http://www.w3.org/2000/svg}"
        if root.tag != ns + "svg" or root.get("viewBox") != "0 0 24 24": raise ValueError("Unexpected SVG canvas")
        title = root.find(ns + "title").text
        # Simple Icons' fixed, path-only SVG subset: no URL, scripting, CSS or images.
        if len(svg) > 262144 or set(root.attrib) - {"viewBox", "role"}: raise ValueError("Unsafe SVG root")
        for child in root:
            if child.tag not in [ns + "path", ns + "title"] or list(child): raise ValueError("Unsafe SVG child")
            if child.tag == ns + "path" and (set(child.attrib) != {"d"} or not re.fullmatch(r"[MmZzLlHhVvCcSsQqTtAaEe0-9+.,\s-]+", child.get("d", ""))): raise ValueError("Unsafe SVG path")
            if child.tag == ns + "title" and child.attrib: raise ValueError("Unsafe SVG title")
        icons.setdefault(title, []).append((Path(path).stem, svg))
    license_sources = json.loads((args.licenses / "sources.json").read_text())
    licenses = {}
    for identifier, path in LICENSE_PATHS.items():
        text = (args.licenses / (identifier + ".txt")).read_bytes()
        source = license_sources[identifier]
        if digest(text) != LICENSE_HASHES[identifier] or source["sha256"] != LICENSE_HASHES[identifier] or len(text) != source["size"] or source["url"] != f"https://raw.githubusercontent.com/spdx/license-list-data/{SPDX_COMMIT}/text/{identifier}.txt": raise ValueError("License text differs")
        licenses[identifier] = {"text": text.decode(), "source": source, "url": "https://creativecommons.org/" + path + "/"}
    records = []
    admitted = {}
    explicit_slugs = {item["slug"] for item in metadata if "slug" in item}
    for item in metadata:
        for url in [item["source"], item.get("guidelines"), item.get("license", {}).get("url")]:
            if url is not None:
                parsed = urlsplit(url)
                if parsed.scheme not in ["http", "https"] or not parsed.hostname or parsed.username or parsed.password: raise ValueError("Unsafe public metadata URL")
        choices = [pair for pair in icons[item["title"]] if pair[0] == item["slug"]] if "slug" in item else [pair for pair in icons[item["title"]] if pair[0] not in explicit_slugs]
        if len(choices) != 1: raise ValueError("Ambiguous upstream title/slug")
        slug, svg = choices[0]
        if not re.fullmatch(r"[a-z0-9_]+", slug): raise ValueError("Unsafe slug")
        identifier = item.get("license", {}).get("type")
        available = identifier in licenses
        reason = None if available else ("No per-icon license is recorded; the collection CC0 license does not grant this brand's rights." if identifier is None else "The recorded license requires additional rights or original notices that are not included in this offline approval set.")
        record = {"entry": {
            "id": "simple-icons:" + RELEASE + ":" + slug, "name": item["title"], "slug": slug,
            "keywords": sorted(set(strings(item.get("aliases", {})))), "color": item["hex"],
            "source_url": item["source"], "guidelines_url": item.get("guidelines"),
            "svg_sha256": digest(svg), "svg_size": len(svg), "license_identifier": identifier,
            "license_url": item.get("license", {}).get("url") or (licenses[identifier]["url"] if available else None),
            "import_available": available, "unavailable_reason": reason,
        }, "metadata": item}
        records.append(record)
        if available: admitted[record["entry"]["id"]] = svg.decode()
    records.sort(key=lambda r: r["entry"]["id"])
    inventory = encoded(records)
    args.output.mkdir(parents=True, exist_ok=True)
    for name, value in [("inventory.json", inventory), ("sources.json", encoded(admitted)), ("licenses.json", encoded(licenses)), ("LICENSE.md", files["LICENSE.md"]), ("DISCLAIMER.md", files["DISCLAIMER.md"])]:
        (args.output / name).write_bytes(value)
    collection = {"name": "Simple Icons", "release": RELEASE, "commit": COMMIT,
        "source_url": "https://github.com/simple-icons/simple-icons/tree/" + COMMIT,
        "inventory_sha256": digest(inventory),
        "collection_license": {"identifier": "CC0-1.0", "source_url": "https://github.com/simple-icons/simple-icons/blob/" + COMMIT + "/LICENSE.md", "notice": files["LICENSE.md"].decode(), "attribution": "Simple Icons contributors; collection metadata only, not a blanket icon license."},
        "disclaimer": files["DISCLAIMER.md"].decode()}
    (args.output / "collection.json").write_bytes(encoded(collection))
    (args.output / "provenance.json").write_bytes(encoded({"repository": "https://github.com/simple-icons/simple-icons", "release": RELEASE, "commit": COMMIT, "tag_object": TAG_OBJECT, "archive_sha256": ARCHIVE_SHA256, "upstream_metadata_sha256": digest(files["data/simple-icons.json"]), "metadata_count": len(records), "admitted_svg_count": len(admitted), "license_sources": license_sources, "policy": "Only explicitly recorded CC0/BY/BY-SA; preserve all provided source, guideline and attribution metadata. No trademark permission or endorsement is granted. Missing/custom/restrictive licenses or unbundled original notices remain unavailable."}))
    print(f"Wrote {len(records)} metadata records and {len(admitted)} admitted SVG sources; no product verification run.")

if __name__ == "__main__": main()
