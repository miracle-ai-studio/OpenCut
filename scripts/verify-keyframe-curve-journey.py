#!/usr/bin/env python3
"""Verify versioned schema 3 cubic easing through CLI, SQLite, jobs and FFmpeg."""
import argparse
import hashlib
import json
import platform
import socket
import subprocess
import time
import urllib.request
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(binary: Path, artifacts: Path):
    run = artifacts.resolve() / f"keyframe-curve-{time.time_ns()}"
    run.mkdir(parents=True)
    database = run / "workspace.sqlite"
    trace = []

    def call(name, body):
        proc = subprocess.run(
            [str(binary), "--database", str(database), "--json", "call", name, "--input", "-"],
            input=json.dumps(body, ensure_ascii=False), text=True, capture_output=True, timeout=90,
        )
        response = json.loads(proc.stdout)
        trace.append({"tool": name, "response": response})
        assert proc.returncode == 0 and response["error"] is None, (response, proc.stderr)
        return response["data"]

    def run_worker():
        worker = subprocess.run(
            [str(binary), "--database", str(database), "worker", "run", "--once"],
            text=True, capture_output=True, timeout=120,
        )
        assert worker.returncode == 0, worker.stderr

    tick = lambda value: {"ticks": str(value), "timebase": 10}
    span = {"start": tick(0), "duration": tick(10)}
    source = run / "white.mp4"
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i",
         "color=white:s=160x90:r=10:d=1", "-c:v", "libx264", "-pix_fmt", "yuv420p",
        str(source)], check=True, timeout=30,
    )
    source_sha256 = sha(source.read_bytes())
    created = call("opencut_project_create", {
        "project": {"id": "eased", "name": "Eased animation", "sequence_id": "main",
                    "width": 160, "height": 90, "fps": {"numerator": 10, "denominator": 1}},
        "idempotency_key": "create-eased",
    })
    setup = {
        "project_id": "eased", "expected_revision": created["revision"],
        "idempotency_key": "setup-eased", "label": "Create animated clip",
        "operations": [
            {"type": "base.apply", "operation": {"type": "asset.bind", "asset": {
                "id": "white", "name": "White", "kind": "video", "uri": str(source),
                "duration": tick(10), "metadata": {}}}},
            {"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main",
                "track": {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                          "muted": False, "locked": False, "clips": [], "params": {}}}},
            {"type": "base.apply", "operation": {"type": "clip.insert", "sequence_id": "main",
                "track_id": "video", "clip": {"id": "white-clip", "asset_id": "white",
                    "timeline_range": span, "source_range": span, "params": {}, "channels": [
                        {"parameter": "opacity", "keyframes": [
                            {"id": "fade-start", "time": tick(0), "value": 0.0, "interpolation": "linear"},
                            {"id": "fade-end", "time": tick(10), "value": 1.0, "interpolation": "linear"},
                        ]},
                    ]}}},
        ],
    }
    call("opencut_edit_plan", setup)
    base = call("opencut_edit_apply", setup)
    assert call("opencut_edit_apply", setup) == base
    base_revision = base["revision"]

    def frame_job(revision, key):
        queued = call("opencut_job_frame", {
            "idempotency_key": key, "project_id": "eased", "revision": revision,
            "sequence_id": "main", "requested_time": tick(5), "alignment": "exact",
        })
        run_worker()
        done = call("opencut_job_view", {"job_id": queued["job_id"]})
        assert done["state"] == "succeeded" and done["target"]["revision"] == revision
        assert done["extended_result"]["kind"] == "preview.project_frame.v3", done
        assert done["artifacts"][0]["mime_type"] == "image/png", done
        return done

    baseline = frame_job(base_revision, "linear-frame")
    curve = {"id": "opacity-ease", "schema_version": 1, "sequence_id": "main",
             "clip_id": "white-clip", "parameter": "opacity", "keyframe_id": "fade-start",
             "curve": {"x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 0.0}}
    edit = {"project_id": "eased", "expected_revision": base_revision,
            "idempotency_key": "ease-opacity", "label": "Ease opacity", "operations": [
                {"type": "keyframe.curve_set", "sequence_id": "main", "curve": curve}
            ]}
    call("opencut_edit_plan", edit)
    applied = call("opencut_edit_apply", edit)
    assert call("opencut_edit_apply", edit) == applied
    revision = applied["revision"]
    inspected = call("opencut_project_inspect", {"project_id": "eased", "revision": revision})
    assert inspected["project"]["keyframe_curves"] == [curve]
    eased = frame_job(revision, "cubic-frame")

    render = call("opencut_job_render", {
        "idempotency_key": "cubic-render", "project_id": "eased", "revision": revision,
        "sequence_id": "main", "container": "mp4", "video_codec": "h264",
    })
    run_worker()
    rendered = call("opencut_job_view", {"job_id": render["job_id"]})
    assert rendered["state"] == "succeeded" and rendered["artifacts"][0]["mime_type"] == "video/mp4", rendered
    assert rendered["target"]["revision"] == revision
    assert rendered["extended_result"]["kind"] == "render.project.v3", rendered
    source.unlink()

    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    token_path = run / "owner.token"
    with (run / "serve.log").open("wb") as log:
        host = subprocess.Popen(
            [str(binary), "--database", str(database), "serve", "--listen",
             f"127.0.0.1:{port}", "--token-file", str(token_path)],
            stdout=log, stderr=log,
        )
        try:
            deadline = time.monotonic() + 20
            while not token_path.exists():
                assert host.poll() is None and time.monotonic() < deadline
                time.sleep(.05)
            while True:
                assert host.poll() is None and time.monotonic() < deadline
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=.25):
                        break
                except OSError:
                    time.sleep(.05)
            token = token_path.read_text().strip()
            artifacts_by_name = {}
            for name, job in (("linear.png", baseline), ("cubic.png", eased), ("cubic.mp4", rendered)):
                artifact = job["artifacts"][0]
                request = urllib.request.Request(
                    f"http://127.0.0.1:{port}/api/jobs/{job['job_id']}/artifact",
                    headers={"Authorization": "Bearer " + token},
                )
                with urllib.request.urlopen(request, timeout=30) as response:
                    body = response.read()
                    assert response.headers["Content-Type"].startswith(artifact["mime_type"])
                assert sha(body) == artifact["content_hash"]
                output = run / name
                output.write_bytes(body)
                artifacts_by_name[name] = output
        finally:
            host.terminate()
            host.wait(timeout=10)

    def center_gray(path, video=False):
        command = ["ffmpeg", "-v", "error"]
        if video:
            command += ["-ss", "0.5"]
        command += ["-i", str(path), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
        pixels = subprocess.check_output(command, timeout=30)
        offset = (45 * 160 + 80) * 3
        return sum(pixels[offset:offset + 3]) // 3

    linear_pixel = center_gray(artifacts_by_name["linear.png"])
    cubic_pixel = center_gray(artifacts_by_name["cubic.png"])
    render_pixel = center_gray(artifacts_by_name["cubic.mp4"], video=True)
    assert linear_pixel > 90, linear_pixel
    assert 5 < cubic_pixel < 45, cubic_pixel
    assert abs(cubic_pixel - render_pixel) < 15, (cubic_pixel, render_pixel)

    report = {
        "status": "passed", "binary_sha256": sha(binary.read_bytes()),
        "environment": {"os": platform.platform(), "machine": platform.machine(),
                        "ffmpeg": subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]},
        "source_sha256": source_sha256,
        "base_revision": base_revision, "curve_revision": revision,
        "jobs": {
            "linear_frame": {"job_id": baseline["job_id"],
                             "artifact_sha256": baseline["artifacts"][0]["content_hash"]},
            "cubic_frame": {"job_id": eased["job_id"],
                            "artifact_sha256": eased["artifacts"][0]["content_hash"]},
            "cubic_render": {"job_id": rendered["job_id"],
                             "artifact_sha256": rendered["artifacts"][0]["content_hash"]},
        },
        "linear_midframe_gray": linear_pixel, "cubic_midframe_gray": cubic_pixel,
        "rendered_midframe_gray": render_pixel,
        "checks": [
            "Versioned keyframe.curve_set is discoverable through schema 3 plan/apply",
            "Curve project persists through inspect and idempotent SQLite replay",
            "Fixed revision frame jobs render linear and cubic easing through composition",
            "Full schema 3 render uses the same cubic easing as the frame job",
            "HTTP artifact downloads match committed hashes after source removal",
        ],
    }
    (run / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    (run / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")
    print(json.dumps({"status": "passed", "report": str(run / "report.json")}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args()
    verify(args.binary.resolve(), args.artifacts_dir.resolve())
