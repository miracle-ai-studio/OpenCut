import { test } from "node:test";
import assert from "node:assert/strict";
import { OpenCutClient, OpenCutApiError, OpenCutProtocolError, OpenCutTransportError, JobWaitTimeoutError } from "../dist/index.js";

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
  const payload = { api_version: "1", request_id: "r", warnings: ["w"], data: null,
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
  for (const payload of ["<html>gateway</html>", JSON.stringify({ api_version: "2", data: [] }),
    JSON.stringify({ api_version: "1", request_id: "r", warnings: [], error: null, data: null })]) {
    const client = new OpenCutClient({ baseUrl: "http://localhost", token: "", fetch: async () => new Response(payload) });
    await assert.rejects(client.call("opencut_project_list", {}), OpenCutProtocolError);
  }
});

test("local deadline aborts stalled network reads without a cancel command", async () => {
  const urls = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "", fetch: async (url, init) => {
    urls.push(url);
    return new Promise((_, reject) => init.signal.addEventListener("abort", () => reject(init.signal.reason), { once: true }));
  } });
  await assert.rejects(client.waitForJob("job", { timeoutMs: 10 }), JobWaitTimeoutError);
  assert.equal(urls.length, 1); assert.ok(urls[0].endsWith("opencut_job_get"));
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
    return new Response(JSON.stringify({api_version:"1", request_id:"chunk", warnings:[], error:null, data:{next_offset:"3"}}));
  }});
  const pending = c.uploadChunk("upload_"+"a".repeat(32), "0", original);
  original.fill(9);
  assert.equal((await pending).next_offset,"3");
  await assert.rejects(c.uploadChunk("../path", "0", original),TypeError);
  await assert.rejects(c.uploadChunk("upload_"+"a".repeat(32), "00", original),TypeError);
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
  assert.equal(client.projectMediaUrl(resource), `http://localhost:8788/v1/projects/team%2Fproject/media/${id}/content`);
  const response = await client.downloadProjectMedia(resource);
  assert.deepEqual([...new Uint8Array(await response.arrayBuffer())], [1, 2, 3]);
  assert.deepEqual(seen, [{url: client.projectMediaUrl(resource), authorization: "Bearer reader"}]);
  assert.throws(() => client.projectMediaUrl({...resource, asset: {...resource.asset, uri: "/private/secret"}}), TypeError);
  const denied = new OpenCutClient({baseUrl:"http://localhost",token:"reader",fetch:async()=>
    new Response(JSON.stringify({api_version:"1",request_id:"r",warnings:[],data:null,
      error:{code:"NOT_AUTHORIZED",message:"Denied",details:{}}}),{status:403})});
  await assert.rejects(denied.downloadProjectMedia(resource), error => error instanceof OpenCutApiError && error.code === "NOT_AUTHORIZED");
});

test("public extension results require matching successful kind and supported version", async () => {
  const { publicJobResult } = await import("../dist/index.js");
  const job = {state:"succeeded",kind:"media.import",result:null,result_unavailable:null,
    extended_result:{kind:"media.import",schema_version:1,data:{media_id:"example"}}};
  assert.equal(publicJobResult(job,"media.import").media_id,"example");
  for(const changed of [ {...job,state:"running"}, {...job,kind:"media.probe"},
    {...job,extended_result:{...job.extended_result,schema_version:2}},
    {...job,result_unavailable:{code:"UNAVAILABLE"}}, {...job,extended_result:{...job.extended_result,kind:"media.probe"}},
    {...job,result:{kind:"media.probe",data:{}}}])
    assert.throws(()=>publicJobResult(changed,"media.import"),TypeError);
});

