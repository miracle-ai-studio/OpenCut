#!/usr/bin/env python3
"""CLI read-only capability journey, with independent filesystem and SQLite checks."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile


def verify(binary):
    root = Path(tempfile.mkdtemp(prefix="opencut-read-only-"))
    database = root / "workspace.sqlite"
    trace = []

    def call(name, arguments, error=None):
        result = subprocess.run(
            [str(binary), "--database", str(database), "--json", "call", name, "--input", "-"],
            input=json.dumps(arguments), text=True, capture_output=True, timeout=30)
        response = json.loads(result.stdout)
        trace.append({"tool": name, "arguments": arguments, "response": response})
        if error:
            assert result.returncode != 0 and response["error"]["code"] == error, response
        else:
            assert result.returncode == 0 and response["error"] is None, response
        return response["data"]

    project = call("opencut_project_create", {
        "project": {"id": "readonly", "name": "Read-only journey", "sequence_id": "main",
                    "width": 160, "height": 90, "fps": {"numerator": 10, "denominator": 1}},
        "idempotency_key": "create"})

    def state():
        with sqlite3.connect(database) as connection:
            return list(connection.iterdump())

    before = state()
    assert not any(p.is_dir() for p in root.iterdir())
    assert call("opencut_plugin_list", {}) == []
    plugin = {"id": "org.example.missing", "version": "1.0.0"}
    call("opencut_plugin_inspect", plugin, "NOT_FOUND")
    call("opencut_plugin_plan", {**plugin, "operation_name": "org.example.missing.plan",
         "project_id": "readonly", "revision": project["revision"], "parameters": {}}, "NOT_FOUND")
    media = call("opencut_media_inspect", {"content_hash": "0" * 64})
    assert media == {"content_hash": "0" * 64, "metadata_hash": None, "asset": None}, media
    call("opencut_media_inspect", {"content_hash": "../escape"}, "INVALID_ARGUMENT")
    report = call("opencut_render_preflight", {
        "project_id": "readonly", "revision": project["revision"], "sequence_id": "missing",
        "container": "mp4", "video_codec": "h264"})
    assert not report["ready"] and report["issues"][0]["code"] == "SEQUENCE_NOT_FOUND", report
    assert call("opencut_project_inspect", {"project_id": "readonly"}) == project
    assert state() == before, "Read-only calls changed database contents"
    assert not any(p.is_dir() for p in root.iterdir()), list(root.iterdir())
    result = {"status": "passed", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "checks": ["empty plugin catalog", "missing plugin inspect and plan", "missing media metadata",
                         "invalid media identity", "fixed revision preflight issue", "database unchanged",
                         "no plugin media or asset directories created"], "root": str(root)}
    (root / "trace.json").write_text(json.dumps(trace, indent=2))
    (root / "report.json").write_text(json.dumps(result, indent=2))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve()), indent=2))
