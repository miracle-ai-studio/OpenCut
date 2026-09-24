#!/usr/bin/env python3
"""A live Worker relinquishes its connection after control-capacity failure."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def verify(binary: Path, root: Path) -> dict:
    if os.name != "posix":
        raise RuntimeError("Attempt lifetime admission requires a Unix host")
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise ValueError("Refusing to reuse a Worker capacity fixture")
    database = root / "workspace.sqlite"
    trace = []
    worker = None
    child_pid = None
    marker_fd = None

    def call(args: list[str], request: dict | None = None,
             error: str | None = None) -> dict:
        result = subprocess.run(
            [str(binary), "--database", str(database), "--json", *args],
            input=json.dumps(request or {}), text=True, capture_output=True, timeout=40,
        )
        envelope = json.loads(result.stdout)
        trace.append({"command": args, "exit": result.returncode,
                      "error": envelope["error"], "request_id": envelope["request_id"]})
        if error:
            assert result.returncode and envelope["error"]["code"] == error, (
                envelope, result.stderr)
        else:
            assert result.returncode == 0 and envelope["error"] is None, (
                envelope, result.stderr)
        return envelope["data"]

    def recovery(action: str, request: dict | None = None) -> dict:
        args = ["storage", "recovery", action]
        if action in ("configure", "enter", "exit"):
            args += ["--input", "-"]
        return call(args, request)

    try:
        source = root / "source.wav"
        source.write_bytes(b"x")
        job = call(["call", "opencut_job_submit", "--input", "-"], {
            "kind": "media.probe", "idempotency_key": "capacity-quiesce",
            "input": {"input": str(source)},
        })
        tools = root / "fake-tools"
        tools.mkdir()
        marker = root / "probe-pid.fifo"
        os.mkfifo(marker)
        marker_fd = os.open(marker, os.O_RDONLY | os.O_NONBLOCK)
        probe = tools / "ffprobe"
        probe.write_text(
            "#!/usr/bin/env python3\n"
            "import os, sys, time\n"
            "if '-version' in sys.argv:\n"
            "    print('ffprobe version OpenCut capacity fixture')\n"
            "else:\n"
            "    with open(os.environ['OPENCUT_QA_PROBE_FIFO'], 'w') as out:\n"
            "        out.write(str(os.getpid()) + '\\n')\n"
            "        out.flush()\n"
            "    time.sleep(120)\n"
        )
        probe.chmod(0o755)
        environment = os.environ.copy()
        environment["PATH"] = str(tools) + os.pathsep + environment["PATH"]
        environment["OPENCUT_QA_PROBE_FIFO"] = str(marker)
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
                    (root / "worker.log").read_text(),
                )
                assert time.monotonic() < deadline, "Worker did not start the probe"
                time.sleep(.05)
        child_pid = int(marker_bytes.split(b"\n", 1)[0])
        os.close(marker_fd)
        marker_fd = None
        assert call(["call", "opencut_job_get", "--input", "-"],
                    {"job_id": job["job_id"]})["state"] == "running"
        policy = recovery("inspect")["policy"]
        recovery("configure", {
            "expected_revision": policy["revision"],
            "idempotency_key": "exhaust-control",
            "payload_limit_bytes": policy["payload_limit_bytes"],
            "minimum_available_bytes": "0",
            "control_limit_bytes": "1",
        })
        assert worker.wait(timeout=12) == 8, (root / "worker.log").read_text()
        assert "DATABASE_CAPACITY_EXHAUSTED" in (root / "worker.log").read_text()
        entered = recovery("enter", {"expected_revision": 0,
                                     "idempotency_key": "quiesced-enter"})
        assert entered["state"] == "active"
        cancelled = call(["storage", "recovery", "cancel", job["job_id"]])
        assert cancelled["state"] == "cancelled"
        assert cancelled["lease_owner"] is None
        assert call(["storage", "recovery", "cancel", job["job_id"]]) == cancelled
        exited = recovery("exit", {"expected_revision": 1,
                                   "idempotency_key": "quiesced-exit"})
        assert exited["state"] == "ready"
        recovery("configure", {
            "expected_revision": policy["revision"] + 1,
            "idempotency_key": "restore-control",
            "payload_limit_bytes": policy["payload_limit_bytes"],
            "minimum_available_bytes": "0",
            "control_limit_bytes": str(64 << 20),
        })
        assert call(["call", "opencut_job_get", "--input", "-"],
                    {"job_id": job["job_id"]})["state"] == "cancelled"
        with sqlite3.connect(database) as connection:
            assert connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
            assert connection.execute(
                "SELECT COUNT(*) FROM audit_records WHERE job_id=? AND action='job.cancelled'",
                (job["job_id"],),
            ).fetchone()[0] == 1
        report = {
            "status": "passed",
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "checks": [
                "A live Worker runs a real blocked ffprobe child under an inherited attempt lock",
                "Tightening control admission during the job makes Worker stop with a capacity error and release its database connection",
                "Owner repair immediately fences and cancels the abandoned running attempt without waiting for its lease deadline",
                "Replenishment, policy restoration, exact cancellation replay, audit and independent SQLite integrity pass",
            ],
            "job_id": job["job_id"],
            "worker_exit": worker.returncode,
            "scope": "Unix live Worker control-capacity failure and owner repair; not physical ENOSPC or multi-host concurrency",
        }
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        return report
    finally:
        if marker_fd is not None:
            os.close(marker_fd)
        if worker is not None and worker.poll() is None:
            worker.kill()
            worker.wait(timeout=10)
        if child_pid is not None:
            try:
                os.kill(child_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        (root / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/opencut")
    parser.add_argument("--artifacts-dir", type=Path)
    args = parser.parse_args()
    path = args.artifacts_dir or Path(tempfile.mkdtemp(prefix="opencut-worker-quiesce-"))
    print(json.dumps(verify(args.binary.resolve(), path.resolve()), indent=2))
