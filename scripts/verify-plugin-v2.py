#!/usr/bin/env python3
"""Real declarative Plugin V2 install/grant/plan/apply/render acceptance."""
import argparse
import hashlib
import json
import subprocess
import time
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(binary: Path, artifacts: Path, schema3=False):
    run = artifacts.resolve() / f"plugin-v2-{time.time_ns()}"
    run.mkdir(parents=True)
    db = run / "workspace.sqlite"
    trace = []
    promoted = False

    def call(name, payload, error=None):
        if promoted:
            if name == "opencut_plugin_v2_preset_plan":
                name = "opencut_plugin_v2_preset_v3_plan"
            elif name == "opencut_edit_apply":
                name, payload = "opencut_edit_v3_apply", {"project_id": payload["project_id"], **payload["request"]}
            elif name == "opencut_project_inspect":
                name = "opencut_project_v3_inspect"
            elif name == "opencut_job_start":
                assert payload["kind"] == "render.project"
                name, payload = "opencut_job_v3_render", {"idempotency_key": payload["idempotency_key"], **payload["input"]}
        proc = subprocess.run([str(binary), "--database", str(db), "--json", "call", name,
            "--input", "-"], input=json.dumps(payload, ensure_ascii=False), text=True,
            capture_output=True, timeout=90)
        response = json.loads(proc.stdout)
        trace.append({"tool": name, "response": response})
        if error:
            assert proc.returncode and response["error"]["code"] == error, (response, proc.stderr)
        else:
            assert proc.returncode == 0 and response["error"] is None, (response, proc.stderr)
        return response.get("data")

    source = run / "red.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "color=red:s=320x180:r=10:d=2", "-c:v", "libx264", "-pix_fmt", "yuv420p",
        str(source)], check=True, timeout=30)
    start = call("opencut_project_create", {"project": {"id": "preset-film",
        "name": "Preset film", "sequence_id": "main", "width": 320, "height": 180,
        "fps": {"numerator": 10, "denominator": 1}}, "idempotency_key": "create-preset"})
    tick = lambda n: {"ticks": str(n), "timebase": 10}
    span = {"start": tick(0), "duration": tick(20)}
    bound = call("opencut_edit_apply", {"project_id": "preset-film", "request": {
        "expected_revision": start["revision"], "idempotency_key": "pin-red",
        "label": "Pin source", "operations": [{"type": "asset.bind", "asset": {
            "id": "red", "name": "Red", "kind": "video", "uri": str(source),
            "duration": tick(20), "metadata": {}}}]}})
    assert call("opencut_project_inspect", {"project_id":"preset-film"})["project"]["assets"][0]["version"]["id"].startswith("av_")
    source.unlink()
    if schema3:
        bound = call("opencut_project_v3_promote", {"project_id":"preset-film",
            "expected_revision":bound["revision"],"idempotency_key":"promote"})
        assert bound["project"]["schema_version"] == 3
        promoted = True

    package = run / "package-v2"
    (package / "specs").mkdir(parents=True)
    style = {"font_family": "Arial", "font_size": 30.0, "color": "#ffffff",
        "background_color": None, "font_policy": {"mode": "explicit_fallback", "families": ["Helvetica"]}}
    styled = {**style, "color": "#ffff00", "font_size": 34.0}
    template = {"kind": "edit_template", "operations": [
        {"type": "track.create", "sequence_id": "main", "track": {"id": "preset-v",
            "name": "Preset Video", "kind": "video", "enabled": True, "muted": False,
            "locked": False, "clips": [], "params": {}}},
        {"type": "clip.insert", "sequence_id": "main", "track_id": "preset-v",
            "clip": {"id": "preset-c", "asset_id": None, "timeline_range": span,
            "source_range": span, "params": {}, "channels": []}},
        {"type": "caption.insert", "sequence_id": "main", "cue": {"id": "preset-caption",
            "timeline_range": {"start": tick(2), "duration": tick(15)},
            "text": "Preset QA", "source_anchor": None, "style": style}}
    ], "clip_bindings": {"preset-c": "hero"}}
    specs = {"movie": template, "yellow": {"kind": "caption_style", "style": styled}}
    manifest = {"manifest_version": 2, "id": "example.preset", "version": "1.0.0",
        "display_name": "Example preset", "presets": []}
    for name, value in specs.items():
        content = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()
        (package / "specs" / f"{name}.json").write_bytes(content)
        manifest["presets"].append({"name": name, "spec_path": f"specs/{name}.json",
            "spec_sha256": sha(content)})
    (package / "manifest_v2.json").write_text(json.dumps(manifest, ensure_ascii=False))
    installed = call("opencut_plugin_v2_install", {"package_root": str(package)})
    assert installed["manifest"]["manifest_version"] == 2
    assert call("opencut_plugin_v2_list", {}) == [installed]
    assert call("opencut_plugin_list", {}) == []  # frozen v1 catalog stays distinct
    target = {"project_id": "preset-film", "id": "example.preset", "version": "1.0.0"}
    request = {"target": target, "preset_name": "movie", "revision": bound["revision"],
        "parameters": {"kind": "edit_template"}, "asset_bindings": {"hero": "red"}}
    call("opencut_plugin_v2_preset_plan", request, error="NOT_AUTHORIZED")
    grant_request = {"target": target, "package_sha256": installed["package_sha256"],
        "preset_names": ["movie", "yellow"], "enabled": True,
        "expected_revision": "0", "idempotency_key": "grant-preset"}
    grant = call("opencut_plugin_v2_grant", grant_request)
    assert call("opencut_plugin_v2_grant", grant_request) == grant
    assert call("opencut_plugin_v2_grants", {"project_id":"preset-film"}) == [grant]
    assert call("opencut_plugin_v2_inspect", {"target": target, "preset_name": "movie"})["package"]["package_sha256"] == installed["package_sha256"]
    plan = call("opencut_plugin_v2_preset_plan", request)
    assert plan["package_sha256"] == installed["package_sha256"]
    inserted = plan["operations"][1]
    if schema3:
        assert inserted["type"] == "base.apply"
        inserted = inserted["operation"]
    assert inserted["clip"]["asset_id"] == "red"
    assert plan["plan"].get("unresolved_asset_ids", []) == []
    published = call("opencut_edit_apply", {"project_id": "preset-film", "request": {
        "expected_revision": bound["revision"], "idempotency_key": "apply-template",
        "label": "Apply preset", "operations": plan["operations"]}})
    style_plan = call("opencut_plugin_v2_preset_plan", {**request,
        "preset_name": "yellow", "revision": published["revision"],
        "parameters": {"kind": "caption_style", "sequence_id": "main",
            "cue_ids": ["preset-caption"]}, "asset_bindings": {}})
    styled_result = call("opencut_edit_apply", {"project_id": "preset-film", "request": {
        "expected_revision": published["revision"], "idempotency_key": "apply-style",
        "label": "Apply style", "operations": style_plan["operations"]}})
    final_snapshot = call("opencut_project_inspect", {"project_id": "preset-film"})
    document = final_snapshot["project"]["base"] if schema3 else final_snapshot["project"]
    assert document["sequences"][0]["caption_cues"][0]["style"]["color"] == "#ffff00"
    job = call("opencut_job_start", {"kind": "render.project", "idempotency_key": "render-preset",
        "input": {"project_id": "preset-film", "revision": styled_result["revision"],
            "sequence_id": "main", "container": "mp4", "video_codec": "h264"}})
    worker = subprocess.run([str(binary), "--database", str(db), "worker", "run", "--once"],
        capture_output=True, text=True, timeout=90)
    assert worker.returncode == 0, worker.stderr
    done = call("opencut_job_get", {"job_id": job["job_id"]})
    assert done["state"] == "succeeded", done
    if schema3:
        public = call("opencut_job_view", {"job_id":job["job_id"]})
        assert public["kind"] == "render.project.v3" and public["result_unavailable"] is None, public
        assert public["extended_result"]["data"]["revision"] == styled_result["revision"]
        assert "output" not in public["extended_result"]["data"]["media"]
    artifact = done["result"]["artifact"]
    output = Path(artifact["path"])
    assert sha(output.read_bytes()) == artifact["content_hash"]
    rgb = subprocess.check_output(["ffmpeg", "-v", "error", "-ss", "0.5", "-i", str(output),
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
    assert sum(rgb[0::3]) / (320 * 180) > 180
    yellow = sum(rgb[i] > 150 and rgb[i+1] > 150 and rgb[i+2] < 100
        for i in range(0, len(rgb), 3))
    assert yellow > 20, yellow

    installed_spec = run / "plugin-packages-v2/example.preset/1.0.0" / installed["package_sha256"] / "specs/movie.json"
    original = installed_spec.read_bytes()
    installed_spec.write_bytes(b"{}")
    call("opencut_plugin_v2_preset_plan", {**request, "revision": styled_result["revision"]},
        error="INVALID_PLUGIN")
    installed_spec.write_bytes(original)
    revoke = call("opencut_plugin_v2_grant", {**grant_request, "enabled": False,
        "expected_revision": "1", "idempotency_key": "revoke-preset"})
    assert not revoke["enabled"]
    call("opencut_plugin_v2_preset_plan", {**request, "revision": styled_result["revision"]},
        error="NOT_AUTHORIZED")
    assert call("opencut_plugin_v2_grant", grant_request) == grant
    call("opencut_plugin_v2_preset_plan", {**request, "revision": styled_result["revision"]},
        error="NOT_AUTHORIZED")
    report = {"binary_sha256": sha(binary.read_bytes()), "package_sha256": installed["package_sha256"],
        "spec_sha256": plan["spec_sha256"], "render_sha256": artifact["content_hash"],
        "yellow_caption_pixels": yellow, "project_schema_version": 3 if schema3 else 2,
        "checks": ["v1 catalog isolation", "exact grant and fixed pinned asset binding",
            "normal apply of template and style", "render after original media removal",
            "tamper rejection", "revoked grant and stale replay do not authorize"], "trace": trace}
    (run / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
    return run / "report.json"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    parser.add_argument("--schema3", action="store_true")
    args = parser.parse_args()
    print(verify(args.binary.resolve(), args.artifacts_dir, args.schema3))
