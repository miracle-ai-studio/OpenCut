#!/usr/bin/env python3
"""One actual V4 worker SIGKILL, natural lease recovery and fenced publication.

Uses the retained source/project and original durable request. No clock/database
mutation, media copy, full interview rerender, or timeout-triggered restart.
"""
import argparse
from array import array
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import time
from acceptance import Harness, sha


def processes():
    result = subprocess.run(["ps", "-axo", "pid=,ppid=,pgid=,stat=,lstart=,comm="], text=True, capture_output=True, check=True)
    rows = {}
    for line in result.stdout.splitlines():
        fields = line.split(None, 9)
        if len(fields) != 10:
            continue
        pid, parent, group = map(int, fields[:3])
        rows[pid] = {"pid": pid, "parent": parent, "group": group, "state": fields[3], "started": " ".join(fields[4:9]), "command": fields[9]}
    return rows


def alive(record, rows):
    current = rows.get(record["pid"])
    return current is not None and current["started"] == record["started"] and not current["state"].startswith("Z")


def descendants(rows, root):
    found = {root}
    while True:
        added = {pid for pid, row in rows.items() if row["parent"] in found} - found
        if not added:
            return [row for pid, row in rows.items() if pid in found and pid != root]
        found.update(added)


def verify(args):
    h = Harness(args.binary, args.workspace, "agent-worker-recovery-v4")
    h.runtime = json.loads(args.native_runtime.read_text())
    reference = json.loads(args.source_report.read_text())
    status = subprocess.run([str(h.binary), "--json", "--database", str(h.workspace / "workspace.sqlite"),
        "server", "--directory", str(h.workspace / "service"), "status"], text=True, capture_output=True, timeout=30)
    status_value = json.loads(status.stdout); assert status_value["error"] is None
    service = status_value["data"]  # Stopped status intentionally exits nonzero.
    assert not service["ready"] and service["state"] != "running", "Stop the owned service's automatic workers before exclusive fault acceptance"
    episode = h.candidate[:12] + "-" + sha(json.dumps(h.runtime, sort_keys=True).encode())[:12] + "-exclusive-worker-v3"
    submission = {"kind": "audio_range", "request": {
        "project_id": reference["project_id"], "revision": reference["revision"], "sequence_id": "long-scene",
        "idempotency_key": h.key("actual-crash-" + episode), "sample_rate": 48000,
        "range": {"start_sample": "201600", "frame_count": "48000"}}}
    if args.retained_request:
        retained = json.loads(args.retained_request.read_text())
        assert retained["tool"] == "opencut_render_submit"
        if retained.get("replaces_failed_job"):
            prior = h.call("opencut_job_view", {"job_id": retained["replaces_failed_job"]})
            assert prior["state"] == "failed", "Replace only an authoritatively terminal failure after its product change"
        submission = retained["input"]
        assert submission["kind"] in ("audio_range", "video_range")
        assert submission["request"]["project_id"] == reference["project_id"]
        assert submission["request"]["revision"] == reference["revision"]
        episode += "-" + submission["kind"] + "-" + sha(json.dumps(submission, sort_keys=True).encode())[:12]
    name = "actual-crash-" + episode
    accepted = h.stage("submit-" + episode, lambda: h.request(name, "opencut_render_submit", submission))
    job_id = accepted["job_id"]
    assert h.call("opencut_render_submit", h.state["requests"][name]["input"])["job_id"] == job_id
    view = lambda: h.call("opencut_job_view", {"job_id": job_id})
    def private():
        with sqlite3.connect(f"file:{h.workspace / 'workspace.sqlite'}?mode=ro", uri=True) as db:
            row = db.execute("SELECT state,attempt,lease_owner,lease_until,result_json FROM jobs WHERE job_id=?", (job_id,)).fetchone()
        return {"state": row[0], "attempt": row[1], "worker": row[2], "lease_until": row[3], "has_result": row[4] is not None}
    def launch(label):
        with (h.workspace / (name + "-" + label + ".log")).open("ab") as log:
            return subprocess.Popen([str(h.binary), "--database", str(h.workspace / "workspace.sqlite"), "worker", "run", "--once"],
                stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
    fault_key = "fault-" + episode
    fault = h.state.get(fault_key)
    first = None
    if not fault:
        assert view()["state"] == "queued", "Do not recreate a completed or currently owned attempt"
        with sqlite3.connect(f"file:{h.workspace / 'workspace.sqlite'}?mode=ro", uri=True) as db:
            assert db.execute("SELECT job_id FROM jobs WHERE state IN ('queued','running')").fetchall() == [(job_id,)], "One owned stateful job at a time"
        first = launch("first")
        rows = processes(); worker = rows[first.pid]
        fault = {"phase": "observing", "worker": worker, "job_id": job_id}
        h.state[fault_key] = fault; h.save()
    if fault["phase"] == "observing":
        deadline = time.monotonic() + 25
        while True:
            rows = processes(); record = private(); children = descendants(rows, fault["worker"]["pid"])
            assert alive(fault["worker"], rows), "Original worker already exited; inspect the original job, never recreate it"
            if children and "observed_children" not in fault:
                fault["observed_children"] = children; h.save()
            # macOS comm may contain the bundle's shortened executable name.
            if any(any(name in child["command"].lower() for name in ("chrome", "chromium")) for child in children):
                assert record["state"] == "running" and record["attempt"] == 1 and not record["has_result"], record
                assert not view()["artifacts"]
                fault.update(phase="kill-requested", accepted_lease=record, owned_children=children); h.save()
                break
            assert time.monotonic() < deadline, "Observation expired; original worker/job remain authoritative"
            time.sleep(.05)
    if fault["phase"] == "kill-requested":
        rows = processes()
        if alive(fault["worker"], rows):
            assert private()["state"] == "running" and private()["attempt"] == 1
            os.kill(fault["worker"]["pid"], signal.SIGKILL)
        if first:
            assert first.wait(timeout=10) == -signal.SIGKILL
        fault.update(phase="crashed", killed_at_ms=round(time.time() * 1000)); h.save()
        print("Original worker SIGKILL with live owned browser; original request/job preserved", flush=True)
    cleanup_key = "old-processes-exited-" + episode
    if cleanup_key not in h.state["stages"]:
        owned_groups = {row["group"] for row in [fault["worker"], *fault["owned_children"]]}
        deadline = time.monotonic() + 20
        while True:
            rows = processes(); remaining = [row for row in [fault["worker"], *fault["owned_children"]] if alive(row, rows)]
            group_members = [row for row in rows.values() if row["group"] in owned_groups and not row["state"].startswith("Z")]
            if not remaining and not group_members:
                break
            assert time.monotonic() < deadline, {"still_live_owned_pids": [row["pid"] for row in remaining],
                "owned_group_live_pids": [row["pid"] for row in group_members], "note": "Do not start another attempt while original processes remain live"}
            time.sleep(.1)
        h.stage(cleanup_key, lambda: {"observed_terminal_pids": [row["pid"] for row in [fault["worker"], *fault["owned_children"]]],
            "observed_terminal_process_groups": sorted(owned_groups), "source": "actual PID/start-time/state and owned process-group membership observation"})
    recovery_key = "recovered-output-" + episode
    if recovery_key not in h.state["stages"]:
        record = private()
        if record["attempt"] == 1 and record["state"] == "running":
            assert not record["has_result"] and not view()["artifacts"]
            print("Waiting for original 30s lease to expire naturally", flush=True)
            while private()["state"] == "running" and round(time.time() * 1000) <= private()["lease_until"] + 100:
                time.sleep(.2)
        record = private()
        if record["attempt"] == 1 and record["state"] in ("running", "queued"):
            second = launch("second")
            h.state["second-" + episode] = {"pid": second.pid}; h.save()
        else:
            second = None
            assert record["attempt"] == 2, record
        deadline = time.monotonic() + 180
        while view()["state"] not in ("succeeded", "failed", "cancelled"):
            assert time.monotonic() < deadline, "Observation timeout; retain and inspect the same second attempt"
            if second and second.poll() is not None:
                assert second.returncode == 0
            time.sleep(.3)
        output = view(); assert output["state"] == "succeeded" and output["result_unavailable"] is None, output
        if second:
            assert second.wait(timeout=20) == 0
        assert private()["attempt"] == 2
        h.stage(recovery_key, lambda: output)
    output = h.state["stages"][recovery_key]["result"]
    def confirm():
        assert len(output["artifacts"]) == 1, "Exactly one fenced primary artifact"
        path = h.artifact(output); assert path.parent.name == "attempt-2"
        data = output["extended_result"]["data"]
        if submission["kind"] == "video_range":
            assert data["output"]["execution"]["audio"]["module_sha256"] == h.runtime["audio_module_sha256"]
            pcm_path = path.parent / "video-audio.wav"
        else:
            assert data["execution"]["module_sha256"] == h.runtime["audio_module_sha256"]
            pcm_path = path
        gold = json.loads(Path(reference["legacy_pcm"]["reference_report"]).read_text()); expected = Path(gold["pcm_path"]).read_bytes(); assert sha(expected) == gold["pcm_sha256"]
        actual = subprocess.check_output(["ffmpeg", "-v", "error", "-i", str(pcm_path), "-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"])
        a, e = array("f"), array("f"); a.frombytes(actual); e.frombytes(expected); assert len(a) == len(e) == 96000
        maximum = max(abs(x-y) for x,y in zip(a,e)); threshold = gold.get("threshold", 1e-5)
        assert 0 <= threshold <= 1e-5 and maximum <= threshold, maximum
        events = h.call("opencut_job_events", {"job_id": job_id, "after_sequence": 0, "limit": 500})
        kinds = [event["event_type"] for event in events]
        assert kinds.count("running") == 2 and kinds.count("recovered") == 1 and kinds.count("succeeded") == 1, kinds
        time.sleep(.2); assert view() == output
        return {"status": "passed_scoped", "binary_sha256": h.candidate, "native_runtime": h.runtime, "job_id": job_id,
            "render_kind": submission["kind"], "project_id": reference["project_id"], "revision": reference["revision"],
            "fault": fault, "published_attempt": 2, "output_sha256": output["artifacts"][0]["content_hash"], "pcm_max_absolute_error": maximum, "pcm_threshold": threshold,
            "events": kinds, "scope": "Actual SIGKILL/pipe cleanup, natural lease requeue and one published replacement attempt in retained V4 source; stale update fences also retain their existing unchanged-source behavior-test evidence, not a multi-host outage matrix"}
    report = h.stage("publication-proof-" + episode, confirm)
    args.report.write_text(json.dumps(report, indent=2) + "\n"); print(json.dumps({key: report[key] for key in ["status", "job_id", "published_attempt", "pcm_max_absolute_error"]}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "workspace", "native-runtime", "source-report", "report"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--retained-request", type=Path)
    verify(parser.parse_args())
