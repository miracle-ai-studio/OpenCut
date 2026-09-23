#!/usr/bin/env python3
"""Exercise owner control policy without implying whole-workspace enforcement."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def verify(binary: Path, workspace: Path, legacy_baseline: bool = False) -> dict:
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
    if legacy_baseline:
        assert before.get("control") is None
        assert not (workspace / ".storage-budget").exists()
        limit = 4096
    else:
        assert before["control"]["database_name"] == database.name
        limit = (int(before["control"]["observed_control_bytes"])
                 + int(before["control"]["reserved_control_bytes"]) + 1024 * 1024)

    first = {
        "expected_revision": 0,
        "idempotency_key": "control-policy",
        "payload_limit_bytes": "200",
        "minimum_available_bytes": "0",
        "control_limit_bytes": str(limit),
    }
    change = call("opencut_storage_budget_configure", first)
    assert change["control_limit_bytes"] == str(limit)
    assert call("opencut_storage_budget_configure", first) == change

    configured = call("opencut_storage_budget_inspect", {})
    assert configured["control"]["control_limit_bytes"] == str(limit)
    if legacy_baseline:
        assert configured["control"]["database_name"] is None
        assert configured["control"]["admission_headroom_bytes"] is None
        assert configured["control"]["reservations"] == []
    else:
        assert configured["control"]["database_name"] == database.name
    assert configured["control"]["write_interception_enabled"] is False

    call("opencut_storage_budget_configure", {
        "expected_revision": 1,
        "idempotency_key": "old-style-policy",
        "payload_limit_bytes": "300",
        "minimum_available_bytes": "0",
    })
    database_bytes = None
    if legacy_baseline:
        created = call("opencut_project_create", {
            "project": {
                "id": "control-gap-project", "name": "Control gap",
                "sequence_id": "main", "width": 64, "height": 48,
                "fps": {"numerator": 10, "denominator": 1},
            },
            "idempotency_key": "control-gap-project",
        })
        assert created["project"]["id"] == "control-gap-project"
        database_bytes = database.stat().st_size
        assert database_bytes > 4096
    final = call("opencut_storage_budget_inspect", {})
    assert final["control"]["control_limit_bytes"] == str(limit)
    assert final["enforces_all_workspace_writes"] is False
    ledger = json.loads((workspace / ".storage-budget/ledger.json").read_text())
    assert ledger["version"] == 2 and ledger["control_limit_bytes"] == str(limit)

    report = {
        "status": "passed",
        "scope": ("Historical ledger-only baseline" if legacy_baseline
                  else "Public CLI control policy; VFS acceptance is separate"),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "checks": [
            ("Read-only inspection does not create the budget ledger" if legacy_baseline
             else "Managed inspect binds the workspace database to the control ledger"),
            "Owner control policy persists with exact idempotency replay",
            "Old-style reconfiguration preserves the independent control limit",
            *(["A real SQLite project commit still succeeds beyond that limit"] if legacy_baseline else []),
            "Report explicitly leaves whole-workspace enforcement disabled",
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
    parser.add_argument("--legacy-baseline", action="store_true")
    args = parser.parse_args()
    workspace = args.workspace or Path(tempfile.mkdtemp(prefix="opencut-control-policy-"))
    result = verify(args.binary.resolve(), workspace.resolve(), args.legacy_baseline)
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
