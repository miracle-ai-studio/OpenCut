#!/usr/bin/env python3
"""Exercise the V4 agent chain in an existing synthetic workspace.

Requires an explicit built CLI and an existing V4 fixture. Copies only the
project aggregate, reusing immutable CAS bytes. Stable requests/jobs survive a
verifier interruption; this is scoped acceptance, not whole-product parity.
"""
import argparse
import copy
import hashlib
import json
import os
import selectors
import subprocess
from array import array
from fractions import Fraction
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def at(seconds):
    value = Fraction(str(seconds))
    return {"ticks": str(value.numerator), "timebase": value.denominator}


def seconds(value):
    return Fraction(int(value["ticks"]), value["timebase"])


def verify(args):
    binary = args.binary.resolve()
    workspace = args.workspace.resolve()
    assert (workspace / "workspace.sqlite").is_file(), "Reuse an existing synthetic workspace"
    candidate = sha(binary.read_bytes())
    checkpoint = workspace / "agent-v4-checkpoint.json"
    state = json.loads(checkpoint.read_text()) if checkpoint.exists() else {}
    prior = json.loads(args.report.read_text()) if args.report.exists() else None
    if prior and "native_candidate_sha256" not in state:
        state["native_candidate_sha256"] = prior["binary_sha256"]
        checkpoint.write_text(json.dumps(state, indent=2) + "\n")
    checks = []

    def save():
        checkpoint.write_text(json.dumps(state, indent=2) + "\n")

    def cli(arguments, value=None, error=None, credential=None):
        result = subprocess.run(
            [str(binary), "--database", str(workspace / "workspace.sqlite"), "--json",
             *(["--credential-file", str(credential)] if credential else []), *arguments],
            input=json.dumps(value) if value is not None else None,
            text=True, capture_output=True, timeout=180,
        )
        if not result.stdout.strip() and arguments[:2] == ["worker", "run"]:
            assert result.returncode == 0, result.stderr
            return None
        out = json.loads(result.stdout)
        if error:
            assert result.returncode and out["error"]["code"] == error, out
        else:
            assert result.returncode == 0 and out.get("error") is None, (out, result.stderr)
        return out.get("data")

    def call(name, value, error=None):
        return cli(["call", name, "--input", "-"], value, error)

    def fixed():
        return inspect(state["project_id"])

    def inspect(project_id):
        view = call("opencut_project_inspect", {"project_id": project_id, "revision": None})
        assert view["kind"] == "editor", view
        return view["snapshot"]

    def job(request, tool="opencut_render_submit"):
        accepted = call(tool, request)
        job_id = accepted["job_id"]
        assert call(tool, request)["job_id"] == job_id, "Original request must replay one job"
        view = call("opencut_job_view", {"job_id": job_id})
        if view["state"] not in ["succeeded", "failed", "cancelled"]:
            cli(["worker", "run", "--once"])
            view = call("opencut_job_view", {"job_id": job_id})
        assert view["state"] == "succeeded", view
        assert view["result_unavailable"] is None and view["result"], view
        return view

    source = inspect(args.source_project)
    if not state:
        state.update(project_id="agent-v4-integrated-acceptance", source_revision=source["revision"],
                     source_project=args.source_project, request_prefix="agent-v4-integrated-v1")
        save()
    assert state["source_project"] == args.source_project
    prefix = state["request_prefix"]
    initial = call("opencut_project_copy", {
        "source_project_id": state["source_project"], "source_revision": state["source_revision"],
        "project_id": state["project_id"], "name": "V4 agent integrated acceptance",
        "idempotency_key": prefix + ":copy",
    })
    project = initial["project"]
    sequence = copy.deepcopy(project["sequences"][0])
    sequence["captions"], sequence["transitions"] = [], []
    main = copy.deepcopy(next(t for t in sequence["tracks"] if t["id"] == sequence["main_track_id"]))
    main["elements"] = []
    graphic_track = copy.deepcopy(next(t for t in sequence["tracks"] if t["kind"] == "graphic"))
    graphic = graphic_track["elements"][0]
    graphic_track["elements"] = [graphic]
    graphic["id"] = "agent-range-graphic"
    graphic["timeline_range"] = {"start": at(0), "duration": at(2)}
    transform = graphic["visual"]["transform"]
    transform.update(position_x_px=-60, position_y_px=0, scale_x=40 / 180, scale_y=40 / 180, rotation_degrees=0)
    graphic["visual"]["masks"] = []
    graphic["content"]["graphic"]["fill"].update(red=224 / 255, green=224 / 255, blue=224 / 255)
    graphic["channels"] = [{
        "kind": "scalar", "id": "agent-range-x", "target": {"kind": "element", "element_id": graphic["id"]},
        "binding": {"kind": "property", "parameter": "position_x_px"},
        "keys": [{"id": f"agent-range-x-{i}", "time": at(i * 2), "value": x,
                  "left_handle": None, "right_handle": None, "segment_to_next": {"kind": "linear"},
                  "tangent_mode": "broken"} for i, x in enumerate([-60, 60])],
        "extrapolation": {"before": "hold", "after": "hold"},
    }]
    audio_track = copy.deepcopy(next(t for t in sequence["tracks"] if t["kind"] == "audio"))
    audio = audio_track["elements"][0]
    audio_track["elements"] = [audio]
    audio["timeline_range"] = {"start": at(0), "duration": at(2)}
    audio["channels"] = []
    audio["content"]["audio"]["gain"] = {"unit": "decibels", "value": 0}
    audio["source_span"]["trim_end"] = at(6)
    sequence["tracks"] = [main, graphic_track, audio_track]
    settings = copy.deepcopy(project["settings"])
    settings["settings"].update(canvas_size={"width": 320, "height": 180}, fps={"numerator": 30, "denominator": 1},
        background={"kind": "color", "color": {"encoding": "srgb", "red": 0, "green": 0, "blue": 0, "alpha": 1}})
    original = {"project_id": project["id"], "expected_revision": initial["revision"],
        "idempotency_key": prefix + ":configure", "label": "Configure reused V4 evidence",
        "steps": [{"kind": "operations", "operations": [{"type": "settings_set", "settings": settings},
            {"type": "sequence_put", "sequence": sequence},
            *[{"type": "bookmark", "operation": {"kind": "delete", "sequence_id": sequence["id"],
                "bookmark_id": b["id"]}} for b in sequence["bookmarks"]]]}]}
    if "configured_revision" not in state:
        planned = call("opencut_edit_agent_plan", original)
        assert fixed()["revision"] == initial["revision"], "Planning must not publish"
        assert planned == call("opencut_edit_agent_plan", original), "Plan identity must be deterministic"
        receipt = call("opencut_edit_apply", planned["transaction"])
        assert receipt == call("opencut_edit_apply", planned["transaction"]), "Apply must replay exact receipt"
        state["configured_revision"] = receipt["revision"]
        save()
    revision = state["configured_revision"]
    scope = {"project_id": project["id"], "revision": revision, "sequence_id": sequence["id"]}
    checks.append("agent plan is deterministic, read-only and one replayable atomic apply")
    summary = call("opencut_project_summary", {"project_id": project["id"], "revision": revision, "sequence_offset": 0, "limit": 1})
    assert seconds(summary["sequences"][0]["authored_duration"]) == 2
    query = {**scope, "range": {"start": at(0), "duration": at(2)}, "track_ids": [],
             "include_annotations": True, "offset": 0, "limit": 1}
    first = call("opencut_project_timeline_query", query)
    second = call("opencut_project_timeline_query", {**query, "offset": first["next_offset"]})
    assert first["matching_items"] == 2 and first["items"] != second["items"]
    context = call("opencut_resource_read", {"uri": f"opencut://projects/{project['id']}/revisions/{revision}/sequences/{sequence['id']}/timeline"})
    assert context["payload"]["data"]["timeline"]["matching_items"] == 2
    checks.append("fixed summary, pagination and V4 resource context")
    target = {"track_id": graphic_track["id"], "element_id": graphic["id"]}
    recipe_request = {"project_id": project["id"], "expected_revision": revision, "sequence_id": sequence["id"],
                      "idempotency_key": prefix + ":recipe", "label": "Dry-run cut recipe",
                      "recipe": {"kind": "jump_cut", "targets": [target],
                                 "ranges": [{"start": at(0.5), "duration": at(0.25)}], "ripple": False}}
    recipe = call("opencut_edit_recipe_plan", recipe_request)
    assert recipe["plan"]["transaction"]["expected_revision"] == revision
    assert recipe == call("opencut_edit_recipe_plan", recipe_request)
    assert fixed()["revision"] == revision
    checks.append("multi-step cut recipe observes its private working graph without publishing")
    audio_target = {"track_id": audio_track["id"], "element_id": audio["id"]}
    duck = call("opencut_edit_recipe_plan", {**recipe_request, "idempotency_key": prefix + ":duck",
        "recipe": {"kind": "duck_music", "targets": [audio_target],
            "speech_ranges": [{"start": at(0.5), "duration": at(0.75)}],
            "duck_decibels": -12, "ramp": at(0.1), "replace_gain_automation": False}})
    ducked = next(o["element"] for o in duck["plan"]["transaction"]["operations"] if o["type"] == "element_put")
    assert ducked["source_span"] == audio["source_span"]
    values = [key["value"] for channel in ducked["channels"] for key in channel["keys"]]
    assert min(values) == -12 and values[0] == values[-1] == 0
    placed = call("opencut_edit_recipe_plan", {**recipe_request, "idempotency_key": prefix + ":broll",
        "recipe": {"kind": "place_media", "bindings": [], "new_tracks": [], "placements": [{
            "element_id": "agent-broll", "name": "Reuse existing video", "track_id": main["id"],
            "asset_id": next(a["id"] for a in project["assets"] if a["kind"] == "video"), "start": at(0),
            "content": {"kind": "video", "source_audio_enabled": False, "timing": {"kind": "pinned_timeline",
                "source_start": at(2), "duration": at(0.5), "rate": {"numerator": 1, "denominator": 1}, "maintain_pitch": False}},
        }]}})
    assert placed["affected_elements"] == ["agent-broll"] and placed["plan"]["transaction"]
    source_sequence = source["project"]["sequences"][0]
    static_track = next(t for t in source_sequence["tracks"] if t["kind"] == "graphic")
    static_element = static_track["elements"][0]
    source_request = {**recipe_request, "project_id": source["project"]["id"], "expected_revision": source["revision"],
        "sequence_id": source_sequence["id"], "idempotency_key": prefix + ":reframe"}
    reframed = call("opencut_edit_recipe_plan", {**source_request, "recipe": {"kind": "reframe",
        "canvas": {"width": 180, "height": 320}, "placements": [{
            "target": {"track_id": static_track["id"], "element_id": static_element["id"]},
            "samples": [{"local_time": at(t), "transform": {**static_element["visual"]["transform"], "position_x_px": x}}
                        for t, x in [(0, 0), (1, 10)]],
        }]}})
    assert any(o["type"] == "settings_set" for o in reframed["plan"]["transaction"]["operations"])
    assert fixed()["revision"] == revision
    call("opencut_edit_agent_plan", original, "REVISION_CONFLICT")
    checks.append("read-only ducking, existing-asset B-roll and explicit reframe recipes; stale agent head refusal")
    mapping = call("opencut_media_map_time", {**scope, "target": audio_target,
        "direction": "sequence_to_source", "times": [at(0.25), at(1.25)]})
    assert all(seconds(p["input"]) == seconds(p["mapped"]) == seconds(p["roundtrip"]) for p in mapping["points"])
    asset_id = audio["content"]["asset_id"]
    analysis = job({"project_id": project["id"], "revision": revision, "asset_id": asset_id,
        "idempotency_key": prefix + ":waveform", "options": {"kind": "waveform", "buckets": 32}}, "opencut_analysis_asset")
    assert analysis["result"]["kind"] == "analysis.asset"
    search = call("opencut_media_content_search", {"project_id": project["id"], "revision": revision,
        "analysis_job_ids": [analysis["job_id"]], "text": None, "source_range": None, "cursors": {}, "limit_per_analysis": 10})
    assert search["pages"][0]["asset_id"] == asset_id
    checks.append("analysis queue/worker/typed public result, content join and exact source clock mapping")
    cancelled = call("opencut_analysis_asset", {"project_id": project["id"], "revision": revision, "asset_id": asset_id,
        "idempotency_key": prefix + ":cancelled", "options": {"kind": "silence", "noise_db": -40, "min_duration_ms": 100}})
    call("opencut_job_request_cancel", {"job_id": cancelled["job_id"]})
    cancelled = call("opencut_job_view", {"job_id": cancelled["job_id"]})
    if cancelled["state"] != "cancelled":
        cli(["worker", "run", "--once"])
        cancelled = call("opencut_job_view", {"job_id": cancelled["job_id"]})
    assert cancelled["state"] == "cancelled" and cancelled["result"] is None
    checks.append("V4 analysis cancellation retires before provider execution")
    video = next(a for a in project["assets"] if a["kind"] == "video")
    evidence = call("opencut_source_art_evidence", {"project_id": project["id"], "revision": revision,
        "request_id": prefix + ":source", "frames": [{"asset_id": video["id"], "time": at(0.25)}],
        "waveforms": [{"asset_id": asset_id, "range": {"start": at(1), "duration": at(0.5)}, "bins": 64, "sample_rate": 48000}]})
    svg = bytes.fromhex(evidence["contact_sheet"]["bytes_hex"])
    assert sha(svg) == evidence["contact_sheet"]["content_hash"] and b"data:image/jpeg;base64," in svg
    assert seconds(evidence["receipts"][0]["requested_time"]) == Fraction(1, 4)
    (workspace / "agent-v4-contact-sheet.svg").write_bytes(svg)
    checks.append("actual non-1s source JPEG, waveform snippet and content-bound contact sheet")
    view = job({"kind": "video", "request": {**scope, "idempotency_key": prefix + ":complete-scene-range",
        "options": {"format": "mp4", "quality": "high", "include_audio": True},
        "range": {"start_frame": "30", "frame_count": "15"}}})
    assert view["result"]["kind"] == "render.video_range"
    result = view["result"]["data"]
    assert result["mapping"]["audio"] == {"start_sample": "48000", "frame_count": "24000"}
    assert seconds(result["mapping"]["source_start"]) == 1
    assert result["output"]["identity"]["frame_count"] == "15"
    artifact = view["artifacts"][0]
    access = call("opencut_artifact_get", {"artifact_id": artifact["artifact_id"]})
    assert access["artifact"]["content_hash"] == artifact["content_hash"]
    # Native paths are only resolved by this local verifier, never a public DTO.
    import sqlite3
    with sqlite3.connect(f"file:{workspace / 'workspace.sqlite'}?mode=ro", uri=True) as db:
        stored = json.loads(db.execute("SELECT result_json FROM jobs WHERE job_id=?", (view["job_id"],)).fetchone()[0])
    native = workspace / "artifacts" / stored["artifact"]["path"]
    source_audio = workspace / "sample.wav"
    audio_version = next(a["version"] for a in project["assets"] if a["id"] == asset_id)
    assert sha(source_audio.read_bytes()) == audio_version["content_hash"]
    def pcm(path, crop=False, channels=2):
        values = array("f")
        values.frombytes(subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(path),
            *(["-ss", "1", "-t", "0.5"] if crop else []), "-ar", "48000", "-ac", str(channels), "-f", "f32le", "pipe:1"]))
        return values
    # Web Audio speakers upmix duplicates mono to L/R; FFmpeg's implicit
    # mono->stereo matrix instead attenuates by sqrt(1/2), so do this explicitly.
    expected_pcm = [sample for sample in pcm(source_audio, True, 1) for _ in range(2)]
    actual_pcm = pcm(native.parent / "video-audio.wav")
    assert len(expected_pcm) == len(actual_pcm) == 48000
    pcm_error = max(abs(a - b) for a, b in zip(actual_pcm, expected_pcm))
    assert pcm_error < 0.00001, pcm_error
    raw = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(native), "-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
    assert len(raw) == 15 * 320 * 180 * 3
    worst = 0
    for index in range(15):
        center = 160 + 60 * (1 + index / 30) - 60
        for x, expected in [(round(center), 224), (round(center) - 30, 0), (round(center) + 30, 0)]:
            offset = (index * 320 * 180 + 90 * 320 + x) * 3
            worst = max(worst, *(abs(c - expected) for c in raw[offset:offset + 3]))
    assert worst <= 6, f"Independent original-clock rectangle probes: {worst}"
    credential = workspace / ".agent-v4-inspector.secret"
    if not credential.exists():
        cli(["access", "set-policy", "--input", "-"], {
            "principal_id": "agent-v4-inspector", "expected_revision": "0", "enabled": True,
            "grants": [{"project_id": project["id"], "actions": ["read", "run_jobs"]}],
        })
        cli(["access", "issue", "agent-v4-inspector", "--output", str(credential)])
    cli(["call", "opencut_quality_inspect", "--input", "-"],
        {"artifact_id": artifact["artifact_id"], "caption_frame_job_ids": []}, "NOT_AUTHORIZED", credential)
    quality = call("opencut_quality_inspect", {"artifact_id": artifact["artifact_id"], "caption_frame_job_ids": []})
    assert quality["measurements"]["video_frames_decoded"] == "15"
    assert quality["measurements"]["audio_blocks_analyzed"] is not None
    assert quality["measurements"]["integrated_lufs"] is not None
    assert not quality["technical_findings"], quality["technical_findings"]
    checks.append("range MP4 retains original animation clock and cropped audio sample identity; independent pixels and full quality decode")
    evidence_plan = call("opencut_quality_evidence_plan", {**scope, "request_key": prefix + ":review", "max_frames": 3})
    assert evidence_plan == call("opencut_quality_evidence_plan", {**scope, "request_key": prefix + ":review", "max_frames": 3})
    assert 0 < len(evidence_plan["frames"]) <= 3
    actors = call("opencut_project_revision_actors", {"project_id": project["id"], "revisions": [revision]})
    assert actors["actors"][revision] == {"principal_id": "local-owner", "source": "cli"}
    checks.append("stable sampled review requests and recorded revision actor")
    checks.append("quality inspection requires live ReadArtifacts in addition to Read and RunJobs")
    mcp = subprocess.Popen([str(binary), "--database", str(workspace / "workspace.sqlite"), "mcp", "serve"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    selector = selectors.DefaultSelector()
    selector.register(mcp.stdout, selectors.EVENT_READ)
    serial, buffer = 0, b""
    def rpc(method, params):
        nonlocal serial, buffer
        serial += 1
        mcp.stdin.write((json.dumps({"jsonrpc": "2.0", "id": serial, "method": method, "params": params}) + "\n").encode())
        mcp.stdin.flush()
        while True:
            while b"\n" not in buffer:
                assert selector.select(30), "MCP response timeout"
                chunk = os.read(mcp.stdout.fileno(), 65536)
                assert chunk, "MCP exited"
                buffer += chunk
            line, _, buffer = buffer.partition(b"\n")
            message = json.loads(line)
            if message.get("id") == serial:
                assert "error" not in message, message
                return message["result"]
    try:
        rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "agent-v4-qa", "version": "1"}})
        mcp.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        mcp.stdin.flush()
        tools = rpc("tools/list", {})["tools"]
        assert 0 < len(tools) <= 12
        templates = rpc("resources/templates/list", {})["resourceTemplates"]
        assert any("opencut://projects/" in t["uriTemplate"] for t in templates)
        resource = rpc("resources/read", {"uri": context["uri"]})
        assert json.loads(resource["contents"][0]["text"])["timeline"]["revision"] == revision
        prompt = rpc("prompts/get", {"name": "interview_rough_cut", "arguments": scope})
        assert "opencut_analysis_asset" in json.dumps(prompt)
    finally:
        selector.close()
        mcp.terminate()
        mcp.wait(timeout=10)
    checks.append("actual MCP compact discovery, V4 resource/template and current rough-cut prompt")
    report = {"status": "passed_scoped", "whole_migration_complete": False, "binary_sha256": candidate,
        "workspace": str(workspace), "project_id": project["id"], "revision": revision,
        "analysis_job_id": analysis["job_id"], "range_job_id": view["job_id"],
        "reused_native_candidate_sha256": state.get("native_candidate_sha256", candidate),
        "independent_range_pcm_max_absolute_error": pcm_error,
        "independent_range_pixels_max_channel_error": worst, "checks": checks,
        "limitations": ["Synthetic short source; no human ASR, physical audio or long-source claim",
            "No full caption pixel or aesthetic claim; prior unrelated media proofs were not rerun"]}
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--source-project", required=True, help="Existing synthetic V4 project with graphic/video/audio assets")
    parser.add_argument("--report", type=Path, required=True)
    verify(parser.parse_args())
