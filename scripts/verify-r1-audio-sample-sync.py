#!/usr/bin/env python3
"""Verify off-grid project audio placement through the public render job path."""
import argparse
import hashlib
import json
import signal
import socket
import struct
import subprocess
import time
import urllib.error
import urllib.request
import wave
from pathlib import Path


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--binary",
    type=Path,
    default=Path(__file__).resolve().parents[1] / "target/debug/opencut",
)
parser.add_argument("--artifacts-dir", type=Path, required=True)
args = parser.parse_args()

root = args.artifacts_dir.resolve()
root.mkdir(parents=True, exist_ok=True)
run = root / f"r1-audio-sync-{time.time_ns()}"
run.mkdir()
binary = args.binary.resolve()
database = run / "workspace.sqlite"
source = run / "impulses-44100.wav"
report_path = run / "report.json"
log = (run / "service.log").open("wb")
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    port = listener.getsockname()[1]
base = f"http://127.0.0.1:{port}"
process = None
token = None
trace = []


def request(path, body=None, *, raw=False):
    headers = {"Authorization": f"Bearer {token}"}
    if body is not None and not isinstance(body, bytes):
        body = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    try:
        with urllib.request.urlopen(
            urllib.request.Request(base + path, data=body, headers=headers), timeout=30
        ) as response:
            status, data = response.status, response.read()
    except urllib.error.HTTPError as error:
        status, data = error.code, error.read()
    if raw:
        trace.append(
            {
                "path": path,
                "status": status,
                "bytes": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
        return data
    value = json.loads(data)
    response_data = value.get("data")
    summary = {}
    if isinstance(response_data, dict):
        for key in ("id", "project_id", "asset_id", "revision", "job_id", "state", "kind"):
            if key in response_data:
                summary[key] = response_data[key]
        if "asset" in response_data and isinstance(response_data["asset"], dict):
            summary["asset_id"] = response_data["asset"].get("id")
        if "job_kinds" in response_data:
            summary["job_kinds"] = [kind["name"] for kind in response_data["job_kinds"]]
    trace.append(
        {
            "path": path,
            "status": status,
            "data_summary": summary,
            "error": value.get("error"),
        }
    )
    assert status < 400 and value.get("error") is None, value
    return value["data"]


def start():
    global process, token
    process = subprocess.Popen(
        [
            str(binary),
            "--database",
            str(database),
            "serve",
            "--listen",
            f"127.0.0.1:{port}",
            "--token-file",
            str(run / "token"),
        ],
        stdout=log,
        stderr=log,
    )
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline and process.poll() is None:
        try:
            token = (run / "token").read_text().strip()
            return request("/api/capabilities")
        except (FileNotFoundError, urllib.error.URLError, ConnectionError):
            time.sleep(0.05)
    raise RuntimeError(f"service failed to start; see {run / 'service.log'}")


def stop():
    if process and process.poll() is None:
        process.send_signal(signal.SIGINT)
        process.wait(timeout=15)


def make_impulse_wav(path):
    source_rate = 44_100
    samples = [0] * source_rate
    for index in (4_410, 26_460):
        samples[index] = 28_000
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(source_rate)
        output.writeframes(b"".join(struct.pack("<h", value) for value in samples))


def wait_for_job(job):
    deadline = time.monotonic() + 90
    while job["state"] not in ("succeeded", "failed", "cancelled"):
        assert time.monotonic() < deadline, job
        time.sleep(0.1)
        job = request(f"/api/jobs/{job['job_id']}")
    assert job["state"] == "succeeded", job
    return job


def render_revision(revision, key, output_name):
    job = wait_for_job(
        request(
            "/api/jobs",
            {
                "kind": "render.project",
                "idempotency_key": key,
                "input": {
                    "project_id": "audio-sync",
                    "revision": revision,
                    "sequence_id": "main",
                    "container": "mp4",
                    "video_codec": "h264",
                },
            },
        )
    )
    artifact = job["result"]["artifact"]
    encoded = request(f"/api/jobs/{job['job_id']}/artifact", raw=True)
    assert hashlib.sha256(encoded).hexdigest() == artifact["content_hash"]
    output = run / output_name
    output.write_bytes(encoded)
    return job, output, decode_audio(output)


def decode_audio(path):
    return subprocess.check_output(
        [
            "ffmpeg",
            "-v",
            "error",
            "-i",
            str(path),
            "-map",
            "0:a:0",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "-",
        ],
        stderr=subprocess.PIPE,
        timeout=90,
    )


def peak_near(pcm, expected, radius=16):
    samples = struct.unpack(f"<{len(pcm) // 2}h", pcm)
    stereo_frames = len(samples) // 2
    low, high = max(0, expected - radius), min(stereo_frames, expected + radius + 1)
    index = max(
        range(low, high),
        key=lambda frame: max(abs(samples[frame * 2]), abs(samples[frame * 2 + 1])),
    )
    magnitude = max(abs(samples[index * 2]), abs(samples[index * 2 + 1]))
    return index, magnitude


checks = []
try:
    make_impulse_wav(source)
    capabilities = start()
    assert "render.project" in {kind["name"] for kind in capabilities["job_kinds"]}

    created = request(
        "/api/projects",
        {
            "project": {
                "id": "audio-sync",
                "name": "R1 audio sample sync",
                "sequence_id": "main",
                "width": 160,
                "height": 90,
                "fps": {"numerator": 25, "denominator": 1},
            },
            "idempotency_key": "audio-sync-project",
        },
    )
    uploaded = request("/api/media/upload?name=impulses.wav", source.read_bytes())
    operations = [
        {"type": "asset.bind", "asset": uploaded["asset"]},
        {
            "type": "track.create",
            "sequence_id": "main",
            "track": {
                "id": "audio",
                "name": "Audio",
                "kind": "audio",
                "enabled": True,
                "muted": False,
                "locked": False,
                "clips": [],
                "params": {},
            },
        },
        {
            "type": "clip.insert",
            "sequence_id": "main",
            "track_id": "audio",
            "clip": {
                "id": "impulses",
                "asset_id": uploaded["asset"]["id"],
                "timeline_range": {
                    "start": {"ticks": "1", "timebase": 7},
                    "duration": {"ticks": "1", "timebase": 1},
                },
                "source_range": {
                    "start": {"ticks": "0", "timebase": 1},
                    "duration": {"ticks": "1", "timebase": 1},
                },
                "params": {},
                "channels": [],
            },
        },
    ]
    transaction = request(
        "/api/projects/audio-sync/branches/main/transactions",
        {
            "expected_revision": created["revision"],
            "idempotency_key": "audio-sync-bind-and-place",
            "label": "Place 44.1 kHz impulses at 1/7 second",
            "operations": operations,
        },
    )
    job, output, pcm = render_revision(
        transaction["revision"], "audio-sync-normal-render", "normal.mp4"
    )
    checks.append("HTTP upload, fixed-revision edit, render Job and verified artifact download")

    # 1/7 s maps to round(48000/7)=6857 samples. 44.1 kHz source
    # impulses at 0.1 s and 0.6 s become 4800 and 28800 output samples.
    expected_normal = [6_857 + 4_800, 6_857 + 28_800]
    observed_normal = [peak_near(pcm, sample) for sample in expected_normal]
    assert all(
        abs(actual - wanted) <= 1 and magnitude > 1000
        for (actual, magnitude), wanted in zip(observed_normal, expected_normal)
    ), {
        "expected_samples": expected_normal,
        "observed": observed_normal,
        "tolerance_samples": 1,
    }
    checks.append("44.1 kHz normal-speed impulses land within one 48 kHz sample of exact rational positions")

    retime = request(
        "/api/projects/audio-sync/branches/main/transactions",
        {
            "expected_revision": transaction["revision"],
            "idempotency_key": "audio-sync-retime",
            "label": "Retime audio to four fifths of source duration",
            "operations": [
                {
                    "type": "clip.retime",
                    "sequence_id": "main",
                    "track_id": "audio",
                    "clip_id": "impulses",
                    "timeline_duration": {"ticks": "4", "timebase": 5},
                }
            ],
        },
    )
    retimed_job, retimed_output, retimed_pcm = render_revision(
        retime["revision"], "audio-sync-retime-render", "retimed.mp4"
    )
    expected_retimed = [6_857 + 3_840, 6_857 + 23_040]
    # Audio retime defaults to Classic's pitch-changing playback-rate mode;
    # its known impulses should stay on the nearest output sample.
    retime_tolerance = 1
    observed_retimed = [
        peak_near(retimed_pcm, sample, retime_tolerance)
        for sample in expected_retimed
    ]
    assert all(
        abs(actual - wanted) <= retime_tolerance and magnitude > 1000
        for (actual, magnitude), wanted in zip(observed_retimed, expected_retimed)
    ), {
        "expected_samples": expected_retimed,
        "observed": observed_retimed,
        "tolerance_samples": retime_tolerance,
    }
    checks.append("Retimed impulses follow the exact 4/5 clip mapping within one 48 kHz output sample")

    split = request(
        "/api/projects/audio-sync/branches/main/transactions",
        {
            "expected_revision": retime["revision"],
            "idempotency_key": "audio-sync-retimed-split",
            "label": "Split retimed audio at its exact midpoint",
            "operations": [
                {
                    "type": "clip.split",
                    "sequence_id": "main",
                    "track_id": "audio",
                    "clip_id": "impulses",
                    "at": {"ticks": "19", "timebase": 35},
                    "right_clip_id": "impulses-right",
                }
            ],
        },
    )
    split_job, split_output, split_pcm = render_revision(
        split["revision"], "audio-sync-retimed-split-render", "retimed-split.mp4"
    )
    observed_split = [
        peak_near(split_pcm, sample, retime_tolerance)
        for sample in expected_retimed
    ]
    assert all(
        abs(actual - wanted) <= retime_tolerance and magnitude > 1000
        for (actual, magnitude), wanted in zip(observed_split, expected_retimed)
    ), {
        "expected_samples": expected_retimed,
        "observed": observed_split,
        "tolerance_samples": retime_tolerance,
    }
    checks.append("Splitting the retimed clip preserves both source-to-sequence pulse positions")

    unsupported_project = request(
        "/api/projects",
        {
            "project": {
                "id": "unsupported-audio-rate",
                "name": "Unrepresentable audio rate",
                "sequence_id": "main",
                "width": 160,
                "height": 90,
                "fps": {"numerator": 25, "denominator": 1},
            },
            "idempotency_key": "unsupported-audio-rate-project",
        },
    )
    unsupported_transaction = request(
        "/api/projects/unsupported-audio-rate/branches/main/transactions",
        {
            "expected_revision": unsupported_project["revision"],
            "idempotency_key": "unsupported-audio-rate-bind",
            "label": "Bind a retime requiring more than one sample of drift",
            "operations": [
                {"type": "asset.bind", "asset": uploaded["asset"]},
                {
                    "type": "track.create",
                    "sequence_id": "main",
                    "track": {
                        "id": "audio",
                        "name": "Audio",
                        "kind": "audio",
                        "enabled": True,
                        "muted": False,
                        "locked": False,
                        "clips": [],
                        "params": {},
                    },
                },
                {
                    "type": "clip.insert",
                    "sequence_id": "main",
                    "track_id": "audio",
                    "clip": {
                        "id": "impulses",
                        "asset_id": uploaded["asset"]["id"],
                        "timeline_range": {
                            "start": {"ticks": "0", "timebase": 1},
                            "duration": {"ticks": "29", "timebase": 15},
                        },
                        "source_range": {
                            "start": {"ticks": "0", "timebase": 1},
                            "duration": {"ticks": "1", "timebase": 1},
                        },
                        "params": {},
                        "channels": [],
                    },
                },
            ],
        },
    )
    preflight = request(
        "/api/render/preflight",
        {
            "project_id": "unsupported-audio-rate",
            "revision": unsupported_transaction["revision"],
            "sequence_id": "main",
            "container": "mp4",
            "video_codec": "h264",
        },
    )
    drift_issue = next(
        (
            issue
            for issue in preflight["issues"]
            if "cannot be represented within one 48 kHz sample" in issue["message"]
        ),
        None,
    )
    assert not preflight["ready"] and drift_issue is not None, preflight
    checks.append(
        "Preflight rejects a non-unity rate whose integer sample-rate approximation drifts by two samples"
    )

    report = {
        "status": "passed",
        "scope": "public HTTP upload/edit/render/download with independent AAC decode; synthetic impulses",
        "checks": checks,
        "evidence": {
            "source_rate": 44_100,
            "output_rate": 48_000,
            "timeline_start": {"ticks": "1", "timebase": 7},
            "normal_speed": {
                "expected_output_samples": expected_normal,
                "observed_peak_and_magnitude": observed_normal,
                "tolerance_samples": 1,
                "artifact_sha256": hashlib.sha256((run / "normal.mp4").read_bytes()).hexdigest(),
                "job_id": job["job_id"],
                "revision": transaction["revision"],
            },
            "retimed": {
                "expected_output_samples": expected_retimed,
                "observed_peak_and_magnitude": observed_retimed,
                "artifact_sha256": hashlib.sha256(retimed_output.read_bytes()).hexdigest(),
                "job_id": retimed_job["job_id"],
                "revision": retime["revision"],
                "tolerance_samples": retime_tolerance,
            },
            "retimed_split": {
                "observed_peak_and_magnitude": observed_split,
                "artifact_sha256": hashlib.sha256(split_output.read_bytes()).hexdigest(),
                "job_id": split_job["job_id"],
                "revision": split["revision"],
                "tolerance_samples": retime_tolerance,
            },
            "unsupported_rate_preflight": {
                "ready": preflight["ready"],
                "issue": drift_issue,
                "revision": unsupported_transaction["revision"],
            },
        },
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "directory": str(run),
        "trace": trace,
    }
    report_path.write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
except Exception as error:
    report_path.write_text(
        json.dumps(
            {
                "status": "failed",
                "error": repr(error),
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "directory": str(run),
                "trace": trace,
            },
            indent=2,
        )
    )
    raise
finally:
    stop()
    log.close()
