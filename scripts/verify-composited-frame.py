#!/usr/bin/env python3
"""Real public acceptance for direct composited project-frame jobs."""
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


def composited_ffmpeg_pids(root_pid):
    rows = subprocess.check_output(
        ["ps", "-axo", "pid=,ppid=,command="], text=True
    ).splitlines()
    processes = {}
    children = {}
    for row in rows:
        fields = row.strip().split(None, 2)
        if len(fields) != 3:
            continue
        pid, parent = map(int, fields[:2])
        processes[pid] = fields[2]
        children.setdefault(parent, set()).add(pid)
    descendants = set()
    frontier = [root_pid]
    while frontier:
        parent = frontier.pop()
        for child in children.get(parent, ()):
            if child not in descendants:
                descendants.add(child)
                frontier.append(child)
    return {
        pid for pid in descendants
        if "ffmpeg" in processes[pid].lower()
        and "ffprobe" not in processes[pid].lower()
        and "-filter_complex" in processes[pid]
        and "-frames:v 1" in processes[pid]
        and ("-c:v png" in processes[pid] or "-f image2" in processes[pid])
    }


def at(ticks, timebase=10):
    return {"ticks": str(ticks), "timebase": timebase}


def rational(value):
    return Fraction(int(value["ticks"]), value["timebase"])


def decode_rgb(path):
    return subprocess.check_output([
        "ffmpeg", "-v", "error", "-i", str(path), "-frames:v", "1",
        "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
    ])


