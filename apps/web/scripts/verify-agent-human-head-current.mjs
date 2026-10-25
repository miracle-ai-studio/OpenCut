import { editorSnapshot } from "./project-inspection.mjs";
/** Agent CLI and human UI on one existing identity fixture; no media jobs. */
import { chromium, expect as baseExpect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
const expect = baseExpect.configure({ timeout: 25000 });
const args = Object.fromEntries(process.argv.slice(2).reduce((all, value, i, list) => i % 2 ? all : [...all, [value.slice(2), list[i + 1]]], []));
for (const k of ["workspace", "candidate", "endpoint", "chromium", "report"]) if (!args[k]) throw Error("Missing --" + k);
const phase = args.phase ?? "head";
if (!["head", "grant"].includes(phase)) throw Error("Unknown phase");
const read = async file => JSON.parse(await fs.readFile(file, "utf8"));
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const candidate = await read(args.candidate), fixtureFile = path.join(args.workspace, "scene-preview-identity-fixture.json"), fixture = await read(fixtureFile);
const service = await read(path.join(args.workspace, "service/state.json"));
expect(await fs.realpath(service.executable)).toBe(await fs.realpath(candidate.binary));
expect(await fs.realpath(service.web_root)).toBe(await fs.realpath(candidate.web_root));
expect(sha(await fs.readFile(candidate.binary))).toBe(candidate.binary_sha256);
const identity = { candidate: args.candidate, binary: candidate.binary_sha256, web: candidate.web_js_css_tree_sha256, browser: sha(await fs.readFile(args.chromium)), project_id: fixture.project_id };
const checkpoint = path.join(args.workspace, `agent-human-${phase}-current-checkpoint.json`);
const state = await read(checkpoint).catch(e => { if (e.code !== "ENOENT") throw e; return { identity, stages: {}, requests: {}, receipts: {} }; });
expect(state.identity).toEqual(identity);
const save = async () => { const text = JSON.stringify(state, null, 2) + "\n"; await fs.writeFile(checkpoint + ".pending", text, { mode: 0o600 }); await fs.rename(checkpoint + ".pending", checkpoint); await fs.writeFile(args.report, text, { mode: 0o600 }); };
const stage = async (name, run) => { if (state.stages[name]) { console.log("Reuse " + name); return; } console.log("Begin " + name); state.stages[name] = await run(); delete state.failure; await save(); console.log("Passed " + name); };
const client = new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(path.join(args.workspace, "service/api-token"), "utf8")).trim() });
const inspect = () => editorSnapshot(client.call("opencut_project_inspect", { project_id: fixture.project_id, revision: null }));
const cli = (tool, input, error) => {
  let stdout; try { stdout = execFileSync(candidate.binary, ["--json", "--database", path.join(args.workspace, "workspace.sqlite"), "call", tool, "--input", "-"], { input: JSON.stringify(input), encoding: "utf8", timeout: 30000 }); }
  catch (e) { if (!error) throw e; stdout = e.stdout; }
  const reply = JSON.parse(stdout);
  if (error) { expect(reply.error?.code).toBe(error); return error; }
  expect(reply.error).toBeNull(); return reply.data;
};
const request = async (key, tool, value) => {
  if (state.requests[key]) expect(value).toEqual(state.requests[key]); else { state.requests[key] = value; await save(); }
  const result = cli(tool, state.requests[key]);
  if (state.receipts[key]) expect(result).toEqual(state.receipts[key]); else { state.receipts[key] = result; await save(); }
  return result;
};
const counts = () => JSON.parse(execFileSync("python3", ["-B", "-c", `
import hashlib,json,sqlite3,sys
c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)
other=c.execute('select project_id,head_revision from projects where project_id!=? order by project_id',[sys.argv[2]]).fetchall()
jobs=c.execute('select job_id,state,attempt,cancel_requested from jobs order by job_id').fetchall()
print(json.dumps({'other_heads_sha256':hashlib.sha256(repr(other).encode()).hexdigest(),'jobs_sha256':hashlib.sha256(repr(jobs).encode()).hexdigest(),'projects':c.execute('select count(*) from projects').fetchone()[0],'revisions':c.execute('select count(*) from revisions where project_id=?',[sys.argv[2]]).fetchone()[0],'transactions':c.execute('select count(*) from editor_transactions where project_id=?',[sys.argv[2]]).fetchone()[0]}))
`, path.join(args.workspace, "workspace.sqlite"), fixture.project_id], { encoding: "utf8" }));
state.initial ??= { snapshot: await inspect(), counts: counts(), fixture: structuredClone(fixture) }; await save();
const project = fixture.project_id, newName = phase === "head" ? "Agent and human collaboration recovery" : "Agent grant recovery collaboration";
let browser, context, page; const errors = [];
try {
  if (phase === "head") await stage("capture-stale-agent-plan", async () => {
    const input = { project_id: project, expected_revision: state.initial.snapshot.revision, idempotency_key: "agent-human-current:stale-plan", label: "Old Agent plan", steps: [{ kind: "operations", operations: [{ type: "rename", name: "This stale plan must not replace later work" }] }] };
    const proposal = cli("opencut_edit_agent_plan", input); expect(await inspect()).toEqual(state.initial.snapshot);
    state.stale_plan = proposal; await save(); return { expected_revision: input.expected_revision, read_only_plan: true };
  });
  browser = await chromium.launch({ headless: true, executablePath: args.chromium }); context = await browser.newContext({ locale: "en-US", viewport: { width: 1440, height: 1000 } });
  await context.route("**/api/tools/*_capability", route => phase === "head" && route.request().url().endsWith("/opencut_audio_preview_capability") ? route.continue() : route.abort("failed"));
  page = await context.newPage(); page.setDefaultTimeout(25000); page.on("pageerror", e => errors.push(e.message));
  await page.goto(args.endpoint + "/editor/" + project); await expect(page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible();
  if (phase === "head") {
  await stage("agent-head-retires-running-preview", async () => {
    if (state.partial_head_prefix && state.receipts["agent-rename"]) {
      await expect(page.getByText(newName + " · V4", { exact: true })).toBeVisible();
      await expect.poll(async () => (await client.call("opencut_audio_preview_recovery_inspect", { project_id: project })).record).toBeNull();
      return { old_revision: state.initial.snapshot.revision, agent_revision: state.receipts["agent-rename"].revision,
        old_audio_stopped_after_actual_head_adoption: true, owner_lease_fixed_identity_preserved: true,
        actual_consumption_preceded_agent_write: true, retained_prefix: state.partial_head_prefix,
        durable_slot_cleared_after_async_ack: true, latency_measurement: "not checkpointed;no latency claim" };
    }
    expect(state.receipts["agent-rename"]).toBeUndefined();
    let playing;
    page.on("response", response => { if (response.url().endsWith("/opencut_audio_preview_recovery_dispatch")) void response.json().then(body => { if (body.data?.state === "playing") playing = body.data; }).catch(() => {}); });
    await expect(page.getByRole("button", { name: "Play scene audio", exact: true })).toBeEnabled(); await page.getByRole("button", { name: "Play scene audio", exact: true }).click();
    await expect.poll(() => !!playing).toBe(true);
    await expect.poll(async () => Number((await client.call("opencut_audio_preview_inspect", { session_id: playing.session_id })).consumed_frames)).toBeGreaterThan(0);
    const receipt = await request("agent-rename", "opencut_edit_apply", { project_id: project, expected_revision: state.initial.snapshot.revision, idempotency_key: "agent-human-current:rename", label: "Agent updates the shared head", operations: [{ type: "rename", name: newName }] });
    const started = Date.now(); await expect(page.getByText(newName + " · V4", { exact: true })).toBeVisible({ timeout: 6000 });
    let stopped; await expect.poll(async () => { stopped = await client.call("opencut_audio_preview_inspect", { session_id: playing.session_id }); return stopped.state; }, { timeout: 6000 }).toBe("stopped");
    expect(stopped.identity).toEqual(playing.identity); expect(stopped.owner_id).toBe(playing.owner_id); expect(stopped.lease_id).toBe(playing.lease_id);
    expect((await inspect()).revision).toBe(receipt.revision);
    state.observed_head_retirement = { session_id: playing.session_id, fixed_identity: playing.identity, stopped: true, elapsed_ms: Date.now() - started }; await save();
    await expect.poll(async () => (await client.call("opencut_audio_preview_recovery_inspect", { project_id: project })).record).toBeNull();
    return { old_revision: state.initial.snapshot.revision, agent_revision: receipt.revision, old_session_id: playing.session_id, old_audio_stopped_after_actual_head_adoption: true, observed_ms: Date.now() - started, owner_lease_fixed_identity_preserved: true, actual_consumption_preceded_agent_write: true };
  });
  await stage("human-continues-and-stale-agent-refused", async () => {
    if (!state.receipts["human-bookmark"]) {
      await context.route("**/api/tools/opencut_edit_apply", async route => { state.requests["human-bookmark"] = route.request().postDataJSON(); await save(); await route.continue(); });
      await page.getByRole("button", { name: "Add bookmark", exact: true }).click(); await page.getByLabel("Canvas note", { exact: true }).fill("Human review after Agent head update");
      const response = page.waitForResponse(r => r.url().endsWith("/opencut_edit_apply")); await page.getByRole("button", { name: "Save bookmark", exact: true }).click();
      const reply = await (await response).json(); expect(reply.error).toBeNull(); state.receipts["human-bookmark"] = reply.data; await save(); await context.unroute("**/api/tools/opencut_edit_apply");
    }
    const current = await inspect(); expect(current.project.name).toBe(newName);
    expect(state.requests["human-bookmark"].expected_revision).toBe(state.receipts["agent-rename"].revision);
    expect(current.project.sequences.flatMap(s => s.bookmarks).some(b => b.note === "Human review after Agent head update")).toBe(true);
    const rejected = cli("opencut_edit_apply", state.stale_plan.transaction, "REVISION_CONFLICT"); expect(await inspect()).toEqual(current);
    const actors = await client.call("opencut_project_revision_actors", { project_id: project, revisions: [state.receipts["agent-rename"].revision, state.receipts["human-bookmark"].revision] });
    expect(actors.actors[state.receipts["agent-rename"].revision].source).toBe("cli"); expect(actors.actors[state.receipts["human-bookmark"].revision].source).toBe("http");
    await page.reload(); await expect(page.getByRole("region", { name: "Timeline", exact: true })).toBeVisible(); expect(await inspect()).toEqual(current);
    return { human_revision: current.revision, human_apply_based_on_agent_head: true, old_agent_apply: rejected, current_content_and_revision_unchanged_by_refusal: true, actors, reload_continues_same_project: true };
  });
  await stage("restore-owned-fixture-content", async () => {
    for (const [key, transaction] of [["undo-human", state.receipts["human-bookmark"]], ["undo-agent", state.receipts["agent-rename"]]]) {
      if (state.receipts[key]) continue;
      const head = await inspect(), value = { project_id: project, expected_revision: head.revision, target_transaction_id: transaction.transaction_id, idempotency_key: "agent-human-current:" + key };
      expect(cli("opencut_edit_undo_plan", value).conflicts).toEqual([]); await request(key, "opencut_edit_undo_apply", value);
    }
    const final = await inspect(); expect(final.project).toEqual(state.initial.snapshot.project);
    const after = counts(); expect(after).toEqual({ ...state.initial.counts, revisions: state.initial.counts.revisions + 4, transactions: state.initial.counts.transactions + 4 });
    // Advance only this mutable fixture's current-head pointer. Historical
    // evidence/source revisions retain their exact original identities.
    const metadata = { ...state.initial.fixture, revision: final.revision }; await fs.writeFile(fixtureFile, JSON.stringify(metadata, null, 2) + "\n");
    await page.reload(); await expect(page.getByText(final.project.name + " · V4", { exact: true })).toBeVisible();
    return { final_revision: final.revision, complete_project_content_restored: true, expected_four_transactions: true, unrelated_project_heads_and_all_job_states_unchanged: true, existing_fixture_head_pointer_updated: true, counts: after };
  });
  } else {
    const { verifyAgentGrantRecovery } = await import("./agent-grant-recovery-case.mjs");
    await verifyAgentGrantRecovery({ args, candidate, state, stage, save, client, cli, inspect, counts, page, context, project, newName, fixtureFile, expect });
  }
  expect(errors).toEqual([]); state.page_errors = errors; state.executed_verifier_sha256 = sha(await fs.readFile(import.meta.filename));
  state.limits = phase === "head" ? ["One existing identity fixture and original CAS;no new project/media/ASR/output job/build/test.", "Live audio/head integration only;prior PCM/render/performance retain their original producer identities.", "Agent CLI and UI owner semantics here;grant loss/reissue and broad multi-host/plugin/browser matrix remain separate."] : ["Agent project-grant loss/restore and credential rotation while human owner UI remains connected;not a restricted-principal UI impersonation.", "No pending secret preview slot is manufactured under a permanently revoked credential;existing credential-bound recovery source checks remain separate.", "No audio/render/ASR/output/project copy or product build;4necessary edits are undone on the existing mutable identity fixture."];
  await save();
} catch (e) { state.failure = { message: e.message, stack: e.stack, page_errors: errors }; await save(); throw e; }
finally { await context?.close().catch(() => {}); await browser?.close().catch(() => {}); }
