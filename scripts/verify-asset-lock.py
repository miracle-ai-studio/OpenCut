#!/usr/bin/env python3
"""Public CLI acceptance: fixed media bytes, retries, preflight and integrity.

Jobs are queued with no worker. Actual source replacement/deletion happens before
execution. FFmpeg independently decodes the resulting artifact pixels.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile


def verify(binary, report=None):
    if report:
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps({"status": "running"}))
    checks = []
    with tempfile.TemporaryDirectory(prefix="opencut-asset-lock-") as folder:
        root = Path(folder)
        database = root / "workspace.sqlite"
        source = root / "source.mp4"
        command = [str(binary), "--database", str(database), "--json"]

        def cli(*args, payload=None, expected_error=None):
            result = subprocess.run(command + list(args), input=json.dumps(payload) if payload is not None else None,
                                    capture_output=True, text=True, timeout=60)
            assert result.stdout, (result.returncode, result.stderr)
            response = json.loads(result.stdout)
            if expected_error:
                assert result.returncode != 0 and response["error"]["code"] == expected_error, response
                return response
            assert result.returncode == 0 and response["error"] is None, (response, result.stderr)
            return response["data"]

        def tool(name, payload):
            return cli("call", name, "--input", "-", payload=payload)

        def generate(color):
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i",
                            f"color=c={color}:s=64x48:r=10:d=1", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(source)],
                           check=True, timeout=30)

        def work(job):
            # Jobs created in the same timestamp bucket need not run in our
            # submission order. Drain available claims, never requeue a failure.
            remaining = sum(item["state"] == "queued" for item in cli("job", "list"))
            observed = cli("job", "get", job["job_id"])
            while observed["state"] == "queued" and remaining:
                subprocess.run(command + ["worker", "run", "--once"], check=True, capture_output=True, timeout=60)
                remaining -= 1
                observed = cli("job", "get", job["job_id"])
            return observed

        def pixels(job, channel):
            assert job["state"] == "succeeded", job
            result = job["result"]
            assert result["asset_locking"] == "media_content_locked", result
            assert result["asset_lock"] == job["input"]["asset_lock"]
            artifact = result["artifact"]
            path = Path(artifact["path"])
            assert hashlib.sha256(path.read_bytes()).hexdigest() == artifact["content_hash"]
            pixel = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(path), "-frames:v", "1",
                                             "-vf", "scale=1:1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
            assert pixel[channel] > 180 and all(pixel[i] < 60 for i in range(3) if i != channel), list(pixel)
            assert not list((root / "artifacts" / job["job_id"]).glob("attempt-*/inputs"))
            events = cli("job", "events", "--job-id", job["job_id"], "--limit", "500")
            assert any(e["event_type"] == "progress" and e["payload"]["stage"] == "resolving_assets" for e in events), events

        generate("red")
        red_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        created = tool("opencut_project_create", {"project": {"id": "locked", "name": "Locked media", "sequence_id": "main",
                       "width": 64, "height": 48, "fps": {"numerator": 10, "denominator": 1}}, "idempotency_key": "create"})
        time_range = {"start": {"ticks": "0", "timebase": 10}, "duration": {"ticks": "10", "timebase": 10}}
        asset = {"id": "source", "name": "source", "kind": "video", "uri": str(source), "duration": time_range["duration"], "metadata": {}}
        unused_path = root / "unused.mp4"
        unused_path.write_bytes(b"unused media content")
        unused = {**asset, "id": "unused", "uri": str(unused_path)}
        track = {"id": "video", "name": "video", "kind": "video", "enabled": True, "muted": False, "locked": False, "clips": [], "params": {}}
        clip = {"id": "clip", "asset_id": "source", "timeline_range": time_range, "source_range": time_range, "params": {}, "channels": []}
        edited = tool("opencut_edit_apply", {"project_id": "locked", "request": {"expected_revision": created["revision"], "idempotency_key": "edit", "label": "bind",
                      "operations": [{"type": "asset.bind", "asset": asset}, {"type": "asset.bind", "asset": unused},
                                     {"type": "track.create", "sequence_id": "main", "track": track},
                                     {"type": "clip.insert", "sequence_id": "main", "track_id": "video", "clip": clip}]}})
        input_value = {"project_id": "locked", "revision": edited["revision"], "sequence_id": "main", "container": "mp4", "video_codec": "h264"}
        unused_path.unlink()
        unused_hash = hashlib.sha256(b"unused media content").hexdigest()
        (root / "asset-store" / unused_hash[:2] / unused_hash).unlink()
        before = cli("job", "list")
        ready = cli("render", "preflight", "--input", "-", payload=input_value)
        assert ready["ready"] and ready["issues"] == [], ready
        assert cli("job", "list") == before
        assert tool("opencut_project_inspect", {"project_id": "locked"})["revision"] == edited["revision"]
        checks.append("preflight is read-only and ignores unused missing media")

        request = {"kind": "render.project", "idempotency_key": "red-export", "input": input_value}
        export = cli("job", "start", "--input", "-", payload=request)
        frame_request = {"kind": "preview.project_frame", "idempotency_key": "red-frame", "input": {
            "project_id": "locked", "revision": edited["revision"], "sequence_id": "main", "requested_time": {"ticks": "0", "timebase": 10}, "alignment": "exact"}}
        frame = cli("job", "start", "--input", "-", payload=frame_request)
        lock = export["input"]["asset_lock"]
        assert lock == frame["input"]["asset_lock"] and len(lock["assets"]) == 1
        assert lock["assets"][0]["content_hash"] == red_hash
        assert export["input"]["snapshot"]["project"]["assets"][0]["uri"] == str(source)
        assert export["state"] == frame["state"] == "queued"
        checks.append("project export and frame bind original snapshot to the same media content")

        generate("blue")
        assert cli("job", "start", "--input", "-", payload=request)["job_id"] == export["job_id"]
        same_revision = cli("job", "start", "--input", "-", payload={**request, "idempotency_key": "same-revision"})
        assert same_revision["input"]["asset_lock"] == lock
        replaced = tool("opencut_edit_apply", {"project_id": "locked", "request": {
            "expected_revision": edited["revision"], "idempotency_key": "replace-blue", "label": "Replace media version",
            "operations": [{"type": "asset.replace", "asset": asset}]}})
        blue_request = {**request, "idempotency_key": "blue-export", "input": {**input_value, "revision": replaced["revision"]}}
        blue = cli("job", "start", "--input", "-", payload=blue_request)
        assert blue["input"]["asset_lock"] != lock
        source.unlink()
        assert cli("job", "start", "--input", "-", payload=request)["job_id"] == export["job_id"]
        assert cli("job", "start", "--input", "-", payload=frame_request)["job_id"] == frame["job_id"]
        checks.append("same revision keeps content across new keys; explicit replacement creates a new version")
        ready_without_source = cli("render", "preflight", "--input", "-", payload=input_value)
        assert ready_without_source["ready"], ready_without_source
        checks.append("preflight resolves immutable versions when provenance paths are missing")
        pixels(work(export), 0)
        pixels(work(frame), 0)
        pixels(work(same_revision), 0)
        pixels(work(blue), 2)
        checks.append("independent decoded export/frame retain red and new job renders blue after source deletion")
        checks.append("verified private inputs are cleaned and resolving-assets progress is durable")

        generate("blue")
        race_request = {**blue_request, "idempotency_key": "race"}
        with ThreadPoolExecutor(max_workers=4) as pool:
            raced = list(pool.map(lambda _: cli("job", "start", "--input", "-", payload=race_request), range(4)))
        assert len({job["job_id"] for job in raced}) == 1
        events = cli("job", "events", "--job-id", raced[0]["job_id"], "--limit", "500")
        assert [event["event_type"] for event in events] == ["queued"], events
        pixels(work(raced[0]), 2)
        checks.append("concurrent same-key capture commits one durable job and one queued event")

        corrupt = cli("job", "start", "--input", "-", payload={**blue_request, "idempotency_key": "corrupt"})
        content_hash = corrupt["input"]["asset_lock"]["assets"][0]["content_hash"]
        object_path = root / "asset-store" / content_hash[:2] / content_hash
        object_path.unlink()
        object_path.write_bytes(b"not the captured movie")
        failed = work(corrupt)
        assert failed["state"] == "failed" and failed["error"]["code"] == "ASSET_INTEGRITY_ERROR", failed
        assert failed["result"] is None
        assert not list((root / "artifacts" / failed["job_id"]).rglob("*.mp4"))
        checks.append("corrupt CAS fails without falling back to the valid original source or publishing an artifact")
        report_corrupt = cli("render", "preflight", "--input", "-", payload=blue_request["input"])
        assert not report_corrupt["ready"] and any(issue["code"] == "ASSET_INTEGRITY_ERROR" and issue["asset_id"] == "source" for issue in report_corrupt["issues"]), report_corrupt
        count = len(cli("job", "list"))
        cli("job", "start", "--input", "-", payload={**blue_request, "idempotency_key": "corrupt-new"}, expected_error="ASSET_INTEGRITY_ERROR")
        assert len(cli("job", "list")) == count
        checks.append("corrupt version preflight locates asset and failed preparation creates no job")

        # Compatibility fixture: simulate a persisted pre-lock job. This is the
        # only direct database write; all normal scenarios use public commands.
        generate("green")
        legacy = cli("job", "start", "--input", "-", payload={**request, "idempotency_key": "legacy"})
        legacy_input = dict(legacy["input"])
        legacy_input.pop("asset_lock")
        legacy_input["snapshot"]["project"]["schema_version"] = 1
        for legacy_asset in legacy_input["snapshot"]["project"]["assets"]:
            legacy_asset.pop("version", None)
        with sqlite3.connect(database) as connection:
            connection.execute("UPDATE jobs SET input_json=? WHERE job_id=?", (json.dumps(legacy_input), legacy["job_id"]))
        completed = work(legacy)
        assert completed["state"] == "succeeded" and completed["result"]["asset_locking"] == "legacy_unlocked", completed
        assert completed["result"]["asset_lock"] is None
        checks.append("persisted legacy jobs retain explicit unlocked compatibility behavior")

    result = {"status": "passed", "checks": checks}
    if report:
        report.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    try:
        verify(args.binary.resolve(), args.report)
    except Exception as error:
        if args.report:
            args.report.write_text(json.dumps({"status": "failed", "error": str(error)}, indent=2) + "\n")
        raise
