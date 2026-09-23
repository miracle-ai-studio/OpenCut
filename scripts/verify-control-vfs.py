#!/usr/bin/env python3
"""Real CLI/WAL admission journey against the managed SQLite VFS."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def verify(binary: Path, workspace: Path) -> dict:
    workspace.mkdir(parents=True, exist_ok=True)
    if any(workspace.iterdir()):
        raise ValueError("Refusing to reuse a control VFS acceptance workspace")
    database = workspace / "workspace.sqlite"
    trace = []
    reader = None

    def call(name: str, request: dict | None = None) -> dict:
        process = subprocess.run(
            [str(binary), "--database", str(database), "--json", "call", name, "--input", "-"],
            input=json.dumps(request or {}), text=True, capture_output=True, timeout=40,
        )
        envelope = json.loads(process.stdout)
        trace.append({"tool": name, "exit_code": process.returncode,
                      "error": envelope.get("error"), "request_id": envelope.get("request_id")})
        assert bool(process.returncode) == bool(envelope.get("error")), (name, process.stderr, envelope)
        return envelope

    try:
        created = call("opencut_project_create", {
            "project": {"id": "wal-control", "name": "WAL control", "sequence_id": "main",
                        "width": 64, "height": 48,
                        "fps": {"numerator": 10, "denominator": 1}},
            "idempotency_key": "create",
        })
        assert created["error"] is None, created
        initial_revision = created["data"]["revision"]
        other_database = workspace / "other.sqlite"
        other_open = subprocess.run(
            [str(binary), "--database", str(other_database), "--json", "call",
             "opencut_project_list", "--input", "-"],
            input="{}", text=True, capture_output=True, timeout=40,
        )
        other_response = json.loads(other_open.stdout)
        trace.append({"tool": "opencut_project_list", "database": str(other_database),
                      "exit_code": other_open.returncode, "error": other_response.get("error")})
        assert other_open.returncode != 0 and other_response["error"]["code"] == "STORAGE_BUDGET_ERROR"
        assert not other_database.exists()
        budget = call("opencut_storage_budget_inspect")["data"]
        control = budget["control"]
        assert control["database_name"] == database.name
        observed = int(control["observed_control_bytes"])
        active = int(control["reserved_control_bytes"])
        tight_limit = observed + active + 128 * 1024
        configured = call("opencut_storage_budget_configure", {
            "expected_revision": 0, "idempotency_key": "tight",
            "payload_limit_bytes": str(64 << 30), "minimum_available_bytes": "0",
            "control_limit_bytes": str(tight_limit),
        })
        assert configured["error"] is None, configured

        reader = sqlite3.connect(database)
        reader.execute("BEGIN")
        reader.execute("SELECT COUNT(*) FROM projects").fetchone()
        track = {"id": "large-track", "name": "x" * (256 << 10), "kind": "video",
                 "enabled": True, "muted": False, "locked": False, "clips": [], "params": {}}
        edit = {"project_id": "wal-control", "request": {
            "expected_revision": initial_revision, "idempotency_key": "large-edit",
            "label": "WAL admission", "operations": [
                {"type": "track.create", "sequence_id": "main", "track": track}],
        }}
        denied = call("opencut_edit_apply", edit)
        assert denied["error"]["code"] == "DATABASE_CAPACITY_EXHAUSTED", denied
        unchanged = call("opencut_project_inspect", {"project_id": "wal-control"})
        assert unchanged["error"] is None and unchanged["data"]["revision"] == initial_revision
        denied_storage = call("opencut_database_inspect")["data"]
        assert denied_storage["journal_mode"] == "wal"
        assert denied_storage["managed_vfs_enabled"] is True
        assert denied_storage["enforces_total_storage_limit"] is False

        current = call("opencut_storage_budget_inspect")["data"]["control"]
        wider_limit = (int(current["observed_control_bytes"])
                       + int(current["reserved_control_bytes"]) + 2 * 1024 * 1024)
        expanded = call("opencut_storage_budget_configure", {
            "expected_revision": 1, "idempotency_key": "room",
            "payload_limit_bytes": str(64 << 30), "minimum_available_bytes": "0",
            "control_limit_bytes": str(wider_limit),
        })
        assert expanded["error"] is None, expanded
        accepted = call("opencut_edit_apply", edit)
        assert accepted["error"] is None and accepted["data"]["revision"] != initial_revision
        replay = call("opencut_edit_apply", edit)
        assert replay["error"] is None and replay["data"]["revision"] == accepted["data"]["revision"]
        final = call("opencut_project_inspect", {"project_id": "wal-control"})
        assert final["data"]["revision"] == accepted["data"]["revision"]

        pinned = call("opencut_database_checkpoint", {"mode": "truncate", "busy_timeout_ms": 25})
        assert pinned["error"] is None and pinned["data"]["outcome"] == "blocked", pinned
        reader.rollback()
        reader.close()
        reader = None
        checkpoint = call("opencut_database_checkpoint", {"mode": "truncate"})
        assert checkpoint["error"] is None and checkpoint["data"]["outcome"] == "complete", checkpoint
        with sqlite3.connect(database) as independent:
            assert independent.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
            assert independent.execute("SELECT COUNT(*) FROM editor_transactions").fetchone()[0] == 1

        result = {"status": "passed", "scope": "managed CLI and pinned-reader WAL; not whole-workspace physical quota",
                  "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "tight_control_limit_bytes": tight_limit, "expanded_control_limit_bytes": wider_limit,
                  "denial_code": denied["error"]["code"],
                  "head_unchanged_after_denial": True, "retry_same_key_committed_once": True,
                  "pinned_checkpoint": pinned["data"]["outcome"],
                  "released_checkpoint": checkpoint["data"]["outcome"],
                  "integrity_check": "ok", "checks": [
                      "Managed CLI binds the database name to the separate control ledger",
                      "A second database in the workspace is rejected before file creation",
                      "Pinned WAL growth rejects the large edit before project commit",
                      "Wider control policy permits the original idempotency key exactly once",
                      "Checkpoint is blocked by the reader, then completes after release",
                      "Independent SQLite integrity and transaction count match the public result",
                  ]}
        (workspace / "report.json").write_text(json.dumps(result, indent=2) + "\n")
        return result
    finally:
        if reader is not None:
            reader.close()
        (workspace / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/opencut")
    parser.add_argument("--workspace", type=Path)
    parser.add_argument("--artifacts-dir", dest="workspace", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    workspace = args.workspace or Path(tempfile.mkdtemp(prefix="opencut-control-vfs-"))
    report = verify(args.binary.resolve(), workspace.resolve())
    if args.output:
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
