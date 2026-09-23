#!/usr/bin/env python3
"""Bounded native QA for the unfinished SQLite/WAL admission boundary.

This is an audit, not a passing R2 gate: it records whether a configured
payload budget also stops control-plane SQL writes while a reader pins WAL.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile


def verify(binary: Path, root: Path) -> dict:
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise ValueError("Refusing to reuse a non-empty database admission audit directory")
    database = root / "workspace.sqlite"
    trace: list[dict] = []
    reader = None

    def call(name: str, request: dict | None = None) -> dict:
        process = subprocess.run(
            [str(binary), "--database", str(database), "--json", "call", name, "--input", "-"],
            input=json.dumps(request or {}), text=True, capture_output=True, timeout=30,
        )
        envelope = json.loads(process.stdout)
        trace.append({"tool": name, "exit_code": process.returncode,
                      "error": envelope.get("error"), "request_id": envelope.get("request_id")})
        if process.returncode and not envelope.get("error"):
            raise AssertionError((name, process.stderr, envelope))
        return envelope

    try:
        created = call("opencut_project_create", {"project": {
            "id": "wal-pressure", "name": "WAL pressure", "sequence_id": "main",
            "width": 64, "height": 48, "fps": {"numerator": 10, "denominator": 1},
        }, "idempotency_key": "create"})
        assert created["error"] is None, created
        initial = created["data"]
        baseline = call("opencut_database_inspect")["data"]
        assert baseline["journal_mode"] == "wal" and not baseline["enforces_total_storage_limit"]
        policy = call("opencut_storage_budget_configure", {
            "expected_revision": 0, "idempotency_key": "deny-payload",
            "payload_limit_bytes": "0", "minimum_available_bytes": str((1 << 63) - 1),
        })
        assert policy["error"] is None, policy
        budget = call("opencut_storage_budget_inspect")["data"]
        assert budget["admission_headroom_bytes"] == "0"
        assert not budget["enforces_all_workspace_writes"]

        reader = sqlite3.connect(database)
        reader.execute("BEGIN")
        reader.execute("SELECT COUNT(*) FROM projects").fetchone()
        revision = initial["revision"]
        committed = 0
        refused = None
        for index in range(4):
            track = {"id": f"track-{index}", "name": f"control-write-{index}-" + "x" * (256 << 10),
                     "kind": "video", "enabled": True, "muted": False,
                     "locked": False, "clips": [], "params": {}}
            result = call("opencut_edit_apply", {"project_id": "wal-pressure", "request": {
                "expected_revision": revision, "idempotency_key": f"edit-{index}",
                "label": "WAL admission audit",
                "operations": [{"type": "track.create", "sequence_id": "main", "track": track}],
            }})
            if result["error"] is not None:
                refused = result["error"]["code"]
                break
            committed += 1
            revision = result["data"]["revision"]

        observed = call("opencut_database_inspect")["data"]
        assert observed["journal_mode"] == "wal"
        assert observed["enforces_total_storage_limit"] is False
        assert call("opencut_storage_budget_inspect")["data"]["admission_headroom_bytes"] == "0"
        if committed:
            assert int(observed["wal_file_bytes"]) > int(baseline["wal_file_bytes"])
        blocked = call("opencut_database_checkpoint", {"mode": "truncate", "busy_timeout_ms": 25})
        assert blocked["error"] is None and blocked["data"]["outcome"] == "blocked", blocked
        reader.rollback()
        reader.close()
        reader = None
        recovered = call("opencut_database_checkpoint", {"mode": "truncate"})
        assert recovered["error"] is None and recovered["data"]["outcome"] == "complete", recovered
        assert recovered["data"]["observed_wal_file_bytes"] == "0"
        final = call("opencut_project_inspect", {"project_id": "wal-pressure"})
        assert final["error"] is None and final["data"]["revision"] == revision

        report = {"status": "gap_confirmed" if committed else "protected_or_refused",
                  "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "payload_headroom_bytes": budget["admission_headroom_bytes"],
                  "committed_control_edits": committed, "first_refusal_code": refused,
                  "wal_before_bytes": baseline["wal_file_bytes"],
                  "wal_pinned_bytes": observed["wal_file_bytes"],
                  "blocked_checkpoint": blocked["data"],
                  "recovered_checkpoint": recovered["data"],
                  "final_revision": revision, "evidence": str(root),
                  "scope": "bounded current-host audit; not a SQLite/WAL admission implementation"}
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))
        return report
    finally:
        if reader is not None:
            reader.close()
        (root / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path)
    args = parser.parse_args()
    verify(args.binary.resolve(), (args.artifacts_dir or Path(tempfile.mkdtemp(prefix="opencut-database-control-gap-"))).resolve())
