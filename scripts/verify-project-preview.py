#!/usr/bin/env python3
"""Public API acceptance for fixed-revision, animated project frame previews.

Uses actual FFmpeg media, worker jobs, authenticated artifact downloads, and an
independent RGB decode. Optional artifacts are retained for visual inspection.
"""
import argparse
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def rational(value):
    return Fraction(int(value["ticks"]), value["timebase"])


def at(ticks, base=10):
    return {"ticks": str(ticks), "timebase": base}


def decode(path, frame=0):
    return subprocess.check_output([
        "ffmpeg", "-v", "error", "-i", str(path), "-vf", f"select=eq(n\\,{frame})",
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
    ])


def red_center(pixels):
    assert len(pixels) == 160 * 90 * 3, len(pixels)
    positions = [(index // 3 % 160, index // 3 // 160)
                 for index in range(0, len(pixels), 3)
                 if pixels[index] > 150 and pixels[index + 1] < 70 and pixels[index + 2] < 70]
    assert len(positions) > 600, len(positions)
    return tuple(sum(point[axis] for point in positions) / len(positions) for axis in (0, 1))


def verify(binary, artifacts_dir=None):
    if artifacts_dir:
        artifacts_dir.mkdir(parents=True, exist_ok=True)
        # An interrupted or failed new run must not leave an older success
        # report beside the new movie artifacts.
        (artifacts_dir / "report.json").write_text(json.dumps({"status": "running"}))
    with tempfile.TemporaryDirectory(prefix="opencut-project-preview-") as directory:
        root = Path(directory)
        source = root / "red.mp4"
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=red:s=160x90:r=10:d=2",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", str(source),
        ], check=True)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        database = root / "editor.sqlite"
        token_file = root / "token"
        token = None
        with tempfile.TemporaryFile() as logs:
            process = subprocess.Popen([
                str(binary), "--database", str(database), "serve", "--listen", f"127.0.0.1:{port}",
                "--token-file", str(token_file),
            ], stdout=logs, stderr=logs)

            def request(path, data=None, raw=False):
                headers = {"Authorization": f"Bearer {token}"}
                if data is not None and not isinstance(data, bytes):
                    data = json.dumps(data).encode()
                    headers["Content-Type"] = "application/json"
                try:
                    with urllib.request.urlopen(urllib.request.Request(base + path, data=data, headers=headers), timeout=15) as response:
                        payload = response.read()
                        return payload if raw else json.loads(payload)
                except urllib.error.HTTPError as error:
                    return json.load(error)

            def call(path, data=None):
                response = request(path, data)
                assert response["error"] is None, response
                return response["data"]

            def wait(job):
                deadline = time.monotonic() + 90
                while job["state"] not in ("succeeded", "failed", "cancelled"):
                    assert process.poll() is None, "Service exited while waiting"
                    assert time.monotonic() < deadline, job
                    time.sleep(.1)
                    job = call(f'/v1/jobs/{job["job_id"]}')
                return job

            def download(job, filename):
                assert job["state"] == "succeeded", job
                payload = request(f'/v1/jobs/{job["job_id"]}/artifact', raw=True)
                descriptor = job["result"]["artifact"]
                assert hashlib.sha256(payload).hexdigest() == descriptor["content_hash"]
                assert len(payload) == descriptor["size"]
                output = root / filename
                output.write_bytes(payload)
                if artifacts_dir:
                    artifacts_dir.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(output, artifacts_dir / filename)
                return output

            try:
                deadline = time.monotonic() + 15
                while True:
                    assert process.poll() is None, "HTTP service exited"
                    try:
                        token = token_file.read_text().strip()
                        capabilities = call("/v1/capabilities")
                        break
                    except (FileNotFoundError, urllib.error.URLError):
                        assert time.monotonic() < deadline, "Startup timeout"
                        time.sleep(.05)
                assert capabilities["rendering"]["project_frame_preview"]
                upload = call("/v1/media/upload?name=red.mp4", source.read_bytes())
                snapshot = call("/v1/projects", {
                    "project": {"id": "preview", "name": "Animated project preview", "sequence_id": "main",
                                "width": 160, "height": 90, "fps": {"numerator": 10, "denominator": 1}},
                    "idempotency_key": "create",
                })
                clip = {
                    "id": "moving", "asset_id": upload["asset"]["id"],
                    "timeline_range": {"start": at(10), "duration": at(20)},
                    "source_range": {"start": at(0), "duration": at(10)},
                    "params": {"scale_x": .25, "scale_y": .25},
                    "channels": [{"parameter": "position_x", "keyframes": [
                        {"id": "left", "time": at(0), "value": -40, "interpolation": "linear"},
                        {"id": "right", "time": at(20), "value": 40, "interpolation": "hold"},
                    ]}],
                }
                edited = call("/v1/projects/preview/branches/main/transactions", {
                    "expected_revision": snapshot["revision"], "idempotency_key": "edit", "label": "Animated clip",
                    "operations": [
                        {"type": "asset.bind", "asset": upload["asset"]},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": True, "locked": False, "clips": [], "params": {},
                        }},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "video", "clip": clip},
                    ],
                })
                revision = edited["revision"]

                def preview_body(ticks, timebase, alignment, key):
                    return {"kind": "preview.project_frame", "idempotency_key": key, "input": {
                        "project_id": "preview", "revision": revision, "sequence_id": "main",
                        "requested_time": at(ticks, timebase), "alignment": alignment,
                    }}

                body = preview_body(126, 100, "nearest", "preview-nearest")
                first = call("/v1/jobs", body)
                assert call("/v1/jobs", body)["job_id"] == first["job_id"]
                split_revision = call("/v1/projects/preview/branches/main/transactions", {
                    "expected_revision": revision, "idempotency_key": "split-animation", "label": "Split animated clip",
                    "operations": [{"type": "clip.split", "sequence_id": "main", "track_id": "video",
                                    "clip_id": "moving", "at": at(20), "right_clip_id": "moving-right"}],
                })["revision"]
                later = call("/v1/projects/preview/branches/main/transactions", {
                    "expected_revision": split_revision, "idempotency_key": "remove", "label": "Later user edit",
                    "operations": [
                        {"type": "clip.delete", "sequence_id": "main", "track_id": "video", "clip_id": "moving"},
                        {"type": "clip.delete", "sequence_id": "main", "track_id": "video", "clip_id": "moving-right"},
                    ],
                })
                assert later["revision"] != revision
                samples = []
                for ticks, timebase, alignment, expected, center in [
                    (126, 100, "nearest", Fraction(13, 10), 52),
                    (126, 100, "floor", Fraction(12, 10), 48),
                    (125, 100, "nearest", Fraction(13, 10), 52),
                    (20, 10, "exact", Fraction(2), 80),
                    (29, 10, "exact", Fraction(29, 10), 116),
                    (5, 10, "exact", Fraction(1, 2), None),
                ]:
                    key = f"preview-{ticks}-{timebase}-{alignment}"
                    job = wait(first if not samples else call("/v1/jobs", preview_body(ticks, timebase, alignment, key)))
                    assert job["state"] == "succeeded", job
                    result = job["result"]
                    assert result["revision"] == revision and result["sequence_id"] == "main", result
                    assert rational(result["requested_time"]) == Fraction(ticks, timebase)
                    assert rational(result["actual_sequence_time"]) == expected, result
                    assert result["quality"] == "encoded_project_frame", result
                    assert result["artifact"]["mime_type"] == "image/png", result
                    assert not Path(result["artifact"]["path"]).with_name("preview-source.mp4").exists()
                    output = download(job, f"{key}.png")
                    pixels = decode(output)
                    if center is None:
                        assert max(pixels) < 8, "Timeline gap must be black"
                    else:
                        x, y = red_center(pixels)
                        assert abs(x - center) < 2 and abs(y - 45) < 2, (x, y, center)
                    samples.append((expected, pixels))

                # Invalid exact/boundary requests cannot become successful clamped frames.
                for ticks, timebase, alignment in [(125, 100, "exact"), (30, 10, "floor"), (299, 100, "ceil")]:
                    response = request("/v1/jobs", preview_body(ticks, timebase, alignment, f"invalid-{ticks}"))
                    if response["error"] is not None:
                        assert response["error"]["code"] == "INVALID_ARGUMENT", response
                    else:
                        failed = wait(response["data"])
                        assert failed["state"] == "failed" and failed["error"]["code"] == "INVALID_ARGUMENT", failed
                        assert failed["result"] is None
                        assert request(f'/v1/jobs/{failed["job_id"]}/artifact')["error"] is not None
                        attempt_root = root / "artifacts" / failed["job_id"]
                        assert not list(attempt_root.glob("attempt-*/preview-source.mp4"))
                        assert not list(attempt_root.glob("attempt-*/frame.png"))

                exported = wait(call("/v1/jobs", {"kind": "render.project", "idempotency_key": "export", "input": {
                    "project_id": "preview", "revision": revision, "sequence_id": "main", "container": "mp4", "video_codec": "h264",
                }}))
                movie = download(exported, "animated-project.mp4")
                for actual, pixels in samples:
                    exported_pixels = decode(movie, int(actual * 10))
                    assert len(exported_pixels) == len(pixels)
                    assert max(abs(a - b) for a, b in zip(exported_pixels, pixels)) <= 2, actual
                split_export = wait(call("/v1/jobs", {"kind": "render.project", "idempotency_key": "split-export", "input": {
                    "project_id": "preview", "revision": split_revision, "sequence_id": "main", "container": "mp4", "video_codec": "h264",
                }}))
                split_movie = download(split_export, "split-animated-project.mp4")
                for index in (18, 19, 20, 21, 22, 29):
                    before = decode(movie, index)
                    after = decode(split_movie, index)
                    assert len(before) == len(after) == 160 * 90 * 3
                    before_center = red_center(before)
                    after_center = red_center(after)
                    assert max(abs(a - b) for a, b in zip(before_center, after_center)) < 1, (index, before_center, after_center)
                    assert max(abs(a - b) for a, b in zip(before, after)) <= 5, index
                offset_clip = json.loads(json.dumps(clip))
                offset_clip["id"] = "offset-clip"
                offset_clip["timeline_range"]["start"] = at(105, 100)
                offset_revision = call("/v1/projects/preview/branches/main/transactions", {
                    "expected_revision": later["revision"], "idempotency_key": "offset-edit", "label": "Sub-frame clip start",
                    "operations": [{"type": "clip.insert", "sequence_id": "main", "track_id": "video", "clip": offset_clip}],
                })["revision"]
                for ticks, expected_center in [(11, 42), (12, 46)]:
                    offset_body = preview_body(ticks, 10, "exact", f"offset-{ticks}")
                    offset_body["input"]["revision"] = offset_revision
                    offset_job = wait(call("/v1/jobs", offset_body))
                    offset_pixels = decode(download(offset_job, f"offset-{ticks}.png"))
                    observed_center, _ = red_center(offset_pixels)
                    assert abs(observed_center - expected_center) < 1, (ticks, observed_center, expected_center)
                events = call(f'/v1/job-events?job_id={first["job_id"]}&limit=500')
                assert events[-1]["state"] == "succeeded", events
                assert {event["payload"].get("stage") for event in events if event["event_type"] == "progress"} >= {"encoding", "selecting_frame", "decoding"}
                independent = json.loads(subprocess.check_output([
                    str(binary), "--database", str(database), "--json", "job", "get", first["job_id"],
                ]))
                assert independent["error"] is None and independent["data"]["result"]["revision"] == revision, independent
                ntsc = call("/v1/projects", {
                    "project": {"id": "ntsc", "name": "Fractional frame grid", "sequence_id": "ntsc-main",
                                "width": 160, "height": 90, "fps": {"numerator": 30000, "denominator": 1001}},
                    "idempotency_key": "create-ntsc",
                })
                ntsc_edit = call("/v1/projects/ntsc/branches/main/transactions", {
                    "expected_revision": ntsc["revision"], "idempotency_key": "ntsc-edit", "label": "Fractional sequence",
                    "operations": [
                        {"type": "asset.bind", "asset": upload["asset"]},
                        {"type": "track.create", "sequence_id": "ntsc-main", "track": {
                            "id": "ntsc-video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": True, "locked": False, "clips": [], "params": {},
                        }},
                        {"type": "clip.insert", "sequence_id": "ntsc-main", "track_id": "ntsc-video", "clip": {
                            "id": "ntsc-clip", "asset_id": upload["asset"]["id"],
                            "timeline_range": {"start": at(0), "duration": at(1001, 1000)},
                            "source_range": {"start": at(0), "duration": at(1, 1)}, "params": {}, "channels": [],
                        }},
                    ],
                })
                ntsc_job = wait(call("/v1/jobs", {"kind": "preview.project_frame", "idempotency_key": "ntsc-frame", "input": {
                    "project_id": "ntsc", "revision": ntsc_edit["revision"], "sequence_id": "ntsc-main",
                    "requested_time": at(1001, 60000), "alignment": "nearest",
                }}))
                assert ntsc_job["state"] == "succeeded", ntsc_job
                assert rational(ntsc_job["result"]["actual_sequence_time"]) == Fraction(1001, 30000), ntsc_job
                ntsc_pixels = decode(download(ntsc_job, "ntsc-frame.png"))
                assert ntsc_pixels[0] > 180 and max(ntsc_pixels[1:3]) < 60, ntsc_pixels[:3]
                third = call("/v1/projects/ntsc/branches/main/transactions", {
                    "expected_revision": ntsc_edit["revision"], "idempotency_key": "third-edit", "label": "Exact third-second duration",
                    "operations": [
                        {"type": "sequence.update_settings", "sequence_id": "ntsc-main", "fps": {"numerator": 3, "denominator": 1}},
                        {"type": "clip.retime", "sequence_id": "ntsc-main", "track_id": "ntsc-video", "clip_id": "ntsc-clip", "timeline_duration": at(1, 3)},
                    ],
                })
                third_frame = wait(call("/v1/jobs", {"kind": "preview.project_frame", "idempotency_key": "third-end", "input": {
                    "project_id": "ntsc", "revision": third["revision"], "sequence_id": "ntsc-main",
                    "requested_time": at(1, 3), "alignment": "exact",
                }}))
                assert third_frame["state"] == "failed" and third_frame["error"]["code"] == "INVALID_ARGUMENT", third_frame

                # Wait for real encoder progress before cancelling. A longer,
                # larger static sequence prevents a queued-only cancellation
                # from masquerading as cancellation of active media work.
                long_edit = call("/v1/projects/ntsc/branches/main/transactions", {
                    "expected_revision": third["revision"], "idempotency_key": "long-edit", "label": "Cancellation fixture",
                    "operations": [
                        {"type": "sequence.update_settings", "sequence_id": "ntsc-main", "width": 1920, "height": 1080,
                         "fps": {"numerator": 30, "denominator": 1}},
                        {"type": "clip.retime", "sequence_id": "ntsc-main", "track_id": "ntsc-video", "clip_id": "ntsc-clip", "timeline_duration": at(100, 1)},
                    ],
                })
                active = call("/v1/jobs", {"kind": "preview.project_frame", "idempotency_key": "cancel-active", "input": {
                    "project_id": "ntsc", "revision": long_edit["revision"], "sequence_id": "ntsc-main",
                    "requested_time": at(1, 1), "alignment": "exact",
                }})
                deadline = time.monotonic() + 45
                while True:
                    active = call(f'/v1/jobs/{active["job_id"]}')
                    assert active["state"] in ("queued", "running"), active
                    progress = active.get("progress") or {}
                    if progress.get("stage") == "encoding" and progress.get("media_time") and rational(progress["media_time"]) > 0:
                        break
                    assert time.monotonic() < deadline, {"failure": "No real encoder progress before cancellation", "job": active}
                    time.sleep(.05)
                cancelled = wait(call(f'/v1/jobs/{active["job_id"]}/cancel', {}))
                assert cancelled["state"] == "cancelled" and cancelled["result"] is None, cancelled
                assert request(f'/v1/jobs/{cancelled["job_id"]}/artifact')["error"] is not None
                attempt_root = root / "artifacts" / cancelled["job_id"]
                assert not list(attempt_root.glob("attempt-*/preview-source.mp4"))
                assert not list(attempt_root.glob("attempt-*/frame.png"))
                assert not list(attempt_root.glob("attempt-*/inputs"))
                report = {"status": "passed", "checks": [
                    "fixed revision despite later clip deletion", "idempotent preview job", "actual frame alignment and later tie",
                    "independent animated pixel position at nonzero clip start and retime", "black timeline gap",
                    "invalid exact time and half-open end rejection", "preview/export pixel agreement",
                    "authenticated PNG hash and size", "private intermediate cleanup", "durable preview stages",
                    "independent CLI result readback", "actual encoded NTSC frame at exact rational presentation time",
                    "real clip.split transaction preserves rendered animation before, at, and after its boundary",
                    "sub-frame clip starts preserve their local clock phase on the sequence frame grid",
                    "fractional sequence end remains excluded without microsecond rounding",
                    "cancellation after real encoder progress without published or private frame leftovers",
                ], "revision": revision}
                if artifacts_dir:
                    (artifacts_dir / "report.json").write_text(json.dumps(report, indent=2))
                return report
            finally:
                if process.poll() is None:
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
    parser.add_argument("--artifacts-dir", type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.binary.resolve(), args.artifacts_dir), indent=2))
    except Exception as error:
        if args.artifacts_dir:
            (args.artifacts_dir / "report.json").write_text(json.dumps({"status": "failed", "error": str(error)}, indent=2))
        raise
