import { editorSnapshot } from "./project-inspection.mjs";
/** Actual CaptionV4 review, stale draft, selective undo and CLI/MCP parity. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";

const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, value, i, all) =>
  i % 2 ? pairs : [...pairs, [value.replace(/^--/, ""), all[i + 1]]], []));
const { binary, workspace, endpoint } = args;
const hash = value => createHash("sha256").update(value).digest("hex");
const ready = JSON.parse(await fs.readFile(path.join(workspace, "agent-interview-ready.json"), "utf8"));
const checkpoint = path.join(workspace, "agent-interview-browser-checkpoint.json");
const files = [];
async function visit(relative = "") {
  const root = path.resolve("dist");
  for (const entry of await fs.readdir(path.join(root, relative), { withFileTypes: true })) {
    const name = path.join(relative, entry.name);
    if (entry.isDirectory()) await visit(name);
    else if (entry.isFile() && /\.(js|css|mjs|cjs)$/u.test(name)) files.push([name, hash(await fs.readFile(path.join(root, name)))]);
    else if (entry.isSymbolicLink()) throw Error("Unbound candidate symlink");
  }
}
await visit(); files.sort((a, b) => a[0].localeCompare(b[0]));
const candidate = { binary_sha256: hash(await fs.readFile(binary)), web_js_css_tree_sha256: hash(JSON.stringify(files)) };
if (args["native-runtime"]) candidate.native_runtime = JSON.parse(await fs.readFile(args["native-runtime"], "utf8"));
const runtimeTag = candidate.native_runtime ? "-" + hash(JSON.stringify(candidate.native_runtime)).slice(0, 12) : "";
let sourceEquivalence = null;
if (candidate.binary_sha256 !== ready.binary_sha256) {
  expect(args["source-equivalence"], "Changed native build needs explicit source evidence").toBeTruthy();
  sourceEquivalence = JSON.parse(await fs.readFile(args["source-equivalence"], "utf8"));
  expect(["identical", "scoped_equivalent"]).toContain(sourceEquivalence.status);
  const prior = JSON.parse(await fs.readFile(sourceEquivalence.previous_candidate, "utf8"));
  expect(prior.binary_sha256).toBe(ready.binary_sha256);
}
let state;
try { state = JSON.parse(await fs.readFile(checkpoint, "utf8")); }
catch (e) { if (e.code !== "ENOENT") throw e; state = { candidate, stages: {}, requests: {} }; }
if (state.candidate.web_js_css_tree_sha256 !== candidate.web_js_css_tree_sha256) {
  expect(sourceEquivalence?.reusable_web_semantic_tree_sha256 ?? []).toContain(state.candidate.web_js_css_tree_sha256);
  expect(sourceEquivalence.web_source_equivalence.current_web_js_css_tree_sha256).toBe(candidate.web_js_css_tree_sha256);
}
if (state.candidate.binary_sha256 !== candidate.binary_sha256) expect(sourceEquivalence).toBeTruthy();
const save = async () => { await fs.writeFile(checkpoint + ".pending", JSON.stringify(state, null, 2) + "\n"); await fs.rename(checkpoint + ".pending", checkpoint); };
async function stage(name, action) {
  if (state.stages[name]) { console.log("Reuse " + name); return state.stages[name]; }
  console.log("Begin " + name); const result = await action(); state.stages[name] = result;
  (state.stage_candidates ??= {})[name] = candidate;
  await save(); console.log("Passed " + name); return result;
}
function cli(tool, input, error = null) {
  let output;
  try { output = execFileSync(binary, ["--json", "--database", path.join(workspace, "workspace.sqlite"), "call", tool, "--input", "-"], { input: JSON.stringify(input), encoding: "utf8", timeout: 180000 }); }
  catch (e) { if (!error) throw e; output = e.stdout; }
  const response = JSON.parse(output);
  if (error) expect(response.error?.code).toBe(error);
  else expect(response.error, JSON.stringify(response.error)).toBeNull();
  return response.data;
}
const inspect = () => editorSnapshot(cli("opencut_project_inspect", { project_id: ready.project_id, revision: null }));
const scene = snapshot => snapshot.project.sequences.find(s => s.id === ready.sequence_id);
async function request(name, tool, input, error) {
  if (state.requests[name]) expect(state.requests[name]).toEqual({ tool, input });
  else { state.requests[name] = { tool, input }; await save(); }
  return cli(tool, input, error);
}
async function mcpApply(input) {
  const child = spawn(binary, ["--database", path.join(workspace, "workspace.sqlite"), "mcp", "serve"], { stdio: ["pipe", "pipe", "ignore"] });
  const lines = createInterface({ input: child.stdout }), pending = new Map(); let serial = 0;
  lines.on("line", line => { const value = JSON.parse(line); const p = pending.get(value.id); if (p) { pending.delete(value.id); clearTimeout(p.timer); p.resolve(value); } });
  const rpc = async (method, params) => {
    const id = ++serial;
    const response = new Promise((resolve, reject) => { const timer = setTimeout(() => reject(Error("MCP response timeout")), 30000); pending.set(id, { resolve, timer }); });
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    const value = await response; expect(value.error).toBeUndefined(); return value.result;
  };
  try {
    await rpc("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "real-interview-qa", version: "1" } });
    child.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n');
    const plan = (await rpc("tools/call", { name: "opencut_edit", arguments: { action: "v4.plan", input } })).structuredContent;
    expect(plan.error).toBeNull();
    const result = (await rpc("tools/call", { name: "opencut_edit", arguments: { action: "v4.apply", input } })).structuredContent;
    expect(result.error).toBeNull(); return result.data;
  } finally {
    lines.close(); child.stdin.end(); child.kill();
    for (const p of pending.values()) clearTimeout(p.timer);
    if (child.exitCode === null) await new Promise(resolve => child.once("exit", resolve));
  }
}

const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
page.setDefaultTimeout(30000);
const errors = []; page.on("pageerror", e => errors.push(e.message));
const network = [], requests = new WeakMap();
page.on("request", request => {
  const url = new URL(request.url());
  if (!url.pathname.startsWith("/api/") || network.length >= 128) return;
  const row = { path: url.pathname, started_ms: Date.now() };
  network.push(row); requests.set(request, row);
});
page.on("requestfinished", request => {
  const row = requests.get(request); if (row) row.elapsed_ms = Date.now() - row.started_ms;
});
page.on("requestfailed", request => {
  const row = requests.get(request); if (row) { row.elapsed_ms = Date.now() - row.started_ms; row.failure = request.failure()?.errorText; }
});
async function open() {
  await page.goto(`${endpoint}/editor/${encodeURIComponent(ready.project_id)}`);
  await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
  const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
  if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("combobox", { name: "Scene", exact: true }).selectOption(ready.sequence_id);
}
async function editCaption(caption, text) {
  await page.locator(".editor-element-select").filter({ hasText: caption.text }).first().click();
  await page.getByRole("button", { name: "Edit caption text", exact: true }).click();
  await page.getByRole("textbox", { name: "Caption text", exact: true }).fill(text);
}
try {
  await open();
  const reviewed = await stage("actual-ui-caption-review", async () => {
    if (!state.baseline) {
      const before = inspect(); expect(before.revision).toBe(ready.revision);
      const captions = scene(before).captions;
      // Prefer a reference-supported recognition correction; otherwise label
      // the first speaker using the official interview source credit.
      const caption = captions.find(c => /solar radiance|aeros(?:ol)? atmosphere/iu.test(c.text)) ?? captions[0];
      const corrected = caption.text.replace(/solar radiance/giu, "solar irradiance").replace(/aeros(?:ol)? atmosphere/giu, "Earth's atmosphere");
      state.baseline = { snapshot: before, caption, text: corrected === caption.text ? "Bryan Fafaul: " + caption.text : corrected };
      await save();
    }
    const { snapshot, caption, text } = state.baseline;
    if (!state.ui_request) {
      await page.route("**/api/tools/opencut_edit_plan", async route => {
        state.ui_request = route.request().postDataJSON(); await save(); await route.continue();
      });
      await editCaption(caption, text);
      await page.getByRole("button", { name: "Save caption text", exact: true }).click();
      await expect.poll(() => scene(inspect()).captions.find(c => c.id === caption.id)?.text, { timeout: 30000 }).toBe(text);
      await page.unroute("**/api/tools/opencut_edit_plan");
    }
    expect(state.ui_request.expected_revision).toBe(snapshot.revision);
    expect(state.ui_request.operations).toEqual([{ type: "caption_put", sequence_id: ready.sequence_id, caption: { ...caption, text } }]);
    const receipt = cli("opencut_edit_apply", state.ui_request);
    const after = editorSnapshot(cli("opencut_project_inspect", { project_id: ready.project_id, revision: receipt.revision }));
    expect(scene(after).captions.find(c => c.id === caption.id)).toEqual({ ...caption, text });
    cli("opencut_edit_plan", { ...state.ui_request, idempotency_key: "interview-stale-agent-plan" }, "REVISION_CONFLICT");
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(String(Number(caption.timeline_range.start.ticks) / caption.timeline_range.start.timebase + .1));
    await page.getByLabel("Playhead (seconds)", { exact: true }).press("Enter");
    await page.screenshot({ path: path.join(workspace, "agent-interview-caption-review.png") });
    return { revision: receipt.revision, receipt, caption_id: caption.id, text, operations: state.ui_request.operations };
  });
  await stage("cli-mcp-ui-semantic-parity", async () => {
    const outputs = [];
    for (const transport of ["cli", "mcp"]) {
      const key = "interview-parity-" + transport;
      const initial = await request(key + "-copy", "opencut_project_copy", {
        source_project_id: ready.project_id, source_revision: state.baseline.snapshot.revision,
        project_id: key, name: key, idempotency_key: key + ":copy" });
      const input = { ...state.ui_request, project_id: key, expected_revision: initial.revision, idempotency_key: key + ":apply" };
      let receipt;
      if (transport === "mcp") receipt = await mcpApply(input);
      else { cli("opencut_edit_plan", input); receipt = await request(key, "opencut_edit_apply", input); }
      const snapshot = editorSnapshot(cli("opencut_project_inspect", { project_id: key, revision: receipt.revision }));
      snapshot.project.id = ready.project_id; snapshot.project.name = state.baseline.snapshot.project.name;
      const ui = editorSnapshot(cli("opencut_project_inspect", { project_id: ready.project_id, revision: reviewed.revision }));
      expect(snapshot.project).toEqual(ui.project);
      outputs.push({ transport, revision: receipt.revision });
    }
    return { outputs, compared: "Complete semantic ProjectDocumentV4; only copy project id/name normalized" };
  });
  await stage("remaining-reference-caption-corrections", async () => {
    if (!state.additional) {
      const before = inspect();
      const caption = scene(before).captions.find(c => /solar radiance/iu.test(c.text));
      state.additional = caption ? { caption, text: caption.text.replace(/solar radiance/giu, "solar irradiance"), revision: before.revision } : {};
      await save();
    }
    const { caption, text } = state.additional;
    if (!caption) return { needed: false };
    if (scene(inspect()).captions.find(c => c.id === caption.id)?.text !== text) {
      await editCaption(caption, text);
      await page.getByRole("button", { name: "Save caption text", exact: true }).click();
      await expect.poll(() => scene(inspect()).captions.find(c => c.id === caption.id)?.text, { timeout: 30000 }).toBe(text);
    }
    expect(scene(inspect()).captions.find(c => c.id === caption.id)).toEqual({ ...caption, text });
    return { caption_id: caption.id, before: caption.text, after: text, revision: inspect().revision };
  });
  await stage("stale-draft-and-selective-agent-undo", async () => {
    const caption = scene(inspect()).captions.find(c => c.id === reviewed.caption_id);
    await editCaption(caption, caption.text + " [unsaved draft]");
    const before = inspect();
    const mutation = state.requests["concurrent-agent-rename"]?.input ?? {
      project_id: ready.project_id, expected_revision: before.revision, idempotency_key: "interview-concurrent-agent-rename",
      label: "Concurrent Agent rename while caption draft is open", operations: [{ type: "rename", name: before.project.name + " reviewed" }] };
    const renamed = await request("concurrent-agent-rename", "opencut_edit_apply", mutation);
    await expect(page.getByRole("alert").filter({ hasText: "The caption changed in another revision." })).toBeVisible({ timeout: 15000 });
    await expect(page.getByRole("textbox", { name: "Caption text", exact: true })).toHaveValue(caption.text + " [unsaved draft]");
    await expect(page.getByRole("button", { name: "Save caption text", exact: true })).toBeDisabled();
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    const undo = { project_id: ready.project_id, expected_revision: inspect().revision,
      target_transaction_id: ready.caption_transaction_id, idempotency_key: "interview-undo-agent-captions" };
    const proposal = cli("opencut_edit_undo_plan", undo);
    expect(proposal.conflicts.length).toBeGreaterThan(0);
    cli("opencut_edit_undo_apply", undo, "SELECTIVE_CONFLICT");
    expect(scene(inspect()).captions.find(c => c.id === caption.id)).toEqual(caption);
    const independent = state.requests["undo-independent-agent-rename"]?.input ?? {
      ...undo, expected_revision: inspect().revision, target_transaction_id: renamed.transaction_id, idempotency_key: "interview-undo-independent-agent-rename" };
    expect(cli("opencut_edit_undo_plan", independent).conflicts).toHaveLength(0);
    const receipt = await request("undo-independent-agent-rename", "opencut_edit_undo_apply", independent);
    expect(scene(inspect()).captions.find(c => c.id === caption.id)).toEqual(caption);
    await expect(page.locator(".editor-history")).toContainText("local-owner · cli", { timeout: 15000 });
    await expect(page.locator(".editor-history")).toContainText("local-owner · http", { timeout: 15000 });
    return { receipt, conflicting_caption_undo: proposal.conflicts, original_draft_preserved: true };
  });
  if (args.phase !== "reopen") await stage("corrected-interview-continuous-preview" + runtimeTag, async () => {
    await open();
    const caption = scene(inspect()).captions.find(c => c.id === reviewed.caption_id);
    const start = Number(caption.timeline_range.start.ticks) / caption.timeline_range.start.timebase + .1;
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(String(start));
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
    const live = page.locator("canvas.editor-scene-frame");
    await expect(live).toBeVisible({ timeout: 60000 });
    const times = [];
    await expect.poll(async () => {
      const time = Number(await live.getAttribute("data-time"));
      if (Number.isFinite(time) && !times.includes(time)) times.push(time);
      return times.filter(time => time >= start).length;
    }, { timeout: 30000 }).toBeGreaterThanOrEqual(3);
    await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
    await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible();
    const time = Number(await live.getAttribute("data-time"));
    const picture = path.join(workspace, "agent-interview-corrected-live-preview.png");
    await live.screenshot({ path: picture });
    return { revision: inspect().revision, caption_id: caption.id, observed_times: times, paused_time: time,
      generation: await live.getAttribute("data-generation"), runtime: await live.getAttribute("data-runtime"), picture,
      scope: "Short actual continuous browser sound/picture preview of the corrected caption; no hardware lip-sync or full playback benchmark" };
  });
  if (args.phase === "reopen") await stage("reopen-after-fixed-export-and-continue-editing" + runtimeTag, async () => {
    const exported = JSON.parse(await fs.readFile(path.join(workspace, "agent-interview-export.json"), "utf8"));
    expect(exported.status).toBe("exported_pending_reopen");
    await open();
    if (!state.reopen) {
      const before = inspect(); expect(before.revision).not.toBe(exported.export_revision);
      const caption = scene(before).captions.find(c => c.text.includes("NASA's"));
      expect(caption).toBeTruthy();
      state.reopen = { caption, text: caption.text.replace("NASA's", "NASA’s"), revision: before.revision };
      await save();
    }
    const { caption, text } = state.reopen;
    if (scene(inspect()).captions.find(c => c.id === caption.id)?.text !== text) {
      await editCaption(caption, text);
      await page.getByRole("button", { name: "Save caption text", exact: true }).click();
      await expect.poll(() => scene(inspect()).captions.find(c => c.id === caption.id)?.text).toBe(text);
    }
    expect(scene(inspect()).captions.find(c => c.id === caption.id)).toEqual({ ...caption, text });
    const after = inspect();
    await page.reload();
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    await expect(page.locator(".editor-element-select").filter({ hasText: text })).toBeVisible();
    expect(inspect().revision).toBe(after.revision);
    return { export_job_id: exported.job_id, export_revision: exported.export_revision,
      opened_revision: state.reopen.revision, continued_revision: after.revision, before: caption.text, after: text };
  });
  expect(errors).toEqual([]);
  const report = { status: "passed_scoped", candidate, reused_semantic_candidate: state.candidate,
    source_equivalence: sourceEquivalence, stage_candidates: state.stage_candidates ?? {},
    project_id: ready.project_id, revision: inspect().revision,
    corrected_caption_id: reviewed.caption_id, stages: state.stages, pageerrors: errors };
  await fs.writeFile(path.join(workspace, "agent-interview-browser.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify({ status: report.status, revision: report.revision, candidate }));
} catch (e) {
  const picture = path.join(workspace, `agent-interview-browser-failure-${candidate.binary_sha256.slice(0, 12)}-${args.phase ?? "review"}.png`);
  await page.screenshot({ path: picture }).catch(() => {});
  state.last_failure = { candidate, phase: args.phase ?? "review", picture, message: String(e).slice(0, 1000) };
  await save();
  throw e;
} finally {
  await page.close(); await browser.close();
  await fs.writeFile(path.join(workspace, "agent-interview-browser-network.json"), JSON.stringify({ candidate, errors, network }, null, 2) + "\n");
}
