import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";

// Root owns one synthetic B1/B2 workspace, service/candidate and serial browser
// run. This module never copies/imports media, creates a project or starts jobs
// outside actual product controls. DOM composition events test browser lifecycle;
// a real macOS input-method session remains a separate manual acceptance scope.
export async function verifyV4CanvasText(browser, endpoint, directory, check, options = {}) {
  const { project_id: projectId } = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const network = [], errors = [], writes = [], plans = [];
  const limits = { network: 256, errors: 32, writes: 32, plans: 32 };
  let writeCount = 0, planCount = 0;
  const prior = options.blurOnly
    ? JSON.parse(await fs.readFile(path.join(directory, "b3-canvas-text-failure.json"), "utf8"))
    : null;
  if (prior) expect(prior.project_id).toBe(projectId);
  let phase = "open", original;
  page.on("pageerror", error => { if (errors.length < limits.errors) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (!operation.startsWith("opencut_")) return;
    if (network.length < limits.network) network.push({ operation, method: request.method() });
    if (operation === "opencut_edit_v4_apply") {
      writeCount++; if (writes.length < limits.writes) writes.push(request.postDataJSON());
    }
    if (operation === "opencut_text_v1_plan") {
      planCount++; if (plans.length < limits.plans) plans.push(request.postDataJSON());
    }
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input),
    })).json(), { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: projectId, revision: null });
  const readTitle = (snapshot, id) => snapshot.project.sequences.flatMap(scene => scene.tracks.flatMap(track => track.elements)).find(element => element.id === id);
  const evidence = prior ? [...prior.evidence] : [];
  try {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    original = await inspect();
    const sequence = original.project.sequences.find(scene => scene.id === original.project.default_sequence_id);
    await page.getByRole("combobox", { name: /^V4 scene(?:\s|$)/ }).selectOption(sequence.id);
    const titles = sequence.tracks.filter(track => track.enabled && !track.locked).flatMap(track => track.elements).filter(element => element.content.kind === "text" && element.enabled);
    expect(titles.length).toBeGreaterThan(0);
    // Reuse B2's Chinese title by its locked font; its display name has changed
    // since B1. Native admission must check all newly entered test glyphs.
    const priorElementId = prior?.evidence[0]?.transaction?.operations.find(operation => operation.type === "element_put")?.element.id;
    const title = titles.find(element => (!priorElementId || element.id === priorElementId) && original.project.fonts.some(font => font.id === element.content.text.font.font_id && font.family === "Ma Shan Zheng"));
    expect(title, "Existing Chinese title with pinned Ma Shan Zheng is required").toBeTruthy();
    const seek = Number(title.timeline_range.start.ticks) / title.timeline_range.start.timebase + .01;
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(String(seek));
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
    const begin = async () => {
      const current = await inspect(), currentTitle = readTitle(current, title.id);
      const target = page.getByRole("button", { name: `Edit text on canvas: ${currentTitle.content.text.content}`, exact: true });
      await expect(target).toBeVisible({ timeout: 30000 }); await target.dblclick();
      const input = page.getByRole("textbox", { name: "Edit text on canvas", exact: true });
      await expect(input).toBeFocused({ timeout: 30000 }); return { input, current };
    };
    if (options.blurOnly) {
      const restoreRevision = prior.evidence[0]?.original_revision;
      expect(restoreRevision, "Original fixed revision from the completed confirm evidence is required").toBeTruthy();
      const baseline = await tool("opencut_project_v4_inspect", { project_id: projectId, revision: restoreRevision });
      const baselineTitle = readTitle(baseline, title.id);
      expect(baselineTitle?.content.kind).toBe("text");
      phase = "IME blur补验";
      const { input, current } = await begin();
      // All characters were admitted by the preceding real run; use a different
      // order so this cannot succeed as an unchanged draft.
      const content = readTitle(current, title.id).content.text.content === "AV To\n中文再次编辑"
        ? "AV To\n再次中文编辑" : "AV To\n中文再次编辑";
      await input.dispatchEvent("compositionstart", { data: "再" });
      await input.fill(content);
      await input.evaluate(node => node.blur());
      expect((await inspect()).revision).toBe(current.revision);
      expect(writeCount).toBe(0);
      await input.dispatchEvent("compositionend", { data: "编辑" });
      await expect(input).toHaveCount(0);
      await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe(content);
      expect(writeCount).toBe(1); expect(planCount).toBe(1);
      expect(writes[0].expected_revision).toBe(current.revision);
      expect(writes[0].idempotency_key).toBe(plans[0].idempotency_key);
      evidence.push({ case: "IME blur", supplemental: true, original_revision: current.revision, revision: (await inspect()).revision, transaction: writes[0] });
      phase = "restore original fixed revision";
      const changed = await inspect();
      await tool("opencut_project_v4_restore", { project_id: projectId, expected_revision: changed.revision, target_revision: restoreRevision, idempotency_key: randomUUID(), asset_versions: {} });
      await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe(baselineTitle.content.text.content);
      expect(errors).toEqual([]);
      const report = { project_id: projectId, sequence_id: sequence.id, element_id: title.id, status: "passed", supplemental: "blurOnly", previous_failure: { phase: prior.phase, error: prior.error }, composition_driver: "DOM composition events and browser native fill; no real macOS IME claim", evidence, restored_from_revision: restoreRevision, final_revision: (await inspect()).revision, counts: { writes: writeCount, plans: planCount }, limits };
      await fs.writeFile(path.join(directory, "b3-canvas-text.json"), JSON.stringify(report, null, 2) + "\n");
      check("B3 supplemental canvas composition blur and original revision restore", report);
      return;
    }
    phase = "IME confirm";
    let { input, current } = await begin();
    await input.dispatchEvent("compositionstart", { data: "中" });
    await input.fill("AV To\n中文编");
    await page.getByRole("button", { name: "Confirm text", exact: true }).click();
    expect((await inspect()).revision).toBe(current.revision); expect(writeCount).toBe(0);
    await input.fill("AV To\n中文编辑");
    await input.dispatchEvent("compositionend", { data: "编辑" });
    await expect(input).toHaveCount(0);
    await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe("AV To\n中文编辑");
    expect(writeCount).toBe(1); expect(planCount).toBe(1);
    expect(plans[0]).toMatchObject({ project_id: projectId, expected_revision: current.revision, sequence_id: sequence.id, command: { kind: "edit", target: { element_id: title.id }, values: { text: { content: "AV To\n中文编辑" } } } });
    expect(writes[0].expected_revision).toBe(current.revision); expect(writes[0].idempotency_key).toBe(plans[0].idempotency_key);
    const committed = await inspect();
    evidence.push({ case: phase, original_revision: current.revision, revision: committed.revision, transaction: writes[0] });
    phase = "IME Escape and shortcut isolation";
    ({ input, current } = await begin());
    await input.dispatchEvent("compositionstart", { data: "草" }); await input.fill("丢弃草稿");
    await input.press("ArrowLeft"); await input.press("ControlOrMeta+a"); await input.press("Backspace");
    await input.press("Escape"); await expect(input).toHaveCount(0);
    expect((await inspect()).revision).toBe(current.revision); expect(writeCount).toBe(1);
    evidence.push({ case: phase, revision: current.revision, commits: 0 });
    phase = "native frame after canvas confirmation";
    const submitted = page.waitForResponse(response => response.url().endsWith("/v1/tools/opencut_job_v4_text_frame_png"));
    await page.getByRole("button", { name: "Render scene frame", exact: true }).click();
    const response = await submitted, envelope = await response.json(); expect(envelope.error).toBeNull();
    expect(response.request().postDataJSON().revision).toBe(committed.revision);
    let job;
    await expect.poll(async () => { job = await tool("opencut_job_view", { job_id: envelope.data.job_id }); return job.state; }, { timeout: 120000, intervals: [100, 250, 500] }).toBe("succeeded");
    const result = job.extended_result.data, receipt = result.texts.find(text => (text.elementId ?? text.element_id) === title.id);
    expect(receipt.layout.lines.map(line => line.content)).toEqual(["AV To", "中文编辑"]);
    await expect(page.getByRole("img", { name: "Rendered scene frame", exact: true })).toBeVisible();
    evidence.push({ case: phase, identity: result.identity, texts: result.texts, font_lock: result.font_lock, artifact: result.artifact, execution: result.execution });
    phase = "Undo and Redo";
    await page.getByRole("button", { name: "Undo", exact: true }).click(); await page.getByRole("button", { name: "Apply undo", exact: true }).click();
    await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe(title.content.text.content);
    await page.getByRole("button", { name: "Redo", exact: true }).click(); await page.getByRole("button", { name: "Apply redo", exact: true }).click();
    await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe("AV To\n中文编辑");
    phase = "IME blur";
    ({ input, current } = await begin());
    await input.dispatchEvent("compositionstart", { data: "再" }); await input.fill("AV To\n中文再次编辑");
    await input.evaluate(node => node.blur());
    expect((await inspect()).revision).toBe(current.revision); expect(writeCount).toBe(1);
    await input.dispatchEvent("compositionend", { data: "编辑" });
    await expect(input).toHaveCount(0);
    await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe("AV To\n中文再次编辑");
    expect(writeCount).toBe(2);
    evidence.push({ case: phase, original_revision: current.revision, revision: (await inspect()).revision, transaction: writes[1] });
    phase = "restore existing text";
    ({ input } = await begin()); await input.fill(title.content.text.content);
    await page.getByRole("button", { name: "Confirm text", exact: true }).click(); await expect(input).toHaveCount(0);
    await expect.poll(async () => readTitle(await inspect(), title.id).content.text.content).toBe(title.content.text.content);
    expect(errors).toEqual([]);
    const report = { project_id: projectId, sequence_id: sequence.id, element_id: title.id, status: "passed", composition_driver: "DOM composition events and browser native fill; no real macOS IME claim", evidence, final_revision: (await inspect()).revision };
    await fs.writeFile(path.join(directory, "b3-canvas-text.json"), JSON.stringify(report, null, 2) + "\n");
    check("B3 fixed-font canvas composition, cancellation, native frame and original transactions", report);
  } catch (error) {
    const failure = { project_id: projectId, phase, status: "failed", error: String(error).slice(0, 4096), errors, network, counts: { writes: writeCount, plans: planCount }, limits, writes: writes.slice(-4), plans: plans.slice(-4), evidence };
    try { failure.ui = await page.evaluate(() => ({ url: location.pathname, alerts: [...document.querySelectorAll('[role="alert"],[role="status"]')].slice(0, 16).map(node => node.textContent.slice(0, 1024)), canvas: [...document.querySelectorAll('.v4-canvas textarea')].map(node => ({ value: node.value.slice(0, 1024), composing: node.isComposing, focused: document.activeElement === node, bounds: node.getBoundingClientRect().toJSON() })) })); } catch {}
    await fs.writeFile(path.join(directory, options.blurOnly ? "b3-canvas-text-blur-failure.json" : "b3-canvas-text-failure.json"), JSON.stringify(failure, null, 2) + "\n");
    await page.screenshot({ path: path.join(directory, options.blurOnly ? "b3-canvas-text-blur-failure.png" : "b3-canvas-text-failure.png"), fullPage: false }).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
