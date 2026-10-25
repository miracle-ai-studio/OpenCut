import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";

// A single owned SQLite fixture, real product controls and actual committed
// responses. Only the delivery of the original write is interrupted.
export async function verifyEditRecovery(browser, endpoint, _directory, check) {
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [], requests = [], cases = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => {
    if (request.url().includes("/api/tools/")) requests.push({ name: request.url().split("/").at(-1), input: request.postDataJSON() });
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error).toBeNull();
    return envelope.data;
  };
  const operations = {
    public_apply: "opencut_edit_apply", undo: "opencut_edit_undo_apply",
    redo: "opencut_edit_redo_apply", restore: "opencut_project_restore",
  };
  let routeHandler, routePattern;
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button", { name: "New project", exact: true })).toBeEnabled();
    for (const kind of Object.keys(operations)) for (const mode of ["submitted_lost", "not_sent"]) {
      const projectId = `recovery-${kind}-${mode}-${crypto.randomUUID()}`, sequenceId = "main";
      let snapshot = await tool("opencut_project_create", { project: {
        id: projectId, name: projectId, sequence_id: sequenceId, width: 320, height: 180,
        fps: { numerator: 24, denominator: 1 },
      }, idempotency_key: `${projectId}-create` });
      const seed = async (id, note) => {
        const receipt = await tool("opencut_edit_apply", { project_id: projectId, expected_revision: snapshot.revision,
          idempotency_key: `${projectId}-${id}`, label: `Seed ${id}`, operations: [{ type: "bookmark", operation: {
            kind: "put", bookmark: { id, sequence_id: sequenceId, time: { ticks: "1", timebase: 1 },
              duration: null, color: null, note },
          } }] });
        snapshot = await editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
        return receipt;
      };
      const first = await seed("first", "First seed"), second = await seed("second", "Second seed");
      if (kind === "redo") {
        await tool("opencut_edit_undo_apply", { project_id: projectId, expected_revision: snapshot.revision,
          target_transaction_id: second.transaction_id, idempotency_key: `${projectId}-prepare-redo` });
      }
      if (kind === "restore") {
        // Restore is project-scoped: the target scene need not still exist at
        // the captured head. Prove this with an actual replacement/deletion.
        const replacement = structuredClone(snapshot.project.sequences[0]);
        replacement.id = "replacement"; replacement.name = "Replacement scene"; replacement.bookmarks = [];
        await tool("opencut_edit_apply", { project_id: projectId, expected_revision: snapshot.revision,
          idempotency_key: `${projectId}-replace-scene`, label: "Replace original scene", operations: [
            { type: "sequence_put", sequence: replacement },
            { type: "default_sequence_set", sequence_id: replacement.id },
            { type: "sequence_delete", sequence_id: sequenceId },
          ] });
      }
      await page.goto(`${endpoint}/editor/${projectId}`);
      await expect(page.getByRole("main", { name: "OpenCut editor" })).toBeVisible();
      const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
      if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
      if (kind === "public_apply") {
        await page.getByRole("button", { name: "Add bookmark", exact: true }).click();
        await page.getByLabel("Canvas note", { exact: true }).fill("Recovered original note");
      } else if (kind === "undo" || kind === "redo") {
        await page.getByRole("button", { name: kind === "undo" ? "Undo" : "Redo", exact: true }).click();
        await expect(page.getByRole("button", { name: kind === "undo" ? "Apply undo" : "Apply redo", exact: true })).toBeEnabled();
      } else {
        await page.locator(".editor-revisions li").filter({ hasText: "Seed first" })
          .getByRole("button", { name: "View revision", exact: true }).click();
        await expect(page.locator(".editor-revisions li").filter({ hasText: "Seed first" })
          .getByRole("button", { name: "Restore this revision", exact: true })).toBeEnabled();
      }
      const before = await editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
      const historyBefore = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
      const start = requests.length;
      let original, originalReply, interrupted = false;
      routePattern = `**/api/tools/${operations[kind]}`;
      routeHandler = async route => {
        original = route.request().postDataJSON();
        if (mode === "submitted_lost") originalReply = await (await route.fetch()).json();
        await route.abort("failed");
        interrupted = true;
      };
      await page.route(routePattern, routeHandler, { times: 1 });
      const dispatch = kind === "restore" ? page.locator(".editor-revisions li").filter({ hasText: "Seed first" })
        .getByRole("button", { name: "Restore this revision", exact: true })
        : page.getByRole("button", { name: kind === "public_apply" ? "Save bookmark" : kind === "undo" ? "Apply undo" : "Apply redo", exact: true });
      await dispatch.click();
      await expect.poll(() => interrupted).toBe(true);
      const panel = page.getByRole("region", { name: "Recover original edit", exact: true });
      await expect(panel).toBeVisible();
      await expect(panel.getByRole("button", { name: "Retry original edit", exact: true })).toBeVisible();
      if (kind !== "restore") await expect(panel.getByRole("button", { name: "Retry original edit", exact: true })).toBeEnabled();
      expect(original.project_id).toBe(projectId); expect(original.expected_revision).toBe(before.revision);
      if (mode === "submitted_lost") expect(originalReply.error).toBeNull();
      const staged = await tool("opencut_edit_recovery_inspect", { project_id: projectId, recovery_id: null });
      expect(staged.record.context.intent).toEqual({ kind, request: original });
      if (kind === "restore") {
        expect(before.project.sequences.some(s => s.id === sequenceId)).toBe(false);
        expect(staged.record.context.sequence_id).toBeNull();
      }
      const expectedReceipt = mode === "submitted_lost" ? { kind, [kind === "undo" || kind === "redo" ? "commit" : "transaction"]: originalReply.data } : null;
      expect(staged.record.receipt).toEqual(expectedReceipt);
      const afterInspect = await editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
      expect(afterInspect.revision).toBe(mode === "not_sent" ? before.revision
        : (kind === "undo" || kind === "redo" ? originalReply.data.transaction : originalReply.data).revision);
      await page.unroute(routePattern, routeHandler); routeHandler = null;
      const plansBeforeReload = requests.filter(r => r.name.endsWith("_plan") || r.name.endsWith("_plan_v4")).length;
      await page.reload();
      await expect(panel).toBeVisible();
      const discovered = await tool("opencut_edit_recovery_inspect", { project_id: projectId, recovery_id: null });
      expect(discovered.record.context).toEqual(staged.record.context);
      expect(discovered.record.receipt).toEqual(expectedReceipt);
      expect(requests.filter(r => r.name.endsWith("_plan") || r.name.endsWith("_plan_v4"))).toHaveLength(plansBeforeReload);
      const originalWrites = () => requests.slice(start).filter(r => r.name === operations[kind]);
      expect(originalWrites()).toHaveLength(1);
      let receipt = originalReply?.data;
      if (mode === "not_sent") {
        const [response] = await Promise.all([
          page.waitForResponse(r => r.url().endsWith(`/api/tools/${operations[kind]}`)),
          panel.getByRole("button", { name: "Retry original edit", exact: true }).click(),
        ]);
        const replay = await response.json(); expect(replay.error).toBeNull(); receipt = replay.data;
        expect(response.request().postDataJSON()).toEqual(original);
        expect(originalWrites().map(r => r.input)).toEqual([original, original]);
      } else {
        await expect(panel.getByRole("button", { name: "Retry original edit", exact: true })).toHaveCount(0);
      }
      if (mode === "submitted_lost") {
        await expect(panel.getByRole("button", { name: "Release resolved edit", exact: true })).toBeEnabled();
        await panel.getByRole("button", { name: "Release resolved edit", exact: true }).click();
      }
      if (kind === "restore" && mode === "not_sent") {
        const writesBeforeLocale = requests.filter(r => Object.values(operations).includes(r.name)).length;
        await page.getByRole("combobox", { name: "Language", exact: true }).selectOption("zh");
        await expect(page.getByRole("region", { name: "恢复原编辑操作", exact: true })).toContainText("原编辑操作已保存。");
        await page.getByRole("combobox", { name: "语言", exact: true }).selectOption("en");
        expect(requests.filter(r => Object.values(operations).includes(r.name))).toHaveLength(writesBeforeLocale);
      }
      await expect(panel.getByRole("button", { name: "Done", exact: true })).toBeEnabled();
      await panel.getByRole("button", { name: "Done", exact: true }).click();
      await expect(panel).toBeHidden();
      expect((await tool("opencut_edit_recovery_inspect", { project_id: projectId, recovery_id: null })).record).toBeNull();
      const result = await editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
      const transaction = kind === "undo" || kind === "redo" ? receipt.transaction : receipt;
      expect(result.revision).toBe(transaction.revision); expect(transaction.base_revision).toBe(before.revision);
      if (kind === "restore") expect(result.project.default_sequence_id).toBe(sequenceId);
      const notes = result.project.sequences.find(s => s.id === sequenceId).bookmarks.map(b => b.note).sort();
      expect(notes).toEqual((kind === "public_apply" ? ["First seed", "Second seed", "Recovered original note"]
        : kind === "redo" ? ["First seed", "Second seed"] : ["First seed"]).sort());
      const historyAfter = await tool("opencut_project_history", { project_id: projectId, limit: 50 });
      expect(historyAfter.revisions).toHaveLength(historyBefore.revisions.length + 1);
      expect(await page.evaluate(() => JSON.parse(sessionStorage.getItem("opencut.edit-recovery") ?? "null")?.entries ?? [])).toEqual([]);
      cases.push({ kind, mode, recovery_id: staged.record.recovery_id, transaction_id: transaction.transaction_id,
        base_revision: before.revision, revision: result.revision, dispatches: originalWrites().length,
        original_key: original.idempotency_key, restore_target: kind === "restore" ? first.revision : null,
        restore_deleted_scene: kind === "restore" });
      check(`${kind}/${mode}: whole-page reload recovers the exact typed intent and receipt; explicit replay uses the original key and commits once`);
    }
    expect(errors).toEqual([]);
    check("four write families, eight real delivery failures, zero page exceptions; pure inspection does not publish and resolved pointers are removed", { edit_recovery_v2: { cases, page_errors: errors } });
  } finally {
    if (routeHandler) await page.unroute(routePattern, routeHandler);
    await page.close();
  }
}
