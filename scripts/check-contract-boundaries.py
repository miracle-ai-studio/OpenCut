#!/usr/bin/env python3
"""Reject dependencies that would pull native execution into public contracts/SDK."""
import json
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--locked"], cwd=root, text=True, stderr=subprocess.PIPE))
packages = {p["id"]: p for p in metadata["packages"]}
nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
forbidden = {
    "opencut-media", "opencut-asset-store", "opencut-editor-application", "opencut-automation",
    "opencut-job-runtime", "opencut-job-execution", "opencut-plugin-host", "opencut-project-render",
    "opencut-host-runtime", "opencut-sqlite-store", "opencut-native-services", "opencut-storage-budget", "rusqlite", "wasmtime", "cosmic-text", "image", "gpui",
}
contracts = ["opencut-contracts", "opencut-media-contracts", "opencut-job-contracts", "opencut-api-contracts"]
checked = {}
for name in contracts + ["opencut-sdk"]:
    package_id = next(i for i, p in packages.items() if p["name"] == name)
    pending, reached = [package_id], set()
    while pending:
        current = pending.pop()
        if current in reached:
            continue
        reached.add(current)
        pending.extend(nodes[current]["dependencies"])
    dependencies = {packages[i]["name"] for i in reached}
    violations = dependencies & forbidden
    assert not violations, f"{name} depends on native implementation: {sorted(violations)}"
    if name in contracts:
        source = Path(packages[package_id]["manifest_path"]).parent / "src"
        for path in source.rglob("*.rs"):
            # DTOs may parameterize locations; they may not choose native paths or execute I/O.
            assert not re.search(r'\bPathBuf\b|\bstd::(?:fs|process|net|path)::', path.read_text()), path
    checked[name] = len(dependencies)
# Application orchestration must not gain native renderer/worker dependencies.
application_id = next(i for i,p in packages.items() if p["name"] == "opencut-editor-application")
pending, reached = [application_id], set()
while pending:
    current = pending.pop()
    if current in reached:
        continue
    reached.add(current)
    pending.extend(nodes[current]["dependencies"])
application_forbidden = {"opencut-sqlite-store", "opencut-asset-store", "rusqlite", "tempfile", "opencut-media", "opencut-project-render", "opencut-job-runtime", "opencut-automation", "wasmtime", "cosmic-text", "image", "gpui"}
application_forbidden.update({"opencut-native-services", "opencut-storage-budget"})
assert not ({packages[i]["name"] for i in reached} & application_forbidden)
automation = next(p for p in packages.values() if p["name"] == "opencut-automation")
assert "opencut-job-execution" not in {d["name"] for d in automation["dependencies"]}, "Automation must not construct prepared jobs"
assert not ({"opencut-plugin-host", "opencut-project-render", "opencut-asset-store", "opencut-media"} &
            {d["name"] for d in automation["dependencies"]}), "Automation must use capability adapters"
native = next(p for p in packages.values() if p["name"] == "opencut-native-services")
assert not ({"opencut-sqlite-store", "rusqlite"} &
            {d["name"] for d in native["dependencies"] if d["kind"] != "dev"}), "Native capabilities must not require SQLite"
for path in (root / "crates/editor-application/src").rglob("*.rs"):
    source = path.read_text()
    assert not re.search(r'\brusqlite\b|\bopencut_asset_store\b|\bopencut_sqlite_store\b|\bPathBuf\b|\bstd::(?:fs|process|net|path)::', source), path
composition_id = next(i for i,p in packages.items() if p["name"] == "opencut-composition")
pending, composition_reached = [composition_id], set()
while pending:
    current = pending.pop()
    if current in composition_reached:
        continue
    composition_reached.add(current)
    pending.extend(nodes[current]["dependencies"])
assert not ({packages[i]["name"] for i in composition_reached} & forbidden), "Composition must be independent of native execution and application storage"
for path in (root / "crates/composition/src").rglob("*.rs"):
    assert not re.search(r'\bPathBuf\b|\bstd::(?:fs|process|net|path)::', path.read_text()), path

# Dispatcher/scheduler boundaries must stay independently instantiable, including
# transitive dependencies; moving a forbidden edge to another adapter cannot bypass this.
for boundary in ["opencut-automation", "opencut-job-runtime"]:
    package_id = next(i for i,p in packages.items() if p["name"] == boundary)
    pending, seen = [package_id], set()
    while pending:
        current = pending.pop()
        if current in seen: continue
        seen.add(current)
        pending.extend(nodes[current]["dependencies"])
    banned = {"opencut-host-runtime", "opencut-sqlite-store", "opencut-native-services", "opencut-media", "opencut-asset-store", "opencut-project-render", "opencut-storage-budget", "rusqlite"}
    assert not ({packages[i]["name"] for i in seen} & banned), f"{boundary} must use injected ports"

cli = next(p for p in packages.values() if p["name"] == "opencut-cli")
assert not ({"opencut-media", "opencut-asset-store", "opencut-native-services", "opencut-sqlite-store", "opencut-job-runtime"} & {d["name"] for d in cli["dependencies"]}), "CLI must construct the host and use typed workspace ports"
print(json.dumps({"status": "passed", "dispatcher_and_scheduler_ports": "passed", "cli_host_boundary": "passed", "dependency_closure_sizes": checked, "application_native_execution_boundary": "passed", "application_storage_boundary": "passed", "application_dependency_closure_size": len(reached), "composition_pure_boundary": "passed", "composition_dependency_closure_size": len(composition_reached)}, indent=2))
