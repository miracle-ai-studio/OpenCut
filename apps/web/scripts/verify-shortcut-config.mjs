import { editorSnapshot } from "./project-inspection.mjs";
/** One F55 preference flow. Existing synthetic content is strictly read-only.
 * Replay is cheap: interrupted runs restore their entering preferences, then
 * repeat the one flow. No media, server, build or fixture setup is performed.
 */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";

const args = {};
const options = new Set(["endpoint", "credential", "workspace", "report", "project"]);
for (let i = 2; i < process.argv.length; i += 2) {
  if (!process.argv[i].startsWith("--") || !options.has(process.argv[i].slice(2)) || !process.argv[i + 1])
    throw Error("Expected --endpoint URL --credential FILE --workspace DIR --report FILE [--project ID_OR_NAME]");
  args[process.argv[i].slice(2)] = process.argv[i + 1];
}
for (const key of ["endpoint", "credential", "workspace", "report"])
  if (!args[key]) throw Error(`Missing --${key}`);
if (!process.env.OPENCUT_CHROME) throw Error("Set OPENCUT_CHROME to the retained Chromium executable");
const endpoint = args.endpoint.replace(/\/$/u, ""), origin = new URL(endpoint).origin;
const key = "opencut.shortcuts", historicalKey = "opencut.shortcuts.v1", checkpoint = path.join(args.workspace, "f55-shortcut-config-checkpoint.json");
const sha = value => createHash("sha256").update(value).digest("hex");
await fs.mkdir(args.workspace, { recursive: true });
await fs.mkdir(path.dirname(path.resolve(args.report)), { recursive: true });
let state;
try { state = JSON.parse(await fs.readFile(checkpoint, "utf8")); }
catch (error) { if (error.code !== "ENOENT") throw error; }
if (state) {
  expect(state.endpoint, "Do not resume a checkpoint against a different service").toBe(endpoint);
  if (state.preference_storage_key !== key) {
    if (state.preference_storage_key !== undefined || state.schema_version !== 1)
      throw Error("Shortcut checkpoint belongs to an unknown preference scope");
    // Preserve original candidate evidence once. Its completed UI facts cannot
    // become evidence for the changed storage/writer path;reuse the project ID.
    state = { endpoint, project_id: state.project_id, historical_checkpoint: state };
  } else if (Object.hasOwn(state, "schema_version") || state.historical_storage_key !== historicalKey
      || (Object.hasOwn(state, "original_storage") && !Object.hasOwn(state, "original_historical_storage")))
    throw Error("Mixed or incomplete shortcut checkpoint scope");
} else state = { endpoint };
state.preference_storage_key = key;
state.historical_storage_key = historicalKey;
const save = async () => {
  await fs.writeFile(`${checkpoint}.pending`, JSON.stringify(state, null, 2) + "\n");
  await fs.rename(`${checkpoint}.pending`, checkpoint);
};
const client = new OpenCutClient({ baseUrl: endpoint, token: (await fs.readFile(args.credential, "utf8")).trim() });
const counts = {}, blocked = [], errors = [];
// These three operations are explicitly read-only in the shared declaration.
// TaskFeed uses changes -> query for bootstrap and view for later updates.
const readOnlyJobTools = new Set(["opencut_job_changes", "opencut_job_query", "opencut_job_view"]);
const toolDeclaration = await fs.readFile(new URL("../../../crates/api-contracts/src/tools.rs", import.meta.url), "utf8");
const declarations = new Map();
for (const match of toolDeclaration.matchAll(/^\s*\w+:\s*\([^\n]*?"(opencut_[a-z0-9_]+)"[^\n]*,\s*(true|false)\),\s*$/gmu)) {
  if (declarations.has(match[1])) throw Error("Duplicate source tool declaration");
  declarations.set(match[1], { read_only: match[2] === "true", declaration: "crates/api-contracts/src/tools.rs",
    line: toolDeclaration.slice(0, match.index + match[0].indexOf(`"${match[1]}"`)).split("\n").length });
}
const jobReadDeclarations = Object.fromEntries([...readOnlyJobTools].map(name => {
  const declaration = declarations.get(name);
  if (!declaration?.read_only) throw Error(`Shared declaration no longer marks ${name} read-only`);
  return [name, declaration];
}));
// Read-only scope follows the authoritative declaration,never a tool-name
// heuristic that can miss a control-plane write or block an ordinary query.
const classify = name => !declarations.get(name)?.read_only ? "blocked_content_write_or_job_mutation"
  : readOnlyJobTools.has(name) ? "read_only_job_observation" : "nonmutating_editor_request";
