/** Durable preview recovery on the owned original workspace. No media jobs. */
import { chromium, expect as baseExpect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";

// Readiness can include real runtime qualification (observed 7615ms). Native
// request/resource limits and the explicit six-second retirement check stay put.
const expect = baseExpect.configure({ timeout: 25000 });

const args = Object.fromEntries(process.argv.slice(2).reduce((all, v, i, list) => i % 2 ? all : [...all, [v.slice(2), list[i + 1]]], []));
for (const k of ["workspace", "candidate", "endpoint", "chromium", "report"]) if (!args[k]) throw Error("Missing --" + k);
const phase = args.phase ?? "recovery";
if (!["recovery", "outage", "bfcache"].includes(phase)) throw Error("Unknown phase");
const read = async p => JSON.parse(await fs.readFile(p, "utf8"));
const sha = b => createHash("sha256").update(b).digest("hex");
const candidate = await read(args.candidate), service = await read(path.join(args.workspace, "service/state.json"));
expect(await fs.realpath(service.executable)).toBe(await fs.realpath(candidate.binary));
expect(await fs.realpath(service.web_root)).toBe(await fs.realpath(candidate.web_root));
expect(sha(await fs.readFile(candidate.binary))).toBe(candidate.binary_sha256);
const manifest = await read(candidate.source_manifest);
const relevant = Object.keys(manifest).filter(p => /(?:audio[_-]preview|web_host\/session\.rs)/.test(p) && !p.includes("scripts/"));
for (const p of relevant) expect(sha(await fs.readFile(p)), p).toBe(manifest[p]);
const identity = { candidate: args.candidate, binary: candidate.binary_sha256, web: candidate.web_js_css_tree_sha256, browser: sha(await fs.readFile(args.chromium)) };
const checkpoint = path.join(args.workspace, "agent-preview-recovery-current-checkpoint.json");
const state = await read(checkpoint).catch(e => { if (e.code !== "ENOENT") throw e; return { identity, stages: {} }; });
expect(state.identity).toEqual(identity);
if (phase === "bfcache" && state.bfcache_probe?.same_document === false && args.recheck_bfcache !== "yes") {
  console.error("Reuse recorded BFCache exclusion for this exact candidate/browser;recheck only after an actual policy/profile/scope change.");
  process.exitCode = 2;
  throw Error("BFCache is not admitted by the retained no-store profile;the failed probe is authoritative,not a missing playback run");
}
const save = async () => {
  const text = JSON.stringify(state, null, 2) + "\n";
  await fs.writeFile(checkpoint + ".pending", text, { mode: 0o600 }); await fs.rename(checkpoint + ".pending", checkpoint);
  await fs.writeFile(args.report, text, { mode: 0o600 });
};
const stage = async (name, run) => {
  if (state.stages[name]) { console.log("Reuse " + name); return; }
  console.log("Begin " + name); state.stages[name] = await run(); delete state.failure; await save(); console.log("Passed " + name);
};
let client;
const refreshClient = async () => { client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(path.join(args.workspace, "service/api-token"), "utf8")).trim() }); };
await refreshClient();
const call = (name, value) => client.call(name, value);
const slot = project_id => call("opencut_audio_preview_recovery_inspect", { project_id });
const native = session_id => call("opencut_audio_preview_inspect", { session_id });
const graph = () => JSON.parse(execFileSync("python3", ["-B", "-c", `
import hashlib,json,sqlite3,sys
c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)
tables={'projects':'select project_id,name,head_revision from projects', 'revisions':'select revision from revisions','transactions':'select transaction_id from editor_transactions','jobs':'select job_id,state,attempt,cancel_requested from jobs'}
result={}
for k,q in tables.items():
 rows=sorted(c.execute(q).fetchall(),key=repr);result[k]={'count':len(rows),'sha256':hashlib.sha256(repr(rows).encode()).hexdigest()}
print(json.dumps(result))
`, path.join(args.workspace, "workspace.sqlite")], { encoding: "utf8" }));
if (phase === "recovery") state.baseline ??= graph();
else if (phase === "outage") state.outage_baseline ??= graph();
else state.bfcache_baseline ??= graph();
await save();
const fixture = await read(path.join(args.workspace, "scene-preview-fixture.json"));
const errors = [], browsers = new Set();
const observe = session => ({ session_id: session.session_id, generation: session.generation, state: session.state, overlay_epoch: session.overlay_epoch });
async function document(project, { bfcache = false } = {}) {
  const browser = await chromium.launch({ headless: true, executablePath: args.chromium,
    ...(bfcache ? { ignoreDefaultArgs: ["--disable-back-forward-cache"] } : {}) }); browsers.add(browser);
  const context = await browser.newContext({ locale: "en-US", viewport: { width: 1440, height: 1000 } });
  // Only audio admission is in scope; do not qualify nine unrelated outputs.
  if (!bfcache) await context.route("**/api/tools/*_capability", route => route.request().url().endsWith("/opencut_audio_preview_capability") ? route.continue() : route.abort("failed"));
  else await context.addInitScript(() => {
    // No network routing/cache disabling during actual BFCache admission.
    // The unrelated output availability scope matches the other lifecycle cases.
    const original = globalThis.fetch;
    globalThis.fetch = (input, init) => {
      const url = typeof input === "string" ? input : input.url;
      if (url.endsWith("_capability") && !url.endsWith("/opencut_audio_preview_capability")) return Promise.reject(new TypeError("Unrelated output discovery is controlled unavailable for lifecycle acceptance"));
      return original(input, init);
    };
  });
  const page = await context.newPage(); page.setDefaultTimeout(25000);
  page.on("pageerror", e => errors.push(e.message));
  await page.addInitScript(() => {
    globalThis.__recoveryPump = { starts: [], progress: false, closed: 0 };
    const Original = globalThis.Worker;
    globalThis.Worker = class extends Original {
      constructor(...args) {
        super(...args); if (args[1]?.name !== "opencut-audio-pump") return;
        const send = this.postMessage.bind(this), stop = this.terminate.bind(this);
        this.postMessage = (data, ...rest) => {
          if (data.channel === "opencut-audio-pump" && data.kind === "start") globalThis.__recoveryPump.starts.push(data.session.session_id);
          return send(data, ...rest);
        };
        this.terminate = () => { globalThis.__recoveryPump.closed++; return stop(); };
        this.addEventListener("message", ({ data }) => { if (data.channel === "opencut-audio-pump" && data.kind === "progress" && BigInt(data.receipt.consumedFrames) > 0n) globalThis.__recoveryPump.progress = true; });
      }
    };
  });
  const open = async () => {
    await page.goto(args.endpoint + "/editor/" + project);
    await expect(page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible();
  };
  const close = async () => { await browser.close(); browsers.delete(browser); };
  return { browser, context, page, open, close };
}
const sameAuthority = (a, b) => { expect(b.owner_id).toBe(a.owner_id); expect(b.lease_id).toBe(a.lease_id); expect(b.identity).toEqual(a.identity); };
try {
  if (phase === "recovery") {
  await stage("retained-dead-host-slot", async () => {
    const project = "agent-real-interview-v4-acceptance";
    const retained = (await slot(project)).record; expect(retained).not.toBeNull();
    expect(retained.status).toBe("staged"); state.retained_dead_host_slot = retained; await save();
    await expect(call("opencut_audio_preview_recovery_dispatch", { project_id: project, recovery_id: retained.recovery_id })).rejects.toMatchObject({ code: "PREVIEW_RECOVERY_RETIRED" });
    const retired = (await slot(project)).record; expect(retired.status).toBe("retired");
    const d = await document(project), requests = [];
    d.page.on("request", r => { if (/\/opencut_audio_preview_(?:recovery_(?:stage|dispatch|ack)|create)$/.test(r.url())) requests.push({ tool: r.url().split("/").at(-1), input: r.postDataJSON() }); });
    try {
      await d.open(); await expect.poll(async () => (await slot(project)).record).toBeNull();
      expect(requests).toEqual([{ tool: "opencut_audio_preview_recovery_ack", input: { project_id: project, recovery_id: retained.recovery_id, disposition: "retire" } }]);
      expect(await d.page.evaluate(() => globalThis.__recoveryPump.starts)).toEqual([]);
      expect(graph()).toEqual(state.baseline);
      return { original_record: retained, old_host_dispatch_refused: "PREVIEW_RECOVERY_RETIRED", automatic_retire_only: true, no_create_dispatch_or_pcm: true, retained_stage_cleared: true };
    } finally { await d.close(); }
  });
  await stage("unknown-create-exact-retry", async () => {
    expect((await slot(fixture.project_id)).record).toBeNull();
    const d = await document(fixture.project_id), records = new Map(), dispatches = [];
    let unknown, playing;
    await d.context.route("**/api/tools/opencut_audio_preview_recovery_stage", async route => {
      const original = route.request().postDataJSON(), response = await route.fetch(), record = (await response.json()).data;
      expect(record).toBeTruthy(); records.set(record.recovery_id, { original, record }); await route.fulfill({ response });
    });
    await d.context.route("**/api/tools/opencut_audio_preview_recovery_dispatch", async route => {
      const input = route.request().postDataJSON(), response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull();
      const saved = records.get(input.recovery_id); expect(saved).toBeTruthy(); dispatches.push({ input, original: saved.original, receipt: observe(body.data) });
      if (!unknown && saved.original.request.kind === "create") {
        unknown = { ...saved, input, receipt: body.data }; state.pending_unknown_create = { record: saved.record, original: saved.original, receipt: observe(body.data) }; await save();
        await route.abort("failed");
      } else { if (body.data.state === "playing") playing = body.data; await route.fulfill({ response }); }
    });
    try {
      await d.open(); await expect(d.page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled();
      await d.page.getByRole("button", { name: "Play scene audio", exact: true }).click();
      const retry = d.page.getByRole("button", { name: "Retry original audio session", exact: true }); await expect(retry).toBeVisible();
      expect((await slot(fixture.project_id)).record).toMatchObject({ recovery_id: unknown.record.recovery_id, status: "completed" });
      const replacement = structuredClone(unknown.original); replacement.request.request.idempotency_key += ":unrelated";
      await expect(call("opencut_audio_preview_recovery_stage", replacement)).rejects.toMatchObject({ code: "RECOVERY_CONFLICT" });
      await retry.click(); await expect.poll(() => !!playing).toBe(true);
      await expect.poll(() => d.page.evaluate(() => globalThis.__recoveryPump.progress)).toBe(true);
      const creates = dispatches.filter(v => v.original.request.kind === "create"); expect(creates).toHaveLength(2);
      expect(creates[0]).toEqual(creates[1]); expect(playing.session_id).toBe(unknown.receipt.session_id); sameAuthority(unknown.receipt, playing);
      await d.page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
      await expect(d.page.getByText("Scene sound paused", { exact: true })).toBeVisible();
      const paused = await native(playing.session_id); expect(paused.state).toBe("paused"); sameAuthority(playing, paused);
      await d.page.goto(args.endpoint + "/projects");
      let stopped; await expect.poll(async () => { stopped = await native(playing.session_id); return stopped.state; }, { timeout: 6000 }).toBe("stopped");
      await expect.poll(async () => (await slot(fixture.project_id)).record).toBeNull();
      expect(graph()).toEqual(state.baseline);
      return { original_create_sha256: sha(JSON.stringify(unknown.original)), recovery_id: unknown.record.recovery_id, duplicate_admission_rejected: "RECOVERY_CONFLICT", exact_dispatch_replay: true, single_session: playing.session_id, acknowledged_play_then_pause_then_stop: true, owner_lease_identity_preserved: true, actual_pcm_progress: true, durable_slot_cleared: true };
    } finally { await d.close(); }
  });
  await stage("renderer-crash-accepted-seek-reload", async () => {
    expect((await slot(fixture.project_id)).record).toBeNull();
    const d = await document(fixture.project_id), records = new Map(); let playing, accepted, release;
    const gate = new Promise(resolve => { release = resolve; });
    await d.context.route("**/api/tools/opencut_audio_preview_recovery_stage", async route => {
      const original = route.request().postDataJSON(), response = await route.fetch(), record = (await response.json()).data;
      expect(record).toBeTruthy(); records.set(record.recovery_id, { original, record }); await route.fulfill({ response });
    });
    await d.context.route("**/api/tools/opencut_audio_preview_recovery_dispatch", async route => {
      const input = route.request().postDataJSON(), response = await route.fetch(), body = await response.json(); expect(body.error).toBeNull();
      const saved = records.get(input.recovery_id); expect(saved).toBeTruthy();
      if (saved.original.request.kind === "control" && saved.original.request.request.command.kind === "seek" && !accepted) {
        accepted = { ...saved, input, receipt: body.data }; state.pending_crash_seek = { record: saved.record, request_sha256: sha(JSON.stringify(saved.original)), receipt: observe(body.data) }; await save();
        await gate; await route.abort("failed").catch(() => {});
      } else { if (body.data.state === "playing") playing = body.data; await route.fulfill({ response }); }
    });
    try {
      await d.open(); await expect(d.page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled();
      await d.page.getByRole("button", { name: "Play scene audio", exact: true }).click();
      await expect.poll(() => !!playing).toBe(true); await expect.poll(() => d.page.evaluate(() => globalThis.__recoveryPump.progress)).toBe(true);
      await d.page.getByLabel("Playhead (seconds)", { exact: true }).fill("1.25"); await d.page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
      await expect.poll(() => !!accepted).toBe(true); expect((await slot(fixture.project_id)).record).toMatchObject({ recovery_id: accepted.record.recovery_id, status: "completed" });
      expect(BigInt(accepted.receipt.generation)).toBe(BigInt(playing.generation) + 1n); sameAuthority(playing, accepted.receipt);
      const cdp = await d.context.newCDPSession(d.page), crashed = d.page.waitForEvent("crash", { timeout: 6000 });
      void cdp.send("Page.crash").catch(() => {}); await crashed; release();
      const persisted = (await slot(fixture.project_id)).record; expect(persisted).toMatchObject({ recovery_id: accepted.record.recovery_id, status: "completed" });
      await d.close();
      const reopened = await document(fixture.project_id), requests = [];
      reopened.page.on("request", r => { if (/\/opencut_audio_preview_(?:recovery_(?:stage|dispatch|ack)|create)$/.test(r.url())) requests.push({ tool: r.url().split("/").at(-1), input: r.postDataJSON() }); });
      try {
        await reopened.open(); await expect.poll(async () => (await slot(fixture.project_id)).record).toBeNull();
        expect(requests).toEqual([{ tool: "opencut_audio_preview_recovery_ack", input: { project_id: fixture.project_id, recovery_id: accepted.record.recovery_id, disposition: "retire" } }]);
        const stopped = await native(accepted.receipt.session_id); expect(stopped.state).toBe("stopped"); sameAuthority(accepted.receipt, stopped);
        expect(await reopened.page.evaluate(() => globalThis.__recoveryPump.starts)).toEqual([]); expect(graph()).toEqual(state.baseline);
        return { session_id: accepted.receipt.session_id, recovery_id: accepted.record.recovery_id, accepted_seek_generation: accepted.receipt.generation, actual_renderer_crash: true, durable_completed_slot_survived: true, reload_only_retired_original: true, stopped_without_replay_or_pcm_restart: true, owner_lease_identity_preserved: true };
      } finally { await reopened.close(); }
    } finally { release(); if (browsers.has(d.browser)) await d.close(); }
  });
  } else if (phase === "outage") {
    const { verifyPreviewOutages } = await import("./preview-outage-cases.mjs");
    await verifyPreviewOutages({ args, candidate, state, stage, save, refreshClient, call, slot, native, graph, fixture, document, observe, sameAuthority, expect });
  } else {
    const { verifyActualBfCache } = await import("./preview-bfcache-case.mjs");
    await verifyActualBfCache({ args, state, stage, save, call, slot, native, graph, fixture, document, observe, sameAuthority, expect });
  }
  expect(errors).toEqual([]); expect(graph()).toEqual(phase === "recovery" ? state.baseline : phase === "outage" ? state.outage_baseline : state.bfcache_baseline);
  state.page_errors = errors; state.relevant_product_sources_unchanged = relevant.length;
  state.executed_verifier_sha256 = sha(await fs.readFile(import.meta.filename));
  state.limits = phase === "recovery" ? ["Lifecycle/recovery only;prior PCM/mastering/long-source/S1 evidence retains its original candidate.", "No BFCache eligibility or eight-hour wall-clock expiry claim;unchanged session expiry/body/stream tests are reused separately.", "Renderer crash is real CDPPage.crash;the preexisting dead-host slot is real retained evidence,not a newly fabricated process-incarnation row.", "No new project,media import,ASR,export job or product build."] : phase === "outage" ? ["One actual browser-offline pause and one owned Native serviceSIGKILL with acceptedSeek;not arbitrary OS/device/browser fault matrices.", "No source change/build/newunit/media import/ASR/output job or full S1/BFCache/performance claim.", "Previously completed recovery stages retain their original report/source and baseline;the outage round has its own post-collaboration baseline."] : ["Actual persisted pageshow/pagehide and same-document identity on bundledChromium155;not other browsers/platforms or a manual OS/device claim.", "Only Playwright's default BFCache-disabling argument is removed;production headers/policy unchanged,no network routing or fabricated lifecycle events.", "No source/build/newunit/project copy/media import/ASR/output job or full S1/performance claim."];
  await save();
} catch (e) { state.failure = { phase, attempted_verifier_sha256: sha(await fs.readFile(import.meta.filename)), message: e.message, stack: e.stack, page_errors: errors }; await save(); throw e; }
finally { for (const browser of browsers) await browser.close().catch(() => {}); }
