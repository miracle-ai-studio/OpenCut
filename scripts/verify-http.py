#!/usr/bin/env python3
"""Real loopback API authorization and cross-host persistence smoke test."""
import argparse
import json
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def assert_job_kind_schemas(capabilities):
    descriptors=capabilities["job_kinds"]
    expected={
        "media.probe":({"input"},{"input","streams"}),
        "preview.frame":({"input","requested_time","alignment"},{"media","artifact"}),
        "preview.project_frame":(
            {"project_id","revision","sequence_id","requested_time","alignment"},
            {"revision","sequence_id","requested_time","actual_sequence_time","alignment","quality","artifact"}),
        "render.export":({"segments","container","video","audio"},{"media","artifact"}),
        "render.project":({"project_id","revision","sequence_id","container","video_codec"},{"revision","media","artifact"}),
        "artifact.verify":({"input"},{"passed","checks","metadata","decoded_samples"}),
    }
    assert {item["name"] for item in descriptors}==set(expected),descriptors

    def dereference(schema,node):
        while "$ref" in node:
            parts=node["$ref"].removeprefix("#/").split("/")
            node=schema
            for part in parts: node=node[part.replace("~1","/").replace("~0","~")]
        return node

    by_name={item["name"]:item for item in descriptors}
    for name,(input_fields,result_fields) in expected.items():
        for key,required in (("input_schema",input_fields),("result_schema",result_fields)):
            schema=by_name[name][key]
            root=dereference(schema,schema)
            assert root.get("type")=="object",(name,key,root)
            assert required<=set(root.get("required",[])),(name,key,root)

    def allows_null(schema,node):
        node=dereference(schema,node)
        kind=node.get("type")
        return kind=="null" or (isinstance(kind,list) and "null" in kind) or any(
            allows_null(schema,choice) for choice in node.get("anyOf",[])
        )

    for name in ("preview.project_frame","render.project"):
        schema=by_name[name]["result_schema"]
        properties=dereference(schema,schema)["properties"]
        mode_property=properties["asset_locking"]
        mode=dereference(schema,mode_property)
        assert mode_property.get("default",mode.get("default"))=="legacy_unlocked",mode_property
        assert set(mode.get("enum",[]))=={"legacy_unlocked","media_content_locked"},mode
        assert allows_null(schema,properties["asset_lock"]),properties["asset_lock"]
    frame_result=by_name["preview.project_frame"]["result_schema"]
    quality=dereference(frame_result,frame_result["properties"]["quality"])
    assert quality.get("enum")==["encoded_project_frame"],quality
    frame_input=by_name["preview.project_frame"]["input_schema"]
    input_time=dereference(frame_input,frame_input["properties"]["requested_time"])
    input_ticks=dereference(frame_input,input_time["properties"]["ticks"])
    assert input_ticks.get("type")=="string",input_ticks
    for field in ("requested_time","actual_sequence_time"):
        instant=dereference(frame_result,frame_result["properties"][field])
        ticks=dereference(frame_result,instant["properties"]["ticks"])
        assert ticks.get("type")=="string",(field,ticks)


