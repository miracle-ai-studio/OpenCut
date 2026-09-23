#!/usr/bin/env python3
"""Kill a real worker mid-render and verify lease recovery from locked media."""
import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


def preview_ffmpeg_pids(root_pid, job_kind, job_root):
    rows = subprocess.check_output(
        ["ps", "-axo", "pid=,ppid=,command="], text=True
    ).splitlines()
    processes = {}
    children = {}
    for row in rows:
        fields = row.strip().split(None, 2)
        if len(fields) != 3:
            continue
        pid, parent = map(int, fields[:2])
        processes[pid] = fields[2]
        children.setdefault(parent, set()).add(pid)
    descendants = set()
    frontier = [root_pid]
    while frontier:
        parent = frontier.pop()
        for child in children.get(parent, ()):
            if child not in descendants:
                descendants.add(child)
                frontier.append(child)
    matches = set()
    for pid in descendants:
        command = processes[pid]
        common = "ffmpeg" in command.lower() and "ffprobe" not in command.lower()
        common = common and "-filter_complex" in command and str(job_root) in command
        if job_kind == "preview.composited_frame":
            specific = "-frames:v 1" in command and (
                "-c:v png" in command or "-f image2" in command
            )
        else:
            specific = "-c:v libx264" in command
        if common and specific:
            matches.add(pid)
    return matches


