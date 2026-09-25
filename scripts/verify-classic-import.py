#!/usr/bin/env python3
"""Real Classic v31 media-mapped import and independent render acceptance."""
import argparse
import hashlib
import json
import math
import subprocess
import time
from array import array
from pathlib import Path


def verify(binary: Path, artifacts: Path):
    run = artifacts.resolve() / f"classic-{time.time_ns()}"
    run.mkdir(parents=True)
    db = run / "workspace.sqlite"
    trace = []

    def command(args, payload=None, error=None):
        process = subprocess.run([str(binary), "--database", str(db), "--json", *args],
            input=json.dumps(payload, ensure_ascii=False) if payload is not None else None,
            text=True, capture_output=True, timeout=90)
        result = json.loads(process.stdout)
        trace.append({"command": args, "response": result})
        if error:
            assert process.returncode and result["error"]["code"] == error, (result, process.stderr)
        else:
            assert process.returncode == 0 and result["error"] is None, (result, process.stderr)
        return result.get("data")

    def call(name, payload, error=None):
        return command(["call", name, "--input", "-"], payload, error)

    video = run / "video.mp4"
    audio = run / "audio.wav"
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "color=red:s=320x180:r=10:d=2", "-c:v", "libx264", "-pix_fmt",
        "yuv420p", str(video)], check=True, timeout=30)
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "sine=frequency=800:sample_rate=48000:duration=2", str(audio)], check=True, timeout=30)
    origin = call("opencut_project_create", {"project": {"id": "library", "name": "Library",
        "sequence_id": "main", "width": 320, "height": 180,
        "fps": {"numerator": 10, "denominator": 1}}, "idempotency_key": "create-library"})
    source = call("opencut_edit_apply", {"project_id": "library", "request": {
        "expected_revision": origin["revision"], "idempotency_key": "pin-media",
        "label": "Pin synthetic assets", "operations": [
            {"type": "asset.bind", "asset": {"id": "video", "name": "Video",
                "kind": "video", "uri": str(video),
                "duration": {"ticks": "240000", "timebase": 120000}, "metadata": {}}},
            {"type": "asset.bind", "asset": {"id": "audio", "name": "Audio",
                "kind": "audio", "uri": str(audio),
                "duration": {"ticks": "240000", "timebase": 120000}, "metadata": {}}},
        ]}})
    media = [{"classic_media_id": "mv", "source_project_id": "library",
        "source_revision": source["revision"], "asset_id": "video"},
        {"classic_media_id": "ma", "source_project_id": "library",
        "source_revision": source["revision"], "asset_id": "audio"}]
    classic = {"version": 31, "metadata": {"id": "classic", "name": "Classic film"},
        "settings": {"fps": {"numerator": 10, "denominator": 1},
            "canvasSize": {"width": 320, "height": 180},
            "background": {"type": "color", "color": "#000000"}},
        "scenes": [{"id": "main", "name": "Main", "isMain": True, "bookmarks": [],
            "tracks": {"main": {"id": "video-track", "name": "Video", "type": "video",
                "muted": False, "hidden": False, "elements": [{"id": "video-clip",
                    "type": "video", "mediaId": "mv", "startTime": 0,
                    "duration": 240000, "trimStart": 0, "trimEnd": 0,
                    "sourceDuration": 240000, "params": {"volume": 0}}]},
                "overlay": [{"id": "captions", "name": "Captions", "type": "text",
                    "hidden": False, "elements": [{"id": "caption", "type": "text",
                        "startTime": 24000, "duration": 180000,
                        "trimStart": 0, "trimEnd": 0,
                        "params": {"content": "Classic 字幕", "fontFamily": "PingFang SC",
                            "fontSize": 28, "color": "#ffffff", "textAlign": "center"}}]}],
                "audio": [{"id": "audio-track", "name": "Audio", "type": "audio",
                    "muted": False, "elements": [{"id": "audio-clip", "type": "audio",
                        "sourceType": "upload", "mediaId": "ma", "startTime": 0,
                        "duration": 240000, "trimStart": 0, "trimEnd": 0,
                        "sourceDuration": 240000, "params": {"volume": -6}}]}]}}]}
    video.unlink()
    audio.unlink()
    analyze_input = {"content": json.dumps(classic, ensure_ascii=False),
        "target_project_id": "converted", "media_mappings": media}
    analysis = call("opencut_classic_analyze", analyze_input)
    assert not analysis["blockers"] and analysis["proposed_project"], analysis
    assert len(analysis["proposed_project"]["assets"]) == 2
    assert analysis["proposed_project"]["sequences"][0]["caption_cues"][0]["text"] == "Classic 字幕"
    accepted = sorted({item["code"] for item in analysis["losses"]})
    request = {"source": analyze_input, "expected_source_content_id": analysis["source_content_id"],
        "idempotency_key": "import-classic", "accepted_loss_codes": accepted}
    call("opencut_classic_import", {**request, "accepted_loss_codes": []}, error="INVALID_ARGUMENT")
    imported = call("opencut_classic_import", request)
    assert call("opencut_classic_import", request) == imported
    assert call("opencut_project_inspect", {"project_id": "library"})["revision"] == source["revision"]
    assert call("opencut_project_inspect", {"project_id": "converted"})["revision"] == imported["revision"]
    damaged = json.loads(analyze_input["content"])
    damaged["scenes"][0]["tracks"]["main"]["elements"][0]["effects"] = [{"type": "blur"}]
    blocked = call("opencut_classic_analyze", {**analyze_input,
        "content": json.dumps(damaged), "target_project_id": "blocked"})
    assert blocked["proposed_project"] is None
    assert any(item["path"].endswith("effects") for item in blocked["blockers"])
    job = call("opencut_job_start", {"kind": "render.project",
        "idempotency_key": "render-classic", "input": {"project_id": "converted",
        "revision": imported["revision"], "sequence_id": "main",
        "container": "mp4", "video_codec": "h264"}})
    worker = subprocess.run([str(binary), "--database", str(db), "worker", "run", "--once"],
        capture_output=True, text=True, timeout=90)
    assert worker.returncode == 0, worker.stderr
    done = call("opencut_job_get", {"job_id": job["job_id"]})
    assert done["state"] == "succeeded", done
    artifact = done["result"]["artifact"]
    output = Path(artifact["path"])
    assert hashlib.sha256(output.read_bytes()).hexdigest() == artifact["content_hash"]
    rgb = subprocess.check_output(["ffmpeg", "-v", "error", "-ss", "0.5", "-i",
        str(output), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
    assert sum(rgb[0::3]) / (320 * 180) > 180
    white = sum(all(channel > 180 for channel in rgb[i:i+3]) for i in range(0,len(rgb),3))
    assert white > 20
    pcm = array("h")
    pcm.frombytes(subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(output),
        "-vn", "-ac", "1", "-ar", "48000", "-f", "s16le", "-"], timeout=30))
    rms = math.sqrt(sum(sample * sample for sample in pcm) / len(pcm))
    assert rms > 80, rms
    report = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "source_content_id": analysis["source_content_id"],
        "imported_revision": imported["revision"], "accepted_loss_codes": accepted,
        "render_sha256": artifact["content_hash"], "white_caption_pixels": white,
        "audio_rms": rms, "checks": ["Pinned existing media resolved after original files removed",
            "Nonempty video/audio/text conversion creates one atomic new project",
            "Explicit loss acknowledgement and exact replay", "Effects block with JSON path",
            "Rendered MP4 decodes visual caption and audio"], "trace": trace}
    (run / "report.json").write_text(json.dumps(report,ensure_ascii=False,indent=2))
    return str(run / "report.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary",type=Path,required=True)
    parser.add_argument("--artifacts-dir",type=Path,required=True)
    args = parser.parse_args()
    print(verify(args.binary.resolve(),args.artifacts_dir))
