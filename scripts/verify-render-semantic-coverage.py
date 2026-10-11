#!/usr/bin/env python3
"""Public CLI/Worker/HTTP tests against independent color and time expectations.

Only synthetic sources are generated. A same-backend preview/export comparison
is deliberately not the oracle for whether the requested transition exists.
"""
import argparse
import array
import math
import sys
import wave
import hashlib
import json
from pathlib import Path
import socket
import shutil
import subprocess
import time
import urllib.request


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(binary, artifacts):
    run = artifacts.resolve() / f"render-semantics-{time.time_ns()}"
    run.mkdir(parents=True)
    # Cargo may replace target/debug/opencut during another check. Every call in
    # this acceptance must use the same immutable candidate, including the hash oracle.
    candidate = run / "opencut-candidate"
    before_copy = sha(binary.read_bytes())
    shutil.copy2(binary, candidate)
    assert sha(candidate.read_bytes()) == before_copy == sha(binary.read_bytes()), "Binary changed while freezing candidate"
    binary = candidate
    database = run / "workspace.sqlite"
    trace, jobs, pixels, pcm_checks = [], [], [], []

    def call(tool, body, error=None):
        proc = subprocess.run([str(binary), "--database", str(database), "--json", "call", tool,
                               "--input", "-"], input=json.dumps(body), text=True, capture_output=True, timeout=90)
        reply = json.loads(proc.stdout)
        trace.append({"tool": tool, "response": reply})
        if error:
            assert proc.returncode and reply["error"]["code"] == error, reply
        else:
            assert proc.returncode == 0 and reply["error"] is None, (reply, proc.stderr)
        return reply.get("data")

    def job(tool, body):
        queued = call(tool, body)
        worker = subprocess.run([str(binary), "--database", str(database), "worker", "run", "--once"],
                                text=True, capture_output=True, timeout=120)
        assert worker.returncode == 0, worker.stderr
        done = call("opencut_job_view", {"job_id": queued["job_id"]})
        assert done["state"] == "succeeded", done
        receipt = call("opencut_job_execution_receipt_v1", {"job_id": queued["job_id"]})
        record = call("opencut_job_get", {"job_id": queued["job_id"]})
        artifact = record["result"]["artifact"]
        assert receipt["job_id"] == record["job_id"] and receipt["attempt"] == record["attempt"]
        assert receipt["source_revision"] == body["revision"]
        assert receipt["execution"]["process_binary_sha256"] == sha(binary.read_bytes())
        assert receipt["artifact_sha256"] == artifact["content_hash"] == sha(Path(artifact["path"]).read_bytes())
        assert receipt["artifact_size"] == Path(artifact["path"]).stat().st_size
        assert receipt["semantic_output_verified"] is False
        # Receipt substitution across attempts must fail even if the artifact exists.
        receipt_path = Path(artifact["path"]).parent / "execution-v1.json"
        original = receipt_path.read_bytes()
        tampered = json.loads(original)
        tampered["attempt"] += 1
        try:
            receipt_path.write_text(json.dumps(tampered))
            call("opencut_job_execution_receipt_v1", {"job_id": queued["job_id"]}, error="ARTIFACT_VERIFICATION_FAILED")
        finally:
            receipt_path.write_bytes(original)
        jobs.append(done)
        return done

    def tick(value):
        return {"ticks": str(value), "timebase": 10}

    def span(start, duration):
        return {"start": tick(start), "duration": tick(duration)}

    initial = call("opencut_project_v3_create", {"project": {"id": "semantic-film", "name": "Synthetic oracle",
        "sequence_id": "main", "width": 160, "height": 90, "fps": {"numerator": 10, "denominator": 1}},
        "idempotency_key": "create"})
    # Long-media control admission and independent quota ceiling, without a huge
    # fixture or media upload. Actual 16 GiB byte IO/performance is a separate test.
    large = {"project_id":"semantic-film","idempotency_key":"large-admission","name":"4k.mov", "size_bytes":str(16 * 1024**3), "content_hash":"a"*64}
    held = call("opencut_upload_create",large)
    assert held["size_bytes"] == large["size_bytes"]
    call("opencut_upload_create",{**large,"idempotency_key":"oversized","size_bytes":str(16*1024**3+1)},error="INVALID_ARGUMENT")
    call("opencut_upload_abort",{"upload_id":held["upload_id"]})
    operations = [{"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main",
        "track": {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                  "muted": True, "locked": False, "clips": [], "params": {}}}}]
    for index, color in enumerate(("red", "blue")):
        source = run / f"{color}.mp4"
        subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", f"color={color}:s=160x90:r=10:d=2",
                        "-c:v", "libx264", "-pix_fmt", "yuv420p", str(source)], check=True, timeout=30)
        operations.extend([
            {"type": "base.apply", "operation": {"type": "asset.bind", "asset": {"id": color, "name": color,
                "kind": "video", "uri": str(source), "duration": tick(20), "metadata": {}}}},
            {"type": "base.apply", "operation": {"type": "clip.insert", "sequence_id": "main", "track_id": "video",
                "clip": {"id": "clip-" + color, "asset_id": color, "timeline_range": span(index * 10, 10),
                         "source_range": span(5, 10), "params": {}, "channels": []}}}])
    # Independent PCM source: stereo avoids implicit mono-to-stereo attenuation.
    # The first second is 330 Hz, the selected second is 660 Hz, peak 0.4.
    tone = run / "known-tone.wav"
    samples = array.array("h")
    for sample in range(96000):
        frequency = 330 if sample < 48000 else 660
        value = round(32767 * .4 * math.sin(2 * math.pi * frequency * sample / 48000))
        samples.extend((value, value))
    if sys.byteorder != "little":
        samples.byteswap()
    with wave.open(str(tone), "wb") as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(48000)
        wav.writeframes(samples.tobytes())
    operations.append({"type": "base.apply", "operation": {"type": "asset.bind", "asset": {
        "id": "tone", "name": "Known PCM", "kind": "audio", "uri": str(tone),
        "duration": tick(20), "metadata": {}}}})
    for name, enabled, muted in (("sound", True, False), ("muted", True, True), ("disabled", False, False)):
        active = name == "sound"
        operations.extend([
            {"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main", "track": {
                "id": name, "name": name, "kind": "audio", "enabled": enabled, "muted": muted,
                "locked": False, "clips": [], "params": {"volume": .5 if active else 1.0}}}},
            {"type": "base.apply", "operation": {"type": "clip.insert", "sequence_id": "main", "track_id": name,
                "clip": {"id": "clip-" + name, "asset_id": "tone", "timeline_range": span(5 if active else 0, 10),
                    "source_range": span(10 if active else 0, 10), "params": {"volume": .5 if active else 1.0}, "channels": []}}}])
    operations.append({"type": "transition.put", "sequence_id": "main", "transition": {
        "id": "crossfade", "outgoing_clip_id": "clip-red", "incoming_clip_id": "clip-blue",
        "duration": tick(6), "transition": {"kind": "crossfade"}}})
    edited = call("opencut_edit_v3_apply", {"project_id": "semantic-film", "expected_revision": initial["revision"],
        "idempotency_key": "transition", "label": "Independent transition oracle", "operations": operations})
    target = {"project_id": "semantic-film", "revision": edited["revision"], "sequence_id": "main"}
    for position in (6, 10, 14):
        job("opencut_job_v3_frame", {**target, "idempotency_key": f"frame-{position}",
            "requested_time": tick(position), "alignment": "exact"})
    job("opencut_job_v3_render", {**target, "idempotency_key": "render", "container": "mp4", "video_codec": "h264"})
    job("opencut_job_v3_range", {**target, "idempotency_key": "range", "requested_range": span(5, 10),
                                 "alignment": "exact"})
    # A separate revision changes incoming opacity. At sequence 1.3, local time is
    # 0.3, so blue is 255 * (0.25 + 0.3 * 0.5) ~= 102, not ~138 from shifted handles.
    animated = call("opencut_edit_v3_apply", {"project_id": "semantic-film", "expected_revision": edited["revision"],
        "idempotency_key": "animate", "label": "Clip-local clock oracle", "operations": [
            {"type": "base.apply", "operation": {"type": "keyframe.channel_set", "sequence_id": "main",
                "track_id": "video", "clip_id": "clip-blue", "channel": {"parameter": "opacity", "keyframes": [
                    {"id": "fade-start", "time": tick(0), "value": 0.25, "interpolation": "linear"},
                    {"id": "fade-end", "time": tick(10), "value": 0.75, "interpolation": "linear"}]}}},
        ]})
    job("opencut_job_v3_frame", {**target, "revision": animated["revision"], "idempotency_key": "animation-clock",
        "requested_time": tick(13), "alignment": "exact"})

    package = call("opencut_project_v3_package_export", {"project_id": "semantic-film", "revision": edited["revision"],
        "destination": str(run / "package.opencut"), "mode": "self_contained"})
    assert "LEGACY_REPRODUCIBILITY_CLAIM_UNVERIFIED" in trace[-1]["response"]["warnings"]
    package_root = Path(package["package_path"])
    before = {str(p.relative_to(package_root)): sha(p.read_bytes()) for p in package_root.rglob('*') if p.is_file()}
    probe_input = {"package_path": str(package_root), "expected_manifest_sha256": package["manifest_sha256"]}
    report = call("opencut_project_package_reproducibility_v1", probe_input)
    assert report["legacy_fully_reproducible_claim"] is True, report
    assert report["strictly_reproducible"] is False and report["execution_environment_locked"] == "not_verified", report
    assert report["native_tool_environment"] == "verified", report
    assert report["current_execution"]["process_binary_sha256"] == sha(binary.read_bytes()), report
    assert report["media_contents_verified"] == report["semantic_output_verified"] == report["bit_exact_verified"] == "not_verified"
    call("opencut_project_package_reproducibility_v1", {**probe_input, "expected_manifest_sha256": "0" * 64}, "PACKAGE_INVALID")
    after = {str(p.relative_to(package_root)): sha(p.read_bytes()) for p in package_root.rglob('*') if p.is_file()}
    assert before == after, "Read-only inspection changed the package"
    assert call("opencut_project_v3_inspect", {"project_id": "semantic-film", "revision": None})["revision"] == animated["revision"]

    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    token = run / "owner.token"
    with (run / "serve.log").open("wb") as log:
        host = subprocess.Popen([str(binary), "--database", str(database), "serve", "--listen", f"127.0.0.1:{port}",
                                 "--token-file", str(token)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while True:
                assert host.poll() is None and time.monotonic() < deadline
                if token.exists():
                    try:
                        with socket.create_connection(("127.0.0.1", port), timeout=.2):
                            break
                    except OSError:
                        pass
                time.sleep(.05)
            for index, done in enumerate(jobs):
                artifact = done["artifacts"][0]
                request = urllib.request.Request(f"http://127.0.0.1:{port}/v1/jobs/{done['job_id']}/artifact",
                    headers={"Authorization": "Bearer " + token.read_text().strip()})
                with urllib.request.urlopen(request, timeout=30) as response:
                    data = response.read()
                assert sha(data) == artifact["content_hash"]
                output = run / f"result-{index}.{'png' if artifact['mime_type'] == 'image/png' else 'mp4'}"
                output.write_bytes(data)
                at = "1" if index == 3 else "0.5" if index == 4 else "0"
                raw = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(output), "-ss", at,
                    "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
                assert len(raw) == 160 * 90 * 3
                offset = (45 * 160 + 80) * 3
                rgb = list(raw[offset:offset+3])
                if index == 0:
                    assert rgb[0] > 230 and rgb[2] < 15, rgb
                elif index == 2:
                    assert rgb[2] > 230 and rgb[0] < 15, rgb
                elif index == 5:
                    assert abs(rgb[2] - 102) <= 10, rgb
                else:
                    assert 95 <= rgb[0] <= 160 and 95 <= rgb[2] <= 160 and rgb[1] < 20, rgb
                pixels.append({"index": index, "rgb": rgb, "job_id": done["job_id"], "artifact_sha256": artifact["content_hash"]})
                if index in (3, 4):
                    decoded = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(output),
                        "-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "-"], timeout=30)
                    pcm = array.array("f")
                    pcm.frombytes(decoded)
                    if sys.byteorder != "little":
                        pcm.byteswap()
                    channels = [pcm[0::2], pcm[1::2]]
                    expected_duration = 2 if index == 3 else 1
                    assert abs(len(channels[0]) / 48000 - expected_duration) < .03
                    at = .7 if index == 3 else .2
                    metrics = []
                    for channel in channels:
                        segment = channel[round(at*48000):round((at+.2)*48000)]
                        rms = math.sqrt(sum(v*v for v in segment) / len(segment))
                        # Hand-computed RMS: source .4 * track .5 * clip .5 / sqrt(2).
                        assert abs(rms - .1/math.sqrt(2)) < .006, (index, "gain", rms)
                        def amplitude(hz):
                            real = sum(v*math.cos(2*math.pi*hz*n/48000) for n,v in enumerate(segment))
                            imag = sum(v*math.sin(2*math.pi*hz*n/48000) for n,v in enumerate(segment))
                            return 2*math.hypot(real,imag)/len(segment)
                        selected, rejected = amplitude(660), amplitude(330)
                        assert abs(selected-.1) < .008 and rejected < .002, (index, selected, rejected)
                        quiet = []
                        if index == 3:
                            for start in (.1, 1.7):
                                window = channel[round(start*48000):round((start+.2)*48000)]
                                level = math.sqrt(sum(v*v for v in window)/len(window))
                                assert level < .001, (index, "timeline silence", start, level)
                                quiet.append(level)
                        metrics.append({"rms":rms,"selected_660hz_peak":selected,"excluded_330hz_peak":rejected,"silence_rms":quiet})
                    pcm_checks.append({"index":index,"job_id":done["job_id"],"artifact_sha256":artifact["content_hash"],
                        "expected_duration_seconds":expected_duration,"decoded_samples_per_channel":len(channels[0]),
                        "expected_peak":.1,"channels":metrics})

        finally:
            host.terminate()
            host.wait(timeout=15)
    receipt = {"status": "passed", "binary_sha256": sha(binary.read_bytes()), "transition_revision": edited["revision"],
        "animation_revision": animated["revision"], "independent_pixel_checks": pixels, "independent_pcm_checks": pcm_checks, "reproducibility_inspection": report,
        "package_inspection_read_only": True, "trace": trace}
    (run / "report.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2))
    return run / "report.json"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args()
    print(verify(args.binary.resolve(), args.artifacts_dir))
