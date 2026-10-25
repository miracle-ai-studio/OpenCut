import { editorSnapshot } from "./project-inspection.mjs";
/** One restored workspace; original historical request, current UI and history. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";

const args = Object.fromEntries(process.argv.slice(2).reduce((all, value, i, list) => i % 2 ? all : [...all, [value.slice(2), list[i + 1]]], []));
for (const key of ["source", "endpoint", "chromium", "report", "candidate"]) if (!args[key]) throw Error(`Missing --${key}`);
const read = async file => JSON.parse(await fs.readFile(file, "utf8"));
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const retained = await read(path.join(args.source, "agent-history-package-v1-checkpoint.json"));
const candidate = await read(args.candidate), workspace = retained.target_workspace;
const service = await read(path.join(workspace, "service/state.json"));
expect(await fs.realpath(service.database)).toBe(await fs.realpath(path.join(workspace, "workspace.sqlite")));
expect(await fs.realpath(service.executable)).toBe(await fs.realpath(candidate.binary));
expect(await fs.realpath(service.web_root)).toBe(await fs.realpath(candidate.web_root));
expect(sha(await fs.readFile(candidate.binary))).toBe(candidate.binary_sha256);
const identity = { candidate: args.candidate, retained_source_candidate: retained.candidate, native: candidate.binary_sha256, web: candidate.web_js_css_tree_sha256, browser: sha(await fs.readFile(args.chromium)) };
const checkpoint = args.checkpoint ?? path.join(workspace, "agent-history-package-ui-checkpoint.json");
const state = await read(checkpoint).catch(e => { if (e.code !== "ENOENT") throw e; return { identity, stages: {}, requests: {} }; });
expect(state.identity).toEqual(identity);
const save = async () => {
  const text = JSON.stringify(state, null, 2) + "\n";
  await fs.writeFile(checkpoint + ".pending", text); await fs.rename(checkpoint + ".pending", checkpoint);
  await fs.writeFile(args.report, text);
};
const stage = async (name, run) => {
  if (state.stages[name]) { console.log("Reuse " + name); return; }
  console.log("Begin " + name); state.stages[name] = await run(); delete state.failure; await save(); console.log("Passed " + name);
};
const counts = () => JSON.parse(execFileSync("python3", ["-B", "-c", `
import json,sqlite3,sys
c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)
print(json.dumps({table:c.execute('select count(*) from '+table).fetchone()[0] for table in ['projects','revisions','editor_transactions','jobs']}))
`, path.join(workspace, "workspace.sqlite")], { encoding: "utf8" }));
const client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(path.join(workspace, "service/api-token"), "utf8")).trim() });
const project = retained.requests.import.target_project_id;
const inspect = () => editorSnapshot(client.call("opencut_project_inspect", { project_id: project, revision: null }));
state.initial ??= { counts: counts(), snapshot: await inspect() }; await save();
let browser, context, release;
const errors = [];
try {
  browser = await chromium.launch({ headless: true, executablePath: args.chromium });
  context = await browser.newContext({ locale: "en-US", viewport: { width: 1440, height: 1000 } });
  // Real project/history/recovery; qualification was already proven by the one
  // native PNG. Do not rediscover all expensive output backends on every reload.
  await context.route("**/api/tools/*_capability", route => route.abort("failed"));
  const page = await context.newPage(); page.setDefaultTimeout(30000);
  page.on("pageerror", e => errors.push(e.message));
  const library = async () => {
    await page.goto(args.endpoint + "/");
    await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
  };
  const dialog = async () => {
    await page.getByRole("button", { name: "Restore project package", exact: true }).click();
    const modal = page.getByRole("dialog", { name: "Restore project package", exact: true });
    await expect(modal.getByRole("button", { name: "Retry restore", exact: true })).toBeEnabled();
    return modal;
  };
  await stage("workspace-authority-isolation", async () => {
    const oldToken = (await fs.readFile(path.join(args.source, "service/api-token"), "utf8")).trim();
    const response = await fetch(args.endpoint + "/api/tools/opencut_access_inspect", { method: "POST", headers: { "Content-Type": "application/json", Authorization: "Bearer " + oldToken }, body: "{}", signal: AbortSignal.timeout(20000) });
    expect(response.status).toBe(403); expect((await response.json()).error.code).toBe("NOT_AUTHORIZED");
    expect((await client.call("opencut_access_inspect", {})).owner).toBe(true);
    return { source_bearer_rejected: true, target_owner_available: true };
  });
  await stage("unknown-ui-response-reload-retry", async () => {
    // The retained first attempt reached the exact native receipt after the
    // real withheld-response/reload sequence. Its only failure was two status
    // elements matching one broad verifier locator. Resume at receipt display.
    const resumeReceipt = state.failure?.message.includes("strict mode violation") &&
      state.failure.message.includes("220 revisions · 219 edits") && state.failure.message.includes("Project restored");
    if (resumeReceipt) {
      state.retained_verifier_failure ??= structuredClone(state.failure);
      expect(counts()).toEqual(state.initial.counts); await save();
    }
    await library();
    const access = await client.call("opencut_access_inspect", {});
    expect(access.owner).toBe(true);
    const identity = JSON.stringify([client.baseUrl, access.principal_id, access.credential_id ?? null, access.owner]);
    // Seed only the already authorized original public request and inspection,
    // never tokens, documents or blobs. Original import producer remains CLI.
    await page.evaluate(value => sessionStorage.setItem("opencut.history-package.v1", JSON.stringify(value)), {
      version: 1, entries: [{ identity, value: { request: retained.requests.import, inspection: retained.receipts.inspect } }],
    });
    let held = false, dropped = resumeReceipt;
    const gate = new Promise(resolve => { release = resolve; });
    const observed = [];
    await context.route("**/api/tools/opencut_project_package_import", async route => {
      const input = route.request().postDataJSON(); expect(input).toEqual(retained.requests.import); observed.push(input);
      const response = await route.fetch(), envelope = await response.json();
      expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
      const receipt = envelope.data;
      const { self_contained_resources, history_included, fully_reproducible, ...originalFacts } = retained.receipts.inspect;
      expect({ self_contained_resources, history_included, fully_reproducible }).toEqual({
        self_contained_resources: true, history_included: true, fully_reproducible: false,
      });
      expect(receipt).toMatchObject({
        target_project_id: retained.receipts.import.target_project_id,
        revision: retained.receipts.import.revision,
        package: { ...originalFacts, mode: "self_contained", scope: "history", authoring_available: true },
        plugin_restore_plan: [],
      });
      if (state.current_import_receipt) expect(receipt).toEqual(state.current_import_receipt);
      else { state.current_import_receipt = receipt; await save(); }
      if (!dropped) {
        dropped = true; held = true; await gate;
        await route.abort("failed").catch(() => {});
      } else await route.fulfill({ response });
    });
    if (!resumeReceipt) {
      await (await dialog()).getByRole("button", { name: "Retry restore", exact: true }).click();
      await expect.poll(() => held).toBe(true);
      await page.reload(); release();
      await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
      expect(await page.evaluate(() => JSON.parse(sessionStorage.getItem("opencut.package")).entries.length)).toBe(1);
    }
    const modal = await dialog();
    await modal.getByRole("button", { name: "Retry restore", exact: true }).click();
    await expect(modal.getByText("Project restored", { exact: true })).toBeVisible();
    expect(observed).toHaveLength(resumeReceipt ? 1 : 2);
    expect(counts()).toEqual(state.initial.counts);
    expect(await page.evaluate(() => JSON.parse(sessionStorage.getItem("opencut.package")).entries)).toEqual([]);
    // Historical storage is read-only. The current empty envelope must prevent
    // resurrection of the original legacy entry after a successful replay.
    expect(await page.evaluate(() => JSON.parse(sessionStorage.getItem("opencut.history-package.v1")).entries.length)).toBe(1);
    await modal.getByRole("button", { name: "Open restored project", exact: true }).click();
    await expect(page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible();
    await page.screenshot({ path: path.join(path.dirname(args.report), "restored-editor.png") });
    expect(await inspect()).toEqual(state.initial.snapshot);
    await context.unroute("**/api/tools/opencut_project_package_import");
    return { original_import_producer: "CLI", original_exact_replays: resumeReceipt ? 3 : 2, reused_first_unknown_response_prefix: resumeReceipt, response_withheld_then_page_reloaded: true, same_receipt: true, recovery_cleared_only_after_receipt: true, no_duplicate_project_history_or_job: true, real_editor_open_and_reload: true };
  });
  await stage("imported-causal-redo-undo", async () => {
    if (!page.url().includes("/editor/")) await page.goto(args.endpoint + "/editor/" + project);
    const history = page.locator(".editor-history"); await expect(history).toBeVisible();
    const navigation = await client.call("opencut_project_history_navigation", { project_id: project, revision: null });
    if (!state.redo_receipt) expect(navigation.redo).not.toBeNull();
    state.imported_navigation ??= navigation; await save();
    const commit = async (name, label, action) => {
      const tool = "opencut_edit_" + name + "_apply";
      await context.route("**/api/tools/" + tool, async route => {
        const request = route.request().postDataJSON();
        if (state.requests[name]) expect(request).toEqual(state.requests[name]);
        else { state.requests[name] = request; await save(); }
        await route.continue();
      });
      await history.getByRole("button", { name: label, exact: true }).click();
      const apply = history.getByRole("button", { name: action, exact: true });
      await expect(apply).toBeEnabled();
      const response = page.waitForResponse(r => new URL(r.url()).pathname.endsWith("/" + tool));
      await apply.click(); const envelope = await (await response).json(); expect(envelope.error).toBeNull();
      state[name + "_receipt"] = envelope.data; await save();
      expect(await client.call(tool, state.requests[name])).toEqual(envelope.data);
      await expect(apply).toHaveCount(0); await context.unroute("**/api/tools/" + tool);
      return envelope.data;
    };
    if (!state.redo_receipt) await commit("redo", "Redo", "Apply redo");
    if (!state.undo_receipt) {
      const afterRedo = await inspect();
      expect(afterRedo.project.fonts.length).toBe(state.initial.snapshot.project.fonts.length + 1);
      await commit("undo", "Undo", "Apply undo");
    }
    await page.reload(); await expect(page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible();
    const final = await inspect(); expect(final.project).toEqual(state.initial.snapshot.project);
    const finalCounts = counts();
    expect(finalCounts).toEqual({ ...state.initial.counts, revisions: state.initial.counts.revisions + 2, editor_transactions: state.initial.counts.editor_transactions + 2 });
    return { navigation: state.imported_navigation, imported_redo_request: state.requests.redo, imported_redo_receipt: state.redo_receipt, undo_receipt: state.undo_receipt, public_project_restored_after_reload: true, final_revision: final.revision, counts: finalCounts };
  });
  await stage("fixed-artifact-after-current-edit", async () => {
    const proof = await read(path.join(path.dirname(args.report), "native-proof.json"));
    const frame = proof.stages["restored-fixed-historical-frame"].result;
    const response = await client.downloadArtifact(frame.artifact);
    expect(response.status).toBe(200);
    const bytes = Buffer.from(await response.arrayBuffer()); expect(sha(bytes)).toBe(frame.artifact.content_hash);
    await fs.writeFile(path.join(path.dirname(args.report), "restored-historical-frame.png"), bytes);
    expect((await inspect()).revision).toBe(state.stages["imported-causal-redo-undo"].final_revision);
    return { job_id: frame.job_id, exact_artifact_sha256: sha(bytes), current_edit_did_not_change_historical_output: true };
  });
  expect(errors).toEqual([]); state.page_errors = errors;
  state.executed_verifier_sha256 = sha(await fs.readFile(import.meta.filename));
  state.limits = ["Original import was CLI; browser replays that retained original request.", "No source branches/merges exist: 220 revisions and 219 transactions cover the real main history only.", "Output discovery is controlled unavailable in UI; one real Native historical PNG is separate evidence.", "No package portability, per-font matrix, device/manual or full A7/D8 claim."];
  await save();
} catch (e) {
  state.failure = { message: e.message, stack: e.stack, page_errors: errors }; await save(); throw e;
} finally {
  release?.();
  await context?.request.delete(args.endpoint + "/app/session", { headers: { Origin: args.endpoint, "X-OpenCut-App": "web" } }).catch(() => {});
  await context?.close(); await browser?.close();
}
