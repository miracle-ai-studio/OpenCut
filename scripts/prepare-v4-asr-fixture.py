#!/usr/bin/env python3
"""Prepare known synthetic speech in an existing owned V4 acceptance workspace.

Uses public tools and the existing service/worker, never writes the database
directly. Speech is local macOS synthesis, not human-language accuracy evidence.
The original project is restored; historical sample revisions remain reusable.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import time
import uuid


SAMPLES = [
    ("en", "Samantha", "Welcome to the video editor. We cut the scene and add clear captions.", "Lato"),
    ("zh", "Tingting", "你好，欢迎使用视频编辑器。现在我们添加清晰的字幕，然后保存这个项目。", "Ma Shan Zheng"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--web-root", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    binary = args.binary.resolve()
    database = workspace / "workspace.sqlite"
    assert database.is_file(), "Only an existing owned workspace is accepted"
    fixture_path = workspace / "b3-asr-fixture.json"
    project_id = json.loads((workspace / "b1-text-project.json").read_text())["project_id"]

    def command(*arguments, payload=None, timeout=60):
        result = subprocess.run(
            [str(binary), "--json", "--database", str(database), *arguments],
            input=json.dumps(payload) if payload is not None else None,
            capture_output=True, text=True, timeout=timeout,
        )
        reply = json.loads(result.stdout)
        stopped = arguments[-1:] == ("status",) and reply.get("data", {}).get("state") == "stopped"
        assert (result.returncode == 0 or stopped) and reply.get("error") is None, (reply, result.stderr[-2000:])
        return reply["data"]

    def tool(name, value):
        return command("call", name, "--input", "-", payload=value)

    def inspect(revision=None):
        return tool("opencut_project_v4_inspect", {"project_id": project_id, "revision": revision})

    def restore(revision):
        current = inspect()
        if current["revision"] != revision:
            tool("opencut_project_v4_restore", {
                "project_id": project_id, "expected_revision": current["revision"],
                "target_revision": revision, "idempotency_key": str(uuid.uuid4()), "asset_versions": {},
            })

    def save(fixture):
        pending = fixture_path.with_suffix(".pending")
        pending.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + "\n")
        pending.replace(fixture_path)

    model_hash = hashlib.file_digest(args.model.open("rb"), "sha256").hexdigest()
    original = inspect()
    if fixture_path.exists():
        fixture = json.loads(fixture_path.read_text())
        assert fixture["project_id"] == project_id and fixture["model_sha256"] == model_hash
        original = inspect(fixture["original_revision"])
    else:
        fixture = {
            "schema_version": 1, "project_id": project_id,
            "sequence_id": original["project"]["default_sequence_id"],
            "original_revision": original["revision"], "model_sha256": model_hash,
            "scope": "Synthetic local speech; does not close human annotated language accuracy",
            "samples": [],
        }
        save(fixture)
    sequence_id = fixture["sequence_id"]
    service = ("server", "--directory", str(workspace / "service"))
    status = command(*service, "status")
    assert not status.get("ready"), "Fixture owns the stopped service; do not overlap browser acceptance"
    started = False
    try:
        state = command(*service, "start", "--listen", "127.0.0.1:0", "--web-root", str(args.web_root.resolve()))
        assert state["ready"]
        started = True
        for language, voice, text, family in SAMPLES:
            prior = next((s for s in fixture["samples"] if s["language"] == language), None)
            if prior and prior.get("revision"):
                inspect(prior["revision"])
                print(json.dumps({"language": language, "reused_revision": prior["revision"]}), flush=True)
                continue
            sample = prior or {"language": language, "expected_text": text, "speech_source": {"kind": "macos_say", "voice": voice, "rate": 150}}
            if not prior:
                fixture["samples"].append(sample)
                save(fixture)
            source = workspace / f"b3-asr-{language}.wav"
            if not source.exists():
                aiff = workspace / f"b3-asr-{language}.aiff"
                subprocess.run(["say", "-v", voice, "-r", "150", "-o", str(aiff), text], check=True, timeout=60)
                subprocess.run(["ffmpeg", "-v", "error", "-i", str(aiff), "-ar", "48000", "-ac", "2", "-c:a", "pcm_s16le", str(source)], check=True, timeout=30)
            source_hash = hashlib.file_digest(source.open("rb"), "sha256").hexdigest()
            sample["source_sha256"] = source_hash
            if "media_id" not in sample:
                if "upload_id" not in sample:
                    upload = tool("opencut_upload_stream_begin", {
                        "project_id": project_id, "idempotency_key": f"b3-asr-{language}-{source_hash}",
                        "name": source.name, "size_bytes": str(source.stat().st_size),
                    })
                    sample["upload_id"] = upload["upload_id"]
                    save(fixture)
                    command("upload", "stream-put", upload["upload_id"], "--offset", "0", "--input", str(source))
                job = tool("opencut_upload_stream_complete", {"upload_id": sample["upload_id"], "content_hash": source_hash})
                deadline = time.monotonic() + 90
                while job["state"] not in ("succeeded", "failed", "cancelled") and time.monotonic() < deadline:
                    time.sleep(.25)
                    job = tool("opencut_job_view", {"job_id": job["job_id"]})
                assert job["state"] == "succeeded", job
                sample["media_id"] = job["extended_result"]["data"]["media_id"]
                save(fixture)
                tool("opencut_upload_release", {"upload_id": sample["upload_id"]})
            asset = tool("opencut_project_media_inspect", {"project_id": project_id, "media_id": sample["media_id"]})["asset"]
            restore(fixture["original_revision"])
            current = inspect()
            scene = next(s for s in current["project"]["sequences"] if s["id"] == sequence_id)
            assert not scene["captions"], "ASR sample requires the existing caption-free baseline"
            font = next(f for f in current["project"]["fonts"] if f["family"] == family and f["weight"] == 400 and f["style"] == "normal")
            operations = []
            for track in scene["tracks"]:
                if track["kind"] in ("audio", "video"):
                    muted = copy.deepcopy(track)
                    muted["muted"] = True
                    operations.append({"type": "track_put", "sequence_id": sequence_id, "track": muted})
            asset_id = f"b3-asr-{language}-voice"
            zero = {"ticks": "0", "timebase": 1}
            duration = asset["duration"]
            caption_track = {"id": f"b3-asr-{language}-caption", "name": f"B3 {language} caption", "kind": "caption", "enabled": True,
                             "muted": False, "locked": False, "gain": {"unit": "decibels", "value": 0.0}, "opacity": 1.0, "channels": [], "elements": []}
            operations.append({"type": "track_put", "sequence_id": sequence_id, "track": caption_track})
            tool("opencut_edit_v4_apply", {"project_id": project_id, "expected_revision": current["revision"], "idempotency_key": str(uuid.uuid4()),
                 "label": f"Prepare {language} speech tracks", "operations": operations})
            # Reuse the product's exact source timing/rounding planner; fixtures
            # must not hand-author another interpretation of Classic trims.
            planned = tool("opencut_media_workspace_plan_v4", {
                "project_id": project_id, "expected_revision": inspect()["revision"], "sequence_id": sequence_id,
                "idempotency_key": str(uuid.uuid4()), "label": f"Place known {language} speech", "ripple": False,
                "new_tracks": [{"id": f"b3-asr-{language}-audio", "name": f"B3 {language} speech", "kind": "audio"}],
                "bindings": [{"media_id": sample["media_id"], "version": asset["version"], "asset_id": asset_id, "name": source.name, "mode": "bind"}],
                "placements": [{"element_id": f"b3-asr-{language}-element", "name": f"B3 {language} speech", "track_id": f"b3-asr-{language}-audio", "asset_id": asset_id,
                                "start": zero, "content": {"kind": "audio", "timing": {"kind": "source_range", "range": {"start": zero, "duration": duration},
                                "rate": {"numerator": 1, "denominator": 1}, "maintain_pitch": True}}}],
            })
            tool("opencut_edit_v4_apply", planned["transaction"])
            sample.update({"revision": inspect()["revision"], "track_id": caption_track["id"], "font_id": font["id"], "source_duration": duration})
            save(fixture)
            print(json.dumps({"language": language, "revision": sample["revision"], "source_duration": duration}), flush=True)
    finally:
        try:
            restore(fixture["original_revision"])
            assert inspect()["project"] == original["project"], "Original complete project must be restored"
        finally:
            if started:
                command(*service, "stop")
    print(json.dumps({"fixture": str(fixture_path), "samples": len(fixture["samples"])}))


if __name__ == "__main__":
    main()
