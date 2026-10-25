import { test } from "node:test";
import assert from "node:assert/strict";
import { OpenCutClient, OpenCutApiError, OpenCutProtocolError, OpenCutTransportError, JobWaitTimeoutError } from "../dist/index.js";

test("document retirement keepalive preserves the original typed command, bearer and response semantics", async () => {
  const input = { session_id: "session", generation: "9", owner_id: "owner", lease_id: "lease", control_id: "original-stop", command: { kind: "stop" } };
  const calls = [];
  const data = { state: "stopped" };
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "private", fetch: async (url, init) => {
    calls.push({ url, init });
    assert.equal(new Headers(init.headers).get("Authorization"), "Bearer private");
    assert.equal(init.redirect, "error");
    assert.deepEqual(JSON.parse(init.body), input);
    return Response.json({ request_id: "stop", warnings: [], error: null, data });
  } });
  assert.deepEqual(await client.call("opencut_audio_preview_control", input, { keepalive: true }), data);
  assert.equal(calls[0].init.keepalive, true);
  await client.call("opencut_audio_preview_control", input);
  assert.equal(Object.hasOwn(calls[1].init, "keepalive"), false);
  const reason = new Error("cancelled original caller");
  await assert.rejects(client.call("opencut_audio_preview_control", input, { keepalive: true, signal: AbortSignal.abort(reason) }), e => e === reason);
  assert.equal(calls.length, 2);
});

test("ambiguous transport failure is surfaced once without retry", async () => {
  let calls = 0;
  const client = new OpenCutClient({ baseUrl: "http://localhost:8788", token: "private", fetch: async () => {
    calls++; throw new TypeError("connection reset");
  } });
  await assert.rejects(client.call("opencut_project_list", {}), error => error instanceof OpenCutTransportError && error.outcome === "unknown");
  assert.equal(calls, 1);
  assert.ok(!JSON.stringify(client).includes("private"));
});

test("structured server errors preserve status, request ID, and details", async () => {
  const payload = { request_id: "r", warnings: ["w"], data: null,
    error: { code: "REVISION_CONFLICT", message: "Changed", details: { actual_revision: "new" } } };
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "", fetch: async () => new Response(JSON.stringify(payload), { status: 409 }) });
  await assert.rejects(client.call("opencut_project_list", {}), error => {
    assert.ok(error instanceof OpenCutApiError);
    assert.equal(error.status, 409); assert.equal(error.requestId, "r");
    assert.deepEqual(error.details, { actual_revision: "new" });
    assert.deepEqual(error.warnings, ["w"]); return true;
  });
});

test("incompatible envelopes and HTML gateway failures remain protocol errors", async () => {
  for (const payload of ["<html>gateway</html>", JSON.stringify({ data: [] }),
    JSON.stringify({ request_id: "r", warnings: [], error: null, data: null })]) {
    const client = new OpenCutClient({ baseUrl: "http://localhost", token: "", fetch: async () => new Response(payload) });
    await assert.rejects(client.call("opencut_project_list", {}), OpenCutProtocolError);
  }
});

test("recovery inspect preserves nullable records inside a non-null success payload", async () => {
  const request = { project_id: "project", expected_revision: "base", sequence_id: "scene",
    idempotency_key: "original", label: "Lock track",
    command: { kind: "set_track_locked", track_id: "track", locked: true } };
  const record = { schema_version: 1, recovery_id: "recovery_original", project_id: "project",
    sequence_id: "scene", request_digest: "sha256:" + "a".repeat(64), receipt: null,
    context: { kind: "timeline_v4", request, plan: {
      original_request_digest: "sha256:" + "b".repeat(64),
      decision: { created_ids: [], changed_ids: ["track"], deleted_ids: [], selection: [],
        ripple_shifted_ids: [], removed_transition_ids: [] },
      transaction: { project_id: "project", expected_revision: "base", idempotency_key: "original",
        label: "Lock track", operations: [{ type: "track_lock_set", sequence_id: "scene", track_id: "track", locked: true }] },
    } } };
  const input = { project_id: "project", recovery_id: null };
  for (const data of [{ schema_version: 1, record: null }, { schema_version: 1, record }, null]) {
    let calls = 0;
    const client = new OpenCutClient({ baseUrl: "http://localhost", token: "reader", fetch: async (url, init) => {
      calls++;
      assert.equal(new URL(url).pathname, "/api/tools/opencut_edit_recovery_inspect");
      assert.deepEqual(JSON.parse(init.body), input);
      return Response.json({ request_id: "recovery", warnings: [], error: null, data });
    } });
    if (data === null)
      await assert.rejects(client.call("opencut_edit_recovery_inspect", input), OpenCutProtocolError);
    else
      assert.deepEqual(await client.call("opencut_edit_recovery_inspect", input), data);
    assert.equal(calls, 1);
  }
});

