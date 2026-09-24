#!/usr/bin/env python3
"""Real 64 MiB full-volume repair: reserve -> bounded cancel/release -> replenish."""
import argparse
from contextlib import closing
import errno
import hashlib
import json
import os
import platform
import signal
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def verify(binary: Path, root: Path) -> dict:
    if platform.system() != "Darwin":
        raise RuntimeError("This physical ENOSPC fixture requires macOS hdiutil")
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise ValueError("Refusing to reuse a physical repair fixture")
    mount = root / "volume"
    mount.mkdir()
    image = root / "disk.dmg"
    attached = False
    worker = None
    probe_pid = None
    marker_fd = None
    trace = []
    checks = []

    def command(args: list[str]) -> None:
        result = subprocess.run(args, capture_output=True, text=True, timeout=45)
        assert result.returncode == 0, (args, result.stdout, result.stderr)

    def call(args: list[str], request: dict | None = None, error: str | None = None) -> dict:
        result = subprocess.run(
            [str(binary), "--database", str(database), "--json", *args],
            input=json.dumps(request or {}), text=True, capture_output=True, timeout=45,
        )
        response = json.loads(result.stdout)
        trace.append({"command": args, "exit": result.returncode,
                      "error": response["error"], "request_id": response["request_id"]})
        if error:
            assert result.returncode and response["error"]["code"] == error, (
                response, result.stderr)
        else:
            assert not result.returncode and response["error"] is None, (
                response, result.stderr)
        return response["data"]

    def tool(name: str, request: dict | None = None, error: str | None = None) -> dict:
        return call(["call", name, "--input", "-"], request, error)

    def recovery(action: str, request: dict | None = None,
                 error: str | None = None) -> dict:
        args = ["storage", "recovery", action]
        if action in ("configure", "enter", "exit"):
            args += ["--input", "-"]
        return call(args, request, error)

    try:
        command(["hdiutil", "create", "-size", "64m", "-fs", "HFS+",
                 "-volname", "OpenCutRepairReserveQA", str(image)])
        command(["hdiutil", "attach", "-nobrowse", "-mountpoint", str(mount), str(image)])
        attached = True
        volume = os.statvfs(mount)
        assert mount.stat().st_dev != root.stat().st_dev
        assert 0 < volume.f_blocks * volume.f_frsize <= 128 << 20
        database = mount / "workspace.sqlite"
        recovery("configure", {"expected_revision": 0, "idempotency_key": "small-volume",
                               "payload_limit_bytes": str(64 << 30),
                               "minimum_available_bytes": "0"})
        project = tool("opencut_project_create", {
            "project": {"id": "repair", "name": "Repair", "sequence_id": "main",
                        "width": 64, "height": 48,
                        "fps": {"numerator": 10, "denominator": 1}},
            "idempotency_key": "create",
        })
        invalid_media = mount / "invalid-media"
        invalid_media.write_bytes(b"x")
        upload = tool("opencut_upload_create", {
            "project_id": "repair", "idempotency_key": "invalid-upload",
            "name": "invalid.wav", "size_bytes": "1",
            "content_hash": hashlib.sha256(b"x").hexdigest(),
        })
        call(["upload", "put", upload["upload_id"], "--offset", "0",
              "--input", str(invalid_media)])
        import_job = tool("opencut_upload_seal", {"upload_id": upload["upload_id"]})
        command([str(binary), "--database", str(database), "worker", "run", "--once"])
        assert tool("opencut_job_get", {"job_id": import_job["job_id"]})["state"] == "failed"
        assert tool("opencut_upload_retention", {"upload_id": upload["upload_id"]})["state"] == "retained"
        fake_tools = root / "fake-tools"
        fake_tools.mkdir()
        marker = root / "live-probe.pid"
        os.mkfifo(marker)
        marker_fd = os.open(marker, os.O_RDONLY | os.O_NONBLOCK)
        fake_probe = fake_tools / "ffprobe"
        fake_probe.write_text(
            "#!/usr/bin/env python3\n"
            "import os, sys, time\n"
            "if '-version' in sys.argv:\n"
            "    print('ffprobe version OpenCut repair fixture')\n"
            "else:\n"
            "    with open(os.environ['OPENCUT_QA_PROBE_MARKER'], 'w') as out:\n"
            "        out.write(str(os.getpid()) + '\\n')\n"
            "        out.flush()\n"
            "    time.sleep(120)\n"
        )
        fake_probe.chmod(0o755)
        running = tool("opencut_job_submit", {
            "kind": "media.probe", "idempotency_key": "running-probe",
            "input": {"input": str(invalid_media)},
        })
        assert tool("opencut_job_get", {"job_id": running["job_id"]})["state"] == "queued"
        environment = os.environ.copy()
        environment["PATH"] = str(fake_tools) + os.pathsep + environment["PATH"]
        environment["OPENCUT_QA_PROBE_MARKER"] = str(marker)
        with (root / "worker.log").open("wb") as log:
            worker = subprocess.Popen(
                [str(binary), "--database", str(database), "worker", "run", "--once"],
                env=environment, stdout=log, stderr=subprocess.STDOUT,
            )
            deadline = time.monotonic() + 15
            marker_bytes = b""
            while b"\n" not in marker_bytes:
                marker_bytes += os.read(marker_fd, 64)
                assert worker.poll() is None, (
                    "Worker exited before probe started", worker.returncode,
                    tool("opencut_job_get", {"job_id": running["job_id"]}),
                    (root / "worker.log").read_text(),
                )
                assert time.monotonic() < deadline, "Worker did not start the probe"
                time.sleep(.05)
            probe_pid = int(marker_bytes.split(b"\n", 1)[0])
            os.close(marker_fd)
            marker_fd = None
            assert tool("opencut_job_get", {"job_id": running["job_id"]})["state"] == "running"
            os.kill(worker.pid, signal.SIGSTOP)
        job = tool("opencut_job_submit", {
            "kind": "preview.project_frame", "idempotency_key": "queued",
            "input": {"project_id": "repair", "revision": project["revision"],
                      "sequence_id": "main", "requested_time": {"ticks": "0", "timebase": 10},
                      "alignment": "exact"},
        })
        ready = recovery("inspect")["control"]["repair"]
        assert ready["state"] == "ready" and ready["target_bytes"] == "8388608"
        assert ready["allocated_file_bytes"] == ready["target_bytes"]
        assert int(ready["allocated_blocks_bytes"]) >= int(ready["target_bytes"])
        reserve_path = mount / ".storage-budget" / "repair.reserve"
        assert reserve_path.stat().st_blocks * 512 >= 8 << 20
        checks.append("The managed 64 MiB workspace physically writes, fsyncs and reports an allocated 8 MiB reserve before failure")

        with closing(sqlite3.connect(database)) as connection:
            before = list(connection.iterdump())
        filler = mount / "fixture-filler"
        filled = 0
        with filler.open("wb", buffering=0) as stream:
            for size in (1 << 20, 4096, 512):
                block = b"X" * size
                while True:
                    try:
                        filled += stream.write(block)
                    except OSError as exc:
                        assert exc.errno == errno.ENOSPC, exc
                        break
            try:
                os.fsync(stream.fileno())
            except OSError as exc:
                assert exc.errno == errno.ENOSPC, exc
        available_full = os.statvfs(mount).f_bavail * os.statvfs(mount).f_frsize
        assert available_full < 4096, available_full
        recovery("enter", {"expected_revision": 0,
                           "idempotency_key": "enter-full-volume"}, "STORAGE_BUDGET_BUSY")
        worker.kill()
        worker.wait(timeout=10)
        checks.append("An active Worker connection prevents repair entry on the full volume; killing the supervisor leaves the inherited attempt lock with its live media child")
        ordinary_edit = {
            "project_id": "repair",
            "request": {"expected_revision": project["revision"],
                        "idempotency_key": "blocked-edit", "label": "blocked",
                        "operations": [{"type": "track.create", "sequence_id": "main",
                                        "track": {"id": "v", "name": "Large" * 32768,
                                                  "kind": "video", "enabled": True,
                                                  "muted": False, "locked": False,
                                                  "clips": [], "params": {}}}]},
        }
        tool("opencut_edit_apply", ordinary_edit, "DATABASE_CAPACITY_EXHAUSTED")
        checks.append("At zero external free bytes, an ordinary edit is rejected before changing the project head")

        policy_before = recovery("inspect")["policy"]
        blocked_policy = {
            "expected_revision": policy_before["revision"],
            "idempotency_key": "policy-while-full",
            "payload_limit_bytes": policy_before["payload_limit_bytes"],
            "minimum_available_bytes": policy_before["minimum_available_bytes"],
        }
        recovery("configure", blocked_policy, "STORAGE_CAPACITY_LOW")
        assert recovery("inspect")["policy"] == policy_before
        assert not (mount / ".storage-budget" / "ledger.tmp").exists()
        checks.append("A full volume rejects the admission ledger staging before changing the owner policy")

        entered = recovery("enter", {"expected_revision": 0,
                                     "idempotency_key": "enter-full-volume"})
        assert entered["state"] == "active" and entered["actor"]["source"] == "cli"
        assert reserve_path.stat().st_size == 0
        released_available = os.statvfs(mount).f_bavail * os.statvfs(mount).f_frsize
        assert released_available >= 7 << 20, released_available
        restored_policy = recovery("configure", blocked_policy)
        assert restored_policy["policy"]["revision"] == policy_before["revision"] + 1
        tool("opencut_edit_apply", ordinary_edit, "DATABASE_CAPACITY_EXHAUSTED")
        call(["storage", "recovery", "cancel", running["job_id"]],
             error="STORAGE_BUDGET_BUSY")
        os.kill(probe_pid, signal.SIGKILL)
        probe_pid = None
        deadline = time.monotonic() + 10
        while True:
            result = subprocess.run(
                [str(binary), "--database", str(database), "--json", "storage",
                 "recovery", "cancel", running["job_id"]],
                capture_output=True, text=True, timeout=45,
            )
            response = json.loads(result.stdout)
            trace.append({"command": ["storage", "recovery", "cancel", running["job_id"]],
                          "exit": result.returncode, "error": response["error"],
                          "request_id": response["request_id"]})
            if result.returncode == 0:
                assert response["data"]["state"] == "cancelled"
                break
            assert response["error"]["code"] == "STORAGE_BUDGET_BUSY", response
            assert time.monotonic() < deadline, "Orphaned probe did not release its attempt lock"
            time.sleep(.05)
        checks.append("Repair refuses to cancel a running job while its orphaned media child holds the attempt lock, then atomically cancels it after the child exits")
        cancelled = call(["storage", "recovery", "cancel", job["job_id"]])
        assert cancelled["state"] == "cancelled"
        assert call(["storage", "recovery", "cancel", job["job_id"]])["state"] == "cancelled"
        released = call(["storage", "recovery", "release", upload["upload_id"]])
        assert released["state"] == "released" and released["reserved_bytes"] == "0"
        assert call(["storage", "recovery", "release", upload["upload_id"]]) == released
        assert not (mount / "uploads" / upload["upload_id"]).exists()
        assert recovery("inspect")["control"]["repair"]["state"] == "active"
        # The extra running-job transitions may consume or free a different
        # number of filesystem blocks than the queued-only fixture. Refill the
        # external writer so premature reserve replenishment has no headroom.
        with filler.open("ab", buffering=0) as stream:
            block = b"X" * 4096
            while True:
                try:
                    filled += stream.write(block)
                except OSError as exc:
                    assert exc.errno == errno.ENOSPC, exc
                    break
            try:
                os.fsync(stream.fileno())
            except OSError as exc:
                assert exc.errno == errno.ENOSPC, exc
        assert os.statvfs(mount).f_bavail * os.statvfs(mount).f_frsize < 4096
        recovery("exit", {"expected_revision": 1,
                          "idempotency_key": "exit-after-cancel"}, "STORAGE_CAPACITY_LOW")
        assert reserve_path.stat().st_size == 0
        checks.append("The owner releases the reserve on an actually full volume, durably retries the refused policy, cancels one queued job and releases one retained upload through the repair VFS, while normal edits and premature replenishment remain blocked")

        filler.unlink()
        exited = recovery("exit", {"expected_revision": 1,
                                   "idempotency_key": "exit-after-cancel"})
        assert exited["state"] == "ready"
        ready_again = recovery("inspect")["control"]["repair"]
        assert ready_again["state"] == "ready"
        assert ready_again["allocated_file_bytes"] == ready_again["target_bytes"]
        assert int(ready_again["allocated_blocks_bytes"]) >= int(ready_again["target_bytes"])
        assert tool("opencut_project_inspect", {"project_id": "repair"}) == project
        assert tool("opencut_job_get", {"job_id": job["job_id"]})["state"] == "cancelled"
        assert tool("opencut_job_get", {"job_id": running["job_id"]})["state"] == "cancelled"
        assert tool("opencut_upload_retention", {"upload_id": upload["upload_id"]}) == released
        with closing(sqlite3.connect(database)) as connection:
            assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
            assert connection.execute(
                "SELECT COUNT(*) FROM audit_records WHERE job_id=? AND action='job.cancelled'",
                (job["job_id"],),
            ).fetchone()[0] == 1
            assert connection.execute(
                "SELECT COUNT(*) FROM audit_records WHERE job_id=? AND action='job.cancelled'",
                (running["job_id"],),
            ).fetchone()[0] == 1
            after = list(connection.iterdump())
        assert after != before
        committed = tool("opencut_edit_apply", ordinary_edit)
        assert tool("opencut_edit_apply", ordinary_edit) == committed
        checks.append("After external capacity returns, reserve replenishment, normal editing with exact-key replay, reopen and independent SQLite integrity all pass")
        report = {"status": "passed", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "checks": checks, "volume_capacity_bytes": volume.f_blocks * volume.f_frsize,
                  "available_at_failure_bytes": available_full,
                  "released_available_bytes": released_available, "filler_bytes": filled,
                  "repair_target_bytes": 8 << 20,
                  "scope": "macOS HFS+ 64 MiB bounded running and queued job cancellation, inherited writer lifetime, retained-upload release and ledger staging; external writers and full workspace physical enforcement remain outside this result"}
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
    finally:
        if marker_fd is not None:
            os.close(marker_fd)
        if worker is not None and worker.poll() is None:
            worker.kill()
            worker.wait(timeout=10)
        if probe_pid is not None:
            try:
                os.kill(probe_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        (root / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")
        if attached:
            for _ in range(10):
                detached = subprocess.run(["hdiutil", "detach", str(mount)],
                                          capture_output=True, text=True, timeout=30)
                if detached.returncode == 0 or "busy" not in detached.stderr.lower():
                    break
                time.sleep(.3)
            assert detached.returncode == 0, (detached.stdout, detached.stderr)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/opencut")
    parser.add_argument("--artifacts-dir", type=Path)
    args = parser.parse_args()
    path = args.artifacts_dir or Path(tempfile.mkdtemp(prefix="opencut-repair-reserve-"))
    print(json.dumps(verify(args.binary.resolve(), path.resolve()), indent=2))