def verify(binary):
    with tempfile.TemporaryDirectory(prefix="opencut-http-") as root:
        db=Path(root)/"editor.sqlite"
        token_file=Path(root)/"token"
        job_input=Path(root)/"queued-job.json"
        job_input.write_text(json.dumps({
            "kind":"media.probe",
            "idempotency_key":"http-cancel",
            "input":{"input":str(Path(root)/"never-read.mp4")}
        }))
        queued=json.loads(subprocess.check_output([
            str(binary),"--database",str(db),"--json","job","start","--input",str(job_input)
        ]))
        queued_id=queued["data"]["job_id"]
        cancelled_fixture=json.loads(subprocess.check_output([
            str(binary),"--database",str(db),"--json","job","cancel",queued_id
        ]))
        assert cancelled_fixture["data"]["state"]=="cancelled",cancelled_fixture
        with socket.socket() as listener:
            listener.bind(("127.0.0.1",0))
            port=listener.getsockname()[1]
        base=f"http://127.0.0.1:{port}"
        with tempfile.TemporaryFile() as logs:
            process=subprocess.Popen([str(binary),"--database",str(db),"serve","--listen",f"127.0.0.1:{port}","--token-file",str(token_file),"--allow-origin","http://localhost:5173"],stdout=logs,stderr=logs)

            def request(path, body=None, token=None, origin=None):
                headers={}
                if token: headers["Authorization"]=f"Bearer {token}"
                if origin: headers["Origin"]=origin
                if body is not None: headers["Content-Type"]="application/json"
                req=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,headers=headers)
                try:
                    with urllib.request.urlopen(req,timeout=5) as result:
                        return result.status,json.load(result)
                except urllib.error.HTTPError as error:
                    return error.code,json.load(error)

            try:
                deadline=time.monotonic()+15
                while True:
                    if process.poll() is not None: raise AssertionError("HTTP host exited before startup")
                    try:
                        code,result=request("/v1/capabilities")
                        break
                    except urllib.error.URLError:
                        if time.monotonic()>deadline: raise
                        time.sleep(.05)
                assert code==403 and result["error"]["code"]=="NOT_AUTHORIZED"
                token=token_file.read_text().strip()
                code,result=request("/v1/capabilities",token=token)
                assert code==200 and result["data"]["features"]["atomic_transactions"]
                assert_job_kind_schemas(result["data"])
                code,_=request("/v1/capabilities",token=token,origin="https://untrusted.invalid")
                assert code==403
                creation={"project":{"id":"http-demo","name":"HTTP acceptance","sequence_id":"main","width":640,"height":360,"fps":{"numerator":24,"denominator":1}},"idempotency_key":"create-http"}
                code,created=request("/v1/projects",creation,token,"http://localhost:5173")
                assert code==200,created
                _,repeat=request("/v1/projects",creation,token)
                assert created["data"]==repeat["data"]
                changed=json.loads(json.dumps(creation))
                changed["project"]["name"]="Different input"
                code,rejected=request("/v1/projects",changed,token)
                assert code==409 and rejected["error"]["code"]=="IDEMPOTENCY_KEY_REUSED",rejected
                code,snapshot=request("/v1/projects/http-demo/branches/main/snapshot",token=token)
                assert code==200 and snapshot["data"]==created["data"]
                preflight_input={"project_id":"http-demo","revision":created["data"]["revision"],"sequence_id":"missing","container":"mp4","video_codec":"h264"}
                code,preflight=request("/v1/render/preflight",preflight_input,token)
                assert code==200 and preflight["data"]["ready"] is False,preflight
                assert preflight["data"]["revision"]==created["data"]["revision"],preflight
                assert preflight["data"]["issues"][0]["code"]=="SEQUENCE_NOT_FOUND",preflight
                invalid_preflight=dict(preflight_input,unexpected=True)
                code,rejected_preflight=request("/v1/render/preflight",invalid_preflight,token)
                assert code==400 and rejected_preflight["error"]["code"]=="INVALID_ARGUMENT",rejected_preflight
                cli=json.loads(subprocess.check_output([str(binary),"--database",str(db),"--json","project","inspect","http-demo"]))
                assert cli["data"]==snapshot["data"]
                event_request=urllib.request.Request(base+"/v1/events?after_sequence=0",headers={"Authorization":f"Bearer {token}"})
                with urllib.request.urlopen(event_request,timeout=5) as stream:
                    fields={}
                    while True:
                        line=stream.readline().decode().strip()
                        if not line and fields: break
                        if ":" in line:
                            key,value=line.split(":",1)
                            fields[key]=value.strip()
                    assert fields["event"]=="committed",fields
                    event=json.loads(fields["data"])
                    assert event["project_id"]=="http-demo",event
                    assert fields["id"]==str(event["sequence"])
                creation2=json.loads(json.dumps(creation))
                creation2["project"]["id"]="http-second"
                creation2["idempotency_key"]="create-second"
                code,_=request("/v1/projects",creation2,token)
                assert code==200
                resumed=urllib.request.Request(base+"/v1/events?after_sequence=0",headers={"Authorization":f"Bearer {token}","Last-Event-ID":fields["id"]})
                with urllib.request.urlopen(resumed,timeout=5) as stream:
                    while True:
                        line=stream.readline().decode().strip()
                        if line.startswith("data:"):
                            event2=json.loads(line.split(":",1)[1])
                            break
                    assert event2["project_id"]=="http-second" and event2["sequence"]>event["sequence"],event2
                code,history=request("/v1/projects/http-demo/revisions?limit=1",token=token)
                assert code==200 and len(history["data"]["revisions"])==1,history
                old_revision=created["data"]["revision"]
                code,historical=request(f"/v1/projects/http-demo/revisions/{old_revision}",token=token)
                assert code==200 and historical["data"]==created["data"]
                code,restored=request("/v1/projects/http-demo/branches/main/restore",{"expected_revision":old_revision,"target_revision":old_revision,"idempotency_key":"restore-http","label":"Restore snapshot"},token)
                assert code==200 and restored["data"]["revision"]!=old_revision,restored
                code,_=request("/v1/projects/http-demo/branches/unsupported/snapshot",token=token)
                assert code==404

                # The fixture was queued and cancelled through the public CLI before
                # server startup, so the HTTP worker cannot race this lifecycle.
                code,cancelled=request(f"/v1/jobs/{queued_id}/cancel",{},token)
                assert code==200 and cancelled["data"]["state"]=="cancelled",cancelled
                code,job_events=request(f"/v1/job-events?after_sequence=0&limit=20&job_id={queued_id}",token=token)
                assert code==200,job_events
                assert [item["event_type"] for item in job_events["data"]]==["queued","cancelled"],job_events
                queued_event,cancelled_event=job_events["data"]

                job_stream=urllib.request.Request(
                    base+f"/v1/job-events/stream?after_sequence=0&job_id={queued_id}",
                    headers={"Authorization":f"Bearer {token}","Last-Event-ID":str(queued_event["sequence"])}
                )
                with urllib.request.urlopen(job_stream,timeout=5) as stream:
                    job_fields={}
                    while True:
                        line=stream.readline().decode().strip()
                        if not line and job_fields: break
                        if ":" in line:
                            key,value=line.split(":",1)
                            job_fields[key]=value.strip()
                    resumed_job_event=json.loads(job_fields["data"])
                    assert job_fields["id"]==str(cancelled_event["sequence"]),job_fields
                    assert job_fields["event"]=="cancelled" and resumed_job_event["job_id"]==queued_id,resumed_job_event

                cli_events=json.loads(subprocess.check_output([
                    str(binary),"--database",str(db),"--json","job","events",
                    "--after-sequence","0","--limit","20","--job-id",queued_id
                ]))
                assert cli_events["data"]==job_events["data"],cli_events

                return {"checks":["bearer required","origin rejected","authenticated discovery","typed job kind schema registry","create","idempotent retry and key conflict status","snapshot","typed fixed-revision render preflight","strict preflight input","CLI readback","durable project SSE event and Last-Event-ID resume","history and restore routes","unknown branch rejected","CLI-created queued/cancelled fixture","HTTP repeated cancel stability","job event polling and job_id filter","job SSE Last-Event-ID resume","CLI job events parity"]}
            finally:
                process.send_signal(signal.SIGINT)
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                if process.returncode not in (0,-signal.SIGINT):
                    logs.seek(0)
                    print(logs.read().decode(errors="replace"),file=__import__("sys").stderr)


if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--binary",type=Path,default=Path("target/debug/opencut"))
    args=parser.parse_args()
    print(json.dumps(verify(args.binary.resolve()),indent=2))