test("local deadline aborts stalled network reads without a cancel command", async () => {
  const urls = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "", fetch: async (url, init) => {
    urls.push(url);
    return new Promise((_, reject) => init.signal.addEventListener("abort", () => reject(init.signal.reason), { once: true }));
  } });
  await assert.rejects(client.waitForJob("job", { timeoutMs: 10 }), JobWaitTimeoutError);
  assert.equal(urls.length, 1); assert.ok(urls[0].endsWith("opencut_job_view"));
});

test("caller abort identity is preserved and unsafe base URLs are rejected", async () => {
  const reason = new Error("changed view");
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "", fetch: async () => { throw new Error("must not call"); } });
  await assert.rejects(client.waitForJob("job", { signal: AbortSignal.abort(reason) }), error => error === reason);
  for (const baseUrl of ["file:///tmp/host", "https://user:password@example.org", "https://example.org?token=private"]) {
    assert.throws(() => new OpenCutClient({ baseUrl, token: "" }), TypeError);
  }
});

test("upload owns its byte snapshot and sends raw content with a verified digest", async () => {
  const { createHash } = await import("node:crypto");
  const original = Uint8Array.of(1, 2, 3);
  const c = new OpenCutClient({baseUrl:"http://localhost",token:"secret",fetch: async (url, init) => {
    assert.equal(init.method, "PUT");
    assert.deepEqual([...init.body], [1,2,3]);
    assert.equal(new URL(url).searchParams.get("content_hash"), createHash("sha256").update(Uint8Array.of(1,2,3)).digest("hex"));
    return new Response(JSON.stringify({request_id:"chunk", warnings:[], error:null, data:{next_offset:"3"}}));
  }});
  const pending = c.uploadChunk("upload_"+"a".repeat(32), "0", original);
  original.fill(9);
  assert.equal((await pending).next_offset,"3");
  await assert.rejects(c.uploadChunk("../path", "0", original),TypeError);
  await assert.rejects(c.uploadChunk("upload_"+"a".repeat(32), "00", original),TypeError);
});

test("stream chunks preserve the byte snapshot and allow a deferred file digest", async () => {
  const { createHash } = await import("node:crypto");
  const bytes = Uint8Array.of(1, 2, 3);
  const client = new OpenCutClient({baseUrl:"http://localhost",fetch:async (url, init) => {
    assert.equal(new URL(url).pathname, "/api/uploads/upload_" + "a".repeat(32) + "/chunks/0");
    assert.equal(new URL(url).searchParams.get("content_hash"), createHash("sha256").update(Uint8Array.of(1,2,3)).digest("hex"));
    assert.deepEqual([...init.body], [1,2,3]);
    return Response.json({request_id:"r",warnings:[],error:null,
      data:{content_hash:null,next_offset:"3",state:"receiving"}});
  }});
  const pending = client.uploadChunk("upload_" + "a".repeat(32), "0", bytes);
  bytes.fill(9);
  assert.equal((await pending).content_hash, null);
});

test("project media URL stays scoped and raw reads preserve authorization errors", async () => {
  const id = "media_" + "a".repeat(32);
  const resource = { project_id: "team/project", media_id: id,
    asset: { uri: `opencut-media://${id}`, version: { content_hash: "b".repeat(64), size: 3 } } };
  const seen = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost:8788", token: "reader", fetch: async (url, init) => {
    seen.push({url, authorization: init.headers.Authorization});
    return new Response(Uint8Array.of(1, 2, 3), {status: 200});
  }});
  assert.equal(client.projectMediaUrl(resource), `http://localhost:8788/api/projects/team%2Fproject/media/${id}/content`);
  const response = await client.downloadProjectMedia(resource);
  assert.deepEqual([...new Uint8Array(await response.arrayBuffer())], [1, 2, 3]);
  assert.deepEqual(seen, [{url: client.projectMediaUrl(resource), authorization: "Bearer reader"}]);
  assert.throws(() => client.projectMediaUrl({...resource, asset: {...resource.asset, uri: "/private/secret"}}), TypeError);
  const denied = new OpenCutClient({baseUrl:"http://localhost",token:"reader",fetch:async()=>
    new Response(JSON.stringify({request_id:"r",warnings:[],data:null,
      error:{code:"NOT_AUTHORIZED",message:"Denied",details:{}}}),{status:403})});
  await assert.rejects(denied.downloadProjectMedia(resource), error => error instanceof OpenCutApiError && error.code === "NOT_AUTHORIZED");
});

