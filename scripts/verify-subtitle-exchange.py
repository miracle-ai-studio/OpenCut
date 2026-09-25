#!/usr/bin/env python3
"""Real CLI SRT/ASS caption import, revision persistence, export and video decode."""
import argparse
import hashlib
import json
import subprocess
import time
from pathlib import Path


def verify(binary: Path, artifacts: Path):
    run = artifacts.resolve() / f"subtitle-{time.time_ns()}"
    run.mkdir(parents=True)
    db = run / "workspace.sqlite"
    trace = []

    def command(args, payload=None, error=None):
        process = subprocess.run([str(binary), "--database", str(db), "--json", *args],
            input=json.dumps(payload, ensure_ascii=False) if payload is not None else None,
            text=True, capture_output=True, timeout=90)
        response = json.loads(process.stdout)
        trace.append({"command": args, "response": response})
        if error:
            assert process.returncode and response["error"]["code"] == error, (response, process.stderr)
        else:
            assert process.returncode == 0 and response["error"] is None, (response, process.stderr)
        return response.get("data")

    def call(name, body, error=None):
        return command(["call", name, "--input", "-"], body, error)

    def project(name):
        return call("opencut_project_create", {"project": {"id": name, "name": name,
            "sequence_id": "main", "width": 320, "height": 180,
            "fps": {"numerator": 10, "denominator": 1}}, "idempotency_key": f"create-{name}"})

    source = run / "red.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "color=red:s=320x180:r=10:d=2", "-c:v", "libx264", "-pix_fmt",
        "yuv420p", str(source)], check=True, timeout=30)
    start = project("captions")
    t = lambda n: {"ticks": str(n), "timebase": 10}
    span = {"start": t(0), "duration": t(20)}
    bound = call("opencut_edit_apply", {"project_id": "captions", "request": {
        "expected_revision": start["revision"], "idempotency_key": "bind",
        "label": "Bind video", "operations": [
            {"type": "asset.bind", "asset": {"id": "red", "name": "Red", "kind": "video",
                "uri": str(source), "duration": t(20), "metadata": {}}},
            {"type": "track.create", "sequence_id": "main", "track": {"id": "v",
                "name": "Video", "kind": "video", "enabled": True, "muted": False,
                "locked": False, "clips": [], "params": {}}},
            {"type": "clip.insert", "sequence_id": "main", "track_id": "v",
                "clip": {"id": "c", "asset_id": "red", "timeline_range": span,
                "source_range": span, "params": {}, "channels": []}},
        ]}})
    srt = "1\n00:00:00,200 --> 00:00:01,700\n字幕 QA\nSecond line\n"
    import_request = {"project_id": "captions", "expected_revision": bound["revision"],
        "sequence_id": "main", "idempotency_key": "import-srt", "format": "srt",
        "content": srt, "style": {"font_family": "PingFang SC", "font_size": 28,
            "color": "#ffffff", "background_color": None,
            "font_policy": {"mode": "explicit_fallback", "families": ["Arial"]}}}
    imported = call("opencut_subtitle_import", import_request)
    assert imported["cue_count"] == 1
    assert call("opencut_subtitle_import", import_request) == imported
    revision = imported["transaction"]["revision"]
    snapshot = call("opencut_project_inspect", {"project_id": "captions"})
    assert snapshot["revision"] == revision
    cue = snapshot["project"]["sequences"][0]["caption_cues"][0]
    assert cue["text"] == "字幕 QA\nSecond line"
    srt_export = call("opencut_subtitle_export", {"project_id": "captions",
        "revision": revision, "sequence_id": "main", "format": "srt"})
    assert "00:00:00,200 --> 00:00:01,700" in srt_export["content"]
    assert any("styles" in loss for loss in srt_export["losses"])
    ass_export = call("opencut_subtitle_export", {"project_id": "captions",
        "revision": revision, "sequence_id": "main", "format": "ass"})
    assert "PingFang SC" in ass_export["content"]
    assert any("fallback" in loss for loss in ass_export["losses"])
    second = project("from-ass")
    ass_import = call("opencut_subtitle_import", {"project_id": "from-ass",
        "expected_revision": second["revision"], "sequence_id": "main",
        "idempotency_key": "import-ass", "format": "ass", "content": ass_export["content"]})
    second_cue = call("opencut_project_inspect", {"project_id": "from-ass"})["project"]["sequences"][0]["caption_cues"][0]
    assert second_cue["text"] == cue["text"]
    assert second_cue["style"]["font_family"] == "PingFang SC"
    assert ass_import["cue_count"] == 1
    call("opencut_subtitle_import", {**import_request, "content": srt + "changed"},
        error="IDEMPOTENCY_KEY_REUSED")
    call("opencut_subtitle_import", {**import_request, "idempotency_key": "unsupported",
        "format": "ass", "content": ass_export["content"].replace("字幕 QA", "{\\pos(1,2)}字幕 QA")},
        error="SUBTITLE_INVALID")
    job = call("opencut_job_start", {"kind": "render.project",
        "idempotency_key": "render-subtitle", "input": {"project_id": "captions",
        "revision": revision, "sequence_id": "main", "container": "mp4",
        "video_codec": "h264"}})
    worker = subprocess.run([str(binary), "--database", str(db), "worker", "run", "--once"],
        capture_output=True, text=True, timeout=90)
    assert worker.returncode == 0, worker.stderr
    done = call("opencut_job_get", {"job_id": job["job_id"]})
    assert done["state"] == "succeeded", done
    artifact = done["result"]["artifact"]
    rendered = Path(artifact["path"])
    assert hashlib.sha256(rendered.read_bytes()).hexdigest() == artifact["content_hash"]
    pixels = subprocess.check_output(["ffmpeg", "-v", "error", "-ss", "0.5", "-i", str(rendered),
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
    white = sum(all(channel > 180 for channel in pixels[i:i+3]) for i in range(0,len(pixels),3))
    assert white > 20, white
    report = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "srt_sha256": hashlib.sha256(srt.encode()).hexdigest(), "revision": revision,
        "render_sha256": artifact["content_hash"], "white_caption_pixels": white,
        "checks": ["SRT import is atomic and idempotent", "SRT/ASS exports report losses",
            "ASS reimport preserves supported caption content/style", "ASS override rejection",
            "Rendered subtitle frame decodes with visible white glyph pixels"], "trace": trace}
    (run / "report.json").write_text(json.dumps(report,ensure_ascii=False,indent=2))
    return str(run / "report.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary",type=Path,required=True)
    parser.add_argument("--artifacts-dir",type=Path,required=True)
    args = parser.parse_args()
    print(verify(args.binary.resolve(),args.artifacts_dir))
