#!/usr/bin/env python3
"""Exercise a real WASM planner through the public CLI, without bypassing contracts."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile


def verify(binary, component):
    assert component.is_file(), f"Build the planner-guest for wasm32-wasip2 first: {component}"
    with tempfile.TemporaryDirectory(prefix="opencut-plugin-") as directory:
        root = Path(directory)
        package = root / "package"
        package.mkdir()
        shutil.copyfile(component, package / "planner.wasm")
        (package / "input.json").write_text(json.dumps({"type": "object", "properties": {"track_id": {"type": "string"}, "fault": {"type": "string"}}, "required": ["track_id"], "additionalProperties": False}))
        (package / "output.json").write_text(json.dumps({"type": "array"}))
        permissions = ["project.snapshot.read", "edit.plan.propose"]
        manifest = {
            "manifest_version": 1, "id": "org.example.planner", "version": "1.0.0",
            "display_name": "Real component acceptance", "plugin_api": ">=1.0.0, <2.0.0",
            "runtime": {"kind": "wasm-component", "entry": "planner.wasm", "world": "opencut:planner/planner@1.0.0"},
            "targets": ["native"], "permissions": permissions,
            "contributes": {"operations": [{"name": "org.example.planner.plan", "version": 1, "kind": "job", "input_schema": "input.json", "output_schema": "output.json"}]},
            "limits": {"memory_mib": 32, "wall_time_ms": 2000, "output_bytes": 65536, "fuel": 10000000},
        }
        (package / "manifest.json").write_text(json.dumps(manifest))
        trace = []

        def tool(name, arguments, error=None):
            result = subprocess.run([str(binary), "--database", str(root / "editor.sqlite"), "--json", "call", name, "--input", "-"],
                                    input=json.dumps(arguments), text=True, capture_output=True, timeout=30)
            output = json.loads(result.stdout)
            trace.append({"tool": name, "arguments": arguments, "response": output})
            if error:
                assert output["error"]["code"] == error, output
                assert result.returncode != 0
                return output["error"]
            else:
                assert result.returncode == 0 and output["error"] is None, output
            return output["data"]

        project = tool("opencut_project_create", {"project": {"id": "plugin-project", "name": "Plugin acceptance", "sequence_id": "main", "width": 160, "height": 90, "fps": {"numerator": 10, "denominator": 1}}, "idempotency_key": "create"})
        duplicate=copy.deepcopy(manifest)
        duplicate["contributes"]["operations"].append(copy.deepcopy(duplicate["contributes"]["operations"][0]))
        (package / "manifest.json").write_text(json.dumps(duplicate))
        tool("opencut_plugin_install", {"package_root": str(package), "granted_permissions": permissions}, "INVALID_PLUGIN")
        unavailable=copy.deepcopy(manifest)
        unavailable["permissions"].append("analysis.read")
        (package / "manifest.json").write_text(json.dumps(unavailable))
        tool("opencut_plugin_install", {"package_root": str(package), "granted_permissions": permissions}, "CAPABILITY_UNAVAILABLE")
        assert tool("opencut_plugin_list", {})==[]
        (package / "manifest.json").write_text(json.dumps(manifest))
        installed = tool("opencut_plugin_install", {"package_root": str(package), "granted_permissions": permissions})
        assert len(installed["package_sha256"]) == 64
        assert len(tool("opencut_plugin_list", {})) == 1
        plan_args = {"id": manifest["id"], "version": "1.0.0", "operation_name": "org.example.planner.plan", "project_id": "plugin-project", "revision": project["revision"], "parameters": {"track_id": "generated"}}
        def stored_state():
            with sqlite3.connect(root / "editor.sqlite") as connection:
                return {table: connection.execute(f"SELECT * FROM {table} ORDER BY rowid").fetchall()
                        for table in ["projects", "revisions", "editor_transactions", "idempotency", "outbox"]}

        before = stored_state()
        for fault in ["invalid_json", "invalid_operation", "missing_sequence"]:
            error = tool("opencut_plugin_plan", {**plan_args, "parameters": {"track_id":"generated", "fault":fault}}, "INVALID_PLUGIN")
            if fault == "missing_sequence":
                assert error["details"]["plugin_id"] == manifest["id"], error
                assert error["details"]["cause"]["code"] == "NOT_FOUND", error
            assert tool("opencut_project_inspect", {"project_id":"plugin-project"})==project
            assert stored_state() == before, fault
        plan = tool("opencut_plugin_plan", plan_args)
        assert plan["operations"][0]["track"]["id"] == "generated", plan
        assert plan["plan"]["base_revision"] == project["revision"]
        assert tool("opencut_project_inspect", {"project_id": "plugin-project"}) == project
        assert stored_state() == before
        request = {"project_id": "plugin-project", "request": {"expected_revision": project["revision"], "idempotency_key": "apply-plan", "label": "Apply verified plugin plan", "operations": plan["operations"]}}
        applied = tool("opencut_edit_apply", request)
        assert tool("opencut_edit_apply", request) == applied
        request["request"]["idempotency_key"] = "stale-plan"
        tool("opencut_edit_apply", request, "REVISION_CONFLICT")
        tool("opencut_plugin_plan", {**plan_args, "parameters": {"unexpected": True}}, "INVALID_ARGUMENT")
        readback = tool("opencut_project_inspect", {"project_id": "plugin-project"})
        assert readback["project"]["sequences"][0]["tracks"][0]["id"] == "generated"
        historical = tool("opencut_plugin_plan", plan_args)
        assert historical == plan, historical
        assert tool("opencut_project_inspect", {"project_id": "plugin-project"}) == readback
        return {"status": "passed", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "component_sha256": hashlib.sha256(component.read_bytes()).hexdigest(), "trace": trace,
                "checks": ["duplicate operation rejected", "unimplemented analysis permission rejected", "invalid JSON and invalid operations attributed to plugin without project mutation", "semantic invalid operations attributed to plugin with structured cause", "failed and successful planning preserve revisions receipts transactions and events", "real component install and digest", "schema validation", "WASM execution", "plan without mutation", "transactional apply", "idempotent retry", "stale plan rejection", "independent readback", "historical revision planning after head changes"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    parser.add_argument("--component", type=Path, default=Path("crates/plugin-host/examples/planner-guest/target/wasm32-wasip2/release/opencut_example_planner_guest.wasm"))
    args = parser.parse_args()
    print(json.dumps(verify(args.binary.resolve(), args.component.resolve()), indent=2))