const requestClasses = {};
const guard = context => context.route("**/api/tools/*", async route => {
  const name = new URL(route.request().url()).pathname.split("/").at(-1), classification = classify(name);
  counts[`browser:${name}`] = (counts[`browser:${name}`] ?? 0) + 1;
  requestClasses[name] = classification;
  if (classification === "blocked_content_write_or_job_mutation") {
    blocked.push({ tool: name, method: route.request().method() });
    await route.abort("blockedbyclient");
  } else await route.continue();
});
const call = (name, input) => {
  counts[`sdk:${name}`] = (counts[`sdk:${name}`] ?? 0) + 1;
  return client.call(name, input);
};
let projectId = state.project_id ?? args.project ?? "agent-background-landscape-v4";
const inspect = () => editorSnapshot(call("opencut_project_inspect", { project_id: projectId, revision: null }));
const snapshot = async () => {
  const document = await inspect();
  const [history, navigation] = await Promise.all([
    call("opencut_project_history", { project_id: projectId, from_revision: document.revision, limit: 50 }),
    call("opencut_project_history_navigation", { project_id: projectId, revision: document.revision }),
  ]);
  return { document, history, navigation };
};
let browser, context, page, failure, restoring = false;
const facts = {};
const setRaw = (raw, storageKey = key) => page.evaluate(({ key, raw }) => {
  if (raw === null) localStorage.removeItem(key);
  else localStorage.setItem(key, raw);
}, { key: storageKey, raw });
const raw = (storageKey = key) => page.evaluate(key => localStorage.getItem(key), storageKey);
const open = async () => {
  await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
  await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
  const welcome = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
  if (await welcome.isVisible()) await welcome.getByRole("button", { name: "Close", exact: true }).click();
};
const settings = async () => {
  await page.getByRole("button", { name: "Shortcuts and settings", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Keyboard shortcuts", exact: true });
  await expect(dialog).toBeVisible();
  return dialog;
};
const button = (scope, name) => scope.getByRole("button", { name, exact: true });
const row = (dialog, action) => dialog.locator(`[data-shortcut-action="${action}"]`);
const transfer = async dialog => {
  const details = dialog.locator("details.shortcut-transfer");
  if (!(await details.evaluate(node => node.open)))
    await dialog.getByText("Import / export shortcut settings", { exact: true }).click();
  return dialog.getByRole("textbox", { name: "Shortcut configuration JSON", exact: true });
};
const exported = async dialog => {
  const input = await transfer(dialog);
  await button(dialog, "Show current configuration").click();
  return JSON.parse(await input.inputValue());
};
const close = dialog => button(dialog, "Close").click();
const resetAll = async (dialog, confirm = true) => {
  await button(dialog, "Reset all shortcuts").click();
  const confirmation = dialog.getByRole("region", { name: "Restore default shortcuts?", exact: true });
  await expect(confirmation).toBeVisible();
  await button(confirmation, confirm ? "Restore defaults" : "Cancel").click();
  await expect(confirmation).toHaveCount(0);
};
const record = async (dialog, action, label, chord) => {
  await row(dialog, action).getByRole("button", { name: new RegExp(`^${label}: Change `, "u") }).first().click();
  await dialog.getByRole("textbox", { name: "Record a shortcut", exact: true }).press(chord);
};
const change = async (dialog, action, label, chord) => {
  await record(dialog, action, label, chord);
  await button(dialog, "Save shortcut").click();
};
try {
  // Direct ID is the retained fixture convention. Resolve an exact name only
  // when no ID was supplied/found; never create or copy a project.
  try { await inspect(); }
  catch (error) {
    if (state.project_id || args.project) throw error;
    let cursor = null, found = [];
    do {
      const page = await call("opencut_project_list", { cursor, limit: 100 });
      found.push(...page.items.filter(item => item.name === projectId));
      cursor = page.next_cursor ?? null;
    } while (cursor);
    expect(found, "Exactly one retained synthetic project must already exist").toHaveLength(1);
    projectId = found[0].project_id;
  }
  state.project_id = projectId;
  const before = await snapshot();
  if (state.before) expect(before, "Source content/history changed since checkpoint").toEqual(state.before);
  else state.before = before;
  state.helper_sha256 = sha(await fs.readFile(new URL(import.meta.url)));
  state.sdk_sha256 = sha(await fs.readFile(new URL("../../../packages/sdk-ts/dist/index.js", import.meta.url)));
  state.status = "running"; delete state.failure; await save();
  browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
  context = await browser.newContext({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  // Browser preferences are private to this context; no shared database access.
  await context.addInitScript(() => localStorage.setItem("opencut.language", "en"));
  await guard(context);
  page = await context.newPage(); page.setDefaultTimeout(15000);
  page.on("pageerror", error => errors.push(error.message));
  const response = await page.goto(`${endpoint}/projects`);
  facts.entry_html_sha256 = sha(await response.body());
  await expect(page.getByRole("button", { name: "Restore project package", exact: true })).toBeEnabled();
  if (!Object.hasOwn(state, "original_storage")) {
    state.original_storage = await raw();
    state.original_historical_storage = await raw(historicalKey);
  } else {
    await setRaw(state.original_storage);
    await setRaw(state.original_historical_storage, historicalKey);
  }
  await save(); await open();
  let dialog = await settings();
  const original = await exported(dialog);
  if (state.original_configuration) expect(original).toEqual(state.original_configuration);
  else { state.original_configuration = original; await save(); }
  expect(Object.keys(original)).toEqual(["bindings"]);
  // Exercise the original preference reader in this same private context,
  // then let all ordinary UI writes use the sole current key.
  const historicalConfiguration = JSON.stringify({ schema_version: 1, bindings: original.bindings });
  await close(dialog);
  await setRaw(null); await setRaw(historicalConfiguration, historicalKey); await open();
  dialog = await settings(); expect(await exported(dialog)).toEqual(original);
  expect(await raw()).toBeNull(); expect(await raw(historicalKey)).toBe(historicalConfiguration);
  facts.original_preferences_read_without_rewrite = true;
  await resetAll(dialog);
  expect(await raw(historicalKey)).toBe(historicalConfiguration);
  const defaults = await exported(dialog);
  expect(defaults.bindings.split).toEqual(["s"]);
  expect(defaults.bindings["split-left"]).toEqual(["q"]);

  await record(dialog, "split", "Split elements at playhead", "q");
  await expect(dialog.getByRole("alert")).toContainText("Shortcut conflict with Split and remove left");
  await expect(button(dialog, "Save shortcut")).toBeDisabled();
  expect(await exported(dialog)).toEqual(defaults);
  const primary = await page.evaluate(() => /Mac|iPhone|iPad|iPod/.test(navigator.platform) ? "Meta" : "Control");
  await dialog.getByRole("textbox", { name: "Record a shortcut", exact: true }).press(`${primary}+w`);
  await expect(dialog.getByRole("alert")).toContainText("reserved by the browser or operating system");
  await expect(button(dialog, "Save shortcut")).toBeDisabled();
  expect(await exported(dialog)).toEqual(defaults);
  await dialog.getByRole("textbox", { name: "Record a shortcut", exact: true }).press("Alt+s");
  await button(dialog, "Save shortcut").click();
  const customized = await exported(dialog);
  expect(customized.bindings.split).toEqual(["alt+s"]);
  const config = await transfer(dialog);
  await config.fill("{broken"); await button(dialog, "Import configuration").click();
  await expect(dialog.getByRole("alert")).toContainText("invalid");
  expect(await exported(dialog)).toEqual(customized);

  await change(dialog, "split-left", "Split and remove left", "s");
  const conflictingReset = await exported(dialog);
  await button(row(dialog, "split"), "Split elements at playhead: Reset action").click();
  await expect(dialog.getByRole("alert")).toContainText("Shortcut conflict: Split and remove left");
  expect(await exported(dialog)).toEqual(conflictingReset);
  await button(row(dialog, "split-left"), "Split and remove left: Clear S").click();
  await button(row(dialog, "split"), "Split elements at playhead: Reset action").click();
  expect((await exported(dialog)).bindings.split).toEqual(["s"]);
  await button(row(dialog, "split-left"), "Split and remove left: Reset action").click();
  expect(await exported(dialog)).toEqual(defaults);
  facts.conflict_reservation_bad_import_and_single_reset = true;
  state.phase = "preference_edges_passed"; await save();

  await change(dialog, "split", "Split elements at playhead", "Alt+s");
  const beforeCancel = await exported(dialog);
  await resetAll(dialog, false); expect(await exported(dialog)).toEqual(beforeCancel);
  await resetAll(dialog); expect(await exported(dialog)).toEqual(defaults);
  await close(dialog); await open(); dialog = await settings();
  expect(await exported(dialog)).toEqual(defaults);
  await button(row(dialog, "split"), "Split elements at playhead: Clear S").click();
  const cleared = await exported(dialog); expect(cleared.bindings.split).toEqual([]);
  await close(dialog); await open(); dialog = await settings();
  expect(await exported(dialog)).toEqual(cleared);
  await close(dialog);
  // Deliberately corrupt only this context's saved preference. The following
  // genuine document navigation exercises production initial-read fallback.
  const damaged = "{f55-controlled-invalid-json";
  await setRaw(damaged); await open(); dialog = await settings();
  await expect(dialog.getByRole("status")).toContainText("Default keys are active");
  expect(await exported(dialog)).toEqual(defaults);
  expect(await raw(), "Fallback must not overwrite damaged saved bytes").toBe(damaged);
  facts.reset_cancel_confirm_reload_clear_reload_and_damaged_saved_fallback = true;
  facts.configuration_action_count = Object.keys(defaults.bindings).length;
  expect(await snapshot(), "Preferences must not enter project content/history/navigation").toEqual(state.before);
  expect(blocked, "No content write or media Job call may be attempted").toEqual([]);
  expect(errors).toEqual([]);
} catch (error) {
  failure = error;
  state.failure = { message: String(error), stack: error.stack };
  if (page) {
    state.failure_view = await page.locator("body").innerText().catch(() => "unavailable");
    state.failure_screenshot = path.join(args.workspace, "f55-shortcut-config-failure.png");
    await page.screenshot({ path: state.failure_screenshot, fullPage: true }).catch(() => {});
  }
} finally {
  // Restore exact entering bytes, including absence or malformed saved data.
  // This also runs after a failed assertion; no hidden controller is invoked.
  if (page && Object.hasOwn(state, "original_storage")) {
    restoring = true;
    try {
      if (new URL(page.url()).origin !== origin) await page.goto(`${endpoint}/projects`);
      await setRaw(state.original_storage);
      await setRaw(state.original_historical_storage, historicalKey); await open();
      const dialog = await settings();
      if (state.original_configuration) expect(await exported(dialog)).toEqual(state.original_configuration);
      expect(await raw()).toBe(state.original_storage);
      expect(await raw(historicalKey)).toBe(state.original_historical_storage);
      await close(dialog);
      expect(await snapshot()).toEqual(state.before);
      facts.original_preferences_restored_exactly = true;
      facts.project_snapshot_history_navigation_unchanged = true;
    } catch (error) {
      state.restore_failure = String(error); failure ??= error;
    }
  }
  if (blocked.length || errors.length) failure ??= Error("Unexpected write/Job attempt or page error");
  if (failure) {
    state.failure ??= { message: String(failure), stack: failure.stack };
    if (page && !state.failure_screenshot) {
      state.failure_screenshot = path.join(args.workspace, "f55-shortcut-config-failure.png");
      await page.screenshot({ path: state.failure_screenshot, fullPage: true }).catch(() => {});
    }
  }
  state.status = failure ? "failed" : "passed";
  state.facts = facts; state.request_counts = counts;
  state.blocked_write_or_job_attempts = blocked; state.page_errors = errors;
  state.request_classification = requestClasses; state.declaration_verified_job_reads = jobReadDeclarations;
  state.browser_version = browser?.version() ?? null;
  state.scope = "One current/strict-original-read preference flow; content/history/navigation read-only. Reuses A0 action/default-replacement evidence; no broad F54, OS IME or media acceptance claim.";
  if (!failure) { delete state.failure; delete state.restore_failure; delete state.failure_view; delete state.failure_screenshot; }
  await save();
  await fs.writeFile(args.report, JSON.stringify({ ...state, checkpoint, restoration_attempted: restoring }, null, 2) + "\n");
  await browser?.close();
}
if (failure) throw failure;
console.log(JSON.stringify({ status: state.status, report: args.report, checkpoint }));
