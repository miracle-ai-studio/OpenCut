#!/usr/bin/env python3
"""Exercise the owner-facing control policy without implying SQLite enforcement."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def verify(binary: Path, workspace: Path) -> dict:
    workspace.mkdir(parents=True, exist_ok=True)
    database = workspace / "workspace.sqlite"
    trace = []

    def call(name: str, data: dict) -> dict:
        command = [str(binary), "--database", str(database), "--json", "call", name, "--input", "-"]
        process = subprocess.run(command, input=json.dumps(data), text=True, capture_output=True, timeout=30)
        response = json.loads(process.stdout)
        trace.append({"tool": name, "input": data, "exit_code": process.returncode, "response": response})
        assert process.returncode == 0 and response.get("error") is None, (response, process.stderr)
        return response["data"]

    before = call("opencut_storage_budget_inspect", {})
    assert before.get("control") is None
    assert not (workspace / ".storage-budget").exists()

    first = {
        "expected_revision": 0,
        "idempotency_key": "control-policy",
        "payload_limit_bytes": "200",
        "minimum_available_bytes": "0",
        "control_limit_bytes": "4096",
    }
    change = call("opencut_storage_budget_configure", first)
    assert change["control_limit_bytes"] == "4096"
    assert call("opencut_storage_budget_configure", first) == change

    configured = call("opencut_storage_budget_inspect", {})
    assert configured["control"]["control_limit_bytes"] == "4096"
    assert configured["control"]["database_name"] is None
    assert configured["control"]["admission_headroom_bytes"] is None
    assert configured["control"]["reservations"] == []
    assert configured["control"]["write_interception_enabled"] is False

    call("opencut_storage_budget_configure", {
        "expected_revision": 1,
        "idempotency_key": "old-style-policy",
        "payload_limit_bytes": "300",
        "minimum_available_bytes": "0",
    })
    created = call("opencut_project_create", {
        "project": {
            "id": "control-gap-project",
            "name": "Control gap",
            "sequence_id": "main",
            "width": 64,
            "height": 48,
            "fps": {"numerator": 10, "denominator": 1},
        },
        "idempotency_key": "control-gap-project",
    })
    assert created["project"]["id"] == "control-gap-project"
    database_bytes = database.stat().st_size
    assert database_bytes > 4096
    final = call("opencut_storage_budget_inspect", {})
    assert final["control"]["control_limit_bytes"] == "4096"
    assert final["enforces_all_workspace_writes"] is False
    ledger = json.loads((workspace / ".storage-budget/ledger.json").read_text())
    assert ledger["version"] == 2 and ledger["control_limit_bytes"] == "4096"

    report = {
        "status": "passed",
        "scope": "Public CLI policy and report; SQLite write interception is not implemented",
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "checks": [
            "Read-only inspection does not create the budget ledger",
            "Owner control policy persists with exact idempotency replay",
            "Old-style reconfiguration preserves the independent control limit",
            "A real SQLite project commit still succeeds beyond that limit; write interception remains the next gate",
            "Report explicitly leaves write interception and whole-workspace enforcement disabled",
        ],
        "ledger_version": ledger["version"],
        "database_bytes_after_project_commit": database_bytes,
        "policy_revision": final["policy"]["revision"],
        "control": final["control"],
        "enforces_all_workspace_writes": final["enforces_all_workspace_writes"],
    }
    (workspace / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/opencut")
    parser.add_argument("--workspace", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    workspace = args.workspace or Path(tempfile.mkdtemp(prefix="opencut-control-policy-"))
    result = verify(args.binary.resolve(), workspace.resolve())
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