test("transcription results remain typed versioned extensions and reject unavailable or mismatched data", async () => {
  const { publicJobResult } = await import("../dist/index.js");
  const data = {project_id:"p",revision:"r",asset_id:"a",asset_version:{asset_id:"a",content_hash:"a".repeat(64),size:12},
    transcription:{language:"zh",provider:"whisper.cpp",model:"tiny",segments:[{start:{ticks:"0",timebase:1000},end:{ticks:"1000",timebase:1000},text:"你好",words:[]}]}};
  const job = {state:"succeeded",kind:"analysis.transcribe",result:null,result_unavailable:null,
    extended_result:{kind:"analysis.transcribe",schema_version:1,data}};
  assert.deepEqual(publicJobResult(job,"analysis.transcribe"),data);
  for (const changed of [ {...job,state:"failed"}, {...job,result_unavailable:{code:"INVALID_STORED_RESULT"}},
    {...job,extended_result:{...job.extended_result,schema_version:2}},
    {...job,extended_result:{...job.extended_result,kind:"media.import"}},
    {...job,extended_result:{...job.extended_result,data:null}},
    {...job,result:{kind:"media.probe",data:{}}} ])
    assert.throws(()=>publicJobResult(changed,"analysis.transcribe"),TypeError);
});


test("audit degradation header survives early refusal and deduplicates envelope warning", async () => {
  for (const warnings of [[], ["DENIAL_AUDIT_UNAVAILABLE"]]) {
    const client = new OpenCutClient({baseUrl:"http://localhost",token:"invalid",fetch:async()=>new Response(JSON.stringify({api_version:"1",request_id:"r",data:null,error:{code:"NOT_AUTHORIZED",message:"Denied",details:{}},warnings}),{status:403,headers:{"X-OpenCut-Audit-Warning":"DENIAL_AUDIT_UNAVAILABLE"}})});
    await assert.rejects(client.call("opencut_project_list",{}), error => {
      assert.equal(error.code,"NOT_AUTHORIZED");assert.deepEqual(error.warnings,["DENIAL_AUDIT_UNAVAILABLE"]);return true;
    });
  }
});

test("domain calls use the compact route while preserving typed leaf behavior", async () => {
  const seen = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "secret", fetch: async (url, init) => {
    seen.push({ url, body: JSON.parse(init.body) });
    return new Response(JSON.stringify({ api_version: "1", request_id: "domain", warnings: [], error: null,
      data: { project: { id: "a" }, revision: "rev_a" } }));
  } });
  const value = await client.callDomain("project", "inspect", { project_id: "a" });
  assert.equal(value.revision, "rev_a");
  assert.equal(seen[0].url, "http://localhost/v1/tools/opencut_project");
  assert.deepEqual(seen[0].body, { action: "inspect", input: { project_id: "a" } });
  await assert.rejects(client.callDomain("project", "missing", {}), TypeError);
  assert.equal(seen.length, 1);
});

test("public job wait reads the public projection, never the raw job record", async () => {
  const paths = [];
  const client = new OpenCutClient({ baseUrl: "http://localhost", token: "reader", fetch: async (url) => {
    paths.push(new URL(url).pathname);
    return new Response(JSON.stringify({ api_version: "1", request_id: "public", warnings: [], error: null,
      data: { job_id: "job_a", state: "succeeded", kind: "media.import", result: null,
        result_unavailable: null, extended_result: { kind: "media.import", schema_version: 1, data: { media_id: "m" } } } }));
  } });
  const job = await client.waitForPublicJob("job_a", { timeoutMs: 100 });
  assert.equal(job.job_id, "job_a");
  assert.deepEqual(paths, ["/v1/tools/opencut_job_view"]);
});


test("V3 render and speaker turns decode only explicit successful public extensions", async () => {
  const { publicJobResult } = await import("../dist/index.js");
  for (const kind of ["render.project.v3", "preview.project_frame.v3", "preview.project_range.v3", "analysis.speaker_turns"]) {
    const data = {revision:"fixed",width:64};
    const job = {state:"succeeded",kind,result:null,result_unavailable:null,
      extended_result:{kind,schema_version:1,data}};
    assert.deepEqual(publicJobResult(job,kind),data);
    assert.throws(()=>publicJobResult({...job,extended_result:{...job.extended_result,schema_version:2}},kind),TypeError);
    assert.throws(()=>publicJobResult({...job,result_unavailable:{code:"INVALID_STORED_RESULT"}},kind),TypeError);
  }
});
