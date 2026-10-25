#!/usr/bin/env python3
"""Exercise the real stdio MCP host, then independently read its state via CLI.

Run after `cargo build -p opencut-cli`. Uses only Python's standard library.
"""
import argparse
import hashlib
import io
import wave
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import time


def assert_job_kind_schemas(capabilities):
    descriptors = capabilities["job_kinds"]
    expected = {
        "media.probe": ({"input"}, {"input", "streams"}),
        "preview.frame": ({"input", "requested_time", "alignment"}, {"media", "artifact"}),
        "preview.project_frame": (
            {"project_id", "revision", "sequence_id", "requested_time", "alignment"},
            {"revision", "sequence_id", "requested_time", "actual_sequence_time",
             "alignment", "quality", "artifact"},
        ),
        "preview.composited_frame": (
            {"project_id", "revision", "sequence_id", "requested_time", "alignment"},
            {"revision", "sequence_id", "requested_time", "actual_sequence_time",
             "alignment", "quality", "artifact"},
        ),
        "preview.project_range": (
            {"project_id", "revision", "sequence_id", "requested_range", "alignment"},
            {"revision", "sequence_id", "requested_range", "actual_sequence_range",
             "alignment", "quality", "media", "artifact"},
        ),
        "render.export": ({"segments", "container", "video", "audio"}, {"media", "artifact"}),
        "render.project": (
            {"project_id", "revision", "sequence_id", "container", "video_codec"},
            {"revision", "media", "artifact"},
        ),
        "artifact.verify": ({"input"}, {"passed", "checks", "metadata", "decoded_samples"}),
        "media.import": ({"upload_id"}, {"media_id", "project_id", "upload_id", "job_id", "asset"}),
    }
    analysis_input = {"idempotency_key", "project_id", "revision", "asset_id"}
    analysis_result = {"project_id", "revision", "asset_id", "asset_version"}
    expected["analysis.transcribe"] = (analysis_input, analysis_result | {"transcription"})
    for kind, options in {
        "silence": {"noise_db", "min_duration_ms"}, "shots": {"threshold"},
        "waveform": {"buckets"}, "speaker_turns": {"language"},
    }.items():
        expected["analysis." + kind] = (analysis_input | options, analysis_result | {"analysis"})
    for v3, legacy in {
        "render.project.v3": "render.project",
        "preview.project_frame.v3": "preview.composited_frame",
        "preview.project_range.v3": "preview.project_range",
    }.items():
        inputs, outputs = expected[legacy]
        expected[v3] = (inputs | {"idempotency_key"}, outputs)
    actual_names = {item["name"] for item in descriptors}
    assert len(actual_names) == len(descriptors), "Duplicate Job descriptors"
    assert actual_names == set(expected), {"missing": sorted(set(expected) - actual_names),
                                            "unexpected": sorted(actual_names - set(expected))}

    def dereference(schema, node):
        while "$ref" in node:
            parts = node["$ref"].removeprefix("#/").split("/")
            node = schema
            for part in parts:
                node = node[part.replace("~1", "/").replace("~0", "~")]
        return node

    by_name = {item["name"]: item for item in descriptors}
    for name, (input_fields, result_fields) in expected.items():
        for key, required in (("input_schema", input_fields), ("result_schema", result_fields)):
            schema = by_name[name][key]
            root = dereference(schema, schema)
            assert root.get("type") == "object", (name, key, root)
            assert required <= set(root.get("required", [])), (name, key, root)

    def allows_null(schema, node):
        node = dereference(schema, node)
        kind = node.get("type")
        return kind == "null" or (isinstance(kind, list) and "null" in kind) or any(
            allows_null(schema, choice) for choice in node.get("anyOf", [])
        )

    for name in ("preview.project_frame", "render.project"):
        schema = by_name[name]["result_schema"]
        properties = dereference(schema, schema)["properties"]
        mode_property = properties["asset_locking"]
        mode = dereference(schema, mode_property)
        assert mode_property.get("default", mode.get("default")) == "legacy_unlocked", mode_property
        assert set(mode.get("enum", [])) == {"legacy_unlocked", "media_content_locked"}, mode
        assert allows_null(schema, properties["asset_lock"]), properties["asset_lock"]

    frame_result = by_name["preview.project_frame"]["result_schema"]
    quality = dereference(frame_result, frame_result["properties"]["quality"])
    assert quality.get("enum") == ["encoded_project_frame"], quality
    frame_input = by_name["preview.project_frame"]["input_schema"]
    input_time = dereference(frame_input, frame_input["properties"]["requested_time"])
    input_ticks = dereference(frame_input, input_time["properties"]["ticks"])
    assert input_ticks.get("type") == "string", input_ticks
    for field in ("requested_time", "actual_sequence_time"):
        instant = dereference(frame_result, frame_result["properties"][field])
        ticks = dereference(frame_result, instant["properties"]["ticks"])
        assert ticks.get("type") == "string", (field, ticks)