test("typed task results require matching successful kind and available data", async () => {
  const { jobResult } = await import("../dist/index.js");
  const job = {state:"succeeded",kind:"media.import",result_unavailable:null,
    result:{kind:"media.import",data:{media_id:"example"}}};
  assert.equal(jobResult(job,"media.import").media_id,"example");
  for (const changed of [ {...job,state:"running"}, {...job,kind:"media.probe"},
    {...job,result:null}, {...job,result_unavailable:{code:"UNAVAILABLE"}},
    {...job,result:{kind:"media.probe",data:{}}} ])
    assert.throws(()=>jobResult(changed,"media.import"),TypeError);
});

test("transcription preserves its source and rejects unavailable or mismatched results", async () => {
  const { jobResult } = await import("../dist/index.js");
  const data = {project_id:"p",revision:"r",asset_id:"a",asset_version:{asset_id:"a",content_hash:"a".repeat(64),size:12},
    options:{kind:"transcribe",language:"zh",word_timestamps:false},
    data:{kind:"transcribe",result:{language:"zh",provider:"whisper.cpp",model:"tiny",segments:[{start:{ticks:"0",timebase:1000},end:{ticks:"1000",timebase:1000},text:"你好",words:[]}]}}};
  const job = {state:"succeeded",kind:"analysis.asset",result_unavailable:null,
    result:{kind:"analysis.asset",data}};
  assert.deepEqual(jobResult(job,"analysis.asset"),data);
  for (const changed of [ {...job,state:"failed"}, {...job,result_unavailable:{code:"INVALID_STORED_RESULT"}},
    {...job,result:null}, {...job,result:{kind:"media.import",data}},
    {...job,result:{kind:"analysis.asset",data:null}} ])
    assert.throws(()=>jobResult(changed,"analysis.asset"),TypeError);
});


test("audit degradation header survives early refusal and deduplicates envelope warning", async () => {
  for (const warnings of [[], ["DENIAL_AUDIT_UNAVAILABLE"]]) {
    const client = new OpenCutClient({baseUrl:"http://localhost",token:"invalid",fetch:async()=>new Response(JSON.stringify({request_id:"r",data:null,error:{code:"NOT_AUTHORIZED",message:"Denied",details:{}},warnings}),{status:403,headers:{"X-OpenCut-Audit-Warning":"DENIAL_AUDIT_UNAVAILABLE"}})});
    await assert.rejects(client.call("opencut_project_list",{}), error => {
      assert.equal(error.code,"NOT_AUTHORIZED");assert.deepEqual(error.warnings,["DENIAL_AUDIT_UNAVAILABLE"]);return true;
    });
  }
});

test("domain calls use the compact route while preserving typed leaf behavior", async () => {
  const seen = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "secret", fetch: async (url, init) => {
    seen.push({ url, body: JSON.parse(init.body) });
    return new Response(JSON.stringify({ request_id: "domain", warnings: [], error: null,
      data: { project: { id: "a" }, revision: "rev_a" } }));
  } });
  const value = await client.callDomain("project", "inspect", { project_id: "a" });
  assert.equal(value.revision, "rev_a");
  assert.equal(seen[0].url, "http://localhost/api/tools/opencut_project");
  assert.deepEqual(seen[0].body, { action: "inspect", input: { project_id: "a" } });
  await assert.rejects(client.callDomain("project", "missing", {}), TypeError);
  assert.equal(seen.length, 1);
});

test("public job wait reads the public projection, never the raw job record", async () => {
  const paths = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "reader", fetch: async (url) => {
    paths.push(new URL(url).pathname);
    return new Response(JSON.stringify({ request_id: "public", warnings: [], error: null,
      data: { job_id: "job_a", state: "succeeded", kind: "media.import", result_unavailable: null, result: { kind: "media.import", data: { media_id: "m" } } } }));
  } });
  const job = await client.waitForJob("job_a", { timeoutMs: 100 });
  assert.equal(job.job_id, "job_a");
  assert.deepEqual(paths, ["/api/tools/opencut_job_view"]);
});


