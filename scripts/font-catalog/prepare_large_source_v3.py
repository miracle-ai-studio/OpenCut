#!/usr/bin/env python3
"""Acquire the two Chiron targets' one fixed source; never publish runtime pins.

Only these three fixed paths may be fetched. Source and license bytes remain in
the existing external cache. The JSON is recipe input, not native/render proof.
"""
import argparse
import hashlib
import json
import os
import time
import urllib.parse
import urllib.request
from pathlib import Path

import generate as g
from fontTools.ttLib import TTFont

DIRECTORY = "ofl/chirongoroundtc/"
SOURCE = DIRECTORY + "ChironGoRoundTC[wght].ttf"
PATHS = (SOURCE, DIRECTORY + "METADATA.pb", DIRECTORY + "OFL.txt")
MAX_SOURCE_BYTES = 64 * 1024 * 1024


def acquire(cache, path, record, download):
    limit = MAX_SOURCE_BYTES if path == SOURCE else 65536
    size = record["size"]
    if not 0 < size <= limit:
        raise ValueError(f"fixed source exceeds its explicit envelope: {path}")
    target = cache / path
    present = target.exists()
    if not present and not download:
        raise ValueError(f"source not cached; use --download for fixed acquisition: {path}")
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(target.name + ".v3-part")
    sha256, blob = hashlib.sha256(), hashlib.sha1(b"blob " + str(size).encode() + b"\0")
    deadline, length = time.monotonic() + 120, 0
    output = None
    try:
        if present:
            stream = target.open("rb")
        else:
            # urllib preserves the environment's scheme-specific HTTPS proxy.
            # The URL is derived only from the reviewed commit and fixed paths.
            stream = urllib.request.urlopen(g.BASE + urllib.parse.quote(path), timeout=35)
            if stream.geturl() != g.BASE + urllib.parse.quote(path):
                raise ValueError("fixed source unexpectedly redirected")
            output = temporary.open("xb")
        with stream:
            while True:
                if time.monotonic() >= deadline:
                    raise TimeoutError("fixed source acquisition deadline exceeded")
                chunk = stream.read(65536)
                if not chunk:
                    break
                length += len(chunk)
                if length > size or length > limit:
                    raise ValueError("source exceeded pinned size")
                sha256.update(chunk)
                blob.update(chunk)
                if output:
                    output.write(chunk)
        if length != size or blob.hexdigest() != record["sha"]:
            raise ValueError(f"fixed tree identity mismatch: {path}")
        if output:
            output.close()
            output = None
            os.replace(temporary, target)
        return sha256.hexdigest()
    finally:
        if output:
            output.close()
        if temporary.exists():
            temporary.unlink()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--tree", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    tree = json.loads(args.tree.read_text())
    if tree.get("truncated") or tree["sha"] != g.COMMIT:
        raise ValueError("complete fixed source tree required")
    records = {row["path"]: row for row in tree["tree"] if row["path"] in PATHS}
    hashes = {path: acquire(args.cache, path, records[path], args.download) for path in PATHS}
    notice = (args.cache / PATHS[2]).read_text()
    if "SIL OPEN FONT LICENSE" not in notice:
        raise ValueError("complete original OFL license required")
    with TTFont(args.cache / SOURCE, lazy=True) as font:
        if "fvar" not in font or any(tag in font for tag in ("COLR", "SVG ", "CBDT", "sbix", "EBDT")):
            raise ValueError("source differs from the explicit variable outline profile")
        axes = {axis.axisTag: axis.defaultValue for axis in font["fvar"].axes}
        weight_axis = next(axis for axis in font["fvar"].axes if axis.axisTag == "wght")
        inputs = []
        for weight in (400, 700):
            if not weight_axis.minValue <= weight <= weight_axis.maxValue:
                raise ValueError("target weight is outside the fixed source axis")
            inputs.append(dict(
                source_commit=g.COMMIT, source_path=SOURCE,
                source_sha256=hashes[SOURCE], source_size=records[SOURCE]["size"],
                source_git_blob=records[SOURCE]["sha"],
                license_path=PATHS[2], license_sha256=hashes[PATHS[2]],
                license_git_blob=records[PATHS[2]]["sha"], license_identifier="OFL-1.1",
                family="Chiron GoRound TC", weight=weight, style="normal",
                render_profile="static_outline", axes=dict(sorted({**axes, "wght": weight}.items())),
                metadata_path=PATHS[1], metadata_sha256=hashes[PATHS[1]],
                source_byte_limit=MAX_SOURCE_BYTES,
            ))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(inputs, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"inputs": len(inputs), "source_bytes": records[SOURCE]["size"],
                      "source_sha256": hashes[SOURCE], "native_verified": False}))


if __name__ == "__main__":
    main()
