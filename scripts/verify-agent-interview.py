#!/usr/bin/env python3
"""Retained local real-interview acceptance; no downloaded media enters Git.

Requires the NASA source/provenance and host-configured real Whisper model.
Original requests, successful jobs and reviewed evidence survive interruption.
"""
import argparse
import copy
import json
import os
import re
import selectors
import subprocess
from array import array
from pathlib import Path
from acceptance import Harness, at, seconds, sha


def prepare(args):
    h = Harness(args.binary, args.workspace, "agent-interview-v4")
    provenance = json.loads((h.workspace / "nasa-interview-provenance.json").read_text())
    source = Path(provenance["files"][0]["path"])
    assert sha(source.read_bytes()) == provenance["files"][0]["sha256"]
    project_id = "agent-real-interview-v4-acceptance"

    def create():
        original = h.inspect("agent-v4-integrated-acceptance")
        return h.request("copy", "opencut_project_copy", {
            "source_project_id": original["project"]["id"], "source_revision": original["revision"],
            "project_id": project_id, "name": "NASA real interview V4 acceptance",
            "idempotency_key": h.key("copy")})
    initial = h.stage("copy-existing-fonts-and-cas", create)
    sequence = copy.deepcopy(initial["project"]["sequences"][0])
    sequence_id = sequence["id"]

    def clear():
        main = copy.deepcopy(next(t for t in sequence["tracks"] if t["id"] == sequence["main_track_id"]))
        main["elements"] = []
        caption_track = {**copy.deepcopy(main), "id": "interview-captions", "kind": "caption", "name": "Reviewed captions"}
        sequence.update(tracks=[main, caption_track], captions=[], transitions=[])
        return h.apply("clear", {"project_id": project_id, "expected_revision": initial["revision"],
            "idempotency_key": h.key("clear"), "label": "Reuse fonts and CAS with empty interview sequence",
            "operations": [{"type": "sequence_put", "sequence": sequence},
                *[{"type": "bookmark", "operation": {"kind": "delete", "sequence_id": sequence_id,
                    "bookmark_id": b["id"]}} for b in sequence["bookmarks"]]]})
    cleared = h.stage("empty-interview-sequence", clear)
    media = h.stage("import-real-interview", lambda: h.import_media("interview", project_id, source))

    def place():
        plan = h.request("place-plan", "opencut_media_import_plan", {
            "project_id": project_id, "expected_revision": cleared["revision"], "sequence_id": sequence_id,
            "idempotency_key": h.key("place"), "label": "Place actual NASA interview including its original audio",
            "ripple": False, "bindings": [{"media_id": media["media_id"], "version": media["asset"]["version"],
                "asset_id": "nasa-interview", "name": source.name, "mode": "bind"}], "new_tracks": [],
            "placements": [{"element_id": "interview-video", "name": "Bryan Fafaul - NASA/GSFC", "track_id": sequence["main_track_id"],
                "asset_id": "nasa-interview", "start": at(0), "content": {"kind": "video", "source_audio_enabled": True,
                    "timing": {"kind": "source_range", "range": {"start": at(0), "duration": media["asset"]["duration"]},
                        "rate": {"numerator": 1, "denominator": 1}, "maintain_pitch": False}}}]})
        return h.apply("place", plan["transaction"])
    placed = h.stage("place-source-video-and-audio", place)

    def settings():
        settings = copy.deepcopy(h.inspect(project_id, placed["revision"])["project"]["settings"])
        settings["settings"].update(fps={"numerator": 30, "denominator": 1}, canvas_size={"width": 640, "height": 360},
            canvas_size_mode="custom", last_custom_canvas_size={"width": 640, "height": 360})
        return h.apply("settings", {"project_id": project_id, "expected_revision": placed["revision"],
            "idempotency_key": h.key("settings"), "label": "Fixed 640x360 30fps review and export",
            "operations": [{"type": "settings_set", "settings": settings}]})
    configured = h.stage("fixed-review-grid", settings)
    asset_scope = {"project_id": project_id, "revision": configured["revision"], "asset_id": "nasa-interview"}
    analysis = {}
    model_hash = sha(Path(os.environ["OPENCUT_WHISPER_MODEL"]).read_bytes())
    for name, options in [("silence", {"kind": "silence", "noise_db": -40, "min_duration_ms": 250}),
                          ("transcribe", {"kind": "transcribe", "language": "en", "word_timestamps": True})]:
        attempt = name + ("-model-" + model_hash[:12] if name == "transcribe" else "")
        analysis[name] = h.stage("source-" + attempt, lambda attempt=attempt, options=options: h.job(attempt,
            "opencut_analysis_asset", {**asset_scope, "idempotency_key": h.key(attempt), "options": options}))
        result = h.result(analysis[name], "analysis.asset")
        assert result["asset_version"]["content_hash"] == provenance["files"][0]["sha256"]
        if name == "transcribe":
            assert result["data"]["result"]["model_sha256"] == model_hash
    evidence = h.stage("source-silence-evidence", lambda: h.call("opencut_analysis_evidence_query", {
        "job_id": analysis["silence"]["job_id"], "text": None, "source_range": None, "cursor": None, "limit": 100}))
    def reference_check():
        reference_html = Path(provenance["files"][1]["path"]).read_text()
        reference = re.sub(r"<[^>]+>", " ", reference_html).split("[Bryan Fafaul]", 1)[1]
        actual = " ".join(s["text"] for s in h.result(analysis["transcribe"], "analysis.asset")["data"]["result"]["segments"])
        tokens = lambda text: re.findall(r"[a-z0-9]+", text.lower())
        expected, observed = tokens(reference), tokens(actual)
        row = list(range(len(observed) + 1))
        for i, a in enumerate(expected, 1):
            next_row = [i]
            for j, b in enumerate(observed, 1):
                next_row.append(min(next_row[-1] + 1, row[j] + 1, row[j - 1] + (a != b)))
            row = next_row
        return {"reference_words": len(expected), "observed_words": len(observed), "word_edit_distance": row[-1],
            "normalized_wer": row[-1] / len(expected), "transcript": actual,
            "reference_sha256": provenance["files"][1]["sha256"], "model_sha256": model_hash}
    reference = h.stage("official-reference-comparison-" + model_hash[:12], reference_check)

    def rough_cut():
        items = evidence["evidence"]
        selected = [e for e in items if seconds(e["source_range"]["duration"]) >= .8]
        # Adjacent detector intervals are the same continuous pause. Preserve
        # shorter interior breathing pauses and every spoken answer.
        selected += [e for e in items if e not in selected and any(
            seconds(e["source_range"]["start"]) == seconds(s["source_range"]["start"]) + seconds(s["source_range"]["duration"])
            for s in selected)]
        assert selected, "A real source pause must justify the cut"
        plan = h.request("rough-cut-plan", "opencut_edit_recipe_plan", {
            "project_id": project_id, "expected_revision": configured["revision"], "sequence_id": sequence_id,
            "idempotency_key": h.key("rough-cut"), "label": "Remove detected lead, answer-gap and tail silence; retain short pauses",
            "recipe": {"kind": "remove_evidence", "target": {"track_id": sequence["main_track_id"], "element_id": "interview-video"},
                "analysis_job_id": analysis["silence"]["job_id"], "evidence_ids": [e["evidence_id"] for e in selected], "ripple": True}})
        receipt = h.apply("rough-cut", plan["plan"]["transaction"])
        return {"receipt": receipt, "selected_source_evidence": selected}
    cut = h.stage("evidence-based-rough-cut", rough_cut)
    voice_scope = {"project_id": project_id, "revision": cut["receipt"]["revision"], "sequence_id": sequence_id}
    transcript = h.stage("whole-edited-scene-transcription", lambda: h.job("scene-transcribe", "opencut_analysis_sequence_transcribe", {
        **voice_scope, "idempotency_key": h.key("scene-transcribe"), "language": "en"}))

    def captions():
        font = next(f for f in initial["project"]["fonts"] if f["family"] == "Lato")
        plan = h.request("captions-plan", "opencut_edit_recipe_plan", {
            "project_id": project_id, "expected_revision": voice_scope["revision"], "sequence_id": sequence_id,
            "idempotency_key": h.key("captions"), "label": "Captions from the fixed edited scene; proportional segment timing is estimated",
            "recipe": {"kind": "captions", "analysis_job_id": transcript["job_id"], "track_id": "interview-captions", "words_per_caption": 6,
                "style": {"font": {"kind": "locked", "font_id": font["id"]}, "font_size": {"unit": "canvas_pixels", "value": 28},
                    "color": {"encoding": "srgb", "red": 1, "green": 1, "blue": 1, "alpha": 1},
                    "background_color": {"encoding": "srgb", "red": 0, "green": 0, "blue": 0, "alpha": .65},
                    "max_width_px": 560, "line_height_px": 34, "text_align": "center", "layout": "pinned_caption_v4"}}})
        return h.apply("captions", plan["plan"]["transaction"])
    captioned = h.stage("generate-source-backed-captions", captions)

    def music():
        audio = next(a for a in initial["project"]["assets"] if a["kind"] == "audio")
        duration = seconds(h.result(transcript, "analysis.sequence_transcribe")["duration"])
        placements, cursor = [], 0
        while cursor < duration:
            length = min(seconds(audio["duration"]), duration - cursor)
            placements.append({"element_id": "interview-music-" + str(len(placements)), "name": "Synthetic low-level acceptance music bed",
                "track_id": "interview-music", "asset_id": audio["id"], "start": at(cursor), "content": {"kind": "audio",
                    "timing": {"kind": "pinned_timeline", "source_start": at(0), "duration": at(length),
                        "rate": {"numerator": 1, "denominator": 1}, "maintain_pitch": False}}})
            cursor += length
        plan = h.request("music-plan", "opencut_edit_recipe_plan", {
            "project_id": project_id, "expected_revision": captioned["revision"], "sequence_id": sequence_id,
            "idempotency_key": h.key("music"), "label": "Reuse synthetic low-level local music fixture",
            "recipe": {"kind": "place_media", "bindings": [], "placements": placements,
                "new_tracks": [{"id": "interview-music", "name": "Synthetic acceptance music", "kind": "audio"}]}})
        return h.apply("music", plan["plan"]["transaction"])
    placed_music = h.stage("place-reused-music", music)
    def levels():
        snapshot = h.inspect(project_id, placed_music["revision"])
        track = next(t for t in snapshot["project"]["sequences"][0]["tracks"] if t["id"] == "interview-music")
        operations = []
        for e in track["elements"]:
            e["content"]["audio"]["gain"] = {"unit": "decibels", "value": -30}
            operations.append({"type": "element_put", "sequence_id": sequence_id, "track_id": track["id"], "element": e})
        return h.apply("music-level", {"project_id": project_id, "expected_revision": placed_music["revision"],
            "idempotency_key": h.key("music-level"), "label": "Low-level synthetic bed", "operations": operations})
    quiet = h.stage("music-baseline-level", levels)
    def duck():
        scene = h.inspect(project_id, quiet["revision"])["project"]["sequences"][0]
        track = next(t for t in scene["tracks"] if t["id"] == "interview-music")
        segments = h.result(transcript, "analysis.sequence_transcribe")["transcription"]["segments"]
        plan = h.request("duck-plan", "opencut_edit_recipe_plan", {"project_id": project_id,
            "expected_revision": quiet["revision"], "sequence_id": sequence_id, "idempotency_key": h.key("duck"),
            "label": "Duck synthetic bed under the actual edited speech",
            "recipe": {"kind": "duck_music", "targets": [{"track_id": track["id"], "element_id": e["id"]} for e in track["elements"]],
                "speech_ranges": [{"start": s["start"], "duration": at(seconds(s["end"]) - seconds(s["start"]))} for s in segments],
                "duck_decibels": -42, "ramp": at(.12), "replace_gain_automation": False}})
        return h.apply("duck", plan["plan"]["transaction"])
    ducked = h.stage("duck-music-under-real-speech", duck)
    summary = {"phase": "ready_for_ui_review", "binary_sha256": h.candidate, "project_id": project_id,
        "revision": ducked["revision"], "sequence_id": sequence_id, "provenance": provenance, "source_reference_comparison": reference,
        "source_analysis_jobs": {k: v["job_id"] for k, v in analysis.items()}, "scene_transcription_job_id": transcript["job_id"],
        "caption_transaction_id": captioned["transaction_id"],
        "limitations": ["A real 32-second English NASA interview; no long-interview or multi-speaker claim",
            "The reused music bed is a synthetic tone acceptance fixture", "Generated captions use proportional segment timing estimates"]}
    (h.workspace / "agent-interview-ready.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


def finish(args):
    h = Harness(args.binary, args.workspace, "agent-interview-v4")
    assert args.native_runtime, "Capture the audio/visual module identities before native acceptance"
    h.runtime = json.loads(args.native_runtime.read_text())
    episode = h.candidate[:12] + "-" + sha(json.dumps(h.runtime, sort_keys=True).encode())[:12]
    ready = json.loads((h.workspace / "agent-interview-ready.json").read_text())
    browser = json.loads((h.workspace / "agent-interview-browser.json").read_text())
    assert browser["status"] == "passed_scoped"
    if browser["candidate"]["binary_sha256"] != h.candidate:
        assert args.source_equivalence, "A changed native candidate requires recorded source equivalence"
        proof = json.loads(args.source_equivalence.read_text())
        prior = json.loads(Path(proof["previous_candidate"]).read_text())
        assert proof["status"] in ("identical", "scoped_equivalent") and prior["binary_sha256"] == browser["candidate"]["binary_sha256"]
    project_id, sequence_id = ready["project_id"], ready["sequence_id"]
    original_frame = h.state["requests"].get("reviewed-caption-frame", {}).get("input", {}).get("request", {})
    scope = h.stage("freeze-reviewed-output-scope", lambda: {"project_id": project_id,
        "revision": original_frame.get("revision", browser["revision"]), "sequence_id": sequence_id})
    def reviewed_reference():
        snapshot = h.inspect(project_id, scope["revision"])
        actual = " ".join(c["text"] for c in snapshot["project"]["sequences"][0]["captions"])
        reference = re.sub(r"<[^>]+>", " ", (h.workspace / "nasa-bryan-fafaul-reference-transcript.html").read_text()).split("[Bryan Fafaul]", 1)[1]
        tokens = lambda text: re.findall(r"[a-z0-9]+", text.lower())
        assert tokens(actual) == tokens(reference), {"captions": actual, "official_reference": reference}
        return {"normalized_words": len(tokens(reference)), "word_edit_distance": 0, "revision": scope["revision"]}
    reviewed_text = h.stage("reviewed-captions-match-official-reference", reviewed_reference)
    def bridge_pcm():
        baseline = json.loads(Path("/private/tmp/opencut-integrated-20261017/agent-v4.json").read_text())
        original = h.inspect(baseline["project_id"], baseline["revision"])
        name = "bridge-pcm-" + episode
        job = h.job(name, "opencut_render_submit", {"kind": "audio", "request": {
            "project_id": baseline["project_id"], "revision": baseline["revision"], "sequence_id": original["project"]["sequences"][0]["id"],
            "idempotency_key": h.key(name), "sample_rate": 48000, "range": {"start_sample": "48000", "frame_count": "24000"}}})
        actual = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(h.artifact(job)), "-ar", "48000", "-ac", "2", "-f", "f32le", "pipe:1"])
        previous = h.call("opencut_job_view", {"job_id": baseline["range_job_id"]})
        old = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(h.artifact(previous).parent / "video-audio.wav"), "-ar", "48000", "-ac", "2", "-f", "f32le", "pipe:1"])
        assert actual == old and len(actual) == 24000 * 2 * 4, "The bridge must not alter previous Float32 output"
        source = array("f"); source.frombytes(subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(h.workspace / "sample.wav"),
            "-ss", "1", "-t", "0.5", "-ar", "48000", "-ac", "1", "-f", "f32le", "pipe:1"]))
        observed = array("f"); observed.frombytes(actual)
        error = max(abs(a - b) for a, b in zip(observed, (sample for value in source for sample in (value, value))))
        assert error < .00001
        return {"job_id": job["job_id"], "previous_job_id": previous["job_id"], "bit_exact_to_previous_pcm": True,
            "pcm_frames": 24000, "independent_source_max_absolute_error": error}
    reusable_pcm_candidates = {h.candidate}
    if args.source_equivalence:
        reusable_pcm_candidates.update(json.loads(args.source_equivalence.read_text()).get("reusable_audio_pcm_candidates", []))
    prior_pcm = next((record for name, record in h.state["stages"].items()
        if name.startswith("bridge-actual-pcm-regression-") and record["candidate"] in reusable_pcm_candidates
        and record.get("runtime", {}).get("audio_module_sha256") == h.runtime["audio_module_sha256"]), None)
    if prior_pcm:
        print("Reuse bridge PCM for unchanged audio module", flush=True)
        pcm_regression = {**prior_pcm["result"], "producer_binary_sha256": prior_pcm["candidate"]}
    else:
        pcm_regression = {**h.stage("bridge-actual-pcm-regression-" + episode, bridge_pcm),
            "producer_binary_sha256": h.candidate}
    def caption_sample():
        snapshot = h.inspect(project_id, scope["revision"])
        caption = next(c for c in snapshot["project"]["sequences"][0]["captions"] if c["id"] == browser["corrected_caption_id"])
        # The exact same output frame grid is used for PNG/video comparison.
        frame = int((seconds(caption["timeline_range"]["start"]) + .1) * 30)
        name = "reviewed-caption-frame-" + episode
        job = h.job(name, "opencut_render_submit", {"kind": "frame", "request": {
            **scope, "idempotency_key": h.key(name), "time": at(str(frame) + "/30")}})
        assert h.result(job, "render.frame")["execution"]["module_sha256"] == h.runtime["visual_module_sha256"]
        return {"job": job, "frame": frame, "caption": caption}
    sample_stage = "reviewed-fixed-revision-caption-frame-" + episode
    sample = h.stage(sample_stage, caption_sample)
    attempt = "full-reviewed-export-" + episode
    export_request = {"kind": "video", "request": {**scope, "idempotency_key": h.key(attempt),
        "options": {"format": "mp4", "quality": "high", "include_audio": True}}}
    def accept_disconnect():
        child = subprocess.Popen([str(h.binary), "--database", str(h.workspace / "workspace.sqlite"), "mcp", "serve"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        selector = selectors.DefaultSelector(); selector.register(child.stdout, selectors.EVENT_READ)
        serial, buffer = 0, b""
        def rpc(method, params):
            nonlocal serial, buffer
            serial += 1
            child.stdin.write((json.dumps({"jsonrpc": "2.0", "id": serial, "method": method, "params": params}) + "\n").encode()); child.stdin.flush()
            while True:
                while b"\n" not in buffer:
                    assert selector.select(30), "MCP timeout"
                    chunk = os.read(child.stdout.fileno(), 65536); assert chunk
                    buffer += chunk
                line, _, buffer = buffer.partition(b"\n"); message = json.loads(line)
                if message.get("id") == serial:
                    assert "error" not in message, message
                    return message["result"]
        try:
            rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "real-interview-export", "version": "1"}})
            child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); child.stdin.flush()
            accepted = rpc("tools/call", {"name": "opencut_render", "arguments": {"action": "submit", "input": export_request}})["structuredContent"]
            assert accepted["error"] is None, accepted
            job_id = accepted["data"]["job_id"]
        finally:
            selector.close(); child.terminate(); child.wait(timeout=10)
        observed = h.call("opencut_job_view", {"job_id": job_id})
        assert observed["state"] == "queued", observed
        return {"job_id": job_id, "after_mcp_disconnect": observed["state"]}
    accepted = h.stage("mcp-accept-disconnect-cli-query-" + episode, accept_disconnect)
    def during_running(view):
        def edit():
            edit_name = "edit-during-export-" + episode
            saved = h.state["requests"].get(edit_name)
            current = h.inspect(project_id, saved["input"]["expected_revision"] if saved else None)
            caption = copy.deepcopy(next(c for c in current["project"]["sequences"][0]["captions"] if c["id"] == sample["caption"]["id"]))
            before_color = copy.deepcopy(caption["style"]["color"])
            cyan = (before_color["red"], before_color["green"], before_color["blue"]) == (0, 1, 1)
            caption["style"]["color"].update(red=1 if cyan else 0, green=0 if cyan else 1, blue=1)
            assert caption["style"]["color"] != before_color
            request = {"project_id": project_id, "expected_revision": current["revision"],
                "idempotency_key": h.key(edit_name), "label": "Emphasize caption while the prior reviewed output is running",
                "operations": [{"type": "caption_put", "sequence_id": sequence_id, "caption": caption}]}
            h.request(edit_name + "-plan", "opencut_edit_plan", request)
            receipt = h.apply(edit_name, request)
            return {"observed_job_id": view["job_id"], "observed_job_state": view["state"], "receipt": receipt,
                "changed_caption_id": caption["id"], "before_color": before_color, "after_color": caption["style"]["color"]}
        h.stage("edit-while-fixed-export-running-" + episode, edit)
    output = h.stage("complete-reviewed-interview-export-" + episode, lambda: h.job(attempt, "opencut_render_submit",
        export_request, on_running=during_running))
    assert output["job_id"] == accepted["job_id"]
    public = h.result(output, "render.video")
    assert public["identity"]["revision"] == scope["revision"]
    assert h.state["stages"]["edit-while-fixed-export-running-" + episode]["result"]["observed_job_state"] == "running"
    assert public["execution"]["audio"]["module_sha256"] == h.runtime["audio_module_sha256"]
    assert public["execution"]["visual"]["module_sha256"] == h.runtime["visual_module_sha256"]
    assert h.inspect(project_id)["revision"] != scope["revision"]
    def pixels():
        image, video = h.artifact(sample["job"]), h.artifact(output)
        expected = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(image), "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        actual = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(video), "-vf", "select=eq(n\\," + str(sample["frame"]) + ")",
            "-frames:v", "1", "-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        assert len(expected) == len(actual) == 640 * 360 * 3
        mae = sum(abs(a - b) for a, b in zip(actual, expected)) / len(actual)
        assert mae < 5, {"png_mp4_mean_absolute_error": mae}
        return {"output_frame": sample["frame"], "png_mp4_mean_absolute_error": mae,
            "scope": "Same fixed reviewed revision PNG/encoded MP4 consistency; separate official transcript is the independent speech oracle"}
    comparison = h.stage("reviewed-png-fixed-export-consistency-" + output["job_id"], pixels)
    quality = h.stage("full-export-quality-and-caption-geometry-" + output["job_id"], lambda: h.call("opencut_quality_inspect", {
        "artifact_id": output["artifacts"][0]["artifact_id"], "caption_frame_job_ids": [sample["job"]["job_id"]]}))
    assert quality["measurements"]["video_frames_decoded"] == public["identity"]["frame_count"]
    assert quality["caption_samples"], "The quality probe must inspect the requested real caption sample"
    assert all(not s["outside_canvas"] and s["same_runtime_as_export"] for s in quality["caption_samples"])
    report = {"status": "exported_pending_reopen", "whole_migration_complete": False, "binary_sha256": h.candidate,
        "web_candidate": browser["candidate"], "project_id": project_id, "sequence_id": sequence_id,
        "export_revision": scope["revision"], "current_revision": h.inspect(project_id)["revision"], "job_id": output["job_id"],
        "artifact": output["artifacts"][0], "local_output": str(h.artifact(output)), "identity": public["identity"],
        "comparison": comparison, "quality": quality, "source_reference_comparison": ready["source_reference_comparison"],
        "bridge_pcm_regression": pcm_regression,
        "reviewed_caption_reference": reviewed_text,
        "frame_binary_sha256": h.state["stages"][sample_stage]["candidate"], "native_runtime": h.runtime,
        "source_equivalence": str(args.source_equivalence) if args.source_equivalence else None,
        "limits": ready["limitations"] + ["Sampled caption geometry and PNG/video consistency do not prove every caption pixel or semantic lip-sync",
            "Five-minute VFR source still exceeds the unchanged whole-source Classic audio PCM budget"]}
    (h.workspace / "agent-interview-export.json").write_text(json.dumps(report, indent=2) + "\n")
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in ["status", "export_revision", "current_revision", "job_id", "local_output", "identity"]}, indent=2))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--workspace", type=Path, required=True)
    p.add_argument("--finish", action="store_true", help="Reuse reviewed browser evidence, export and inspect")
    p.add_argument("--report", type=Path)
    p.add_argument("--source-equivalence", type=Path, help="Explicit same-source build-profile transition evidence")
    p.add_argument("--native-runtime", type=Path, help="Captured current audio and visual module SHA-256 identities")
    args = p.parse_args()
    finish(args) if args.finish else prepare(args)
