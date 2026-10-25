"""Local acceptance helpers; public calls, durable original requests and stages.

Never imported by product code. Callers own one retained synthetic workspace and
run native/browser stages serially. Completed stages retain their own candidate.
"""
import hashlib
import copy
import json
import subprocess
import time
from fractions import Fraction
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def at(value):
    n = Fraction(str(value))
    return {"ticks": str(n.numerator), "timebase": n.denominator}


def seconds(value):
    return Fraction(int(value["ticks"]), value["timebase"])


class Harness:
    def __init__(self, binary, workspace, name):
        self.binary, self.workspace = Path(binary).resolve(), Path(workspace).resolve()
        assert (self.workspace / "workspace.sqlite").is_file(), "Reuse the owned workspace"
        self.checkpoint = self.workspace / (name + "-checkpoint.json")
        self.name, self.candidate = name, sha(self.binary.read_bytes())
        self.state = json.loads(self.checkpoint.read_text()) if self.checkpoint.exists() else {"requests": {}, "stages": {}}

    def save(self):
        pending = self.checkpoint.with_suffix(".pending")
        pending.write_text(json.dumps(self.state, ensure_ascii=False, indent=2) + "\n")
        pending.replace(self.checkpoint)

    def stage(self, name, action):
        if name in self.state["stages"]:
            print("Reuse " + name, flush=True)
            return self.state["stages"][name]["result"]
        print("Begin " + name, flush=True)
        result = action()
        self.state["stages"][name] = {"candidate": self.candidate, "result": result}
        if hasattr(self, "runtime"):
            self.state["stages"][name]["runtime"] = self.runtime
        self.save()
        print("Passed " + name, flush=True)
        return result

    def cli(self, args, value=None, error=None, timeout=180, credential=None):
        p = subprocess.run([str(self.binary), "--json", "--database", str(self.workspace / "workspace.sqlite"),
            *(["--credential-file", str(credential)] if credential else []), *args],
            input=json.dumps(value) if value is not None else None, text=True, capture_output=True, timeout=timeout)
        result = json.loads(p.stdout)
        if error:
            assert p.returncode and result["error"]["code"] == error, result
        else:
            assert p.returncode == 0 and result.get("error") is None, (result, p.stderr[-1000:])
        return result.get("data")

    def call(self, name, value, error=None, credential=None):
        return self.cli(["call", name, "--input", "-"], value, error, credential=credential)

    def request(self, name, tool, value, credential=None):
        saved = self.state["requests"].get(name)
        request = {"tool": tool, "input": copy.deepcopy(value)}
        if credential is not None:
            request["credential_file"] = str(Path(credential).resolve())
        assert saved is None or saved == request, "Original request changed: " + name
        if saved is None:
            self.state["requests"][name] = request
            self.save()
        # A read-only fixed-head plan cannot be re-planned after its original
        # apply moved the head. Keep that exact proposal; apply reauthorizes it.
        plans = self.state.setdefault("plans", {})
        if name.endswith("-plan") and name in plans:
            return plans[name]
        result = self.call(tool, value, credential=credential)
        if name.endswith("-plan"):
            plans[name] = result
            self.save()
        return result

    def key(self, stage):
        return self.name + ":" + stage

    def inspect(self, project, revision=None):
        view=self.call("opencut_project_inspect",{"project_id":project,"revision":revision})
        assert view["kind"]=="editor", "Expected current editable project content"
        return view["snapshot"]

    def result(self, job, kind):
        """Read today's public projection of the original task, without editing
        retained evidence or re-submitting a historical request under a new shape.
        This read does not certify that the current candidate produced its bytes.
        """
        view = self.call("opencut_job_view", {"job_id": job["job_id"]})
        assert view["state"] == "succeeded" and view["result_unavailable"] is None, view
        assert view["result"]["kind"] == kind, view
        assert view["target"] == job["target"]
        assert view["artifacts"] == job["artifacts"]
        return view["result"]["data"]

    def apply(self, name, request, credential=None):
        receipt = self.request(name, "opencut_edit_apply", request, credential=credential)
        assert self.call("opencut_edit_apply", request, credential=credential) == receipt
        return receipt

    def job(self, name, tool, request, timeout=600, on_running=None):
        accepted = self.request(name, tool, request)
        assert self.call(tool, request)["job_id"] == accepted["job_id"]
        job_id = accepted["job_id"]
        view = self.call("opencut_job_view", {"job_id": job_id})
        worker, output = None, None
        try:
            if view["state"] == "queued":
                output = (self.workspace / (name + "-worker.log")).open("ab")
                worker = subprocess.Popen([str(self.binary), "--database", str(self.workspace / "workspace.sqlite"),
                    "worker", "run", "--once"], stdout=output, stderr=subprocess.STDOUT)
            deadline = time.monotonic() + timeout
            notified = False
            while view["state"] not in ("succeeded", "failed", "cancelled"):
                assert time.monotonic() < deadline, {"observation_timeout": job_id, "state": view["state"],
                    "worker_pid": worker.pid if worker else None, "note": "Timeout does not prove task termination"}
                if worker and worker.poll() is not None:
                    assert worker.returncode == 0, "Worker failed; inspect original job " + job_id
                if view["state"] == "running" and on_running and not notified:
                    on_running(view)
                    notified = True
                time.sleep(.5)
                view = self.call("opencut_job_view", {"job_id": job_id})
            assert view["state"] == "succeeded" and view["result_unavailable"] is None, view
            if worker:
                assert worker.wait(timeout=30) == 0
            return view
        finally:
            # A timed-out observation never kills/restarts a still-running task.
            if output:
                output.close()

    def artifact(self, job):
        """Verifier-only path inspection; never return native paths from a tool."""
        import sqlite3
        with sqlite3.connect(f"file:{self.workspace / 'workspace.sqlite'}?mode=ro", uri=True) as db:
            raw = json.loads(db.execute("SELECT result_json FROM jobs WHERE job_id=?", (job["job_id"],)).fetchone()[0])
        path = Path(raw["artifact"]["path"])
        assert sha(path.read_bytes()) == job["artifacts"][0]["content_hash"]
        return path

    def import_media(self, stage, project_id, path):
        path = Path(path)
        digest = sha(path.read_bytes())
        upload = self.request(stage + ":begin", "opencut_upload_begin", {"project_id": project_id,
            "name": path.name, "size_bytes": str(path.stat().st_size), "idempotency_key": self.key(stage + ":begin")})
        observed = self.call("opencut_upload_inspect", {"upload_id": upload["upload_id"]})
        if observed["state"] == "receiving":
            offset = int(observed["next_offset"])
            with path.open("rb") as source:
                source.seek(offset)
                while chunk := source.read(1024 * 1024):
                    p = subprocess.run([str(self.binary), "--json", "--database", str(self.workspace / "workspace.sqlite"),
                        "upload", "put", upload["upload_id"], "--offset", str(offset), "--input", "-"],
                        input=chunk, capture_output=True, timeout=60)
                    reply = json.loads(p.stdout)
                    assert p.returncode == 0 and reply.get("error") is None, reply
                    offset += len(chunk)
                    assert int(reply["data"]["next_offset"]) == offset
        job = self.job(stage + ":complete", "opencut_upload_complete", {
            "upload_id": upload["upload_id"], "content_hash": digest})
        return self.call("opencut_project_media_inspect", {"project_id": project_id,
            "media_id": self.result(job, "media.import")["media_id"]})