def verify(binary: Path, report: Path | None = None, job_kind="render.project", crash_point="active-inputs") -> dict:
    if crash_point == "after-rename" and job_kind != "render.project":
        raise ValueError("after-rename currently verifies render.project only")
    if report:
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps({"status": "running"}))
    workers = []
    with tempfile.TemporaryDirectory(prefix="opencut-asset-recovery-") as folder:
        root = Path(folder)
        database = root / "workspace.sqlite"
        source = root / "long-red.mp4"
        command = [str(binary), "--database", str(database), "--json"]

        def cli(*args, payload=None):
            result = subprocess.run(
                command + list(args),
                input=json.dumps(payload) if payload is not None else None,
                capture_output=True,
                text=True,
                timeout=60,
            )
            assert result.stdout, (result.returncode, result.stderr)
            response = json.loads(result.stdout)
            assert result.returncode == 0 and response["error"] is None, (response, result.stderr)
            return response["data"]

        def tool(name, payload):
            return cli("call", name, "--input", "-", payload=payload)

        def start_worker():
            log = tempfile.TemporaryFile()
            worker = subprocess.Popen(
                [str(binary), "--database", str(database), "worker", "run", "--once"],
                stdout=log,
                stderr=log,
                start_new_session=True,
            )
            workers.append((worker, log))
            return worker, log

        def kill_worker_group(worker):
            if worker.poll() is None:
                os.killpg(worker.pid, signal.SIGKILL)
            worker.wait(timeout=10)

        try:
            subprocess.run(
                [
                    "ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i",
                    "color=c=red:s=1920x1080:r=30:d=90", "-c:v", "libx264",
                    "-preset", "ultrafast", "-pix_fmt", "yuv420p", str(source),
                ],
                check=True,
                timeout=120,
            )
            captured_hash = hashlib.sha256(source.read_bytes()).hexdigest()
            created = tool("opencut_project_create", {
                "project": {"id": "recovery", "name": "Crash recovery", "sequence_id": "main",
                            "width": 1920, "height": 1080,
                            "fps": {"numerator": 30, "denominator": 1}},
                "idempotency_key": "create-recovery",
            })
            span = {"start": {"ticks": "0", "timebase": 30},
                    "duration": {"ticks": "2700", "timebase": 30}}
            asset = {"id": "source", "name": "Captured red", "kind": "video",
                     "uri": str(source), "duration": span["duration"], "metadata": {}}
            track = {"id": "video", "name": "Video", "kind": "video", "enabled": True,
                     "muted": False, "locked": False, "clips": [], "params": {}}
            clip = {"id": "clip", "asset_id": "source", "timeline_range": span,
                    "source_range": span, "params": {}, "channels": []}
            edited = tool("opencut_edit_apply", {
                "project_id": "recovery",
                "request": {"expected_revision": created["revision"],
                            "idempotency_key": "bind-recovery", "label": "Bind long media",
                            "operations": [
                                {"type": "asset.bind", "asset": asset},
                                {"type": "track.create", "sequence_id": "main", "track": track},
                                {"type": "clip.insert", "sequence_id": "main",
                                 "track_id": "video", "clip": clip},
                            ]},
            })
            if job_kind == "render.project":
                job_input = {"project_id": "recovery", "revision": edited["revision"],
                             "sequence_id": "main", "container": "mp4", "video_codec": "h264"}
            elif job_kind == "preview.composited_frame":
                job_input = {"project_id": "recovery", "revision": edited["revision"],
                             "sequence_id": "main", "requested_time": {"ticks": "2400", "timebase": 30},
                             "alignment": "exact"}
            else:
                job_input = {"project_id": "recovery", "revision": edited["revision"],
                             "sequence_id": "main", "requested_range": {
                                 "start": {"ticks": "900", "timebase": 30},
                                 "duration": {"ticks": "900", "timebase": 30}},
                             "alignment": "exact"}
            job = cli("job", "start", "--input", "-", payload={
                "kind": job_kind, "idempotency_key": f"crash-recovery-{job_kind}",
                "input": job_input,
            })
            assert job["input"]["asset_lock"]["assets"][0]["content_hash"] == captured_hash
            source.unlink()

            first_worker, first_log = start_worker()
            deadline = time.monotonic() + 30
            running = None
            old_inputs = None
            previous_ffmpeg = set()
            observed_ffmpeg = set()
            job_root = root / "artifacts" / job["job_id"]
            renamed_output = job_root / "attempt-1" / "render.mp4"
            if crash_point == "after-rename":
                # Observe the real backend's final filename, then freeze the
                # entire worker before independently checking DB publication.
                # No production sleep/hook or simulated output is involved.
                deadline = time.monotonic() + 180
                while not renamed_output.is_file():
                    assert first_worker.poll() is None, "worker exited before output rename"
                    assert time.monotonic() < deadline, "final output rename not observed"
                    time.sleep(.0005)
                os.killpg(first_worker.pid, signal.SIGSTOP)
                running = cli("job", "get", job["job_id"])
                assert running["state"] == "running" and running["result"] is None, "missed pre-publication window"
                assert renamed_output.stat().st_size > 0
                old_inputs = renamed_output.parent / "inputs"
                renamed_hash = hashlib.sha256(renamed_output.read_bytes()).hexdigest()
                # Independently confirm this is a complete encoded movie.
                subprocess.run(["ffmpeg", "-v", "error", "-i", str(renamed_output),
                                "-f", "null", "-"], check=True, timeout=90)
            while running is None and time.monotonic() < deadline:
                assert first_worker.poll() is None, "worker finished before crash could be injected"
                current = cli("job", "get", job["job_id"])
                candidates = list((root / "artifacts" / job["job_id"]).glob("attempt-*/inputs"))
                events = cli("job", "events", "--job-id", job["job_id"], "--limit", "500")
                resolving = any(
                    event["event_type"] == "progress"
                    and event["payload"].get("stage") == "resolving_assets"
                    for event in events
                )
                current_ffmpeg = (
                    preview_ffmpeg_pids(first_worker.pid, job_kind, job_root)
                    if job_kind != "render.project" else set()
                )
                stable_ffmpeg = previous_ffmpeg & current_ffmpeg
                previous_ffmpeg = current_ffmpeg
                if stable_ffmpeg:
                    observed_ffmpeg.update(stable_ffmpeg)
                active_enough = (
                    resolving if job_kind == "render.project" else bool(stable_ffmpeg)
                )
                if current["state"] == "running" and candidates and active_enough:
                    running, old_inputs = current, candidates[0]
                    break
                time.sleep(.03)
            assert running is not None and old_inputs is not None, "did not observe live private inputs"
            if job_kind != "render.project":
                assert observed_ffmpeg, "did not observe stable preview FFmpeg process"
            if crash_point == "active-inputs":
                assert any(old_inputs.iterdir()), old_inputs
            old_attempt_root = old_inputs.parent
            old_attempt = running["attempt"]
            kill_worker_group(first_worker)
            assert first_worker.returncode == -signal.SIGKILL
            if crash_point == "active-inputs":
                assert old_inputs.exists() and any(old_inputs.iterdir()), old_inputs
            else:
                assert renamed_output.exists(), "unpublished output must survive the crash"

            crashed = cli("job", "get", job["job_id"])
            assert crashed["state"] == "running" and crashed["attempt"] == old_attempt, crashed
            lease_until = crashed["lease_until"]
            while int(time.time() * 1000) <= lease_until + 100:
                time.sleep(min(.25, max(.01, (lease_until + 101 - int(time.time() * 1000)) / 1000)))

            second_worker, second_log = start_worker()
            second_worker.wait(timeout=180)
            assert second_worker.returncode == 0, second_log
            completed = cli("job", "get", job["job_id"])
            assert completed["state"] == "succeeded", completed
            assert completed["attempt"] == old_attempt + 1, completed
            assert completed["result"]["asset_locking"] == "media_content_locked"
            assert completed["result"]["asset_lock"] == job["input"]["asset_lock"]
            if job_kind == "preview.composited_frame":
                assert completed["result"]["quality"] == "composited_project_frame"
                actual = completed["result"]["actual_sequence_time"]
                assert Fraction(int(actual["ticks"]), actual["timebase"]) == 80
            elif job_kind == "preview.project_range":
                assert completed["result"]["quality"] == "encoded_project_range"
                actual = completed["result"]["actual_sequence_range"]
                assert Fraction(int(actual["start"]["ticks"]), actual["start"]["timebase"]) == 30
                assert Fraction(int(actual["duration"]["ticks"]), actual["duration"]["timebase"]) == 30
            assert not old_inputs.exists(), old_inputs
            assert not old_attempt_root.exists(), old_attempt_root
            assert not list((root / "artifacts" / job["job_id"]).glob("attempt-*/inputs"))

            artifact = completed["result"]["artifact"]
            output = Path(artifact["path"])
            assert hashlib.sha256(output.read_bytes()).hexdigest() == artifact["content_hash"]
            pixel = subprocess.check_output(
                ["ffmpeg", "-v", "error", "-i", str(output), "-frames:v", "1",
                 "-vf", "scale=1:1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                timeout=30,
            )
            assert pixel[0] > 180 and pixel[1] < 60 and pixel[2] < 60, list(pixel)
            events = cli("job", "events", "--job-id", job["job_id"], "--limit", "500")
            assert any(event["event_type"] == "recovered" for event in events), events
            assert events[-1]["event_type"] == "succeeded", events
            if crash_point == "after-rename":
                activity_check = "real completed output observed and decoded after rename; DB still running with no result; stopped worker then SIGKILL"
            elif job_kind == "preview.composited_frame":
                activity_check = "real worker killed while stable compositing FFmpeg child and private inputs were active"
            elif job_kind == "preview.project_range":
                activity_check = "real worker killed while stable path-scoped range FFmpeg child and private inputs were active"
            else:
                activity_check = "real worker process group killed with verified private inputs active"
            result = {"status": "passed", "checks": [
                activity_check,
                "lease expired naturally and next attempt recovered",
                "deleted source rendered from captured content",
                "abandoned and successful attempt inputs cleaned",
                "artifact hash and independent red pixel verified",
            ], "job_id": job["job_id"], "attempts": completed["attempt"],
                "crash_point": crash_point,
                "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "activity_evidence": ({"renamed_output_sha256": renamed_hash,
                    "state_before_kill": running["state"], "result_before_kill": running["result"]}
                    if crash_point == "after-rename" else (
                    {"stable_preview_ffmpeg_pids": sorted(observed_ffmpeg)}
                    if job_kind != "render.project" else {"progress_stage": "resolving_assets"}
                ))}
            if report:
                report.write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result, indent=2))
            return result
        except Exception as error:
            for worker, log in workers:
                if worker.poll() is not None and worker.returncode not in (0, -signal.SIGKILL):
                    log.seek(0)
                    print(log.read().decode(errors="replace"), file=__import__("sys").stderr)
            if report:
                report.write_text(json.dumps({
                    "status": "failed",
                    "error": f"{type(error).__name__}: {error}",
                }, indent=2) + "\n")
            raise
        finally:
            for worker, log in workers:
                try:
                    kill_worker_group(worker)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    pass
                log.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/opencut"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--crash-point", choices=("active-inputs", "after-rename"), default="active-inputs")
    parser.add_argument(
        "--job-kind", choices=(
            "render.project", "preview.composited_frame", "preview.project_range",
        ),
        default="render.project",
    )
    args = parser.parse_args()
    verify(args.binary.resolve(), args.report, args.job_kind, args.crash_point)