def _verify(binary, artifacts_dir=None):
    if artifacts_dir:
        artifacts_dir.mkdir(parents=True, exist_ok=True)
        for name in ("direct.png", "encoded.png", "scoped-visual.png", "scoped-audio-tail.png"):
            (artifacts_dir / name).unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="opencut-composited-frame-") as directory:
        root = Path(directory)
        source = root / "red.mp4"
        long_source = root / "long-red.mp4"
        audio_source = root / "tone.m4a"
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "color=c=red:s=160x90:r=10:d=2", "-c:v", "libx264",
            "-pix_fmt", "yuv420p", str(source),
        ], check=True)
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "sine=frequency=440:sample_rate=48000:duration=4", "-c:a", "aac",
            str(audio_source),
        ], check=True)
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "color=c=red:s=1280x720:r=30:d=60", "-c:v", "libx264",
            "-preset", "ultrafast", "-pix_fmt", "yuv420p", str(long_source),
        ], check=True)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        database = root / "editor.sqlite"
        token_file = root / "token"
        base = f"http://127.0.0.1:{port}"
        token = None
        with tempfile.TemporaryFile() as logs:
            process = subprocess.Popen([
                str(binary), "--database", str(database), "serve", "--listen",
                f"127.0.0.1:{port}", "--token-file", str(token_file),
            ], stdout=logs, stderr=logs)

            def request(path, data=None, raw=False):
                headers = {"Authorization": f"Bearer {token}"}
                if data is not None and not isinstance(data, bytes):
                    data = json.dumps(data).encode()
                    headers["Content-Type"] = "application/json"
                try:
                    req = urllib.request.Request(base + path, data=data, headers=headers)
                    with urllib.request.urlopen(req, timeout=15) as response:
                        payload = response.read()
                        return payload if raw else json.loads(payload)
                except urllib.error.HTTPError as error:
                    return json.load(error)

            def call(path, data=None):
                response = request(path, data)
                assert response["error"] is None, response
                return response["data"]

            def wait(job, reject_mp4=False, expected_state="succeeded"):
                deadline = time.monotonic() + 90
                observed_mp4 = set()
                attempt_root = root / "artifacts" / job["job_id"]
                while job["state"] not in ("succeeded", "failed", "cancelled"):
                    observed_mp4.update(attempt_root.rglob("*.mp4"))
                    assert process.poll() is None, "service exited while waiting"
                    assert time.monotonic() < deadline, job
                    time.sleep(.02)
                    job = call(f'/v1/jobs/{job["job_id"]}')
                observed_mp4.update(attempt_root.rglob("*.mp4"))
                if reject_mp4:
                    assert not observed_mp4, sorted(map(str, observed_mp4))
                assert job["state"] == expected_state, job
                return job

            def download(job, name):
                payload = request(f'/v1/jobs/{job["job_id"]}/artifact', raw=True)
                descriptor = job["result"]["artifact"]
                assert descriptor["mime_type"] == "image/png", descriptor
                assert len(payload) == descriptor["size"]
                assert hashlib.sha256(payload).hexdigest() == descriptor["content_hash"]
                path = root / name
                path.write_bytes(payload)
                if artifacts_dir:
                    shutil.copyfile(path, artifacts_dir / name)
                return path

            try:
                deadline = time.monotonic() + 15
                while True:
                    assert process.poll() is None, "service exited during startup"
                    try:
                        token = token_file.read_text().strip()
                        capabilities = call("/v1/capabilities")
                        break
                    except (FileNotFoundError, urllib.error.URLError):
                        assert time.monotonic() < deadline, "startup timeout"
                        time.sleep(.05)
                rendering = capabilities["rendering"]
                assert rendering["composited_frame_preview"] is True, rendering
                assert rendering["composited_frame_execution"] == "direct_png_may_decode_prefix"
                descriptors = {item["name"]: item for item in capabilities["job_kinds"]}
                direct_schema = json.dumps(descriptors["preview.composited_frame"]["result_schema"])
                encoded_schema = json.dumps(descriptors["preview.project_frame"]["result_schema"])
                assert "composited_project_frame" in direct_schema
                assert "encoded_project_frame" not in direct_schema
                assert "encoded_project_frame" in encoded_schema
                assert "composited_project_frame" not in encoded_schema

                upload = call("/v1/media/upload?name=red.mp4", source.read_bytes())
                snapshot = call("/v1/projects", {
                    "project": {"id": "direct", "name": "Direct frame", "sequence_id": "main",
                                "width": 160, "height": 90,
                                "fps": {"numerator": 10, "denominator": 1}},
                    "idempotency_key": "create",
                })
                edited = call("/v1/projects/direct/branches/main/transactions", {
                    "expected_revision": snapshot["revision"], "idempotency_key": "edit",
                    "label": "Red visual", "operations": [
                        {"type": "asset.bind", "asset": upload["asset"]},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": True, "locked": False, "clips": [], "params": {}}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "video",
                         "clip": {"id": "red", "asset_id": upload["asset"]["id"],
                                  "timeline_range": {"start": at(0), "duration": at(20)},
                                  "source_range": {"start": at(0), "duration": at(20)},
                                  "params": {}, "channels": []}},
                    ],
                })
                revision = edited["revision"]

                def body(kind, key):
                    return {"kind": kind, "idempotency_key": key, "input": {
                        "project_id": "direct", "revision": revision, "sequence_id": "main",
                        "requested_time": at(5), "alignment": "exact",
                    }}

                direct = call("/v1/jobs", body("preview.composited_frame", "direct-frame"))
                assert call("/v1/jobs", body("preview.composited_frame", "direct-frame"))["job_id"] == direct["job_id"]
                encoded = call("/v1/jobs", body("preview.project_frame", "encoded-frame"))
                later = call("/v1/projects/direct/branches/main/transactions", {
                    "expected_revision": revision, "idempotency_key": "delete", "label": "Later edit",
                    "operations": [{"type": "clip.delete", "sequence_id": "main",
                                    "track_id": "video", "clip_id": "red"}],
                })
                assert later["revision"] != revision

                direct = wait(direct, reject_mp4=True)
                encoded = wait(encoded)
                assert direct["input"]["snapshot"]["revision"] == revision
                assert direct["result"]["revision"] == revision
                assert direct["result"]["quality"] == "composited_project_frame"
                assert encoded["result"]["quality"] == "encoded_project_frame"
                assert rational(direct["result"]["requested_time"]) == Fraction(1, 2)
                assert rational(direct["result"]["actual_sequence_time"]) == Fraction(1, 2)
                direct_png = download(direct, "direct.png")
                encoded_png = download(encoded, "encoded.png")
                direct_pixels = decode_rgb(direct_png)
                encoded_pixels = decode_rgb(encoded_png)
                assert len(direct_pixels) == len(encoded_pixels) == 160 * 90 * 3
                assert direct_pixels[0] > 180 and max(direct_pixels[1:3]) < 70, direct_pixels[:3]
                baseline_differences = [
                    abs(a - b) for a, b in zip(direct_pixels, encoded_pixels)
                ]
                baseline_max_difference = max(baseline_differences)
                baseline_mean_difference = sum(baseline_differences) / len(baseline_differences)
                assert baseline_max_difference <= 5
                cli = json.loads(subprocess.check_output([
                    str(binary), "--database", str(database), "--json", "job", "get",
                    direct["job_id"],
                ]))
                assert cli["data"]["result"] == direct["result"], cli

                # A direct visual frame must not depend on audio bytes. Use a
                # separate revision so deliberately corrupting its audio CAS
                # cannot affect the baseline checks above.
                scoped_video = call("/v1/media/upload?name=scoped-red.mp4", source.read_bytes())
                scoped_audio = call("/v1/media/upload?name=tone.m4a", audio_source.read_bytes())
                scoped = call("/v1/projects", {
                    "project": {"id": "dependency-scope", "name": "Visual dependency scope",
                                "sequence_id": "main", "width": 160, "height": 90,
                                "fps": {"numerator": 10, "denominator": 1}},
                    "idempotency_key": "create-scope",
                })
                scoped_edit = call("/v1/projects/dependency-scope/branches/main/transactions", {
                    "expected_revision": scoped["revision"], "idempotency_key": "bind-scope",
                    "label": "Video with longer audio tail", "operations": [
                        {"type": "asset.bind", "asset": scoped_video["asset"]},
                        {"type": "asset.bind", "asset": scoped_audio["asset"]},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "audio", "name": "Audio", "kind": "audio", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "video",
                         "clip": {"id": "visual", "asset_id": scoped_video["asset"]["id"],
                                  "timeline_range": {"start": at(0), "duration": at(20)},
                                  "source_range": {"start": at(0), "duration": at(20)},
                                  "params": {}, "channels": []}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "audio",
                         "clip": {"id": "tone", "asset_id": scoped_audio["asset"]["id"],
                                  "timeline_range": {"start": at(0), "duration": at(40)},
                                  "source_range": {"start": at(0), "duration": at(40)},
                                  "params": {}, "channels": []}},
                    ],
                })
                scoped_snapshot = call(
                    f'/v1/projects/dependency-scope/revisions/{scoped_edit["revision"]}'
                )
                assert scoped_snapshot["revision"] == scoped_edit["revision"]
                assets = {asset["id"]: asset for asset in scoped_snapshot["project"]["assets"]}
                audio = assets[scoped_audio["asset"]["id"]]
                visual_id = scoped_video["asset"]["id"]
                audio_uri = audio["uri"].removeprefix("file://")
                Path(audio_uri).unlink()
                audio_hash = audio["version"]["content_hash"]
                (root / "asset-store" / audio_hash[:2] / audio_hash).unlink()

                def scoped_body(kind, key, ticks):
                    common = {"project_id": "dependency-scope",
                              "revision": scoped_edit["revision"], "sequence_id": "main"}
                    if kind == "render.project":
                        common.update({"container": "mp4", "video_codec": "h264"})
                    else:
                        common.update({"requested_time": at(ticks), "alignment": "exact"})
                    return {"kind": kind, "idempotency_key": key, "input": common}

                scoped_visual = wait(call(
                    "/v1/jobs", scoped_body("preview.composited_frame", "scope-visual", 5)
                ))
                locked_ids = {
                    asset["asset_id"] for asset in scoped_visual["input"]["asset_lock"]["assets"]
                }
                assert locked_ids == {visual_id}, scoped_visual["input"]["asset_lock"]
                scoped_visual_pixels = decode_rgb(download(scoped_visual, "scoped-visual.png"))
                assert scoped_visual_pixels[0] > 180 and max(scoped_visual_pixels[1:3]) < 70

                scoped_tail = wait(call(
                    "/v1/jobs", scoped_body("preview.composited_frame", "scope-audio-tail", 30)
                ))
                assert rational(scoped_tail["result"]["actual_sequence_time"]) == 3
                tail_pixels = decode_rgb(download(scoped_tail, "scoped-audio-tail.png"))
                assert max(tail_pixels) < 8, "audio-only tail must render as black"

                for kind in ("preview.project_frame", "render.project"):
                    rejected = request("/v1/jobs", scoped_body(kind, f"missing-audio-{kind}", 5))
                    assert rejected["error"] is not None, rejected
                    assert rejected["error"]["code"] == "ASSET_INTEGRITY_ERROR", rejected

                long_upload = call("/v1/media/upload?name=long-red.mp4", long_source.read_bytes())
                cancel_project = call("/v1/projects", {
                    "project": {"id": "cancel-direct", "name": "Cancellation", "sequence_id": "main",
                                "width": 1280, "height": 720,
                                "fps": {"numerator": 30, "denominator": 1}},
                    "idempotency_key": "create-cancel",
                })
                cancel_edit = call("/v1/projects/cancel-direct/branches/main/transactions", {
                    "expected_revision": cancel_project["revision"], "idempotency_key": "edit-cancel",
                    "label": "Long red visual", "operations": [
                        {"type": "asset.bind", "asset": long_upload["asset"]},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "video",
                         "clip": {"id": "long", "asset_id": long_upload["asset"]["id"],
                                  "timeline_range": {"start": at(0, 30), "duration": at(1800, 30)},
                                  "source_range": {"start": at(0, 30), "duration": at(1800, 30)},
                                  "params": {}, "channels": []}},
                    ],
                })
                active = call("/v1/jobs", {
                    "kind": "preview.composited_frame", "idempotency_key": "cancel-active-direct",
                    "input": {"project_id": "cancel-direct", "revision": cancel_edit["revision"],
                              "sequence_id": "main", "requested_time": at(1500, 30),
                              "alignment": "exact"},
                })
                deadline = time.monotonic() + 45
                previous_ffmpeg = set()
                observed_ffmpeg = set()
                while time.monotonic() < deadline:
                    active = call(f'/v1/jobs/{active["job_id"]}')
                    current_ffmpeg = composited_ffmpeg_pids(process.pid)
                    stable_ffmpeg = previous_ffmpeg & current_ffmpeg
                    previous_ffmpeg = current_ffmpeg
                    if active["state"] == "running" and stable_ffmpeg:
                        observed_ffmpeg.update(stable_ffmpeg)
                        break
                    assert active["state"] in ("queued", "running"), active
                    time.sleep(.05)
                assert observed_ffmpeg, {
                    "failure": "no stable compositing FFmpeg child before cancellation", "job": active,
                }
                cancelled = call(f'/v1/jobs/{active["job_id"]}/cancel', {})
                if cancelled["state"] not in ("cancelled", "failed"):
                    cancelled = wait(cancelled, expected_state="cancelled")
                assert cancelled["state"] == "cancelled" and cancelled["result"] is None, cancelled
                assert request(f'/v1/jobs/{cancelled["job_id"]}/artifact')["error"] is not None
                cancelled_root = root / "artifacts" / cancelled["job_id"]
                assert not list(cancelled_root.glob("attempt-*/inputs")), cancelled_root
                assert not list(cancelled_root.rglob("*.png")), cancelled_root
                return {"status": "passed", "checks": [
                    "seventh typed job schema and distinct quality enums",
                    "fixed revision survives later edit", "idempotent direct frame request",
                    "direct PNG pixel and exact time", "authenticated artifact size and hash",
                    "no MP4 appeared in direct job attempt",
                    "bounded encoded/direct comparison for the fixed red fixture",
                    "independent CLI result readback",
                    "active compositing-FFmpeg cancellation publishes no PNG and cleans private inputs",
                    "direct lock excludes missing audio while encoded and render jobs reject it",
                    "audio-only sequence tail preserves time and renders black",
                ], "revision": revision, "job_id": direct["job_id"],
                    "cancel_activity_evidence": {"stable_compositing_ffmpeg_pids": sorted(observed_ffmpeg)},
                    "baseline_fixture_difference": {
                        "scope": "fixed red fixture only; not a general pixel-parity guarantee",
                        "max_channel_difference": baseline_max_difference,
                        "mean_channel_difference": baseline_mean_difference,
                    },
                    "saved_artifacts": str(artifacts_dir.resolve()) if artifacts_dir else None}
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


def verify(binary, report=None, artifacts_dir=None):
    if report:
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps({"status": "running"}, indent=2) + "\n")
    try:
        result = _verify(binary, artifacts_dir)
        if report:
            report.write_text(json.dumps(result, indent=2) + "\n")
        return result
    except Exception as error:
        if report:
            report.write_text(json.dumps({
                "status": "failed", "error": f"{type(error).__name__}: {error}",
            }, indent=2) + "\n")
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--artifacts-dir", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve(), args.report, args.artifacts_dir), indent=2))
