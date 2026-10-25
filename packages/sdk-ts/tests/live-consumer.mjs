// Copied outside the repository and resolved against the installed tarball by verify-sdk.mjs.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import http from "node:http";
import { OpenCutClient, OpenCutApiError, OpenCutTransportError, JobCompletionError, JobWaitTimeoutError, toolNames, jobResult } from "@opencut/sdk";

const [baseUrl, tokenFile, binary, database, fixturePath, cancelledId, reportPath] = process.argv.slice(2);
const token = readFileSync(tokenFile, "utf8").trim();
const client = new OpenCutClient({ baseUrl, token });
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
const checks = [], traces = [];
const check = (name) => checks.push(name);
function cli(name, input) {
  const path = `${database}.request.json`;
  writeFileSync(path, JSON.stringify(input));
  return JSON.parse(execFileSync(binary, ["--database", database, "--json", "call", name, "--input", path], { encoding: "utf8" })).data;
}
let proxy;
try {
  const capability = await client.callEnvelope("opencut_capabilities", {});
  assert.ok(capability.request_id);
  assert.ok(toolNames.includes("opencut_media_repair"));
  check("installed package calls authenticated discovery and retains response identity");
  const unauthorized = new OpenCutClient({ baseUrl, token: "incorrect" });
  await assert.rejects(unauthorized.call("opencut_project_list", {}), error => error instanceof OpenCutApiError && error.status === 403 && error.code === "NOT_AUTHORIZED");
  check("authentication failure is structured");

  const created = await client.call("opencut_project_create", fixture.create);
  assert.deepEqual(await client.call("opencut_project_create", fixture.create), created);
  const initial=await client.inspectEditorProject({project_id:created.project_id,revision:created.revision});
  assert.equal(initial.revision,created.revision);
  const view={kind:"editor",snapshot:initial};
  assert.deepEqual(await client.callDomain("project", "inspect", { project_id: created.project_id }), view);
  const domainEnvelope = await client.callDomainEnvelope("project", "inspect", { project_id: created.project_id });
  assert.ok(domainEnvelope.request_id);
  assert.deepEqual(domainEnvelope.data, view);
  check("installed SDK domain action and envelope match the canonical project view");
  const edit = { project_id: created.project_id,
    ...fixture.request, expected_revision: created.revision, operations: fixture.operations,
  };
  await client.call("opencut_edit_plan", edit);
  assert.deepEqual(cli("opencut_project_inspect", { project_id: created.project_id }), view);
  check("shared fixture creates idempotently and planning is read-only by independent CLI readback");

  // Real HTTP forwarder: the authoritative host commits, then the response is lost.
  // No fake application/worker or replacement response payload is involved.
  let dropWrite = true, delayReads = false, writeCount = 0;
  proxy = http.createServer(async (req, res) => {
    try {
      const chunks = [];
      for await (const chunk of req) chunks.push(chunk);
      const body = Buffer.concat(chunks);
      const tool = req.url.split("/").at(-1);
      traces.push({ tool, input: JSON.parse(body.toString()) });
      const forwarded = await fetch(`${baseUrl}${req.url}`, { method: "POST", headers: {
        Authorization: `Bearer ${token}`, "Content-Type": "application/json",
      }, body });
      const text = await forwarded.text();
      if (tool === "opencut_edit_apply") {
        writeCount++;
        if (dropWrite) { dropWrite = false; res.destroy(); return; }
      }
      if (delayReads && tool === "opencut_job_view") await new Promise(resolve => setTimeout(resolve, 250));
      res.writeHead(forwarded.status, { "Content-Type": "application/json" });
      res.end(text);
    } catch (error) { res.destroy(error); }
  });
  await new Promise(resolve => proxy.listen(0, "127.0.0.1", resolve));
  const throughProxy = new OpenCutClient({ baseUrl: `http://127.0.0.1:${proxy.address().port}`, token });
  await assert.rejects(throughProxy.call("opencut_edit_apply", edit), error => error instanceof OpenCutTransportError && error.outcome === "unknown");
  assert.equal(writeCount, 1);
  const committed = cli("opencut_project_inspect", { project_id: created.project_id });
  assert.notEqual(committed.snapshot.revision, created.revision);
  const applied = await throughProxy.call("opencut_edit_apply", edit);
  assert.equal(applied.revision, committed.snapshot.revision);
  assert.deepEqual(await client.call("opencut_edit_apply", edit), applied);
  assert.equal(writeCount, 2);
  assert.deepEqual(traces.filter(t => t.tool === "opencut_edit_apply").map(t => t.input), [edit, edit]);
  check("commit followed by real connection loss has no automatic retry; exact manual replay returns original transaction");

  const stale = { ...edit,
    idempotency_key: "sdk-stale", operations: fixture.conflicting_operations,
  };
  await assert.rejects(client.call("opencut_edit_apply", stale), error => {
    assert.ok(error instanceof OpenCutApiError);
    assert.equal(error.code, fixture.expected_conflict.code);
    assert.equal(error.status, fixture.expected_conflict.http_status);
    assert.ok(error.details); assert.ok(error.requestId); return true;
  });
  assert.deepEqual(cli("opencut_project_inspect", { project_id: created.project_id }), committed);
  const reused = structuredClone(edit); reused.operations = fixture.conflicting_operations;
  await assert.rejects(client.call("opencut_edit_apply", reused), error => error.code === "IDEMPOTENCY_KEY_REUSED");
  check("stale revision and reused key reject without revision refresh or unintended edits");
  const events = await client.call("opencut_event_changes", { after_sequence: 0 });
  assert.deepEqual(events, cli("opencut_event_changes", { after_sequence: 0 }));
  assert.equal(events.filter(e => e.revision === applied.revision).length, 1);
  const next = await client.call("opencut_event_changes", { after_sequence: events.at(-1).sequence });
  assert.deepEqual(next, []);
  check("committed event cursor and CLI agree, duplicate submissions produce one commit");

  // A real WAV is generated by FFmpeg, then consumed by the real Worker through the SDK.
  const wave = `${database}.wav`;
  execFileSync("ffmpeg", ["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-y", wave]);
  const accepted = await client.call("opencut_job_submit", { kind: "media.probe", idempotency_key: "sdk-probe", input: { input: wave } });
  delayReads = true;
  await assert.rejects(throughProxy.waitForJob(accepted.job_id, { timeoutMs: 20 }), JobWaitTimeoutError);
  const abort = new AbortController(), reason = new Error("consumer stopped observing");
  const timer = setTimeout(() => abort.abort(reason), 20);
  try { await assert.rejects(throughProxy.waitForJob(accepted.job_id, { signal: abort.signal }), error => error === reason); }
  finally { clearTimeout(timer); }
  assert.ok(!traces.some(t => t.tool === "opencut_job_request_cancel"));
  const completed = await client.waitForJob(accepted.job_id, { timeoutMs: 15_000, pollIntervalMs: 50 });
  assert.equal(completed.state, "succeeded"); assert.equal(completed.cancel_requested, false);
  assert.ok(jobResult(completed, "media.probe").streams.some(s => s.codec_type === "audio"));
  assert.throws(() => jobResult(completed, "render.project"), TypeError);
  assert.deepEqual(cli("opencut_job_view", { job_id: accepted.job_id }), completed);
  check("local wait timeout and caller abort do not cancel a real job; independent CLI confirms successful probe");
  const publicJob = await client.waitForJob(accepted.job_id, { timeoutMs: 15_000, pollIntervalMs: 50 });
  assert.equal(publicJob.state, "succeeded");
  assert.equal(publicJob.job_id, accepted.job_id);
  assert.equal(publicJob.kind, "media.probe");
  check("installed SDK waits through the public job projection on a real worker result");

  const failed = await client.call("opencut_job_submit", { kind: "media.probe", idempotency_key: "sdk-probe-fail", input: { input: `${database}.missing` } });
  await assert.rejects(client.waitForJob(failed.job_id, { timeoutMs: 15_000, pollIntervalMs: 50 }), error => error instanceof JobCompletionError && error.code === "JOB_FAILED" && error.job.error !== null);
  const cancelled = await client.cancelJob(cancelledId);
  assert.equal(cancelled.state, "cancelled");
  await assert.rejects(client.waitForJob(cancelledId), error => error instanceof JobCompletionError && error.code === "JOB_CANCELLED");
  check("failed and explicitly cancelled jobs retain terminal record and distinct error codes");

  writeFileSync(reportPath, JSON.stringify({ status: "passed", checks, project_id: created.project_id, revision: applied.revision,
    transaction: applied, jobs: [accepted.job_id, failed.job_id, cancelledId], traces }, null, 2));
} finally {
  if (proxy) { proxy.closeAllConnections(); await new Promise(resolve => proxy.close(resolve)); }
}
