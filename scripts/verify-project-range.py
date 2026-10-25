#!/usr/bin/env python3
"""Real public acceptance for fixed-revision project range previews."""
import argparse
from array import array
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


def range_ffmpeg_pids(root_pid, job_root):
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
    job_path = str(job_root)
    return {
        pid for pid in descendants
        if "ffmpeg" in processes[pid].lower()
        and "ffprobe" not in processes[pid].lower()
        and "-filter_complex" in processes[pid]
        and "-c:v libx264" in processes[pid]
        and job_path in processes[pid]
    }


def at(ticks, timebase=10):
    return {"ticks": str(ticks), "timebase": timebase}


def rational(value):
    return Fraction(int(value["ticks"]), value["timebase"])


def _verify(binary, artifacts_dir=None):
    if artifacts_dir:
        artifacts_dir.mkdir(parents=True, exist_ok=True)
        (artifacts_dir / "project-range.mp4").unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="opencut-project-range-") as directory:
        root = Path(directory)
        video = root / "red.mp4"
        audio = root / "tone.m4a"
        long_video = root / "long-blue.mp4"
        long_audio = root / "long-tone.m4a"
        subprocess.run([
            "ffmpeg", "-v", "error",
            "-f", "lavfi", "-i", "color=c=red:s=160x90:r=10:d=1",
            "-f", "lavfi", "-i", "color=c=green:s=160x90:r=10:d=1",
            "-f", "lavfi", "-i", "color=c=blue:s=160x90:r=10:d=2",
            "-filter_complex", "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
            "-map", "[v]", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(video),
        ], check=True)
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "color=c=blue:s=1280x720:r=30:d=45", "-c:v", "libx264",
            "-preset", "ultrafast", "-pix_fmt", "yuv420p", str(long_video),
        ], check=True)
        subprocess.run([
            "ffmpeg", "-v", "error", "-f", "lavfi", "-i",
            "sine=frequency=330:sample_rate=48000:duration=45", "-c:a", "aac", str(long_audio),
        ], check=True)
        subprocess.run([
            "ffmpeg", "-v", "error",
            "-f", "lavfi", "-i", "anullsrc=r=48000:cl=mono:d=1",
            "-f", "lavfi", "-i", "sine=frequency=880:sample_rate=48000:duration=1",
            "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=2",
            "-filter_complex",
            "[1:a]volume=0.2[a1];[2:a]volume=0.8[a2];[0:a][a1][a2]concat=n=3:v=0:a=1[a]",
            "-map", "[a]", "-c:a", "aac", str(audio),
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

            def wait(job):
                deadline = time.monotonic() + 90
                while job["state"] not in ("succeeded", "failed", "cancelled"):
                    assert process.poll() is None, "service exited while waiting"
                    assert time.monotonic() < deadline, job
                    time.sleep(.05)
                    job = call(f'/api/jobs/{job["job_id"]}')
                assert job["state"] == "succeeded", job
                return job

            try:
                deadline = time.monotonic() + 15
                while True:
                    assert process.poll() is None, "service exited during startup"
                    try:
                        token = token_file.read_text().strip()
                        capabilities = call("/api/capabilities")
                        break
                    except (FileNotFoundError, urllib.error.URLError):
                        assert time.monotonic() < deadline, "startup timeout"
                        time.sleep(.05)
                rendering = capabilities["rendering"]
                assert rendering["project_range_preview"] is True, rendering
                assert rendering["preview_range_execution"] == "range_encode_may_decode_prefix"
                descriptors = {item["name"]: item for item in capabilities["job_kinds"]}
                range_schema = json.dumps(descriptors["preview.project_range"])
                assert "requested_range" in range_schema and "actual_sequence_range" in range_schema
                assert "encoded_project_range" in range_schema

                uploaded_video = call("/api/media/upload?name=red.mp4", video.read_bytes())
                uploaded_audio = call("/api/media/upload?name=tone.m4a", audio.read_bytes())
                created = call("/api/projects", {
                    "project": {"id": "range", "name": "Range preview", "sequence_id": "main",
                                "width": 160, "height": 90,
                                "fps": {"numerator": 10, "denominator": 1}},
                    "idempotency_key": "create-range",
                })
                transaction = call("/api/projects/range/branches/main/transactions", {
                    "expected_revision": created["revision"], "idempotency_key": "edit-range",
                    "label": "Four second audiovisual fixture", "operations": [
                        {"type": "asset.bind", "asset": uploaded_video["asset"]},
                        {"type": "asset.bind", "asset": uploaded_audio["asset"]},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "audio", "name": "Audio", "kind": "audio", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "video",
                         "clip": {"id": "red", "asset_id": uploaded_video["asset"]["id"],
                                  "timeline_range": {"start": at(0), "duration": at(40)},
                                  "source_range": {"start": at(0), "duration": at(40)},
                                  "params": {}, "channels": []}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "audio",
                         "clip": {"id": "tone", "asset_id": uploaded_audio["asset"]["id"],
                                  "timeline_range": {"start": at(0), "duration": at(40)},
                                  "source_range": {"start": at(0), "duration": at(40)},
                                  "params": {}, "channels": []}},
                    ],
                })
                # Transaction results are commit metadata. Read the immutable
                # snapshot explicitly before building the fixed-revision job.
                fixed = call("/api/projects/range/branches/main/snapshot")
                assert fixed["revision"] == transaction["revision"]
                revision = fixed["revision"]
                body = {"kind": "preview.project_range", "idempotency_key": "range-1", "input": {
                    "project_id": "range", "revision": revision, "sequence_id": "main",
                    "requested_range": {"start": at(10), "duration": at(20)},
                    "alignment": "exact",
                }}
                queued = call("/api/jobs", body)
                assert call("/api/jobs", body)["job_id"] == queued["job_id"]
                later = call("/api/projects/range/branches/main/transactions", {
                    "expected_revision": revision, "idempotency_key": "later-edit",
                    "label": "Remove clips after queue", "operations": [
                        {"type": "clip.delete", "sequence_id": "main", "track_id": "video", "clip_id": "red"},
                        {"type": "clip.delete", "sequence_id": "main", "track_id": "audio", "clip_id": "tone"},
                    ],
                })
                assert later["revision"] != revision
                completed = wait(queued)
                result = completed["result"]
                assert result["revision"] == revision and result["sequence_id"] == "main"
                assert result["quality"] == "encoded_project_range"
                assert rational(result["requested_range"]["start"]) == 1
                assert rational(result["requested_range"]["duration"]) == 2
                assert rational(result["actual_sequence_range"]["start"]) == 1
                assert rational(result["actual_sequence_range"]["duration"]) == 2
                assert completed["input"]["snapshot"] == fixed
                assert {item["asset_id"] for item in completed["input"]["asset_lock"]["assets"]} == {
                    uploaded_video["asset"]["id"], uploaded_audio["asset"]["id"]}

                payload = request(f'/api/jobs/{completed["job_id"]}/artifact', raw=True)
                artifact = result["artifact"]
                assert artifact["mime_type"] == "video/mp4"
                assert len(payload) == artifact["size"]
                assert hashlib.sha256(payload).hexdigest() == artifact["content_hash"]
                output = root / "project-range.mp4"
                output.write_bytes(payload)
                if artifacts_dir:
                    shutil.copyfile(output, artifacts_dir / output.name)
                duration = float(subprocess.check_output([
                    "ffprobe", "-v", "error", "-show_entries", "format=duration",
                    "-of", "default=nw=1:nk=1", str(output),
                ], text=True).strip())
                assert abs(duration - 2.0) <= 0.08, duration
                def pixel_at(seconds):
                    return subprocess.check_output([
                        "ffmpeg", "-v", "error", "-ss", str(seconds), "-i", str(output),
                        "-frames:v", "1", "-vf", "scale=1:1", "-f", "rawvideo",
                        "-pix_fmt", "rgb24", "-",
                    ])

                beginning = pixel_at(0.2)
                ending = pixel_at(1.5)
                assert beginning[1] > 80 and beginning[0] < 30 and beginning[2] < 30, list(beginning)
                assert ending[2] > 180 and ending[0] < 40 and ending[1] < 80, list(ending)
                pcm_bytes = subprocess.check_output([
                    "ffmpeg", "-v", "error", "-i", str(output), "-vn", "-ac", "1",
                    "-ar", "48000", "-f", "s16le", "-",
                ])
                pcm = array("h")
                pcm.frombytes(pcm_bytes)
                assert 90000 <= len(pcm) <= 102000, len(pcm)
                def audio_evidence(start, end):
                    samples = pcm[int(start * 48000):int(end * 48000)]
                    rms = (sum(sample * sample for sample in samples) / len(samples)) ** 0.5
                    # AAC can introduce small oscillations near zero. Count a
                    # crossing only after reaching the opposite side of a
                    # symmetric hysteresis band, without changing Hz bounds.
                    threshold = rms * 0.1
                    state = 0
                    crossings = 0
                    for sample in samples:
                        next_state = (1 if sample > threshold else
                                      -1 if sample < -threshold else state)
                        if state and next_state != state:
                            crossings += 1
                        state = next_state
                    frequency = crossings / (2 * (end - start))
                    return frequency, rms

                first_frequency, first_rms = audio_evidence(0.1, 0.9)
                second_frequency, second_rms = audio_evidence(1.1, 1.9)
                assert 820 <= first_frequency <= 940, first_frequency
                assert 400 <= second_frequency <= 480, second_frequency
                assert second_rms > first_rms * 2.5, (first_rms, second_rms)

                independent = json.loads(subprocess.check_output([
                    str(binary), "--database", str(database), "--json", "job", "get",
                    completed["job_id"],
                ]))
                assert independent["data"]["result"] == result, independent

                assets = {asset["id"]: asset for asset in fixed["project"]["assets"]}
                audio_asset = assets[uploaded_audio["asset"]["id"]]
                Path(audio_asset["uri"].removeprefix("file://")).unlink()
                audio_hash = audio_asset["version"]["content_hash"]
                (root / "asset-store" / audio_hash[:2] / audio_hash).unlink()
                assert call("/api/jobs", body)["job_id"] == completed["job_id"]
                missing_audio = json.loads(json.dumps(body))
                missing_audio["idempotency_key"] = "range-missing-audio"
                rejected = request("/api/jobs", missing_audio)
                assert rejected["error"]["code"] == "ASSET_INTEGRITY_ERROR", rejected

                cancel_video = call("/api/media/upload?name=long-blue.mp4", long_video.read_bytes())
                cancel_audio = call("/api/media/upload?name=long-tone.m4a", long_audio.read_bytes())
                cancel_project = call("/api/projects", {
                    "project": {"id": "cancel-range", "name": "Range cancellation",
                                "sequence_id": "main", "width": 1280, "height": 720,
                                "fps": {"numerator": 30, "denominator": 1}},
                    "idempotency_key": "create-cancel-range",
                })
                cancel_edit = call("/api/projects/cancel-range/branches/main/transactions", {
                    "expected_revision": cancel_project["revision"],
                    "idempotency_key": "edit-cancel-range", "label": "Long audiovisual range",
                    "operations": [
                        {"type": "asset.bind", "asset": cancel_video["asset"]},
                        {"type": "asset.bind", "asset": cancel_audio["asset"]},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "track.create", "sequence_id": "main", "track": {
                            "id": "audio", "name": "Audio", "kind": "audio", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {}}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "video",
                         "clip": {"id": "cancel-video-clip", "asset_id": cancel_video["asset"]["id"],
                                  "timeline_range": {"start": at(0, 30), "duration": at(1350, 30)},
                                  "source_range": {"start": at(0, 30), "duration": at(1350, 30)},
                                  "params": {}, "channels": []}},
                        {"type": "clip.insert", "sequence_id": "main", "track_id": "audio",
                         "clip": {"id": "cancel-audio-clip", "asset_id": cancel_audio["asset"]["id"],
                                  "timeline_range": {"start": at(0, 30), "duration": at(1350, 30)},
                                  "source_range": {"start": at(0, 30), "duration": at(1350, 30)},
                                  "params": {}, "channels": []}},
                    ],
                })
                active = call("/api/jobs", {
                    "kind": "preview.project_range", "idempotency_key": "cancel-active-range",
                    "input": {"project_id": "cancel-range", "revision": cancel_edit["revision"],
                              "sequence_id": "main",
                              "requested_range": {"start": at(0, 30), "duration": at(1200, 30)},
                              "alignment": "exact"},
                })
                active_root = root / "artifacts" / active["job_id"]
                previous_ffmpeg = set()
                observed_ffmpeg = set()
                deadline = time.monotonic() + 45
                while time.monotonic() < deadline:
                    active = call(f'/api/jobs/{active["job_id"]}')
                    current_ffmpeg = range_ffmpeg_pids(process.pid, active_root)
                    stable_ffmpeg = previous_ffmpeg & current_ffmpeg
                    previous_ffmpeg = current_ffmpeg
                    if active["state"] == "running" and stable_ffmpeg:
                        observed_ffmpeg.update(stable_ffmpeg)
                        break
                    assert active["state"] in ("queued", "running"), active
                    time.sleep(.05)
                assert observed_ffmpeg, {"failure": "no stable range FFmpeg child", "job": active}
                cancelled = call(f'/api/jobs/{active["job_id"]}/cancel', {})
                deadline = time.monotonic() + 30
                while cancelled["state"] not in ("cancelled", "failed", "succeeded"):
                    assert time.monotonic() < deadline, cancelled
                    time.sleep(.05)
                    cancelled = call(f'/api/jobs/{active["job_id"]}')
                assert cancelled["state"] == "cancelled" and cancelled["result"] is None, cancelled
                assert request(f'/api/jobs/{active["job_id"]}/artifact')["error"] is not None
                assert not list(active_root.rglob("*.mp4")), active_root
                assert not list(active_root.glob("attempt-*/inputs")), active_root

                return {"status": "passed", "checks": [
                    "eighth typed job schema and fixed revision snapshot readback",
                    "idempotent range job and later-edit isolation", "nonzero exact range identity",
                    "authenticated MP4 hash and two-second duration",
                    "absolute one-second start proven by green-to-blue visual transition",
                    "absolute audio start proven by 880Hz-to-440Hz frequency and RMS transition",
                    "full audiovisual asset lock",
                    "missing audio rejects new request without breaking idempotent retry",
                    "independent CLI result readback",
                    "active path-scoped range FFmpeg cancellation leaves no MP4 or private inputs",
                ], "revision": revision, "job_id": completed["job_id"],
                    "saved_artifact": str((artifacts_dir / output.name).resolve()) if artifacts_dir else None,
                    "evidence": {"duration_seconds": duration, "pcm_samples": len(pcm),
                                 "first_audio_hz": first_frequency, "second_audio_hz": second_frequency,
                                 "first_audio_rms": first_rms, "second_audio_rms": second_rms,
                                 "cancel_ffmpeg_pids": sorted(observed_ffmpeg)}}
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
