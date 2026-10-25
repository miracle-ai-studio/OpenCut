import { editorSnapshot } from "./project-inspection.mjs";
/** Shared-current harness extension: a limited Agent and a human owner. */
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";

export async function verifyAgentGrantRecovery(h) {
  const { args, candidate, state, stage, save, client, cli, inspect, counts, page, context, project, newName, fixtureFile, expect } = h;
  const principal = "agent-collaboration-recovery-v1";
  const grants = [{ project_id: project, actions: ["read", "edit", "restore", "run_jobs"] }];
  const admin = (words, value) => {
    const stdout = execFileSync(candidate.binary, ["--json", "--database", path.join(args.workspace, "workspace.sqlite"), "access", ...words], { encoding: "utf8", input: value ? JSON.stringify(value) : undefined, timeout: 30000 });
    const envelope = JSON.parse(stdout); expect(envelope.error).toBeNull(); return envelope.data;
  };
  const privateFile = role => path.join(args.workspace, `agent-grant-recovery-${role}.secret`);
  const actor = async role => new OpenCutClient({ baseUrl: args.endpoint, token: (await fs.readFile(privateFile(role), "utf8")).trim() });
  const remember = async (key, action) => {
    if (!state.receipts[key]) { state.receipts[key] = await action(); await save(); }
    return state.receipts[key];
  };
  const commit = async (key, api, tool, value) => {
    if (state.requests[key]) expect(value).toEqual(state.requests[key]); else { state.requests[key] = value; await save(); }
    return remember(key, () => api.call(tool, state.requests[key]));
  };
  await stage("agent-authority-and-original-edit", async () => {
    await remember("initial-policy", () => admin(["set-policy", "--input", "-"], { principal_id: principal, expected_revision: "0", enabled: true, grants }));
    const credential = await remember("credential-A", () => admin(["issue", principal, "--output", privateFile("A")]));
    const a = await actor("A"), access = await a.call("opencut_access_inspect", {});
    expect(access.owner).toBe(false); expect(access.principal_id).toBe(principal); expect(access.credential_id).toBe(credential.credential_id);
    if (!state.stale_plan) {
      state.stale_plan = await a.call("opencut_edit_agent_plan", { project_id: project, expected_revision: state.initial.snapshot.revision, idempotency_key: "grant-recovery:old-plan", label: "Old restricted Agent plan", steps: [{ kind: "operations", operations: [{ type: "rename", name: "Old plan must not overwrite human work" }] }] }); await save();
    }
    const receipt = await commit("agent-original-edit", a, "opencut_edit_apply", { project_id: project, expected_revision: state.initial.snapshot.revision, idempotency_key: "grant-recovery:rename", label: "Agent edit before authority interruption", operations: [{ type: "rename", name: newName }] });
    await expect(page.getByText(newName + " · V4", { exact: true })).toBeVisible();
    return { principal_id: principal, credential_id: credential.credential_id, restricted_agent: true, agent_revision: receipt.revision, human_ui_observed: true };
  });
  await stage("grant-denial-human-continues", async () => {
    await remember("denied-policy", () => admin(["set-policy", "--input", "-"], { principal_id: principal, expected_revision: state.receipts["initial-policy"].revision, enabled: true, grants: [] }));
    const a = await actor("A");
    await expect(editorSnapshot(a.call("opencut_project_inspect", { project_id: project, revision: null }))).rejects.toMatchObject({ code: "NOT_AUTHORIZED" });
    await expect(a.call("opencut_edit_apply", state.stale_plan.transaction)).rejects.toMatchObject({ code: "NOT_AUTHORIZED" });
    if (!state.receipts["human-during-denial"]) {
      await context.route("**/api/tools/opencut_edit_apply", async route => { state.requests["human-during-denial"] = route.request().postDataJSON(); await save(); await route.continue(); });
      await page.getByRole("button", { name: "Add bookmark", exact: true }).click(); await page.getByLabel("Canvas note", { exact: true }).fill("Human work survives Agent grant recovery");
      const response = page.waitForResponse(r => r.url().endsWith("/opencut_edit_apply")); await page.getByRole("button", { name: "Save bookmark", exact: true }).click();
      const envelope = await (await response).json(); expect(envelope.error).toBeNull(); state.receipts["human-during-denial"] = envelope.data; await save(); await context.unroute("**/api/tools/opencut_edit_apply");
    }
    const current = await inspect(); expect(current.revision).toBe(state.receipts["human-during-denial"].revision);
    expect(state.requests["human-during-denial"].expected_revision).toBe(state.receipts["agent-original-edit"].revision);
    return { denied_read_and_apply: "NOT_AUTHORIZED", human_saved_while_agent_denied: true, human_revision: current.revision };
  });
  await stage("grant-restore-rotate-selective-undo", async () => {
    await remember("restored-policy", () => admin(["set-policy", "--input", "-"], { principal_id: principal, expected_revision: state.receipts["denied-policy"].revision, enabled: true, grants }));
    const a = await actor("A"), before = await inspect();
    if (!state.receipts["revoked-A"]) {
      expect(await editorSnapshot(a.call("opencut_project_inspect", { project_id: project, revision: null }))).toEqual(before);
      await expect(a.call("opencut_edit_apply", state.stale_plan.transaction)).rejects.toMatchObject({ code: "REVISION_CONFLICT" });
      expect(await inspect()).toEqual(before);
    }
    await remember("revoked-A", () => admin(["revoke", state.receipts["credential-A"].credential_id]));
    await expect(a.call("opencut_access_inspect", {})).rejects.toMatchObject({ code: "NOT_AUTHORIZED" });
    const issued = await remember("credential-B", () => admin(["issue", principal, "--output", privateFile("B")]));
    const b = await actor("B"), access = await b.call("opencut_access_inspect", {});
    expect(access.principal_id).toBe(principal); expect(access.credential_id).toBe(issued.credential_id); expect(issued.credential_id).not.toBe(state.receipts["credential-A"].credential_id);
    if (!state.receipts["agent-undo-own-edit"]) {
      const current = await editorSnapshot(b.call("opencut_project_inspect", { project_id: project, revision: null })); expect(current.revision).toBe(state.receipts["human-during-denial"].revision);
      const input = { project_id: project, expected_revision: current.revision, target_transaction_id: state.receipts["agent-original-edit"].transaction_id, idempotency_key: "grant-recovery:undo-own-edit" };
      expect((await b.call("opencut_edit_undo_plan", input)).conflicts).toEqual([]); await commit("agent-undo-own-edit", b, "opencut_edit_undo_apply", input);
    }
    const after = await inspect(), bookmarkId = state.receipts["human-during-denial"].created_ids[0];
    expect(after.project.name).toBe(state.initial.snapshot.project.name);
    expect(after.project.sequences.flatMap(s => s.bookmarks).some(v => v.id === bookmarkId && v.note === "Human work survives Agent grant recovery")).toBe(true);
    const withoutHuman = structuredClone(after.project); for (const s of withoutHuman.sequences) s.bookmarks = s.bookmarks.filter(v => v.id !== bookmarkId);
    expect(withoutHuman).toEqual(state.initial.snapshot.project);
    const actors = await client.call("opencut_project_revision_actors", { project_id: project, revisions: [state.receipts["agent-original-edit"].revision, state.receipts["human-during-denial"].revision, after.revision] });
    expect(actors.actors[state.receipts["agent-original-edit"].revision].principal_id).toBe(principal);
    expect(actors.actors[state.receipts["human-during-denial"].revision].principal_id).toBe("local-owner"); expect(actors.actors[after.revision].principal_id).toBe(principal);
    await expect(page.getByText(after.project.name + " · V4", { exact: true })).toBeVisible();
    return { restored_agent_observed_human_head: true, stale_apply_after_restore: "REVISION_CONFLICT", old_credential_denied: true, same_principal_new_credential: issued.credential_id, selective_agent_undo_preserves_later_human_bookmark: true, actors };
  });
  await stage("restore-fixture-and-disable-test-authority", async () => {
    if (!state.receipts["owner-undo-human"]) {
      const current = await inspect(), input = { project_id: project, expected_revision: current.revision, target_transaction_id: state.receipts["human-during-denial"].transaction_id, idempotency_key: "grant-recovery:owner-undo-human" };
      state.requests["owner-undo-human"] ??= input; await save(); expect(cli("opencut_edit_undo_plan", state.requests["owner-undo-human"]).conflicts).toEqual([]);
      await remember("owner-undo-human", () => cli("opencut_edit_undo_apply", state.requests["owner-undo-human"]));
    }
    await remember("disabled-policy", () => admin(["set-policy", "--input", "-"], { principal_id: principal, expected_revision: state.receipts["restored-policy"].revision, enabled: false, grants: [] }));
    await remember("revoked-B", () => admin(["revoke", state.receipts["credential-B"].credential_id]));
    const final = await inspect(); expect(final.project).toEqual(state.initial.snapshot.project);
    const after = counts(); expect(after).toEqual({ ...state.initial.counts, revisions: state.initial.counts.revisions + 4, transactions: state.initial.counts.transactions + 4 });
    await fs.writeFile(fixtureFile, JSON.stringify({ ...state.initial.fixture, revision: final.revision }, null, 2) + "\n");
    await page.reload(); await expect(page.getByText(final.project.name + " · V4", { exact: true })).toBeVisible();
    return { final_revision: final.revision, full_project_restored: true, expected_four_transactions: true, other_heads_jobs_and_project_count_unchanged: true, principal_disabled_and_both_credentials_revoked: true, counts: after };
  });
}
