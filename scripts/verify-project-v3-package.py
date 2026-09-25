#!/usr/bin/env python3
"""Real schema 3 package round trip, source removal, continued edit and render."""
import argparse
import hashlib
import json
import shutil
import socket
import subprocess
import time
import urllib.request
from pathlib import Path


def verify(binary: Path, artifacts: Path):
    run = artifacts.resolve() / f"package-v3-{time.time_ns()}"
    origin, target = run / "origin", run / "target"
    origin.mkdir(parents=True)
    target.mkdir()
    source = origin / "source.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
        "color=red:s=64x48:r=10:d=1", "-c:v", "libx264", "-pix_fmt", "yuv420p",
        str(source)], check=True, timeout=30)
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    trace = []

    def call(workspace, name, body, error=None):
        proc = subprocess.run([str(binary), "--database", str(workspace / "workspace.sqlite"),
            "--json", "call", name, "--input", "-"], input=json.dumps(body),
            text=True, capture_output=True, timeout=90)
        response = json.loads(proc.stdout)
        trace.append({"workspace": workspace.name, "tool": name, "response": response})
        if error:
            assert proc.returncode and response["error"]["code"] == error, (response, proc.stderr)
        else:
            assert proc.returncode == 0 and response["error"] is None, (response, proc.stderr)
        return response.get("data")

    tick = lambda n: {"ticks": str(n), "timebase": 10}
    span = {"start": tick(0), "duration": tick(10)}
    created = call(origin, "opencut_project_v3_create", {"project": {
        "id": "source-v3", "name": "Portable V3", "sequence_id": "main",
        "width": 64, "height": 48, "fps": {"numerator": 10, "denominator": 1}},
        "idempotency_key": "create-v3"})
    operations = [
        {"type": "base.apply", "operation": {"type": "asset.bind", "asset": {
            "id": "red", "name": "Red", "kind": "video", "uri": str(source),
            "duration": tick(10), "metadata": {}}}},
        {"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main",
            "track": {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                "muted": False, "locked": False, "clips": [], "params": {}}}},
        {"type": "base.apply", "operation": {"type": "clip.insert", "sequence_id": "main",
            "track_id": "video", "clip": {"id": "clip", "asset_id": "red",
                "timeline_range": span, "source_range": span, "params": {}, "channels": []}}},
        {"type": "base.apply", "operation": {"type": "track.create", "sequence_id": "main",
            "track": {"id": "graphics", "name": "Graphics", "kind": "graphic", "enabled": True,
                "muted": False, "locked": False, "clips": [], "params": {}}}},
        {"type": "shape.put", "sequence_id": "main", "shape": {"id": "blue",
            "track_id": "graphics", "timeline_range": span,
            "primitive": {"kind": "rectangle", "width_px": 20.0, "height_px": 16.0},
            "fill": {"red": 0, "green": 20, "blue": 255, "alpha": 255},
            "position_x": 0.0, "position_y": 0.0, "rotation_degrees": 0.0, "opacity": 1.0}},
        {"type": "effect.put", "sequence_id": "main", "effect": {"id": "grade",
            "target_id": "clip", "effect": {"kind": "color_adjust", "exposure_ev": 0.0,
                "contrast": 1.0, "saturation": 0.0}}},
        {"type": "mask.put", "sequence_id": "main", "mask": {"id": "half",
            "target_id": "clip", "shape": {"kind": "rectangle", "x": 0.0, "y": 0.0,
                "width": 0.5, "height": 1.0}, "inverted": False, "feather": 0.0}},
        {"type": "base.apply", "operation": {"type": "caption.insert", "sequence_id": "main",
            "cue": {"id": "portable-font", "timeline_range": span, "text": "x",
                "source_anchor": None, "style": {"font_family": "Arial", "font_size": 1.0,
                    "color": "white", "background_color": None}}}},
    ]
    edited = call(origin, "opencut_edit_v3_apply", {"project_id": "source-v3",
        "expected_revision": created["revision"], "idempotency_key": "build-v3",
        "label": "Build portable composition", "operations": operations})
    revision = edited["revision"]
    v1_target = {"project_id": "source-v3", "id": "org.example.package-planner", "version": "1.2.0"}
    v1_grant = {"target": v1_target, "package_sha256": hashlib.sha256(b"planner package fixture").hexdigest(),
        "operations": ["org.example.package-planner.apply"], "enabled": True,
        "expected_revision": "0", "idempotency_key": "grant-v1-package"}
    call(origin, "opencut_project_plugin_grant", v1_grant)
    stored_v1_grant = call(origin, "opencut_project_plugin_grant", {**v1_grant,
        "enabled": False, "expected_revision": "1", "idempotency_key": "disable-v1-package"})
    assert stored_v1_grant["revision"] == "2" and not stored_v1_grant["enabled"]

    v2_package = run / "package-v2"
    (v2_package / "specs").mkdir(parents=True)
    v2_style = {"font_family": "Arial", "font_size": 30.0, "color": "#ffffff",
        "background_color": None,
        "font_policy": {"mode": "explicit_fallback", "families": ["Helvetica"]}}
    v2_spec = {"kind": "caption_style", "style": v2_style}
    v2_spec_bytes = json.dumps(v2_spec, separators=(",", ":"), ensure_ascii=False).encode()
    (v2_package / "specs" / "captions.json").write_bytes(v2_spec_bytes)
    v2_manifest = {"manifest_version": 2, "id": "org.example.package-preset", "version": "2.1.0",
        "display_name": "Package preset", "presets": [{"name": "captions",
            "spec_path": "specs/captions.json", "spec_sha256": hashlib.sha256(v2_spec_bytes).hexdigest()}]}
    (v2_package / "manifest_v2.json").write_text(json.dumps(v2_manifest, ensure_ascii=False))
    installed_v2 = call(origin, "opencut_plugin_v2_install", {"package_root": str(v2_package)})
    v2_grant = {"target": {"project_id": "source-v3", "id": v2_manifest["id"],
        "version": v2_manifest["version"]}, "package_sha256": installed_v2["package_sha256"],
        "preset_names": ["captions"], "enabled": True, "expected_revision": "0",
        "idempotency_key": "grant-v2-package"}
    stored_v2_grant = call(origin, "opencut_plugin_v2_grant", v2_grant)
    linked = call(origin, "opencut_project_v3_package_export", {"project_id": "source-v3",
        "revision": revision, "destination": str(run / "linked.opencut"), "mode": "linked"})
    portable = call(origin, "opencut_project_v3_package_export", {"project_id": "source-v3",
        "revision": revision, "destination": str(run / "portable.opencut"), "mode": "self_contained"})
    manifest = json.loads((Path(portable["package_path"]) / "manifest.json").read_text())
    assert manifest["schema_version"] == 4 and manifest["project_schema_version"] == 3
    assert [lock["name"] for lock in manifest["tool_locks"]] == ["ffmpeg", "ffprobe"]
    assert all(len(lock["binary_sha256"]) == 64 and lock["version"] for lock in manifest["tool_locks"])
    assert manifest["assets"][0]["content_hash"] == source_hash
    assert portable["tool_locks"] == manifest["tool_locks"]
    assert portable["plugin_grants"] == manifest["plugin_grants"]
    assert {grant["kind"] for grant in manifest["plugin_grants"]} == {"planner_v1", "preset_v2"}
    assert any(grant["kind"] == "planner_v1"
        and grant["operations"] == ["org.example.package-planner.apply"]
        and not grant["enabled"]
        and grant["package_sha256"] == stored_v1_grant["package_sha256"]
        for grant in manifest["plugin_grants"])
    assert any(grant["kind"] == "preset_v2"
        and grant["preset_names"] == ["captions"]
        and grant["package_sha256"] == stored_v2_grant["package_sha256"]
        for grant in manifest["plugin_grants"])
    assert portable["self_contained_media"] and not portable["fully_reproducible"]
    linked_request = {"package_path": linked["package_path"], "target_project_id": "linked-v3",
        "expected_manifest_sha256": linked["manifest_sha256"], "idempotency_key": "import-linked-v3"}
    linked_import = call(target, "opencut_project_v3_package_import", linked_request)
    assert linked_import["font_environment_checked"] and not linked_import["fonts_missing"]
    shutil.rmtree(origin)
    call(target, "opencut_project_v3_package_import", {**linked_request,
        "target_project_id": "linked-v3-after-removal", "idempotency_key": "linked-after-removal"},
        error="PACKAGE_IO_ERROR")
    request = {"package_path": portable["package_path"], "target_project_id": "restored-v3",
        "expected_manifest_sha256": portable["manifest_sha256"], "idempotency_key": "import-portable-v3"}
    imported = call(target, "opencut_project_v3_package_import", request)
    assert imported["tool_environment_matches"] and not imported["tool_lock_mismatches"]
    assert imported["font_environment_checked"] and not imported["fonts_missing"]
    restore_plan = imported["plugin_grant_restore_plan"]
    assert [item["tool"] for item in restore_plan] == [
        "opencut_project_plugin_grant", "opencut_plugin_v2_grant"]
    v1_action = restore_plan[0]["input"]
    v2_action = restore_plan[1]["input"]
    assert v1_action["target"]["project_id"] == "restored-v3"
    assert v1_action["expected_revision"] == v2_action["expected_revision"] == "0"
    assert v1_action["operations"] == ["org.example.package-planner.apply"]
    assert not v1_action["enabled"]
    assert v2_action["target"]["id"] == v2_manifest["id"]
    assert v2_action["preset_names"] == ["captions"] and v2_action["enabled"]
    assert imported["dependencies"]["plugins_not_bundled"] == manifest["dependencies"]["plugins_not_bundled"]
    # Import only returns actions. Neither V1/V2 grants nor V2 packages appear until Owner acts.
    assert call(target, "opencut_project_plugin_list", {"project_id": "restored-v3"}) == []
    assert call(target, "opencut_plugin_v2_grants", {"project_id": "restored-v3"}) == []
    assert call(target, "opencut_plugin_list", {}) == []
    assert call(target, "opencut_plugin_v2_list", {}) == []
    target_v2 = call(target, "opencut_plugin_v2_install", {"package_root": str(v2_package)})
    assert target_v2["package_sha256"] == v2_action["package_sha256"]
    restored_v1 = call(target, restore_plan[0]["tool"], v1_action)
    restored_v2 = call(target, restore_plan[1]["tool"], v2_action)
    assert not restored_v1["enabled"] and restored_v1["revision"] == "1"
    assert restored_v2["enabled"] and restored_v2["revision"] == "1"
    assert restored_v1["package_sha256"] == v1_action["package_sha256"]
    assert restored_v1["operations"] == v1_action["operations"]
    assert restored_v2["package_sha256"] == v2_action["package_sha256"]
    assert restored_v2["preset_names"] == v2_action["preset_names"]
    assert call(target, restore_plan[0]["tool"], v1_action) == restored_v1
    assert call(target, restore_plan[1]["tool"], v2_action) == restored_v2
    assert call(target, "opencut_project_plugin_list", {"project_id": "restored-v3"}) == [restored_v1]
    assert call(target, "opencut_plugin_v2_grants", {"project_id": "restored-v3"}) == [restored_v2]
    snapshot = call(target, "opencut_project_v3_inspect", {"project_id": "restored-v3", "revision": None})
    assert snapshot["revision"] == imported["revision"]
    assert snapshot["project"]["schema_version"] == 3
    assert snapshot["project"]["visual"] == json.loads((Path(portable["package_path"]) / "project.json").read_text())["visual"]
    call(target, "opencut_project_inspect", {"project_id": "restored-v3"}, error="UNSUPPORTED_PROJECT_VERSION")
    shape = {"type": "shape.put", "sequence_id": "main", "shape": {
        "id": "green", "track_id": "graphics", "timeline_range": span,
        "primitive": {"kind": "ellipse", "radius_x_px": 5.0, "radius_y_px": 5.0},
        "fill": {"red": 0, "green": 255, "blue": 0, "alpha": 255},
        "position_x": -20.0, "position_y": 12.0, "rotation_degrees": 0.0, "opacity": 1.0}}
    continued = call(target, "opencut_edit_v3_apply", {"project_id": "restored-v3",
        "expected_revision": imported["revision"], "idempotency_key": "continue-v3",
        "label": "Continue imported V3", "operations": [shape]})
    preflight = call(target, "opencut_render_v3_preflight", {
        "project_id": "restored-v3", "revision": continued["revision"], "sequence_id": "main",
        "container": "mp4", "video_codec": "h264"})
    assert preflight["ready"], preflight
    job = call(target, "opencut_job_v3_render",
 {"idempotency_key": "render-imported-v3",
        "project_id": "restored-v3", "revision": continued["revision"], "sequence_id": "main",
        "container": "mp4", "video_codec": "h264"})
    worker = subprocess.run([str(binary), "--database", str(target / "workspace.sqlite"),
        "worker", "run", "--once"], capture_output=True, text=True, timeout=120)
    assert worker.returncode == 0, worker.stderr
    done = call(target, "opencut_job_view", {"job_id": job["job_id"]})
    assert done["state"] == "succeeded" and done["extended_result"]["kind"] == "render.project.v3", done
    artifact = done["artifacts"][0]
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0)); port = probe.getsockname()[1]
    token_path = run / "token"
    with (run / "serve.log").open("wb") as log:
        host = subprocess.Popen([str(binary), "--database", str(target / "workspace.sqlite"),
            "serve", "--listen", f"127.0.0.1:{port}", "--token-file", str(token_path)],
            stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 20
            while not token_path.exists():
                assert host.poll() is None and time.monotonic() < deadline; time.sleep(.05)
            while True:
                assert host.poll() is None and time.monotonic() < deadline
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=.25): break
                except OSError: time.sleep(.05)
            req = urllib.request.Request(f"http://127.0.0.1:{port}/v1/jobs/{job['job_id']}/artifact",
                headers={"Authorization": "Bearer " + token_path.read_text().strip()})
            with urllib.request.urlopen(req, timeout=30) as response: video = response.read()
        finally:
            host.terminate(); host.wait(timeout=10)
    render_hash = hashlib.sha256(video).hexdigest()
    assert render_hash == artifact["content_hash"]
    output = run / "render.mp4"; output.write_bytes(video)
    frame = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(output),
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], timeout=30)
    def pixel(x,y):
        i=(y*64+x)*3; return tuple(frame[i:i+3])
    left, right, blue, green = pixel(5,24), pixel(58,24), pixel(32,24), pixel(12,36)
    assert max(left)-min(left)<35 and max(right)<30, (left,right)
    assert blue[2]>blue[0]+80 and green[1]>green[0]+80, (blue,green)

    mismatch_package = run / "mismatch.opencut"
    shutil.copytree(portable["package_path"], mismatch_package)
    mismatch_manifest_path = mismatch_package / "manifest.json"
    mismatch_manifest = json.loads(mismatch_manifest_path.read_text())
    mismatch_manifest["tool_locks"][0]["version"] += "-different"
    mismatch_manifest["tool_locks"][0]["binary_sha256"] = "0" * 64
    mismatch_manifest_bytes = json.dumps(mismatch_manifest, separators=(",", ":")).encode()
    mismatch_manifest_path.write_bytes(mismatch_manifest_bytes)
    mismatch_request = {"package_path": str(mismatch_package),
        "target_project_id": "restored-v3-tool-mismatch",
        "expected_manifest_sha256": hashlib.sha256(mismatch_manifest_bytes).hexdigest(),
        "idempotency_key": "import-v3-tool-mismatch"}
    mismatch_import = call(target, "opencut_project_v3_package_import", mismatch_request)
    assert not mismatch_import["tool_environment_matches"]
    assert mismatch_import["tool_lock_mismatches"] == ["ffmpeg"]

    legacy_package = run / "legacy-v2.opencut"
    shutil.copytree(portable["package_path"], legacy_package)
    legacy_manifest_path = legacy_package / "manifest.json"
    legacy_manifest = json.loads(legacy_manifest_path.read_text())
    legacy_manifest["schema_version"] = 2
    legacy_manifest.pop("tool_locks")
    legacy_manifest.pop("plugin_grants")
    legacy_manifest["dependencies"]["plugins_not_bundled"] = []
    legacy_manifest["dependencies"]["tools_not_locked"] = ["ffmpeg", "ffprobe"]
    legacy_manifest["fully_reproducible"] = False
    legacy_manifest_bytes = json.dumps(legacy_manifest, separators=(",", ":")).encode()
    legacy_manifest_path.write_bytes(legacy_manifest_bytes)
    legacy_request = {"package_path": str(legacy_package), "target_project_id": "restored-v3-legacy",
        "expected_manifest_sha256": hashlib.sha256(legacy_manifest_bytes).hexdigest(),
        "idempotency_key": "import-v3-legacy"}
    legacy_import = call(target, "opencut_project_v3_package_import", legacy_request)
    assert not legacy_import["tool_environment_matches"]
    assert legacy_import["tool_lock_mismatches"] == ["ffmpeg", "ffprobe"]

    legacy_plugin_package = run / "legacy-v3-plugin-unlocked.opencut"
    shutil.copytree(portable["package_path"], legacy_plugin_package)
    legacy_plugin_manifest_path = legacy_plugin_package / "manifest.json"
    legacy_plugin_manifest = json.loads(legacy_plugin_manifest_path.read_text())
    legacy_plugin_manifest["schema_version"] = 3
    legacy_plugin_manifest.pop("plugin_grants")
    legacy_plugin_manifest_bytes = json.dumps(legacy_plugin_manifest, separators=(",", ":")).encode()
    legacy_plugin_manifest_path.write_bytes(legacy_plugin_manifest_bytes)
    legacy_plugin_request = {"package_path": str(legacy_plugin_package),
        "target_project_id": "restored-v3-legacy-plugin",
        "expected_manifest_sha256": hashlib.sha256(legacy_plugin_manifest_bytes).hexdigest(),
        "idempotency_key": "import-v3-legacy-plugin"}
    call(target, "opencut_project_v3_package_import", legacy_plugin_request,
        error="PACKAGE_DEPENDENCY_MISSING")
    call(target, "opencut_project_v3_inspect", {"project_id": "restored-v3-legacy-plugin", "revision": None},
        error="NOT_FOUND")

    grant_mismatch_package = run / "grant-mismatch.opencut"
    shutil.copytree(portable["package_path"], grant_mismatch_package)
    grant_manifest_path = grant_mismatch_package / "manifest.json"
    grant_manifest = json.loads(grant_manifest_path.read_text())
    grant_manifest["plugin_grants"][0]["package_sha256"] = "0" * 64
    grant_manifest_bytes = json.dumps(grant_manifest, separators=(",", ":")).encode()
    grant_manifest_path.write_bytes(grant_manifest_bytes)
    grant_mismatch_request = {"package_path": str(grant_mismatch_package),
        "target_project_id": "restored-v3-grant-mismatch",
        "expected_manifest_sha256": hashlib.sha256(grant_manifest_bytes).hexdigest(),
        "idempotency_key": "import-v3-grant-mismatch"}
    call(target, "opencut_project_v3_package_import", grant_mismatch_request, error="PACKAGE_INVALID")
    call(target, "opencut_project_v3_inspect", {"project_id": "restored-v3-grant-mismatch", "revision": None},
        error="NOT_FOUND")

    font_package = run / "font-missing.opencut"
    shutil.copytree(portable["package_path"], font_package)
    font_project_path = font_package / "project.json"
    font_project = json.loads(font_project_path.read_text())
    missing_family = "OpenCut Missing Package Font 4D27C9"
    font_project["base"]["sequences"][0]["caption_cues"][0]["style"]["font_family"] = missing_family
    font_project_bytes = json.dumps(font_project, separators=(",", ":"), ensure_ascii=False).encode()
    font_project_path.write_bytes(font_project_bytes)
    font_manifest_path = font_package / "manifest.json"
    font_manifest = json.loads(font_manifest_path.read_text())
    font_manifest["project_sha256"] = hashlib.sha256(font_project_bytes).hexdigest()
    font_manifest["dependencies"]["fonts_not_bundled"] = [missing_family]
    font_manifest_bytes = json.dumps(font_manifest, separators=(",", ":")).encode()
    font_manifest_path.write_bytes(font_manifest_bytes)
    font_request = {"package_path": str(font_package), "target_project_id": "restored-v3-font-missing",
        "expected_manifest_sha256": hashlib.sha256(font_manifest_bytes).hexdigest(),
        "idempotency_key": "import-v3-font-missing"}
    font_import = call(target, "opencut_project_v3_package_import", font_request)
    assert font_import["font_environment_checked"]
    assert font_import["fonts_missing"] == [missing_family]
    font_preflight = call(target, "opencut_render_v3_preflight", {
        "project_id": "restored-v3-font-missing", "revision": font_import["revision"],
        "sequence_id": "main", "container": "mp4", "video_codec": "h264"})
    assert not font_preflight["ready"]
    assert any(issue["cue_id"] == "portable-font" and "font family" in issue["message"]
        for issue in font_preflight["issues"])
    shutil.rmtree(portable["package_path"])
    assert call(target, "opencut_project_v3_package_import", request) == imported
    report={"binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
        "source_sha256":source_hash,"manifest_sha256":portable["manifest_sha256"],
        "tool_locks": manifest["tool_locks"],
        "tool_environment_checks": {"matching": imported["tool_environment_matches"],
            "mismatch_preserved_import": mismatch_import["tool_lock_mismatches"],
            "legacy_unlocked_import": legacy_import["tool_lock_mismatches"]},
        "font_environment_checks": {"matching": imported["fonts_missing"],
            "missing_import": font_import["fonts_missing"],
            "render_preflight_ready": font_preflight["ready"]},
        "source_revision":revision,"imported_revision":imported["revision"],
        "render_sha256":render_hash,"pixels":{"masked_left":left,"masked_right":right,"blue":blue,"green":green},
        "checks":["schema4 manifest/document, visual graph, and exact V1/V2 grant snapshots validated",
            "linked package requires original CAS after source workspace removal",
            "self-contained package imports after origin workspace removal",
            "V3 visual graph survives relocation and accepts a new edit",
            "schema3 preflight resolves pinned assets and visual render dependencies",
            "render artifact download hash and actual mask/effect/shape pixels verified",
            "identical import replay succeeds after package removal",
            "exact FFmpeg and ffprobe binary locks are recorded and verified",
            "tool lock mismatch remains importable and is reported",
            "schema 2 package compatibility remains explicit as unlocked",
            "legacy plugin grants without complete snapshots remain explicitly blocked; mismatched schema4 identities are rejected",
            "typed restore actions do not run during import; Owner installs the exact V2 package, applies V1/V2 actions explicitly and replays them idempotently",
            "missing font families are reported on import and block render preflight"],"trace":trace}
    (run/"report.json").write_text(json.dumps(report,ensure_ascii=False,indent=2))
    return {"status":"passed","report":str(run/"report.json"),"checks":report["checks"]}

if __name__ == "__main__":
    parser=argparse.ArgumentParser();parser.add_argument("--binary",type=Path,required=True);parser.add_argument("--artifacts-dir",type=Path,required=True);args=parser.parse_args()
    print(json.dumps(verify(args.binary.resolve(),args.artifacts_dir),ensure_ascii=False))
