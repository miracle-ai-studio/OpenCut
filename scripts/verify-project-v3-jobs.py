#!/usr/bin/env python3
"""Real V3 fixed-revision render, frame, range and HTTP artifact acceptance."""
import argparse
import hashlib
import json
import socket
import subprocess
import time
import urllib.request
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(binary: Path, artifacts: Path):
    run = artifacts.resolve() / f"project-v3-jobs-{time.time_ns()}"
    run.mkdir(parents=True)
    db = run / "workspace.sqlite"
    trace = []

    def call(name, body, error=None):
        proc = subprocess.run([str(binary), "--database", str(db), "--json", "call", name,
            "--input", "-"], input=json.dumps(body, ensure_ascii=False), text=True,
            capture_output=True, timeout=90)
        response = json.loads(proc.stdout)
        trace.append({"tool": name, "response": response})
        if error:
            assert proc.returncode and response["error"]["code"] == error, (response, proc.stderr)
        else:
            assert proc.returncode == 0 and response["error"] is None, (response, proc.stderr)
        return response.get("data")

    tick = lambda n: {"ticks": str(n), "timebase": 10}
    span = {"start": tick(0), "duration": tick(20)}
    source = run / "red.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "color=red:s=320x180:r=10:d=2", "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
        str(source)], check=True, timeout=30)
    initial = call("opencut_project_v3_create", {"project": {"id": "visual-film",
        "name": "Visual film", "sequence_id": "main", "width": 320, "height": 180,
        "fps": {"numerator": 10, "denominator": 1}}, "idempotency_key": "create-v3"})
    request = {"project_id": "visual-film", "expected_revision": initial["revision"],
        "idempotency_key": "build-visual-graph", "label": "Base and visual graph",
        "operations": [
            {"type": "base.apply", "operation": {"type": "asset.bind", "asset": {
                "id": "red", "name": "Red", "kind": "video", "uri": str(source),
                "duration": tick(20), "metadata": {}}}},
            {"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main",
                "track": {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                    "muted": False, "locked": False, "clips": [], "params": {}}}},
            {"type": "base.apply", "operation": {"type": "clip.insert", "sequence_id": "main",
                "track_id": "video", "clip": {"id": "red-clip", "asset_id": "red",
                    "timeline_range": span, "source_range": span, "params": {}, "channels": []}}},
            {"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main",
                "track": {"id": "graphics", "name": "Graphics", "kind": "graphic", "enabled": True,
                    "muted": False, "locked": False, "clips": [], "params": {}}}},
            {"type": "shape.put", "sequence_id": "main", "shape": {"id": "blue-card",
                "track_id": "graphics", "timeline_range": span,
                "primitive": {"kind": "rectangle", "width_px": 100.0, "height_px": 70.0},
                "fill": {"red": 0, "green": 50, "blue": 255, "alpha": 255},
                "position_x": 0.0, "position_y": 0.0, "rotation_degrees": 0.0,
                "opacity": 1.0}},
            {"type": "effect.put", "sequence_id": "main", "effect": {"id": "desaturate",
                "target_id": "red-clip", "effect": {"kind": "color_adjust", "exposure_ev": 0.0,
                    "contrast": 1.0, "saturation": 0.0}}},
            {"type": "mask.put", "sequence_id": "main", "mask": {"id": "left-half",
                "target_id": "red-clip", "shape": {"kind": "rectangle", "x": 0.0,
                    "y": 0.0, "width": 0.5, "height": 1.0}, "inverted": False, "feather": 0.0}},
        ]}
    call("opencut_edit_v3_plan", request)
    applied = call("opencut_edit_v3_apply", request)
    assert call("opencut_edit_v3_apply", request) == applied
    revision = applied["revision"]
    assert call("opencut_project_v3_inspect", {"project_id": "visual-film", "revision": revision})["project"]["visual"][0]["masks"]
    call("opencut_project_inspect", {"project_id": "visual-film"}, error="UNSUPPORTED_PROJECT_VERSION")
    source.unlink()

    jobs = []
    base = {"project_id": "visual-film", "revision": revision, "sequence_id": "main"}
    submissions = [
        ("opencut_job_v3_render", {**base, "idempotency_key": "v3-render",
            "container": "mp4", "video_codec": "h264"}, "video/mp4"),
        ("opencut_job_v3_frame", {**base, "idempotency_key": "v3-frame",
            "requested_time": tick(5), "alignment": "exact"}, "image/png"),
        ("opencut_job_v3_range", {**base, "idempotency_key": "v3-range",
            "requested_range": {"start": tick(2), "duration": tick(10)},
            "alignment": "exact"}, "video/mp4"),
    ]
    for tool, body, mime in submissions:
        queued = call(tool, body)
        assert queued["state"] == "queued" and queued["target"]["revision"] == revision
        assert call(tool, body)["job_id"] == queued["job_id"]
        worker = subprocess.run([str(binary), "--database", str(db), "worker", "run", "--once"],
            capture_output=True, text=True, timeout=120)
        assert worker.returncode == 0, worker.stderr
        done = call("opencut_job_view", {"job_id": queued["job_id"]})
        assert done["state"] == "succeeded" and done["result"] is None, done
        assert done["extended_result"]["kind"] == queued["kind"]
        assert done["extended_result"]["data"]["revision"] == revision
        artifact = done["artifacts"][0]
        assert artifact["mime_type"] == mime
        assert artifact["content_hash"]
        jobs.append((done, artifact))
    analysis = call("opencut_analysis_shots", {"idempotency_key": "v3-shots",
        "project_id": "visual-film", "revision": revision, "asset_id": "red", "threshold": 0.4})
    worker = subprocess.run([str(binary), "--database", str(db), "worker", "run", "--once"],
        capture_output=True, text=True, timeout=120)
    assert worker.returncode == 0, worker.stderr
    shots = call("opencut_job_view", {"job_id": analysis["job_id"]})
    assert shots["state"] == "succeeded" and shots["target"]["revision"] == revision, shots
    assert shots["extended_result"]["kind"] == "analysis.shots", shots
    head = call("opencut_project_v3_inspect", {"project_id": "visual-film", "revision": None})
    assert head["revision"] == revision

    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    token_path = run / "owner.token"
    with (run / "serve.log").open("wb") as log:
        host = subprocess.Popen([str(binary), "--database", str(db), "serve",
            "--listen", f"127.0.0.1:{port}", "--token-file", str(token_path)],
            stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 20
            while not token_path.exists():
                assert host.poll() is None and time.monotonic() < deadline
                time.sleep(.05)
            token = token_path.read_text().strip()
            base_url = f"http://127.0.0.1:{port}"
            while True:
                assert host.poll() is None and time.monotonic() < deadline
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=.25):
                        break
                except OSError:
                    time.sleep(.05)
            downloaded = []
            for done, artifact in jobs:
                request_http = urllib.request.Request(base_url + "/v1/jobs/" + done["job_id"] + "/artifact",
                    headers={"Authorization": "Bearer " + token})
                with urllib.request.urlopen(request_http, timeout=30) as response:
                    body = response.read()
                    assert response.headers["Content-Type"].startswith(artifact["mime_type"])
                assert sha(body) == artifact["content_hash"]
                downloaded.append(run / (done["job_id"] + (".png" if artifact["mime_type"] == "image/png" else ".mp4")))
                downloaded[-1].write_bytes(body)
        finally:
            host.terminate()
            host.wait(timeout=10)
    pixels = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(downloaded[1]),
        "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
    def pixel(x, y):
        offset = (y * 320 + x) * 3
        return tuple(pixels[offset:offset+3])
    left, right, center = pixel(30, 90), pixel(290, 90), pixel(160, 90)
    assert max(left)-min(left) < 35 and 30 < sum(left)/3 < 180, left
    assert max(right) < 30, right
    assert center[2] > center[0] + 80 and center[2] > center[1] + 50, center
    report = {"binary_sha256": sha(binary.read_bytes()), "revision": revision,
        "jobs": [{"job_id": done["job_id"], "kind": done["kind"],
            "artifact_sha256": artifact["content_hash"]} for done, artifact in jobs],
        "frame_pixels": {"left_effect": left, "right_mask": right, "center_shape": center},
        "checks": ["V3 fixed revision and legacy rejection", "same queue/worker/job view",
            "source deletion after CAS pin", "render/frame/range HTTP artifact download and hash",
            "effect, mask and shape visible in decoded frame", "analysis.shots on V3 base media",
            "job success leaves head unchanged"],
        "analysis_job_id": analysis["job_id"],
        "trace": trace}
    (run / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
    return run / "report.json"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args()
    print(verify(args.binary.resolve(), args.artifacts_dir))