def verify(binary: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix="opencut-mcp-") as root:
        db = Path(root) / "editor.sqlite"
        with tempfile.TemporaryFile() as diagnostics:
            process = subprocess.Popen(
                [str(binary), "--database", str(db), "mcp", "serve", "--tool-surface", "legacy"],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=diagnostics,
            )
            selector = selectors.DefaultSelector()
            selector.register(process.stdout, selectors.EVENT_READ)
            pending = bytearray()
            serial = 0

            def send(message):
                process.stdin.write((json.dumps(message) + "\n").encode())
                process.stdin.flush()

            def request(method, params):
                nonlocal serial
                serial += 1
                send({"jsonrpc": "2.0", "id": serial, "method": method, "params": params})
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if b"\n" not in pending:
                        if not selector.select(max(0, deadline - time.monotonic())):
                            break
                        chunk = os.read(process.stdout.fileno(), 65536)
                        if not chunk:
                            raise AssertionError("MCP host closed stdout unexpectedly")
                        pending.extend(chunk)
                    while b"\n" in pending:
                        line, _, rest = pending.partition(b"\n")
                        pending[:] = rest
                        message = json.loads(line)
                        if message.get("id") == serial:
                            assert "error" not in message, message
                            return message["result"]
                raise AssertionError(f"Timed out waiting for {method}")

            def tool(name, arguments):
                return request("tools/call", {"name": name, "arguments": arguments})

            try:
                initialized = request("initialize", {
                    "protocolVersion": "2025-11-25", "capabilities": {},
                    "clientInfo": {"name": "opencut-conformance", "version": "1"},
                })
                assert initialized["capabilities"]["tools"] is not None
                send({"jsonrpc": "2.0", "method": "notifications/initialized"})
                listed = request("tools/list", {})
                names = [item["name"] for item in listed["tools"]]
                assert len(names) == len(set(names)), "duplicate advertised tools"
                assert {"opencut_media_inspect", "opencut_media_repair"} <= set(names)
                assert any(t["name"] == "opencut_edit_apply" for t in listed["tools"])
                assert all("outputSchema" in t for t in listed["tools"])
                job_start_tool = next(
                    t for t in listed["tools"] if t["name"] == "opencut_job_start"
                )
                job_start_schema = json.dumps(job_start_tool["inputSchema"])
                assert "preview.project_frame" in job_start_schema, job_start_tool
                assert "requested_time" in job_start_schema, job_start_tool
                capabilities = tool("opencut_capabilities", {})
                assert not capabilities.get("isError"), capabilities
                assert_job_kind_schemas(capabilities["structuredContent"]["data"])
                buffer = io.BytesIO()
                with wave.open(buffer, 'wb') as wav:
                    wav.setnchannels(1); wav.setsampwidth(2); wav.setframerate(48000)
                    wav.writeframes(b'\0' * 9600)
                blob = buffer.getvalue()
                content_hash = hashlib.sha256(blob).hexdigest()
                media = Path(root) / "media"
                media.mkdir(exist_ok=True)
                (media / content_hash).write_bytes(blob)
                (media / (content_hash + '.json')).write_text('{malformed legacy JSON')
                inspected = tool("opencut_media_inspect", {"content_hash": content_hash})
                assert not inspected.get("isError"), inspected
                entry = inspected["structuredContent"]["data"]
                assert entry['asset'] is None and entry['metadata_hash'] is not None
                repaired = tool("opencut_media_repair", {"content_hash": content_hash,
                    "expected_metadata_hash": entry['metadata_hash']})
                assert not repaired.get("isError"), repaired
                fixed = repaired["structuredContent"]["data"]
                assert fixed['asset']['kind']=='audio' and fixed['asset']['metadata']['mime_type']=='audio/wav'
                independent = json.loads(subprocess.check_output([str(binary), '--database', str(db), '--json',
                    'call', 'opencut_media_inspect', '--input', '-'], input=json.dumps({'content_hash':content_hash}).encode()))
                assert independent['data']==fixed
                creation = {"project": {
                    "id": "mcp-project", "name": "Protocol acceptance", "sequence_id": "main",
                    "width": 640, "height": 360, "fps": {"numerator": 30, "denominator": 1},
                }, "idempotency_key": "create-mcp"}
                created = tool("opencut_project_create", creation)
                assert not created.get("isError"), created
                snapshot = created["structuredContent"]["data"]
                preflight_input = {
                    "project_id": "mcp-project", "revision": snapshot["revision"],
                    "sequence_id": "missing", "container": "mp4", "video_codec": "h264",
                }
                preflight = tool("opencut_render_preflight", preflight_input)
                assert not preflight.get("isError"), preflight
                report = preflight["structuredContent"]["data"]
                assert report["revision"] == snapshot["revision"] and report["ready"] is False
                assert report["issues"][0]["code"] == "SEQUENCE_NOT_FOUND", report
                preflight_file = Path(root) / "preflight.json"
                preflight_file.write_text(json.dumps(preflight_input))
                cli_preflight = json.loads(subprocess.check_output([
                    str(binary), "--database", str(db), "--json", "render", "preflight",
                    "--input", str(preflight_file),
                ]))
                assert cli_preflight["data"] == report, cli_preflight
                repeated = tool("opencut_project_create", creation)
                assert repeated["structuredContent"]["data"] == snapshot
                edit = {"project_id": "mcp-project", "request": {
                    "expected_revision": snapshot["revision"], "idempotency_key": "add-track",
                    "label": "Create video track", "operations": [{
                        "type": "track.create", "sequence_id": "main", "track": {
                            "id": "video", "name": "Video", "kind": "video", "enabled": True,
                            "muted": False, "locked": False, "clips": [], "params": {},
                        },
                    }],
                }}
                planned = tool("opencut_edit_plan", edit)
                assert not planned.get("isError"), planned
                before_apply = tool("opencut_project_inspect", {"project_id": "mcp-project"})
                assert before_apply["structuredContent"]["data"] == snapshot
                applied = tool("opencut_edit_apply", edit)
                assert not applied.get("isError"), applied
                repeated = tool("opencut_edit_apply", edit)
                assert repeated["structuredContent"]["data"] == applied["structuredContent"]["data"]
                project_frame_request = {
                    "kind": "preview.project_frame",
                    "idempotency_key": "mcp-project-frame",
                    "input": {
                        "project_id": "mcp-project",
                        "revision": snapshot["revision"],
                        "sequence_id": "main",
                        "requested_time": {"ticks": "0", "timebase": 30},
                        "alignment": "nearest",
                    },
                }
                project_frame = tool("opencut_job_start", project_frame_request)
                assert not project_frame.get("isError"), project_frame
                project_frame_job = project_frame["structuredContent"]["data"]
                assert project_frame_job["kind"] == "preview.project_frame"
                assert project_frame_job["input"]["snapshot"] == snapshot
                repeated_frame = tool("opencut_job_start", project_frame_request)
                assert repeated_frame["structuredContent"]["data"]["job_id"] == project_frame_job["job_id"]
                frame_readback = json.loads(subprocess.check_output([
                    str(binary), "--database", str(db), "--json", "job", "get",
                    project_frame_job["job_id"],
                ]))
                assert frame_readback["data"]["input"]["snapshot"]["revision"] == snapshot["revision"]
                assert frame_readback["data"]["input"]["sequence_id"] == "main"
                direct_request = json.loads(json.dumps(project_frame_request))
                direct_request["kind"] = "preview.composited_frame"
                direct_request["idempotency_key"] = "mcp-composited-frame"
                direct = tool("opencut_job_start", direct_request)
                assert not direct.get("isError"), direct
                direct_job = direct["structuredContent"]["data"]
                assert direct_job["kind"] == "preview.composited_frame", direct_job
                assert direct_job["input"]["snapshot"] == snapshot, direct_job
                assert direct_job["input"]["asset_lock"]["assets"] == []
                direct_retry = tool("opencut_job_start", direct_request)
                assert direct_retry["structuredContent"]["data"]["job_id"] == direct_job["job_id"]
                cancelled_direct = tool("opencut_job_cancel", {"job_id": direct_job["job_id"]})
                assert cancelled_direct["structuredContent"]["data"]["state"] == "cancelled"
                range_request = {
                    "kind": "preview.project_range",
                    "idempotency_key": "mcp-project-range",
                    "input": {
                        "project_id": "mcp-project",
                        "revision": snapshot["revision"],
                        "sequence_id": "main",
                        "requested_range": {
                            "start": {"ticks": "0", "timebase": 30},
                            "duration": {"ticks": "30", "timebase": 30},
                        },
                        "alignment": "exact",
                    },
                }
                project_range = tool("opencut_job_start", range_request)
                assert not project_range.get("isError"), project_range
                range_job = project_range["structuredContent"]["data"]
                assert range_job["kind"] == "preview.project_range", range_job
                assert range_job["input"]["snapshot"] == snapshot, range_job
                assert range_job["input"]["requested_range"] == range_request["input"]["requested_range"]
                range_retry = tool("opencut_job_start", range_request)
                assert range_retry["structuredContent"]["data"]["job_id"] == range_job["job_id"]
                invalid_range = json.loads(json.dumps(range_request))
                invalid_range["idempotency_key"] = "mcp-project-range-invalid"
                invalid_range["input"]["requested_range"]["duration"]["ticks"] = "0"
                rejected_range = tool("opencut_job_start", invalid_range)
                assert rejected_range["isError"] is True, rejected_range
                assert rejected_range["structuredContent"]["error"]["code"] == "INVALID_ARGUMENT"
                cancelled_range = tool("opencut_job_cancel", {"job_id": range_job["job_id"]})
                assert cancelled_range["structuredContent"]["data"]["state"] == "cancelled"
                range_readback = json.loads(subprocess.check_output([
                    str(binary), "--database", str(db), "--json", "job", "get",
                    range_job["job_id"],
                ]))
                assert range_readback["data"]["state"] == "cancelled", range_readback
                assert range_readback["data"]["input"] == range_job["input"], range_readback
                invalid_frame = json.loads(json.dumps(project_frame_request))
                invalid_frame["idempotency_key"] = "mcp-project-frame-invalid"
                invalid_frame["input"]["requested_time"]["ticks"] = "-1"
                rejected_frame = tool("opencut_job_start", invalid_frame)
                assert rejected_frame["isError"] is True, rejected_frame
                assert rejected_frame["structuredContent"]["error"]["code"] == "INVALID_ARGUMENT"
                stale = json.loads(json.dumps(edit))
                stale["request"]["idempotency_key"] = "different-edit"
                rejected = tool("opencut_edit_apply", stale)
                assert rejected["isError"] is True
                assert rejected["structuredContent"]["error"]["code"] == "REVISION_CONFLICT"
                readback = json.loads(subprocess.check_output([
                    str(binary), "--database", str(db), "--json", "project", "inspect", "mcp-project",
                ]))
                assert readback["data"]["revision"] == applied["structuredContent"]["data"]["revision"]
                assert len(readback["data"]["project"]["sequences"][0]["tracks"]) == 1
                history=tool("opencut_project_history",{"project_id":"mcp-project","limit":1})["structuredContent"]["data"]
                assert history["revisions"][0]["revision"]==applied["structuredContent"]["data"]["revision"]
                assert history["next_cursor"]==snapshot["revision"]
                historical=tool("opencut_project_inspect",{"project_id":"mcp-project","revision":snapshot["revision"]})
                assert historical["structuredContent"]["data"]["kind"]=="historical"
                assert historical["structuredContent"]["data"]["snapshot"]["revision"]==snapshot["revision"]
                restoration={"project_id":"mcp-project","request":{"expected_revision":history["head_revision"],"target_revision":snapshot["revision"],"idempotency_key":"restore-initial","label":"Restore initial contents"}}
                restored=tool("opencut_project_restore",restoration)
                assert not restored.get("isError"),restored
                assert tool("opencut_project_restore",restoration)["structuredContent"]["data"]==restored["structuredContent"]["data"]
                after_restore=tool("opencut_project_inspect",{"project_id":"mcp-project"})["structuredContent"]["data"]
                assert after_restore["project"]==snapshot["project"] and after_restore["revision"]!=snapshot["revision"]
                restoration["request"]["label"]="Different request with same key"
                rejected=tool("opencut_project_restore",restoration)
                assert rejected["structuredContent"]["error"]["code"]=="IDEMPOTENCY_KEY_REUSED",rejected
                queued = tool("opencut_job_start", {
                    "kind": "media.probe", "idempotency_key": "mcp-probe",
                    "input": {"input": "awaiting-worker.mp4"},
                })["structuredContent"]["data"]
                job_id = queued["job_id"]
                job_events = tool("opencut_job_events", {"job_id": job_id})["structuredContent"]["data"]
                assert len(job_events) == 1 and job_events[0]["event_type"] == "queued", job_events
                cancelled = tool("opencut_job_cancel", {"job_id": job_id})
                assert cancelled["structuredContent"]["data"]["state"] == "cancelled", cancelled
                resumed = tool("opencut_job_events", {
                    "job_id": job_id, "after_sequence": job_events[0]["sequence"], "limit": 1,
                })["structuredContent"]["data"]
                assert len(resumed) == 1 and resumed[0]["event_type"] == "cancelled", resumed
                assert resumed[0]["sequence"] > job_events[0]["sequence"]
                invalid_cursor = tool("opencut_job_events", {"after_sequence": -1})
                assert invalid_cursor["isError"] is True, invalid_cursor
                assert invalid_cursor["structuredContent"]["error"]["code"] == "INVALID_ARGUMENT"
                return {"protocol": initialized["protocolVersion"], "tools": len(listed["tools"]),
                        "checks": ["MCP malformed media metadata repair and independent CLI readback", "schema discovery", "typed job kind schema registry", "create", "idempotent retry", "dry plan",
                                   "typed render preflight and CLI parity",
                                   "atomic apply", "stale revision rejection", "independent CLI readback",
                                   "project-frame schema and fixed revision job", "project-frame retry and invalid time",
                                   "composited-frame fixed revision, idempotency and queued cancellation",
                                   "project-range fixed revision, idempotency, invalid request, queued cancellation and CLI readback",
                                   "immutable history and snapshot", "restore as new revision", "restore retry and key conflict",
                                   "queued/cancelled job event cursor and validation"]}
            except Exception:
                diagnostics.seek(0)
                print(diagnostics.read().decode(errors="replace"), file=__import__("sys").stderr)
                raise
            finally:
                selector.close()
                process.stdin.close()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve()), indent=2))