test("render and analysis kinds use the same successful result channel", async () => {
  const { jobResult } = await import("../dist/index.js");
  for (const kind of ["render.project", "preview.composited_frame", "preview.project_range", "render.audio", "render.graphic_frame", "render.graphic_video", "render.blend_frame", "render.blend_video", "analysis.asset", "render.video_range", "render.video", "render.text_frame", "render.caption_frame", "render.frame"]) {
    const data = {revision:"fixed",width:64};
    const job = {state:"succeeded",kind,result_unavailable:null,result:{kind,data}};
    assert.deepEqual(jobResult(job,kind),data);
    assert.throws(()=>jobResult({...job,result:{kind:"media.import",data}},kind),TypeError);
    assert.throws(()=>jobResult({...job,result_unavailable:{code:"INVALID_STORED_RESULT"}},kind),TypeError);
  }
});

test("long media chunks reach transport beyond 1 GiB and stop at 16 GiB", async () => {
  let calls = 0;
  const client = new OpenCutClient({baseUrl:"http://localhost",token:"",fetch:async(url, init) => {
    calls++;
    assert.equal(new URL(url).pathname.split("/").at(-1), "17179869181");
    assert.equal(init.body.byteLength,3);
    return new Response(JSON.stringify({request_id:"large",warnings:[],error:null,data:{next_offset:"17179869184"}}));
  }});
  const id = "upload_"+"a".repeat(32);
  assert.equal((await client.uploadChunk(id,"17179869181",Uint8Array.of(1,2,3))).next_offset,"17179869184");
  await assert.rejects(client.uploadChunk(id,"17179869182",Uint8Array.of(1,2,3)),TypeError);
  assert.equal(calls,1);
});

test("opt-in admission deferral reuses captured bytes and idempotency after caller mutations", async () => {
  const bodies=[], input={project_id:"p",idempotency_key:"original",name:"a",size_bytes:"1"};
  const client=new OpenCutClient({baseUrl:"http://localhost",token:"",fetch:async (_url,init)=>{
    bodies.push(init.body);input.idempotency_key="changed";
    const error=bodies.length===1?{code:"STORAGE_BUDGET_BUSY",message:"busy",details:{}}:null;
    return new Response(JSON.stringify({request_id:"r",warnings:[],data:error?null:{ok:true},error}),{status:error?409:200});
  }});
  assert.deepEqual(await client.callWithAdmissionRetry("opencut_upload_begin",input),{ok:true});
  assert.equal(bodies.length,2);assert.equal(bodies[0],bodies[1]);assert.equal(JSON.parse(bodies[1]).idempotency_key,"original");
});

test("admission deferral stops on caller cancellation and rechecks navigation before each send", async () => {
  for(const mode of ["cancel","navigate"]){
    let calls=0,checks=0;const signal=new AbortController(),reason=new Error(mode);
    const client=new OpenCutClient({baseUrl:"http://localhost",token:"",fetch:async()=>{
      calls++;if(mode==="cancel")signal.abort(reason);
      return new Response(JSON.stringify({request_id:"r",warnings:[],data:null,error:{code:"STORAGE_BUDGET_BUSY",message:"busy",details:{}}}),{status:409});
    }});
    await assert.rejects(client.callWithAdmissionRetry("opencut_project_list",{},{signal:signal.signal,beforeAttempt:()=>{checks++;if(checks===2)throw reason}}),e=>e===reason);
    assert.equal(calls,1);assert.equal(checks,mode==="cancel"?1:2);
  }
});

test("admission deferral never repeats ambiguous outcomes, capacity faults or revision conflicts", async () => {
  for(const code of ["transport","REVISION_CONFLICT","STORAGE_CAPACITY_LOW","DATABASE_CAPACITY_EXHAUSTED"]){
    let calls=0;const client=new OpenCutClient({baseUrl:"http://localhost",token:"",fetch:async()=>{
      calls++;if(code==="transport")throw new Error("lost reply");
      return new Response(JSON.stringify({request_id:"r",warnings:[],data:null,error:{code,message:"rejected",details:{}}}),{status:409});
    }});
    await assert.rejects(client.callWithAdmissionRetry("opencut_project_list",{}));assert.equal(calls,1);
  }
});

