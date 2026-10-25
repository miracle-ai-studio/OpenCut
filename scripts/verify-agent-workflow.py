#!/usr/bin/env python3
"""One ordinary short-media journey over discovered domain actions.

Reuses the owned workspace, imported media and pinned font bytes. A single empty
project exercises current creation; no project aggregate or source file is copied.
Original requests, accepted Jobs and coarse stages survive interruptions.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import time
import urllib.request

from acceptance import Harness, at, sha


def load_mcp():
    spec = importlib.util.spec_from_file_location("workflow_mcp", Path(__file__).with_name("verify-editorial.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.Mcp


def verify(args):
    args.output.mkdir(parents=True, exist_ok=True)
    h = Harness(args.binary, args.workspace, "agent-domain-workflow")
    project, sequence = "agent-domain-workflow", "agent-domain-workflow-scene"
    candidate = h.candidate
    previous = h.state.setdefault("candidate", candidate)
    if previous != candidate:
        assert args.previous_candidate == previous and "accepted_video" not in h.state, "Retain prior candidate evidence before an explicit source transition"
        h.state.setdefault("candidate_transitions", []).append({"from": previous, "to": candidate,
            "retained_stages": list(h.state["stages"]), "reason": "Ordinary decoder idle-read protocol fix; completed stages keep their producer candidate"})
        h.state["candidate"] = candidate
        h.save()
    source_id = "fb92adbb-b999-414e-b351-35f012b8e7b8"
    cli_call = h.call
    mcp = load_mcp()(h.binary, h.workspace / "workspace.sqlite", args.output,
        [str(h.binary), "--database", str(h.workspace / "workspace.sqlite"), "mcp", "serve"])
    try:
        compact = mcp.request("tools/list", {})["tools"]
        exposed = {tool["name"] for tool in compact}
        assert {"opencut_project", "opencut_edit", "opencut_media", "opencut_analysis",
                "opencut_render", "opencut_catalog", "opencut_task"} <= exposed
        routes = {}
        for domain in sorted(exposed):
            name = domain.removeprefix("opencut_")
            result = mcp.request("resources/read", {"uri": f"opencut://schemas/domains/{name}"})
            envelope = json.loads(result["contents"][0]["text"])
            for group in envelope["payload"]["data"]:
                for action in group["actions"]:
                    assert action.get("input_schema") and action.get("output_schema")
                    assert action["operation"] not in routes
                    routes[action["operation"]] = (domain, action["action"])

        def call(operation, value, error=None, credential=None):
            assert credential is None
            domain, action = routes[operation]
            result = mcp.request("tools/call", {"name": domain,
                "arguments": {"action": action, "input": value}})["structuredContent"]
            if error:
                assert result["error"]["code"] == error, result
            else:
                assert result.get("error") is None, result
            return result.get("data")

        h.call = call
        if args.phase == "export":
            capability = call("opencut_render_capability", {})
            assert capability["video"]["available"] and capability["video_audio"], capability
            h.state["render_capability"] = capability
            h.save()

        def inspect(revision=None):
            return h.inspect(project, revision)

        def wait_job(accepted):
            deadline, last = time.monotonic() + 600, None
            while True:
                job = cli_call("opencut_job_view", {"job_id": accepted["job_id"]})
                progress = (job["state"], str(job.get("progress")))
                if progress != last:
                    print("Observe", job["job_id"], *progress, flush=True)
                    last = progress
                if job["state"] in ("succeeded", "failed", "cancelled"):
                    assert job["state"] == "succeeded" and job["result_unavailable"] is None, job
                    return job
                assert time.monotonic() < deadline, {"observation_timeout": job["job_id"], "state": job["state"]}
                time.sleep(1)

        def planned_apply(name, request):
            planned = h.request(name + "-plan", "opencut_edit_agent_plan", request)
            if name not in h.state["requests"]:
                assert inspect()["revision"] == request["expected_revision"]
            return h.apply(name, planned["transaction"])

        if args.phase == "prepare":
            source = h.stage("retained-source", lambda: h.inspect(source_id))
            h.stage("create", lambda: h.request("create", "opencut_project_create", {
                "project": {"id": project, "name": "Agent domain workflow draft", "sequence_id": sequence,
                    "width": 320, "height": 180, "fps": {"numerator": 30, "denominator": 1}},
                "idempotency_key": h.key("create")}))
            created = h.state["stages"]["create"]["result"]
            assert call("opencut_project_create", h.state["requests"]["create"]["input"]) == created

            def register_media():
                registered = {}
                for filename in ("sample.mp4", "sample.wav"):
                    original = next(asset for asset in source["project"]["assets"] if asset["name"] == filename)
                    file = h.workspace / filename
                    assert sha(file.read_bytes()) == original["version"]["content_hash"]
                    upload = h.request(filename + "-begin", "opencut_upload_begin", {"project_id": project,
                        "name": filename, "size_bytes": str(file.stat().st_size), "idempotency_key": h.key(filename + "-begin")})
                    observed = call("opencut_upload_inspect", {"upload_id": upload["upload_id"]})
                    if observed["state"] == "receiving":
                        offset = int(observed["next_offset"])
                        with file.open("rb") as stream:
                            stream.seek(offset)
                            while data := stream.read(256 * 1024):
                                result = subprocess.run([str(h.binary), "--json", "--database", str(h.workspace / "workspace.sqlite"),
                                    "upload", "put", upload["upload_id"], "--offset", str(offset), "--input", "-"],
                                    input=data, capture_output=True, timeout=60)
                                envelope = json.loads(result.stdout)
                                assert result.returncode == 0 and envelope.get("error") is None, envelope
                                offset += len(data)
                                assert int(envelope["data"]["next_offset"]) == offset
                    accepted = h.request(filename + "-complete", "opencut_upload_complete", {
                        "upload_id": upload["upload_id"], "content_hash": original["version"]["content_hash"]})
                    job = wait_job(accepted)
                    assert job["result"]["kind"] == "media.import", job
                    imported = call("opencut_project_media_inspect", {"project_id": project,
                        "media_id": job["result"]["data"]["media_id"]})
                    assert imported["asset"]["version"] == original["version"]
                    registered[filename] = imported
                return registered

            registered = h.stage("register-retained-source-files", register_media)
            h.stage("acquire-approved-font", lambda: h.request("font", "opencut_font_import", {
                "project_id": project, "expected_revision": created["revision"], "idempotency_key": h.key("font"),
                "catalog_id": "google-lato-regular-v1", "font_id": "workflow-font", "required_text": "Draft OpenCut"}))

            def configure():
                if "configure-public" in h.state["requests"]:
                    return h.apply("configure-public", h.state["requests"]["configure-public"]["input"])
                initial = inspect()
                library = call("opencut_project_media_query", {"project_id": project, "cursor": None, "limit": 100})
                media = library["media"]
                assets = {asset["name"]: asset for asset in source["project"]["assets"]}
                bindings = []
                for filename in ("sample.mp4", "sample.wav"):
                    original = assets[filename]
                    entry = registered[filename]
                    assert any(row["media_id"] == entry["media_id"] for row in media)
                    bindings.append({"media_id": entry["media_id"], "version": original["version"],
                        "asset_id": original["id"], "name": filename, "mode": "bind"})
                scene = initial["project"]["sequences"][0]
                timing = {"kind": "source_range", "range": {"start": at(0), "duration": at(3)},
                    "rate": {"numerator": 1, "denominator": 1}, "maintain_pitch": False}
                plan = call("opencut_media_import_plan", {"project_id": project,
                    "expected_revision": initial["revision"], "sequence_id": sequence,
                    "idempotency_key": h.key("media"), "label": "Place retained short media",
                    "bindings": bindings, "new_tracks": [{"id": "workflow-music", "name": "Music", "kind": "audio"}],
                    "ripple": False, "placements": [
                        {"element_id": "workflow-video", "name": "sample.mp4", "track_id": scene["main_track_id"],
                            "asset_id": assets["sample.mp4"]["id"], "start": at(0),
                            "content": {"kind": "video", "source_audio_enabled": False, "timing": timing}},
                        {"element_id": "workflow-music-element", "name": "sample.wav", "track_id": "workflow-music",
                            "asset_id": assets["sample.wav"]["id"], "start": at(0),
                            "content": {"kind": "audio", "timing": timing}},
                    ]})
                caption_track = copy.deepcopy(scene["tracks"][0])
                caption_track.update(id="workflow-captions", name="Captions", kind="caption", elements=[], channels=[])
                request = {"project_id": project, "expected_revision": initial["revision"],
                    "idempotency_key": h.key("configure-public"), "label": "Configure short Agent journey",
                    "steps": [{"kind": "operations", "operations": [
                        {"type": "track_put", "sequence_id": sequence, "track": caption_track},
                        *plan["transaction"]["operations"]]}]}
                return planned_apply("configure-public", request)

            configured = h.stage("bind-retained-media-and-font", configure)
            audio = next(asset for asset in inspect()["project"]["assets"] if asset["name"] == "sample.wav")

            def analyze():
                accepted = h.request("waveform", "opencut_analysis_asset", {"project_id": project,
                    "revision": configured["revision"], "asset_id": audio["id"], "idempotency_key": h.key("waveform"),
                    "options": {"kind": "waveform", "buckets": 32}})
                assert call("opencut_analysis_asset", h.state["requests"]["waveform"]["input"])["job_id"] == accepted["job_id"]
                job = wait_job(accepted)
                assert job["result"]["kind"] == "analysis.asset"
                assert job["result"]["data"]["asset_version"] == {
                    "asset_id": audio["id"], "content_hash": audio["version"]["content_hash"],
                    "size": audio["version"]["size"]}
                evidence = call("opencut_media_content_search", {"project_id": project, "revision": configured["revision"],
                    "analysis_job_ids": [job["job_id"]], "text": None, "source_range": None, "cursors": {}, "limit_per_analysis": 10})
                assert evidence["pages"][0]["asset_id"] == audio["id"]
                return {"job": job, "evidence": evidence}

            h.stage("analysis-and-evidence", analyze)

            def captions():
                font = inspect()["project"]["fonts"][0]
                return h.request("captions", "opencut_subtitle_import", {"project_id": project,
                    "expected_revision": configured["revision"], "sequence_id": sequence,
                    "track_id": "workflow-captions", "idempotency_key": h.key("captions"), "format": "srt",
                    "content": "1\n00:00:00,000 --> 00:00:03,000\nDraft\n",
                    "style": {"font": {"kind": "locked", "font_id": font["id"]},
                        "font_size": {"unit": "canvas_pixels", "value": 24},
                        "color": {"encoding": "srgb", "red": 1, "green": 1, "blue": 1, "alpha": 1},
                        "background_color": None, "max_width_px": None, "line_height_px": None,
                        "text_align": "center", "layout": "pinned_caption_v4"}})

            h.stage("import-captions", captions)

            def correct():
                if "correct" in h.state["requests"]:
                    return h.apply("correct", h.state["requests"]["correct"]["input"])
                current = inspect()
                cue = copy.deepcopy(current["project"]["sequences"][0]["captions"][0])
                cue["text"] = "OpenCut"
                return planned_apply("correct", {"project_id": project, "expected_revision": current["revision"],
                    "idempotency_key": h.key("correct"), "label": "Review caption text",
                    "steps": [{"kind": "operations", "operations": [{"type": "caption_put", "sequence_id": sequence, "caption": cue}]}]})

            h.stage("review-caption-edit", correct)

            def review_profile():
                current = inspect()
                settings = copy.deepcopy(current["project"]["settings"])
                assert settings["profile"] == "classic_global"
                settings["settings"].update(canvas_size={"width": 320, "height": 180},
                    last_custom_canvas_size={"width": 320, "height": 180}, canvas_size_mode="custom",
                    fps={"numerator": 30, "denominator": 1})
                return planned_apply("review-profile", {"project_id": project, "expected_revision": current["revision"],
                    "idempotency_key": h.key("review-profile"), "label": "Choose ordinary short review output",
                    "steps": [{"kind": "operations", "operations": [{"type": "settings_set", "settings": settings}]}]})

            h.stage("configure-review-profile", review_profile)
            current = inspect()
            h.state.setdefault("before_ui", current)
            h.state.setdefault("stale_request", {"project_id": project, "expected_revision": current["revision"],
                "idempotency_key": h.key("stale-after-human"), "label": "Old Agent proposal",
                "steps": [{"kind": "operations", "operations": [{"type": "rename", "name": "Stale overwrite"}]}]})
            h.save()
            assert h.inspect(source_id) == source
        else:
            browser = json.loads(args.browser_report.read_text())
            assert browser["status"] == "passed" and browser["project_id"] == project
            assert browser["binary_sha256"] == candidate
            reviewed = browser["snapshot"]
            call("opencut_edit_agent_plan", h.state["stale_request"], error="REVISION_CONFLICT")

            def export():
                nonlocal mcp
                accepted = h.request("video", "opencut_render_submit", {"kind": "video", "request": {
                    "project_id": project, "revision": reviewed["revision"], "sequence_id": sequence,
                    "idempotency_key": h.key("video"), "options": {"format": "mp4", "quality": "high", "include_audio": True}, "range": None}})
                assert call("opencut_render_submit", h.state["requests"]["video"]["input"])["job_id"] == accepted["job_id"]
                h.state["accepted_video"] = accepted["job_id"]
                h.save()
                # Accepted work belongs to the service, not this MCP connection.
                mcp.close()
                mcp = None
                h.call = cli_call

                def concurrent_edit():
                    active = cli_call("opencut_job_view", {"job_id": accepted["job_id"]})
                    assert active["state"] in ("queued", "running"), active["state"]
                    receipt = planned_apply("during-export", {"project_id": project,
                        "expected_revision": reviewed["revision"], "idempotency_key": h.key("during-export"),
                        "label": "Continue editing while fixed output runs",
                        "steps": [{"kind": "operations", "operations": [{"type": "bookmark", "operation": {
                            "kind": "put", "bookmark": {"id": "workflow-export-note", "sequence_id": sequence,
                                "time": at(.5), "duration": None, "color": "#44aaff", "note": "Fixed output keeps its reviewed revision"}}}]}]})
                    return {"observed_job_state": active["state"], "receipt": receipt,
                        "mcp_connection_closed": True}

                h.stage("continued-edit-after-agent-disconnect", concurrent_edit)
                return wait_job(accepted)

            job = h.stage("fixed-revision-video", export)
            assert job["target"]["revision"] == reviewed["revision"]
            artifact = next(a for a in job["artifacts"] if a["mime_type"] == "video/mp4")
            access = h.call("opencut_artifact_get", {"artifact_id": artifact["artifact_id"]})
            token = (h.workspace / "service/api-token").read_text().strip()
            with urllib.request.urlopen(urllib.request.Request(args.endpoint + access["http_path"],
                    headers={"Authorization": "Bearer " + token}), timeout=60) as response:
                data = response.read()
            assert sha(data) == artifact["content_hash"]
            output = h.workspace / "agent-domain-workflow.mp4"
            output.write_bytes(data)
            composition = json.loads(args.browser_report.with_name("workflow-composition-evidence.json").read_text())
            composition.update(output=str(output), sha256=sha(data), job=job, export_passed=True,
                scope="Ordinary three-second synthetic source, caption and music; no large/long-media qualification.")
            (h.workspace / "agent-domain-workflow-video-evidence.json").write_text(json.dumps(composition, indent=2) + "\n")
            def restore_review():
                request = h.state["requests"].get("restore-review", {}).get("input")
                if request is None:
                    request = {"project_id": project, "expected_revision": inspect()["revision"],
                        "target_revision": reviewed["revision"], "idempotency_key": h.key("restore-review"), "asset_versions": {}}
                receipt = h.request("restore-review", "opencut_project_restore", request)
                assert inspect()["project"] == reviewed["project"]
                return receipt

            h.stage("reopen-and-restore-reviewed-content", restore_review)

            def delivery_note():
                if "delivery-note" in h.state["requests"]:
                    return h.apply("delivery-note", h.state["requests"]["delivery-note"]["input"])
                current = inspect()
                return planned_apply("delivery-note", {"project_id": project, "expected_revision": current["revision"],
                    "idempotency_key": h.key("delivery-note"), "label": "Mark reviewed project for exchange",
                    "steps": [{"kind": "operations", "operations": [{"type": "bookmark", "operation": {
                        "kind": "put", "bookmark": {"id": "workflow-delivery-note", "sequence_id": sequence,
                            "time": at(1), "duration": None, "color": "#44aaff", "note": "Reviewed short project ready for exchange"}}}]}]})

            h.stage("continue-editing-after-output", delivery_note)
            h.state["final_snapshot"] = inspect()

        h.state["domain_discovery"] = {"compact_tools": len(compact), "fully_expanded_operations": len(routes),
            "business_domains": ["project", "edit", "media", "analysis", "render", "catalog", "plugin"],
            "authorization_unit": "original registered leaf operation"}
        h.save()
        (args.output / "native-report.json").write_text(json.dumps({"candidate": candidate, "phase": args.phase,
            "project_id": project, "stages": h.state["stages"], "discovery": h.state["domain_discovery"]}, indent=2) + "\n")
    finally:
        if mcp is not None:
            mcp.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["binary", "workspace", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--phase", choices=["prepare", "export"], required=True)
    parser.add_argument("--browser-report", type=Path)
    parser.add_argument("--previous-candidate", help="Explicitly preserve completed stages across a pre-output native correction")
    verify(parser.parse_args())
