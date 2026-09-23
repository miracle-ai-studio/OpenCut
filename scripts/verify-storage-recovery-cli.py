#!/usr/bin/env python3
"""CLI owner recovery when managed SQLite cannot open under a bad capacity floor."""
import argparse
from contextlib import closing
import hashlib
import json
import platform
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def verify(binary: Path, root: Path) -> dict:
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise ValueError("Refusing to reuse a storage recovery acceptance directory")
    workspace = root / "workspace"
    database = workspace / "workspace.sqlite"
    trace = []
    checks = []

    def call(db: Path, args: list[str], request: dict | None = None,
             error: str | None = None, credential: Path | None = None) -> dict:
        command = [str(binary), "--database", str(db), "--json"]
        if credential:
            command.extend(["--credential-file", str(credential)])
        command.extend(args)
        process = subprocess.run(command, input=json.dumps(request or {}), text=True,
                                 capture_output=True, timeout=40)
        envelope = json.loads(process.stdout)
        trace.append({"command": args, "exit_code": process.returncode,
                      "error": envelope.get("error"), "request_id": envelope["request_id"]})
        if error:
            assert process.returncode and envelope["error"]["code"] == error, (envelope, process.stderr)
        else:
            assert process.returncode == 0 and envelope["error"] is None, (envelope, process.stderr)
        return envelope["data"]

    def tool(db: Path, name: str, request: dict | None = None,
             error: str | None = None) -> dict:
        return call(db, ["call", name, "--input", "-"], request, error)

    def recovery(db: Path, action: str, request: dict | None = None,
                 error: str | None = None, credential: Path | None = None) -> dict:
        args = ["storage", "recovery", action]
        if action == "configure":
            args.extend(["--input", "-"])
        return call(db, args, request, error, credential)

    try:
        created = tool(database, "opencut_project_create", {
            "project": {"id": "recovery", "name": "Recovery", "sequence_id": "main",
                        "width": 64, "height": 48,
                        "fps": {"numerator": 10, "denominator": 1}},
            "idempotency_key": "create",
        })
        revision = created["revision"]
        with closing(sqlite3.connect(database)) as connection:
            before = list(connection.iterdump())
        initial = recovery(database, "inspect")
        assert initial["policy"]["revision"] == 0
        bad_policy = {
            "expected_revision": 0, "idempotency_key": "unreachable-floor",
            "payload_limit_bytes": str(64 << 30),
            "minimum_available_bytes": str((1 << 63) - 1),
        }
        applied = tool(database, "opencut_storage_budget_configure", bad_policy)
        assert applied["policy"]["revision"] == 1
        # A crashed direct SQLite writer leaves a committed WAL that must
        # rebuild its SHM file. This is an external-writer recovery fixture,
        # not a claim that unmanaged writes obey OpenCut admission.
        subprocess.run([
            sys.executable, "-c",
            "import os,sqlite3,sys; c=sqlite3.connect(sys.argv[1]); "
            "c.execute('PRAGMA wal_autocheckpoint=0'); "
            "c.execute('PRAGMA user_version=123'); os._exit(0)",
            str(database),
        ], check=True, timeout=10)
        wal = Path(str(database) + "-wal")
        assert wal.is_file() and wal.stat().st_size > 0
        Path(str(database) + "-shm").unlink(missing_ok=True)
        tool(database, "opencut_storage_budget_inspect", error="DATABASE_CAPACITY_EXHAUSTED")
        tool(database, "opencut_project_inspect", {"project_id": "recovery"},
             error="DATABASE_CAPACITY_EXHAUSTED")
        native = recovery(database, "inspect")
        assert native["policy"] == applied["policy"]
        assert native["last_policy_change"] == applied
        restricted = root / "restricted.secret"
        restricted.write_text("unread-by-recovery")
        recovery(database, "inspect", error="NOT_AUTHORIZED", credential=restricted)
        restored_policy = {
            "expected_revision": 1, "idempotency_key": "restore-floor",
            "payload_limit_bytes": str(64 << 30), "minimum_available_bytes": "0",
        }
        recovery(database, "configure", restored_policy,
                 error="NOT_AUTHORIZED", credential=restricted)
        restored = recovery(database, "configure", restored_policy)
        assert restored["policy"]["revision"] == 2
        assert recovery(database, "configure", restored_policy) == restored
        assert tool(database, "opencut_storage_budget_inspect")["policy"] == restored["policy"]
        assert tool(database, "opencut_project_inspect", {"project_id": "recovery"})["revision"] == revision
        with closing(sqlite3.connect(database)) as connection:
            assert list(connection.iterdump()) == before
            assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        checks.append("An unreachable margin blocks normal SQLite tools; local owner native inspect and exact policy replay restore access without changing SQL rows or project head")
        checks.append("Credential-bearing recovery is rejected before SQLite authentication and cannot alter the ledger")

        small_volume = None
        if platform.system() == "Darwin":
            image = root / "small-volume.dmg"
            mount = root / "small-volume"
            mount.mkdir()
            attached = False
            try:
                subprocess.run(["hdiutil", "create", "-size", "64m", "-fs", "HFS+",
                                "-volname", "OpenCutRepairBootstrapQA", str(image)],
                               check=True, capture_output=True, text=True, timeout=40)
                subprocess.run(["hdiutil", "attach", "-nobrowse", "-mountpoint",
                                str(mount), str(image)], check=True, capture_output=True,
                               text=True, timeout=40)
                attached = True
                small_db = mount / "workspace.sqlite"
                configured = recovery(small_db, "configure", {
                    "expected_revision": 0, "idempotency_key": "small-volume-bootstrap",
                    "payload_limit_bytes": str(64 << 30), "minimum_available_bytes": "0",
                })
                assert configured["policy"]["revision"] == 1
                small_project = tool(small_db, "opencut_project_create", {
                    "project": {"id": "small", "name": "Small", "sequence_id": "main",
                                "width": 64, "height": 48,
                                "fps": {"numerator": 10, "denominator": 1}},
                    "idempotency_key": "create",
                })
                assert tool(small_db, "opencut_project_inspect", {"project_id": "small"}) == small_project
                with closing(sqlite3.connect(small_db)) as connection:
                    assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
                small_volume = "64 MiB HFS+ bootstrap and project reopen passed"
                checks.append("Owner configures a 64 MiB independent volume before SQLite initialization; real managed project create, reopen and integrity check pass")
            finally:
                if attached:
                    detached = subprocess.run(["hdiutil", "detach", str(mount)],
                                              capture_output=True, text=True, timeout=40)
                    assert detached.returncode == 0, (detached.stdout, detached.stderr)

        report = {"status": "passed", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "checks": checks, "small_volume": small_volume,
                  "scope": "local OS-owner policy recovery and 64 MiB bootstrap; no preallocated repair reserve or disk-full repair transaction"}
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
    finally:
        (root / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/opencut")
    parser.add_argument("--artifacts-dir", type=Path)
    args = parser.parse_args()
    root = args.artifacts_dir or Path(tempfile.mkdtemp(prefix="opencut-storage-recovery-cli-"))
    print(json.dumps(verify(args.binary.resolve(), root.resolve()), indent=2))
