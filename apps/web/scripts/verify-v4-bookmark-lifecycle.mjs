import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";

// Existing B1/B2 fixture only; root owns all service/browser lifecycle.
export async function verifyV4BookmarkLifecycle(browser, endpoint, directory, check) {
  const page = await browser.newPage({locale:"en-US",viewport:{width:1440,height:1100}});
  page.setDefaultTimeout(30000);
  const errors = [], passed = [];
  let phase = "bootstrap", original, target, deleted, restored;
  const id = process.env.OPENCUT_B1_PROJECT_ID || JSON.parse(await fs.readFile(path.join(directory,"b1-text-project.json"),"utf8")).project_id;
  page.on("pageerror", error => errors.push(error.message));
  const tool = async (name,input) => {
    const result = await page.evaluate(async ({name,input}) => (await fetch(`/v1/tools/${name}`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(input)})).json(),{name,input});
    expect(result.error,JSON.stringify(result.error)).toBeNull();
    return result.data;
  };
  const inspect = () => tool("opencut_project_v4_inspect",{project_id:id,revision:null});
  const ready = async () => {
    const guide = page.getByRole("dialog",{name:"Welcome to OpenCut",exact:true});
    if (await guide.isVisible()) await guide.getByRole("button",{name:"Close",exact:true}).click();
    await expect(page.getByRole("main",{name:"V4 editor",exact:true})).toBeVisible();
  };
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button",{name:"New project with scene notes",exact:true})).toBeEnabled();
    await page.goto(`${endpoint}/editor/${encodeURIComponent(id)}`);
    await ready();
    original = await inspect();
    const sequence = original.project.sequences.find(s => s.id === original.project.default_sequence_id);
    expect(sequence.bookmarks.length,"existing B2 bookmark fixture").toBeGreaterThanOrEqual(2);
    // B2 deliberately has two notes at the same time. Select the last painted
    // marker through normal pointer input, without forcing a covered click.
    target = sequence.bookmarks.findLast(b => b.note?.includes("B2 EDITOR NOTE MUST NOT ENTER OUTPUT"));
    expect(target,"existing full-duration B2 note").toBeTruthy();
    const expectedDeleted = structuredClone(original.project);
    expectedDeleted.sequences.find(s => s.id === sequence.id).bookmarks = sequence.bookmarks.filter(b => b.id !== target.id);
    const selectScene = async () => page.getByRole("combobox",{name:/^V4 scene(?:\s|$)/}).selectOption(sequence.id);
    await selectScene();
    await expect(page.locator("button.v4-marker")).toHaveCount(sequence.bookmarks.length);
    const marker = page.locator("button.v4-marker").nth(sequence.bookmarks.findIndex(b => b.id === target.id));
    await marker.click();
    await expect(marker).toHaveAttribute("aria-pressed","true");
    phase = "delete";
    const deleteButton = page.getByRole("button",{name:"Delete bookmark",exact:true});
    await expect(deleteButton).toBeEnabled();
    expect(await inspect()).toEqual(original);
    await deleteButton.click();
    await expect.poll(async () => { deleted = await inspect(); return deleted.project; }).toEqual(expectedDeleted);
    expect(deleted.revision).not.toBe(original.revision);
    await expect(page.locator("button.v4-marker")).toHaveCount(sequence.bookmarks.length-1);
    passed.push("delete: exact target absent, all other project content unchanged");
    phase = "reload_deleted";
    await page.reload();
    await ready();
    await selectScene();
    const reopenedDeleted = await inspect();
    expect(reopenedDeleted.revision).toBe(deleted.revision);
    expect(reopenedDeleted.project).toEqual(expectedDeleted);
    await expect(page.locator("button.v4-marker")).toHaveCount(sequence.bookmarks.length-1);
    passed.push("reload: deletion persisted at fixed revision");
    phase = "undo_plan";
    const undo = page.getByRole("button",{name:"Undo",exact:true});
    await expect(undo).toBeEnabled();
    await undo.click();
    await expect(page.getByText("Undo plan",{exact:true})).toBeVisible();
    const apply = page.getByRole("button",{name:"Apply undo",exact:true});
    await expect(apply).toBeEnabled();
    // Reading the plan must not publish a revision or mutate the fixed head.
    expect(await inspect()).toEqual(reopenedDeleted);
    passed.push("undo plan ready: fixed deleted head unchanged");
    phase = "apply_undo";
    await apply.click();
    await expect.poll(async () => { restored = await inspect(); return restored.project; }).toEqual(original.project);
    expect(restored.revision).not.toBe(deleted.revision);
    await expect(apply).toBeHidden();
    await expect(page.locator("button.v4-marker")).toHaveCount(sequence.bookmarks.length);
    passed.push("undo: original bookmark identity and complete project restored");
    phase = "reload_restored";
    await page.reload();
    await ready();
    await selectScene();
    const reopened = await inspect();
    expect(reopened.revision).toBe(restored.revision);
    expect(reopened.project).toEqual(original.project);
    expect(reopened.project.sequences.find(s => s.id === sequence.id).bookmarks.find(b => b.id === target.id)).toEqual(target);
    await expect(page.locator("button.v4-marker")).toHaveCount(sequence.bookmarks.length);
    expect(errors).toEqual([]);
    passed.push("reload: original identity and full project restoration persisted");
    const evidence = {project_id:id,sequence_id:sequence.id,bookmark:target,original_revision:original.revision,deleted_revision:deleted.revision,restored_revision:restored.revision,passed,pageerrors:errors};
    await fs.writeFile(path.join(directory,"f10-bookmark-lifecycle.json"),JSON.stringify(evidence,null,2)+"\n");
    check("F10 real bookmark deletion/reopen and Undo/reopen preserve original identity and all other project content",evidence);
  } catch (error) {
    const ui = await page.locator("main").innerText().catch(() => "unavailable");
    await fs.writeFile(path.join(directory,"f10-bookmark-lifecycle-failure.json"),JSON.stringify({project_id:id,phase,passed,bookmark_id:target?.id,original_revision:original?.revision,deleted_revision:deleted?.revision,restored_revision:restored?.revision,error:String(error),pageerrors:errors.slice(-8),ui:ui.slice(0,3500)},null,2)+"\n");
    throw error;
  } finally { await page.close(); }
}
