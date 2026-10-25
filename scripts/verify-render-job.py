#!/usr/bin/env python3
"""Real upload -> revision transaction -> durable export -> download acceptance."""
import argparse
import hashlib
import json
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def verify(binary, caption_font=None, caption_fallback_font=None):
    with tempfile.TemporaryDirectory(prefix="opencut-render-job-") as directory:
        root = Path(directory)
        source = root / "source.mp4"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
                        "color=c=red:s=160x90:r=10:d=2", "-c:v", "libx264",
                        "-pix_fmt", "yuv420p", str(source)], check=True)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        token_file = root / "token"
        with tempfile.TemporaryFile() as logs:
            process = subprocess.Popen([str(binary), "--database", str(root / "editor.sqlite"),
                                        "serve", "--listen", f"127.0.0.1:{port}",
                                        "--token-file", str(token_file)], stdout=logs, stderr=logs)
            token = None

            def request(path, data=None, raw=False):
                headers = {"Authorization": f"Bearer {token}"}
                if data is not None and not isinstance(data, bytes):
                    data = json.dumps(data).encode()
                    headers["Content-Type"] = "application/json"
                req = urllib.request.Request(base + path, data=data, headers=headers)
                try:
                    with urllib.request.urlopen(req, timeout=15) as response:
                        value = response.read()
                        return value if raw else json.loads(value)
                except urllib.error.HTTPError as error:
                    return json.load(error)

            def call(path, data=None):
                result = request(path, data)
                assert result["error"] is None, result
                return result["data"]

            try:
                deadline = time.monotonic() + 15
                while True:
                    assert process.poll() is None, "HTTP process exited"
                    try:
                        token = token_file.read_text().strip()
                        call("/api/capabilities")
                        break
                    except (FileNotFoundError, urllib.error.URLError):
                        assert time.monotonic() < deadline, "Startup timeout"
                        time.sleep(.05)
                upload = call("/api/media/upload?name=source.mp4", source.read_bytes())
                assert request(upload["media_url"], raw=True) == source.read_bytes()
                snapshot = call("/api/projects", {
                    "project": {"id": "render", "name": "Render acceptance", "sequence_id": "main",
                                "width": 160, "height": 90, "fps": {"numerator": 10, "denominator": 1}},
                    "idempotency_key": "create"})
                time_range = {"start": {"ticks": "0", "timebase": 10},
                              "duration": {"ticks": "10", "timebase": 10}}
                track = {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                         "muted": False, "locked": False, "clips": [], "params": {}}
                clip = {"id": "clip", "asset_id": upload["asset"]["id"], "timeline_range": time_range,
                        "source_range": time_range, "params": {}, "channels": []}
                edited = call("/api/projects/render/branches/main/transactions", {
                    "expected_revision": snapshot["revision"], "idempotency_key": "edit", "label": "One second cut",
                    "operations": [{"type": "asset.bind", "asset": upload["asset"]},
                                   {"type": "track.create", "sequence_id": "main", "track": track},
                                   {"type": "clip.insert", "sequence_id": "main", "track_id": "video", "clip": clip}]})
                export = {"kind": "render.project", "idempotency_key": "export", "input": {
                    "project_id": "render", "revision": edited["revision"], "sequence_id": "main",
                    "container": "mp4", "video_codec": "h264"}}
                job = call("/api/jobs", export)
                repeated = call("/api/jobs", export)
                assert repeated["job_id"] == job["job_id"]
                # Remove the clip while the accepted job retains its original snapshot.
                call("/api/projects/render/branches/main/transactions", {
                    "expected_revision": edited["revision"], "idempotency_key": "remove", "label": "Later edit",
                    "operations": [{"type": "clip.delete", "sequence_id": "main", "track_id": "video", "clip_id": "clip"}]})
                deadline = time.monotonic() + 60
                while job["state"] not in ("succeeded", "failed", "cancelled"):
                    assert time.monotonic() < deadline, job
                    time.sleep(.1)
                    job = call(f'/api/jobs/{job["job_id"]}')
                assert job["state"] == "succeeded", job
                assert job["result"]["revision"] == edited["revision"]
                # Observe actual worker samples, not just application test fixtures.
                events = call(f'/api/job-events?job_id={job["job_id"]}&limit=500')
                assert [e["event_type"] for e in events[:2]] == ["queued", "running"], events
                assert events[-1]["event_type"] == "succeeded", events
                assert [e["sequence"] for e in events] == sorted({e["sequence"] for e in events})
                samples = [e for e in events if e["event_type"] == "progress"]
                assert samples, events
                assert {e["payload"]["stage"] for e in samples} >= {"probing", "encoding", "verifying"}, samples
                assert all(e["job_id"] == job["job_id"] and e["attempt"] == job["attempt"] for e in samples)
                for event in samples:
                    progress = event["payload"]
                    assert progress["fraction"] is None or 0 <= progress["fraction"] <= 1
                    if progress["media_time"]:
                        assert isinstance(progress["media_time"]["ticks"], str)
                        assert progress["media_time"]["timebase"] > 0
                assert job["progress"] == samples[-1]["payload"], job
                assert call(f'/api/job-events?job_id={job["job_id"]}&after_sequence={events[-1]["sequence"]}') == []
                independent = json.loads(subprocess.check_output([
                    str(binary), "--database", str(root / "editor.sqlite"), "--json",
                    "job", "events", "--job-id", job["job_id"], "--limit", "500",
                ]))
                assert independent["error"] is None and independent["data"] == events, independent
                artifact = request(f'/api/jobs/{job["job_id"]}/artifact', raw=True)
                assert hashlib.sha256(artifact).hexdigest() == job["result"]["artifact"]["content_hash"]
                output = root / "downloaded.mp4"
                output.write_bytes(artifact)
                probed = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-show_format", "-show_streams", "-of", "json", str(output)]))
                assert abs(float(probed["format"]["duration"]) - 1) < .15, probed
                video = next(s for s in probed["streams"] if s["codec_type"] == "video")
                assert (video["width"], video["height"]) == (160, 90)
                pixel = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(output), "-frames:v", "1", "-vf", "scale=1:1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
                assert pixel[0] > 180 and pixel[1] < 60 and pixel[2] < 60, pixel
                assert request("/api/jobs/missing/artifact")["error"]["code"] == "NOT_FOUND"
                checks = ["authenticated upload and readback", "atomic timeline edit", "durable export",
                          "idempotent job", "fixed revision despite later edit", "artifact hash",
                          "real worker progress and durable lifecycle events", "independent CLI event readback",
                          "independent decode, dimensions and duration", "missing artifact rejection"]
                if caption_font:
                    head = call("/api/projects/render/snapshot")
                    cue = {
                        "id": "bilingual", "timeline_range": time_range, "text": "Hi 中文",
                        "source_anchor": None,
                        "style": {"font_family": caption_font, "font_size": 16, "color": "white",
                                  "background_color": None, "max_width": 140, "line_height": 22,
                                  "text_align": "left", "font_policy": (
                                      {"mode": "explicit_fallback", "families": [caption_fallback_font]}
                                      if caption_fallback_font else {"mode": "strict"})},
                    }
                    captioned = call("/api/projects/render/branches/main/transactions", {
                        "expected_revision": head["revision"], "idempotency_key": "caption-edit", "label": "Bilingual caption",
                        "operations": [{"type": "clip.insert", "sequence_id": "main", "track_id": "video", "clip": clip},
                                       {"type": "caption.insert", "sequence_id": "main", "cue": cue}],
                    })

                    def export_revision(revision, key):
                        task = call("/api/jobs", {"kind": "render.project", "idempotency_key": key, "input": {
                            "project_id": "render", "revision": revision, "sequence_id": "main",
                            "container": "mp4", "video_codec": "h264",
                        }})
                        deadline = time.monotonic() + 60
                        while task["state"] not in ("succeeded", "failed", "cancelled"):
                            assert time.monotonic() < deadline, task
                            time.sleep(.1)
                            task = call(f'/api/jobs/{task["job_id"]}')
                        return task

                    caption_job = export_revision(captioned["revision"], "caption-export")
                    assert caption_job["state"] == "succeeded", caption_job
                    assert len(json.dumps(caption_job).encode()) < 64 * 1024, "Caption metadata must not embed font bytes"
                    reports = caption_job["result"]["media"]["text_layouts"]
                    assert len(reports) == 1 and reports[0]["requested_family"] == caption_font, reports
                    families = {face["family"] for face in reports[0]["used_faces"]}
                    assert all(len(face["source"].encode()) < 4096 for face in reports[0]["used_faces"])
                    assert caption_font in families, reports
                    if caption_fallback_font:
                        assert caption_fallback_font in families, reports
                    bounds = reports[0]["pixel_bounds"]
                    assert 0 < bounds["width"] <= 140 and bounds["x"] + bounds["width"] <= 160, bounds
                    caption_output = root / "captioned.mp4"
                    caption_bytes = request(f'/api/jobs/{caption_job["job_id"]}/artifact', raw=True)
                    assert hashlib.sha256(caption_bytes).hexdigest() == caption_job["result"]["artifact"]["content_hash"]
                    caption_output.write_bytes(caption_bytes)
                    decoded = subprocess.check_output([
                        "ffmpeg", "-v", "error", "-i", str(caption_output), "-frames:v", "1",
                        "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
                    ])
                    assert len(decoded) == 160 * 90 * 3
                    white_pixels = 0
                    for y in range(bounds["y"], bounds["y"] + bounds["height"]):
                        for x in range(bounds["x"], bounds["x"] + bounds["width"]):
                            offset = (y * 160 + x) * 3
                            if min(decoded[offset:offset + 3]) > 170:
                                white_pixels += 1
                    assert white_pixels > 30, (white_pixels, reports)

                    # Strict missing-font failures must remain machine-actionable and
                    # must never publish a substituted-font artifact as successful.
                    cue["id"] = "missing-font"
                    cue["style"]["font_family"] = "OpenCut Missing Font Acceptance 918273"
                    cue["style"]["font_policy"] = {"mode": "strict"}
                    rejected_revision = call("/api/projects/render/branches/main/transactions", {
                        "expected_revision": captioned["revision"], "idempotency_key": "missing-font-edit", "label": "Missing font",
                        "operations": [{"type": "caption.delete", "sequence_id": "main", "cue_id": "bilingual"},
                                       {"type": "caption.insert", "sequence_id": "main", "cue": cue}],
                    })
                    rejected = export_revision(rejected_revision["revision"], "missing-font-export")
                    assert rejected["state"] == "failed" and rejected["error"]["code"] == "CAPABILITY_UNAVAILABLE", rejected
                    assert rejected["result"] is None
                    assert request(f'/api/jobs/{rejected["job_id"]}/artifact')["error"] is not None
                    checks += ["bilingual caption through public API", "reported primary and explicit fallback faces",
                               "bounded font metadata without embedded font bytes", "independent decoded subtitle pixels",
                               "missing strict font fails without artifact"]
                return {"checks": checks}
            finally:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                logs.seek(0)
                print(logs.read().decode(errors="replace"), file=__import__("sys").stderr)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    parser.add_argument("--caption-font", help="Enable bilingual caption checks using this installed font")
    parser.add_argument("--caption-fallback-font", help="An installed font containing the Chinese glyphs")
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve(), args.caption_font, args.caption_fallback_font), indent=2))