test("admission deadlines are bounded and plain calls still do not retry budget contention", async()=>{
  let calls=0;const client=new OpenCutClient({baseUrl:"http://localhost",token:"",fetch:async()=>{
    calls++;await new Promise(resolve=>setTimeout(resolve,5));return new Response(JSON.stringify({request_id:"r",warnings:[],data:null,error:{code:"STORAGE_BUDGET_BUSY",message:"busy",details:{}}}),{status:409});
  }});
  await assert.rejects(client.call("opencut_project_list",{}),OpenCutApiError);assert.equal(calls,1);
  await assert.rejects(client.callWithAdmissionRetry("opencut_project_list",{},{admissionTimeoutMs:1}),OpenCutApiError);assert.equal(calls,2);
  for(const admissionTimeoutMs of [0,30001,1.5])await assert.rejects(client.callWithAdmissionRetry("opencut_project_list",{},{admissionTimeoutMs}),TypeError);
  assert.equal(calls,2);
});


test("V4 resource URLs preserve query identity and reject substituted capabilities", async () => {
  const path = "/api/project-resources/content?project_id=..%2F%E5%B7%A5%E7%A8%8B%26other%3D1&revision=r%2B%20%2F%3F%23&kind=font&resource_id=..";
  const resource = {project_id:"../工程&other=1",revision:"r+ /?#",kind:"font",resource_id:"..",
    version:{id:"av_"+"a".repeat(64),content_hash:"a".repeat(64),size:3},mime_type:null,content_path:path};
  let seen;
  const client = new OpenCutClient({baseUrl:"http://localhost:8788",token:"reader",fetch:async(url,init)=>{
    seen={url,auth:init.headers.Authorization,redirect:init.redirect};return new Response(Uint8Array.of(1,2,3));
  }});
  const url=client.projectResourceUrl(resource);
  assert.equal(new URL(url).pathname,"/api/project-resources/content");
  assert.equal(new URL(url).searchParams.get("project_id"),resource.project_id);
  await client.downloadProjectResource(resource);
  assert.deepEqual(seen,{url:"http://localhost:8788"+path,auth:"Bearer reader",redirect:"error"});
  for(const changed of [{...resource,content_path:"https://attacker.invalid"},{...resource,resource_id:"other"},
    {...resource,kind:"unknown"},{...resource,version:{...resource.version,id:"wrong"}}]) {
    assert.throws(()=>client.projectResourceUrl(changed),TypeError);
  }
});

test("Audio stream sends only the fixed session lease and preserves raw authenticated cancellation", async () => {
  const signal = new AbortController().signal;
  const fixed = { session_id: "session", generation: "12", overlay_epoch: "3", owner_id: "agent", lease_id: "lease", secret_native_path: "/private/source" };
  const client = new OpenCutClient({ baseUrl: "http://localhost:8788", token: "audio-reader", fetch: async (url, init) => {
    assert.equal(new URL(url).pathname, "/api/preview/audio/stream");
    assert.equal(init.method, "POST");
    assert.equal(new Headers(init.headers).get("authorization"), "Bearer audio-reader");
    assert.equal(init.signal, signal);
    assert.deepEqual(JSON.parse(init.body), { session_id: "session", generation: "12", overlay_epoch: "3", owner_id: "agent", lease_id: "lease" });
    return new Response(Uint8Array.of(79, 67, 65, 80), { headers: { "content-type": "application/vnd.opencut.audio-preview" } });
  } });
  const response = await client.openAudioPreviewStream(fixed, { signal });
  assert.deepEqual([...new Uint8Array(await response.arrayBuffer())], [79, 67, 65, 80]);
});


test("binary visual ranges admit512 KiB with exact length and reject a larger allocation before fetching", async () => {
  let calls = 0;
  const bytes = Uint8Array.from({ length: 512 * 1024 }, (_, i) => i % 251);
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "private", fetch: async (_url, init) => {
    calls++; const input = JSON.parse(init.body); assert.equal(input.count, bytes.length);
    return new Response(bytes, { headers: { "content-type": "application/octet-stream", "x-opencut-resource-offset": "0", "x-opencut-resource-eof": "false" } });
  } });
  const input = { session: { session_id: "s", generation: "0", overlay_epoch: "0", owner_id: "o", lease_id: "l" }, transfer_id: "t", offset: "0", count: bytes.length };
  const result = await client.readVisualPreviewResource(input);
  assert.deepEqual(result.bytes, bytes); assert.equal(result.eof, false);
  await assert.rejects(client.readVisualPreviewResource({ ...input, count: bytes.length + 1 }), /bounded visual resource range/);
  assert.equal(calls, 1);
});
