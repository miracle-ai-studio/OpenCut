#!/usr/bin/env python3
"""Public acceptance for revision-pinned asset bytes and asset.replace."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def verify(binary: Path, report: Path | None = None) -> dict:
    if report:
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps({"status": "running"}))
    checks = []
    with tempfile.TemporaryDirectory(prefix="opencut-asset-version-") as folder:
        root = Path(folder)
        database = root / "workspace.sqlite"
        source = root / "source.mp4"
        command = [str(binary), "--database", str(database), "--json"]

        def cli(*args, payload=None, expected_error=None):
            result = subprocess.run(
                command + list(args),
                input=json.dumps(payload) if payload is not None else None,
                capture_output=True,
                text=True,
                timeout=90,
            )
            assert result.stdout, (result.returncode, result.stderr)
            response = json.loads(result.stdout)
            if expected_error:
                assert result.returncode != 0 and response["error"]["code"] == expected_error, response
                return response
            assert result.returncode == 0 and response["error"] is None, (response, result.stderr)
            return response["data"]

        def tool(name, payload, expected_error=None):
            return cli("call", name, "--input", "-", payload=payload, expected_error=expected_error)

        def generate(color):
            subprocess.run(
                ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i",
                 f"color=c={color}:s=64x48:r=10:d=1", "-c:v", "libx264",
                 "-pix_fmt", "yuv420p", str(source)],
                check=True,
                timeout=30,
            )

        def render_job(revision, key):
            return cli("job", "start", "--input", "-", payload={
                "kind": "render.project", "idempotency_key": key,
                "input": {"project_id": "versioned", "revision": revision,
                          "sequence_id": "main", "container": "mp4", "video_codec": "h264"},
            })

        def work(job):
            remaining = sum(item["state"] == "queued" for item in cli("job", "list"))
            current = cli("job", "get", job["job_id"])
            while current["state"] == "queued" and remaining:
                subprocess.run(
                    command + ["worker", "run", "--once"],
                    check=True,
                    capture_output=True,
                    timeout=90,
                )
                remaining -= 1
                current = cli("job", "get", job["job_id"])
            assert current["state"] == "succeeded", current
            return current

        def assert_color(job, channel):
            completed = work(job)
            artifact = completed["result"]["artifact"]
            path = Path(artifact["path"])
            assert hashlib.sha256(path.read_bytes()).hexdigest() == artifact["content_hash"]
            pixel = subprocess.check_output(
                ["ffmpeg", "-v", "error", "-i", str(path), "-frames:v", "1",
                 "-vf", "scale=1:1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                timeout=30,
            )
            assert pixel[channel] > 180 and all(pixel[i] < 60 for i in range(3) if i != channel), list(pixel)
            return completed

        generate("red")
        red_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        created = tool("opencut_project_create", {
            "project": {"id": "versioned", "name": "Versioned assets", "sequence_id": "main",
                        "width": 64, "height": 48,
                        "fps": {"numerator": 10, "denominator": 1}},
            "idempotency_key": "create-versioned",
        })
        span = {"start": {"ticks": "0", "timebase": 10},
                "duration": {"ticks": "10", "timebase": 10}}
        asset = {"id": "source", "name": "Source", "kind": "video", "uri": str(source),
                 "duration": span["duration"], "metadata": {}}
        track = {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                 "muted": False, "locked": False, "clips": [], "params": {}}
        clip = {"id": "clip", "asset_id": "source", "timeline_range": span,
                "source_range": span, "params": {}, "channels": []}
        bind_request = {
            "expected_revision": created["revision"], "idempotency_key": "bind-source",
            "label": "Bind red source", "operations": [
                {"type": "asset.bind", "asset": asset},
                {"type": "track.create", "sequence_id": "main", "track": track},
                {"type": "clip.insert", "sequence_id": "main", "track_id": "video", "clip": clip},
            ],
        }
        plan = tool("opencut_edit_plan", {"project_id": "versioned", "request": bind_request})
        assert plan["unresolved_asset_ids"] == ["source"]
        assert not (root / "asset-store").exists(), "planning must not create the CAS"
        bound = tool("opencut_edit_apply", {"project_id": "versioned", "request": bind_request})
        bound_snapshot = tool("opencut_project_inspect", {
            "project_id": "versioned", "revision": bound["revision"]})
        assert bound_snapshot["kind"] == "historical"
        red_version = bound_snapshot["snapshot"]["project"]["assets"][0]["version"]
        assert red_version["content_hash"] == red_hash
        assert red_version["id"] == f"av_{red_hash}"
        checks.append("asset.bind pins bytes in the committed revision while plan remains unresolved and read-only")

        source.unlink()
        assert tool("opencut_edit_apply", {"project_id": "versioned", "request": bind_request}) == bound
        preflight_input = {"project_id": "versioned", "revision": bound["revision"],
                           "sequence_id": "main", "container": "mp4", "video_codec": "h264"}
        ready = cli("render", "preflight", "--input", "-", payload=preflight_input)
        assert ready["ready"] and ready["issues"] == [], ready
        red_one = render_job(bound["revision"], "red-one")
        red_two = render_job(bound["revision"], "red-two")
        assert red_one["input"]["asset_lock"] == red_two["input"]["asset_lock"]
        assert red_one["input"]["asset_lock"]["assets"][0]["content_hash"] == red_hash
        assert_color(red_one, 0)
        assert_color(red_two, 0)
        checks.append("deleted source retry succeeds and independent jobs from one revision lock identical red bytes")

        generate("blue")
        blue_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        replacement = dict(asset, name="Blue replacement")
        replace_request = {"expected_revision": bound["revision"], "idempotency_key": "replace-source",
                           "label": "Replace with blue", "operations": [
                               {"type": "asset.replace", "asset": replacement}]}
        replaced = tool("opencut_edit_apply", {"project_id": "versioned", "request": replace_request})
        replaced_snapshot = tool("opencut_project_inspect", {
            "project_id": "versioned", "revision": replaced["revision"]})
        assert replaced_snapshot["kind"] == "historical"
        blue_version = replaced_snapshot["snapshot"]["project"]["assets"][0]["version"]
        assert blue_version["content_hash"] == blue_hash and blue_version != red_version
        source.unlink()
        assert cli("render", "preflight", "--input", "-", payload={
            **preflight_input, "revision": replaced["revision"]})["ready"]
        blue_job = render_job(replaced["revision"], "blue-job")
        assert_color(blue_job, 2)
        assert_color(render_job(bound["revision"], "red-after-replace"), 0)
        checks.append("asset.replace creates a new blue revision while the old revision still renders red")

        restored = tool("opencut_project_restore", {"project_id": "versioned", "request": {
            "expected_revision": replaced["revision"], "target_revision": bound["revision"],
            "idempotency_key": "restore-red-version", "label": "Restore red revision"}})
        restored_snapshot = tool("opencut_project_inspect", {"project_id": "versioned"})
        assert restored_snapshot["revision"] == restored["revision"]
        assert restored_snapshot["project"]["assets"][0]["version"] == red_version
        assert_color(render_job(restored["revision"], "restored-red"), 0)
        checks.append("restoring the old revision restores its exact pinned media version")

        generate("blue")
        object_path = root / "asset-store" / blue_hash[:2] / blue_hash
        object_path.unlink()
        object_path.write_bytes(b"corrupt CAS object")
        broken = cli("render", "preflight", "--input", "-", payload={
            **preflight_input, "revision": replaced["revision"]})
        assert not broken["ready"]
        assert any(issue["asset_id"] == "source" and issue["code"] == "ASSET_INTEGRITY_ERROR"
                   for issue in broken["issues"]), broken
        before = len(cli("job", "list"))
        render_job_request = {"kind": "render.project", "idempotency_key": "corrupt-version",
                              "input": {**preflight_input, "revision": replaced["revision"]}}
        cli("job", "start", "--input", "-", payload=render_job_request,
            expected_error="ASSET_INTEGRITY_ERROR")
        assert len(cli("job", "list")) == before
        checks.append("corrupt versioned CAS is rejected without falling back to a valid original URI")

    result = {"status": "passed", "checks": checks}
    if report:
        report.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    try:
        verify(args.binary.resolve(), args.report)
    except Exception as error:
        if args.report:
            args.report.write_text(json.dumps({
                "status": "failed",
                "error": f"{type(error).__name__}: {error}",
            }, indent=2) + "\n")
        raise
