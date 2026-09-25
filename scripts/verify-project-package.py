#!/usr/bin/env python3
"""Real owner-local project package exchange across independent workspaces."""
import argparse
import hashlib
import json
import shutil
import subprocess
import time
from pathlib import Path


def verify(binary: Path, artifacts_dir: Path) -> dict:
    run = artifacts_dir.resolve() / f"package-{time.time_ns()}"
    origin, target = run / "origin", run / "target"
    origin.mkdir(parents=True)
    target.mkdir()
    source = origin / "source.mp4"
    subprocess.run([
        "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "color=red:s=64x48:r=10:d=1", "-c:v", "libx264",
        "-pix_fmt", "yuv420p", str(source),
    ], check=True, timeout=30)
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    trace = []

    def cli(workspace, args, payload=None, error=None):
        process = subprocess.run(
            [str(binary), "--database", str(workspace / "workspace.sqlite"),
             "--json", *args],
            input=json.dumps(payload) if payload is not None else None,
            text=True, capture_output=True, timeout=90,
        )
        result = json.loads(process.stdout)
        trace.append({"workspace": workspace.name, "command": args, "response": result})
        if error:
            assert process.returncode and result["error"]["code"] == error, (result, process.stderr)
        else:
            assert process.returncode == 0 and result["error"] is None, (result, process.stderr)
        return result.get("data")

    def call(workspace, name, payload, error=None):
        return cli(workspace, ["call", name, "--input", "-"], payload, error)

    def snapshot(workspace, project):
        return call(workspace, "opencut_project_inspect", {"project_id": project})

    def time_value(ticks):
        return {"ticks": str(ticks), "timebase": 10}

    span = {"start": time_value(0), "duration": time_value(10)}
    created = call(origin, "opencut_project_create", {
        "project": {"id": "source", "name": "Portable", "sequence_id": "main",
                    "width": 64, "height": 48, "fps": {"numerator": 10, "denominator": 1}},
        "idempotency_key": "create",
    })
    bound = call(origin, "opencut_edit_apply", {"project_id": "source", "request": {
        "expected_revision": created["revision"], "idempotency_key": "bind",
        "label": "Bind source", "operations": [
            {"type": "asset.bind", "asset": {"id": "video", "name": "Video",
             "kind": "video", "uri": str(source), "duration": time_value(10), "metadata": {}}},
            {"type": "track.create", "sequence_id": "main", "track": {
                "id": "v", "name": "Video", "kind": "video", "enabled": True,
                "muted": False, "locked": False, "clips": [], "params": {}}},
            {"type": "clip.insert", "sequence_id": "main", "track_id": "v", "clip": {
                "id": "clip", "asset_id": "video", "timeline_range": span,
                "source_range": span, "params": {}, "channels": []}},
        ]}})
    fixed_revision = bound["revision"]
    linked = call(origin, "opencut_project_package_export", {
        "project_id": "source", "revision": fixed_revision,
        "destination": str(run / "linked.opencut"), "mode": "linked"})
    assert not linked["self_contained_media"]
    portable = call(origin, "opencut_project_package_export", {
        "project_id": "source", "revision": fixed_revision,
        "destination": str(run / "portable.opencut"), "mode": "self_contained"})
    assert portable["self_contained_media"] and not portable["fully_reproducible"]
    manifest = json.loads((Path(portable["package_path"]) / "manifest.json").read_text())
    assert manifest["assets"][0]["content_hash"] == source_hash
    assert manifest["source_revision"] == fixed_revision
    linked_request = {"package_path": linked["package_path"],
                      "target_project_id": "linked", "expected_manifest_sha256": linked["manifest_sha256"],
                      "idempotency_key": "import-linked"}
    call(target, "opencut_project_package_import", linked_request)
    shutil.rmtree(origin)
    call(target, "opencut_project_package_import", {
        **linked_request, "target_project_id": "linked-after-removal",
        "idempotency_key": "missing-linked"}, error="PACKAGE_IO_ERROR")
    request = {"package_path": portable["package_path"], "target_project_id": "restored",
               "expected_manifest_sha256": portable["manifest_sha256"],
               "idempotency_key": "import-portable"}
    imported = call(target, "opencut_project_package_import", request)
    assert imported["source_revision"] == fixed_revision
    assert snapshot(target, "restored")["revision"] == imported["revision"]
    edited = call(target, "opencut_edit_apply", {"project_id": "restored", "request": {
        "expected_revision": imported["revision"], "idempotency_key": "continued-edit",
        "label": "Continue after relocation", "operations": [{
            "type": "parameter.set_clip", "sequence_id": "main", "track_id": "v",
            "clip_id": "clip", "params": {"opacity": 0.5}}]}})
    job = call(target, "opencut_job_start", {"kind": "render.project",
        "idempotency_key": "render-restored", "input": {"project_id": "restored",
        "revision": edited["revision"], "sequence_id": "main", "container": "mp4",
        "video_codec": "h264"}})
    worker = subprocess.run([str(binary), "--database", str(target / "workspace.sqlite"),
        "worker", "run", "--once"], capture_output=True, text=True, timeout=90)
    assert worker.returncode == 0, worker.stderr
    completed = call(target, "opencut_job_get", {"job_id": job["job_id"]})
    assert completed["state"] == "succeeded", completed
    artifact = completed["result"]["artifact"]
    video = Path(artifact["path"])
    assert hashlib.sha256(video.read_bytes()).hexdigest() == artifact["content_hash"]
    rgb = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(video),
        "-frames:v", "1", "-vf", "scale=1:1", "-f", "rawvideo",
        "-pix_fmt", "rgb24", "-"], timeout=30)
    assert 105 < rgb[0] < 150 and rgb[1] < 20 and rgb[2] < 20, list(rgb)
    shutil.rmtree(portable["package_path"])
    assert call(target, "opencut_project_package_import", request) == imported
    report = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "source_sha256": source_hash, "manifest_sha256": portable["manifest_sha256"],
              "source_revision": fixed_revision, "imported_revision": imported["revision"],
              "render_sha256": artifact["content_hash"], "checks": [
                  "Linked import needs verified source bytes",
                  "Self-contained import survives deleted source workspace",
                  "Imported project accepts edit and renders independently decoded video",
                  "Exact import retry succeeds after package removal",
              ], "trace": trace}
    (run / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
    return {"report": str(run / "report.json"), "checks": report["checks"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve(), args.artifacts_dir), ensure_ascii=False))
