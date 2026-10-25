#!/usr/bin/env python3
"""Compile only SDK/contracts outside the repository; exercise real HTTP + independent CLI."""
import hashlib
import argparse
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import urllib.request
import urllib.error

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path)
arguments = parser.parse_args()
evidence = Path(tempfile.mkdtemp(prefix="opencut-rust-sdk-"))
print(f"Rust SDK acceptance evidence: {evidence}", flush=True)

def run(args, cwd=ROOT):
    return subprocess.check_output([str(x) for x in args], cwd=cwd, stderr=subprocess.STDOUT, text=True)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def cli(tool, body):
    path = evidence / "request.json"
    path.write_text(json.dumps(body))
    return json.loads(run([binary,"--database",database,"--json","call",tool,"--input",path]))["data"]

binary = arguments.binary.resolve() if arguments.binary else ROOT / "target/debug/opencut"
try:
    if arguments.binary is None:
        (evidence / "build.log").write_text(run(["cargo","build","-p","opencut-cli"]))
    vendor = evidence / "vendor"
    vendor.mkdir()
    pure = ["contracts","media-contracts","job-contracts","api-contracts","sdk-rust"]
    for name in pure:
        shutil.copytree(ROOT / "crates" / name, vendor / name)
    (vendor / "Cargo.toml").write_text('[workspace]\nresolver="3"\nmembers=["*"]\n[workspace.package]\nversion="0.1.0"\nedition="2024"\nlicense="MIT"\n')
    consumer = evidence / "consumer"
    (consumer / "src").mkdir(parents=True)
    (consumer / "Cargo.toml").write_text('[package]\nname="independent-sdk-consumer"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nopencut-sdk={path="../vendor/sdk-rust"}\nserde_json="1"\n')
    shutil.copy(ROOT / "crates/sdk-rust/examples/acceptance.rs", consumer / "src/main.rs")
    # Reuse a build cache, but all package sources/manifest paths resolve inside the evidence directory.
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target/rust-sdk-acceptance"))
    built = subprocess.run(["cargo","build","--manifest-path",str(consumer / "Cargo.toml")], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (evidence / "consumer-build.log").write_text(built.stdout)
    assert built.returncode == 0, built.stdout
    metadata = json.loads(subprocess.check_output(["cargo","metadata","--manifest-path",str(consumer / "Cargo.toml"),"--format-version","1"], stderr=subprocess.PIPE, text=True))
    packages = {p["id"]:p for p in metadata["packages"]}
    nodes = {n["id"]:n for n in metadata["resolve"]["nodes"]}
    sdk_id = next(i for i,p in packages.items() if p["name"]=="opencut-sdk")
    pending, reached = [sdk_id], set()
    while pending:
        current=pending.pop()
        if current in reached: continue
        reached.add(current)
        pending.extend(nodes[current]["dependencies"])
    deps = sorted({packages[i]["name"] for i in reached})
    forbidden={"opencut-native-services","opencut-sqlite-store","opencut-media","opencut-asset-store","opencut-editor-application","opencut-automation","opencut-job-runtime","opencut-job-execution","opencut-plugin-host","rusqlite","wasmtime","cosmic-text","image"}
    assert not forbidden.intersection(deps), forbidden.intersection(deps)
    (evidence / "dependencies.json").write_text(json.dumps(deps,indent=2))
    database=evidence / "editor.sqlite"
    token_file=evidence / "token"
    queued=cli("opencut_job_start", {"kind":"media.probe","idempotency_key":"rust-cancelled","input":{"input":str(evidence / "never-read")}})
    cli("opencut_job_cancel", {"job_id":queued["job_id"]})
    with socket.socket() as s:
        s.bind(("127.0.0.1",0)); port=s.getsockname()[1]
    base=f"http://127.0.0.1:{port}"
    logs=open(evidence / "host.log","w")
    host=subprocess.Popen([str(binary),"--database",str(database),"serve","--listen",f"127.0.0.1:{port}","--token-file",str(token_file)],stdout=logs,stderr=logs)
    proxy=None
    traces=[]
    try:
        deadline=time.monotonic()+15
        while True:
            assert host.poll() is None,"Host exited before readiness"
            try:
                urllib.request.urlopen(base+"/api/capabilities",timeout=1)
            except urllib.error.HTTPError as e:
                assert e.code==403;break
            except urllib.error.URLError:
                assert time.monotonic()<deadline,"Host startup timeout"
                time.sleep(.05)
        class Proxy(http.server.BaseHTTPRequestHandler):
            def log_message(self,*args): pass
            def do_POST(self):
                body=self.rfile.read(int(self.headers["Content-Length"]))
                tool=self.path.split("/")[-1]
                traces.append({"tool":tool,"input":json.loads(body)})
                req=urllib.request.Request(base+self.path,data=body,headers={"Content-Type":"application/json","Authorization":"Bearer "+token_file.read_text().strip()})
                try:
                    with urllib.request.urlopen(req,timeout=15) as upstream: status,data=upstream.status,upstream.read()
                except urllib.error.HTTPError as e: status,data=e.code,e.read()
                if tool=="opencut_edit_apply" and sum(t["tool"]==tool for t in traces)==1:
                    self.connection.shutdown(socket.SHUT_RDWR); self.connection.close(); return
                if tool=="opencut_job_get": time.sleep(.25)
                try:
                    self.send_response(status);self.send_header("Content-Type","application/json");self.send_header("Content-Length",str(len(data)));self.end_headers();self.wfile.write(data)
                except (BrokenPipeError,ConnectionResetError): pass
        proxy=http.server.ThreadingHTTPServer(("127.0.0.1",0),Proxy)
        threading.Thread(target=proxy.serve_forever,daemon=True).start()
        proxy_url=f"http://127.0.0.1:{proxy.server_address[1]}"
        wave=evidence / "tone.wav"
        run(["ffmpeg","-v","error","-f","lavfi","-i","sine=frequency=440:duration=1","-y",wave])
        report=evidence / "report.json"
        executable=ROOT / "target/rust-sdk-acceptance/debug/independent-sdk-consumer"
        run([executable,base,proxy_url,token_file,binary,database,ROOT / "packages/contract-fixtures/edit-journey.json",queued["job_id"],wave,report])
        writes=[t for t in traces if t["tool"]=="opencut_edit_apply"]
        assert len(writes)==2 and writes[0]==writes[1],writes
        assert not any(t["tool"]=="opencut_job_cancel" for t in traces),traces
        result=json.loads(report.read_text())
        assert result["snapshot"]==cli("opencut_project_inspect",{"project_id":"sdk-acceptance"})
        # Same host, same key and original revision: TS must replay the Rust transaction,
        # even where Rust serialized optional fields as null and TS omitted them.
        sdk_ts=ROOT / "packages/sdk-ts"
        run(["npm","run","build"], cwd=sdk_ts)
        packed=json.loads(run(["npm","pack","--ignore-scripts","--json","--pack-destination",evidence], cwd=sdk_ts))
        (evidence / "package.json").write_text(json.dumps({"private":True,"type":"module"}))
        run(["npm","install","--ignore-scripts","--no-audit","--no-fund",evidence / packed[0]["filename"]], cwd=evidence)
        shutil.copy(sdk_ts / "tests/rust-conformance.mjs", evidence / "conformance.mjs")
        (evidence / "cross-sdk.log").write_text(run(["node",evidence / "conformance.mjs",base,token_file,ROOT / "packages/contract-fixtures/edit-journey.json",report], cwd=evidence))
        result.update({"binary_sha256":sha(binary),"consumer_sha256":sha(executable),"trace":traces,"sdk_dependencies":deps,"rustc":run(["rustc","--version"]).strip()})
        result["checks"] += ["SDK source compiles outside repository with only four portable contract crates", "transitive dependency closure excludes native execution, SQLite and Wasmtime", "wire proxy proves exactly two explicit writes, byte-equivalent retry and no implicit task cancel", "independently installed TS SDK replays the Rust SDK transaction without creating another commit"]
        report.write_text(json.dumps(result,indent=2))
        print(json.dumps({"status":"passed","checks":len(result["checks"]),"report":str(report)},indent=2))
    finally:
        if proxy: proxy.shutdown();proxy.server_close()
        host.terminate()
        try:host.wait(timeout=5)
        except subprocess.TimeoutExpired:host.kill();host.wait()
        logs.close()
except Exception as error:
    (evidence / "failure.txt").write_text(str(error)+"\n"+getattr(error,"output",""))
    raise
