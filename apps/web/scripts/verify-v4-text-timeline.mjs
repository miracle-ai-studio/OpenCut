import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID, createHash } from "node:crypto";

const IDS = ["cf25b163-cc52-46f7-ab1a-9719bb1e6180", "5a79aa1f-eb9a-4564-a07e-5ea09b2d721d"];
const exact = (actual, numerator, denominator = 1) => expect(BigInt(actual.ticks) * BigInt(denominator)).toBe(BigInt(numerator) * BigInt(actual.timebase));
const digest = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const prefix = name => new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);

// Root owns registration/candidate and serial execution. This uses existing
// text IDs and fonts only. No project/media copy, rendering or S1 rerun.
export async function verifyV4TextTimeline(browser, endpoint, directory, check, options = {}) {
  const fixture = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const projectId = fixture.project_id;
  const sequenceId = "61d3fde3-3689-428c-ad73-04e87a43d257";
  const checkpointPath = path.join(directory, "f19-text-timeline-checkpoint.json");
  expect(process.env.OPENCUT_WEB_BINARY, "Explicit candidate binary required").toBeTruthy();
  const candidate = { binary_sha256: createHash("sha256").update(await fs.readFile(process.env.OPENCUT_WEB_BINARY)).digest("hex") };
  // Record the exact built asset tree for explicit same-candidate continuation.
  const tree = [];
  const walk = async relative => {
    for (const entry of await fs.readdir(path.join(process.env.OPENCUT_WEB_ROOT, relative), { withFileTypes: true })) {
      const name = path.join(relative, entry.name);
      if (entry.isDirectory()) await walk(name);
      else if (entry.isFile() && /\.(js|mjs|cjs|css)$/u.test(name)) tree.push([name, createHash("sha256").update(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name))).digest("hex")]);
      else if (entry.isSymbolicLink()) throw new Error(`Unhashed Web candidate symlink ${name}`);
    }
  };
  expect(process.env.OPENCUT_WEB_ROOT, "Explicit built Web root required").toBeTruthy();
  await walk(""); expect(tree.length).toBeGreaterThan(0); tree.sort((a, b) => a[0].localeCompare(b[0])); candidate.web_js_css_tree_sha256 = digest(tree);
  let prior;
  if (options.resumeFromCheckpoint) {
    prior = JSON.parse(await fs.readFile(checkpointPath, "utf8"));
    expect(prior.candidate).toEqual(candidate); expect(prior.project_id).toBe(projectId); expect(prior.sequence_id).toBe(sequenceId);
  }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const evidence = prior?.evidence ?? [], state = prior?.state ?? {}, network = [], errors = [];
  let phase = "open", original, failure, mutated = false, writeCount = 0;
  page.on("pageerror", error => { if (errors.length < 32) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 256) network.push({ operation });
    if (operation === "opencut_edit_v4_apply") writeCount++;
  });
  const tool = async (name, input) => {
    const result = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { name, input });
    expect(result.error, JSON.stringify(result.error)).toBeNull(); return result.data;
  };
  const inspect = revision => tool("opencut_project_v4_inspect", { project_id: projectId, revision: revision ?? null });
  const scene = snapshot => snapshot.project.sequences.find(scene => scene.id === sequenceId);
  const find = (snapshot, id) => scene(snapshot).tracks.flatMap(track => track.elements).find(element => element.id === id);
  const target = (snapshot, id) => ({ track_id: scene(snapshot).tracks.find(track => track.elements.some(element => element.id === id)).id, element_id: id });
  const save = async facts => {
    Object.assign(state, facts);
    await fs.writeFile(checkpointPath, JSON.stringify({ project_id: projectId, sequence_id: sequenceId, candidate, phase, state, evidence }, null, 2) + "\n");
  };
  const exchange = async (name, action) => {
    const pending = page.waitForResponse(response => response.url().endsWith(`/v1/tools/${name}`));
    await action(); const response = await pending, envelope = await response.json();
    const facts = { request: response.request().postDataJSON(), envelope };
    await save({ last_exchange: { operation: name, ...facts } });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return { request: facts.request, data: envelope.data };
  };
  const reopen = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}?scene=${encodeURIComponent(sequenceId)}`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("combobox", { name: /^V4 scene(?:\s|$)/ }).selectOption(sequenceId);
  };
  const restore = async revision => {
    const head = await inspect();
    await tool("opencut_project_v4_restore", { project_id: projectId, expected_revision: head.revision, target_revision: revision, idempotency_key: randomUUID(), asset_versions: {} });
    return inspect();
  };
  const toolbar = page.locator('[aria-label="Timeline editing"]'), clipboard = page.locator('[aria-label="Timeline clipboard"]');
  const select = async id => {
    const clip = page.locator(`[data-element-id="${id}"] .v4-element-select`);
    await clip.scrollIntoViewIfNeeded(); await clip.click({ position: { x: 12, y: 12 } });
    await expect(clip).toHaveAttribute("aria-pressed", "true"); await expect(toolbar).toContainText("1 selected");
  };
  const seek = async seconds => {
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill(seconds);
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
  };
  const authored = (actual, source, start, duration, trimStart, trimEnd, name = source.name) => {
    expect(actual.content).toEqual(source.content); expect(actual.visual).toEqual(source.visual);
    expect(actual.channels).toEqual(source.channels); expect(actual.enabled).toBe(source.enabled);
    expect(actual.name).toBe(name); expect(actual.source_span.profile).toBe(source.source_span.profile);
    expect(actual.source_span.source_duration).toBeNull(); exact(actual.source_span.trim_start, ...trimStart); exact(actual.source_span.trim_end, ...trimEnd);
    exact(actual.timeline_range.start, ...start); exact(actual.timeline_range.duration, ...duration);
  };
  const timeline = async (action, fill, expectedCommand) => {
    const before = await inspect(), beforeCount = writeCount;
    const selectedIds = await page.locator("[data-element-id] .v4-element-select[aria-pressed=true]").evaluateAll(nodes => nodes.map(node => node.closest("[data-element-id]").dataset.elementId));
    await toolbar.getByRole("button", { name: action, exact: true }).click();
    const form = page.getByRole("dialog", { name: action, exact: true }); await fill(form);
    const planned = await exchange("opencut_edit_element_timeline_plan_v4", () => form.getByRole("button", { name: "Review timeline changes", exact: true }).click());
    await save({ pending_plan: planned, pending_base_revision: before.revision });
    expect(planned.request).toMatchObject({ project_id: projectId, expected_revision: before.revision, sequence_id: sequenceId, command: expectedCommand });
    const commandTargets = planned.request.command.kind === "move_selection" ? planned.request.command.moves.map(move => move.target) : planned.request.command.kind === "trim" ? [planned.request.command.target] : planned.request.command.targets.map(value => value.target ?? value);
    expect(commandTargets.map(value => value.element_id).sort()).toEqual(selectedIds.sort());
    for (const value of commandTargets) expect(value).toEqual(target(before, value.element_id));
    expect(planned.request.idempotency_key).toBeTruthy(); expect(planned.data.transaction.idempotency_key).toBe(planned.request.idempotency_key);
    expect((await inspect()).revision).toBe(before.revision); expect(writeCount).toBe(beforeCount);
    const review = page.getByRole("dialog", { name: "Review timeline changes", exact: true });
    await expect(review.getByRole("status")).toHaveText("Review timeline changes before saving.");
    mutated = true;
    const applied = await exchange("opencut_edit_v4_apply", () => review.getByRole("button", { name: "Apply timeline changes", exact: true }).click());
    await save({ last_revision: applied.data.revision });
    expect(applied.request).toEqual(planned.data.transaction); expect(writeCount).toBe(beforeCount + 1);
    expect(applied.data.base_revision).toBe(before.revision);
    await expect(review.getByRole("status")).toHaveText("Timeline changes saved.");
    await review.getByRole("button", { name: "Done", exact: true }).click(); await expect(review).toBeHidden();
    await expect.poll(async () => (await inspect()).revision).toBe(applied.data.revision);
    return { before, planned, applied, snapshot: await inspect() };
  };
  const drag = async (locator, dx) => {
    await locator.scrollIntoViewIfNeeded();
    await locator.evaluate(node => {
      const viewport = node.closest(".v4-timeline-viewport"), box = node.getBoundingClientRect();
      const label = node.closest(".v4-track").querySelector(".v4-track-name").getBoundingClientRect();
      if (box.left < label.right + 40) viewport.scrollLeft -= label.right + 40 - box.left;
    });
    const point = await locator.evaluate(node => {
      const rect = node.getBoundingClientRect(), x = rect.left + Math.min(12, rect.width / 2), y = rect.top + Math.min(12, rect.height / 2);
      return { x, y, hittable: node.contains(document.elementFromPoint(x, y)) };
    });
    expect(point.hittable, "Physical pointer must hit the actual text body/trim handle").toBe(true);
    await page.mouse.move(point.x, point.y); await page.mouse.down();
    await page.mouse.move(point.x + dx, point.y, { steps: 4 }); await page.mouse.up();
  };
  const gesture = async (action, expectedCommand) => {
    const before = await inspect(), count = writeCount;
    const planned = await exchange("opencut_edit_element_timeline_plan_v4", action);
    await save({ pending_plan: planned, pending_base_revision: before.revision });
    expect(planned.request).toMatchObject({ project_id: projectId, expected_revision: before.revision, sequence_id: sequenceId, command: expectedCommand });
    expect((await inspect()).revision).toBe(before.revision); expect(writeCount).toBe(count);
    const review = page.getByRole("dialog", { name: "Review timeline changes", exact: true });
    await expect(review.getByRole("status")).toHaveText("Review timeline changes before saving."); mutated = true;
    const applied = await exchange("opencut_edit_v4_apply", () => review.getByRole("button", { name: "Apply timeline changes", exact: true }).click());
    await save({ last_revision: applied.data.revision });
    expect(applied.request).toEqual(planned.data.transaction); expect(writeCount).toBe(count + 1);
    await expect(review.getByRole("status")).toHaveText("Timeline changes saved."); await review.getByRole("button", { name: "Done", exact: true }).click();
    await expect(review).toBeHidden();
    return { before, planned, applied, snapshot: await inspect() };
  };
  const history = async (direction, expectedProject) => {
    const area = page.locator(".v4-history"), before = await inspect();
    const proposal = await exchange(`opencut_edit_v4_${direction}_plan`, () => area.getByRole("button", { name: direction === "undo" ? "Undo" : "Redo", exact: true }).click());
    expect(proposal.request.expected_revision).toBe(before.revision); expect((await inspect()).revision).toBe(before.revision);
    const committed = await exchange(`opencut_edit_v4_${direction}_apply`, () => area.getByRole("button", { name: `Apply ${direction}`, exact: true }).click());
    await save({ last_revision: committed.data.transaction.revision });
    expect(committed.request).toEqual(proposal.request);
    await expect(area.getByRole("button", { name: `Apply ${direction}`, exact: true })).toBeHidden();
    await expect.poll(async () => (await inspect()).project).toEqual(expectedProject);
    return { proposal, committed };
  };
  const clone = (actual, source, start) => {
    expect(actual.id).not.toBe(source.id);
    const expected = structuredClone(source); expected.id = actual.id; expected.timeline_range.start = actual.timeline_range.start;
    expect(actual).toEqual(expected); exact(actual.timeline_range.start, start);
  };
  const paste = async (action, source, start) => {
    const before = await inspect(), count = writeCount;
    await seek(String(start));
    const planned = await exchange("opencut_timeline_v4_paste_plan", () => clipboard.getByRole("button", { name: action, exact: true }).click());
    expect(planned.request).toMatchObject({ project_id: projectId, expected_revision: before.revision, sequence_id: sequenceId }); exact(planned.request.at, start);
    expect(planned.request.clipboard.source_revision).toBe(before.revision);
    expect((await inspect()).revision).toBe(before.revision); expect(writeCount).toBe(count);
    const review = page.getByRole("dialog", { name: "Review paste", exact: true });
    await expect(review.getByRole("status")).toHaveText("Review copied elements before saving."); mutated = true;
    const applied = await exchange("opencut_edit_v4_apply", () => review.getByRole("button", { name: "Apply copied elements", exact: true }).click());
    await save({ last_revision: applied.data.revision });
    expect(applied.request).toEqual(planned.data.transaction); expect(writeCount).toBe(count + 1);
    await expect(review.getByRole("status")).toHaveText("Pasted elements saved."); await review.getByRole("button", { name: "Done", exact: true }).click();
    const snapshot = await inspect(), id = planned.data.decision.selection[0].element_id;
    expect(planned.data.decision.selection).toHaveLength(1); clone(find(snapshot, id), source, start);
    expect(find(snapshot, source.id)).toEqual(source); return { planned, applied, snapshot, id };
  };
  try {
    await reopen(); original = await inspect();
    const baseline = prior ? await inspect(prior.state.baseline_revision) : original;
    expect(baseline.project.settings.settings.fps).toEqual({ numerator: 30, denominator: 1 });
    for (const id of IDS) {
      const text = find(baseline, id); expect(text?.content.kind).toBe("text"); expect(text.channels).toEqual([]);
      expect(text.source_span.source_duration).toBeNull(); exact(text.timeline_range.start, 0); exact(text.timeline_range.duration, 3);
      exact(text.source_span.trim_start, 0); exact(text.source_span.trim_end, 0);
    }
    if (prior) { expect(original.project).toEqual(baseline.project); mutated = true; await restore(state.completed_revision ?? state.baseline_revision); await reopen(); }
    else await save({ baseline_revision: original.revision });
    const latin = find(baseline, IDS[0]), chinese = find(baseline, IDS[1]);
    const steps = [
      ["Move", async () => {
        await select(latin.id); const base = await inspect(), selected = target(base, latin.id);
        const result = await timeline("Move selection", async form => {
          await form.getByRole("combobox", { name: prefix(`Target track for ${latin.name}`) }).selectOption(selected.track_id);
          await form.getByRole("textbox", { name: prefix(`New start for ${latin.name} (seconds)`) }).fill("0.333333");
        }, { kind: "move_selection", ripple: false });
        exact(result.planned.request.command.moves[0].new_start, 333333, 1000000);
        authored(find(result.snapshot, latin.id), latin, [1, 3], [3], [0], [0]); expect(find(result.snapshot, chinese.id)).toEqual(chinese); return result;
      }],
      ["Left trim", async () => {
        await select(latin.id); const result = await timeline("Trim element", async form => {
          await form.getByRole("combobox", { name: prefix("Trim edge") }).selectOption("left");
          await form.getByRole("textbox", { name: prefix("Boundary (scene seconds)") }).fill("0.666667");
        }, { kind: "trim", edge: "left", ripple: false });
        authored(find(result.snapshot, latin.id), latin, [2, 3], [8, 3], [1, 3], [0]); return result;
      }],
      ["Right trim", async () => {
        await select(latin.id); const result = await timeline("Trim element", async form => {
          await form.getByRole("combobox", { name: prefix("Trim edge") }).selectOption("right");
          await form.getByRole("textbox", { name: prefix("Boundary (scene seconds)") }).fill("2.666667");
        }, { kind: "trim", edge: "right", ripple: false });
        authored(find(result.snapshot, latin.id), latin, [2, 3], [2], [1, 3], [2, 3]); return result;
      }],
      ["Split", async () => {
        await select(latin.id);
        const ruler = page.locator(".v4-timeline-panel").getByRole("slider", { name: "Scene playhead" });
        await ruler.scrollIntoViewIfNeeded();
        await page.locator(".v4-timeline-viewport").evaluate(node => { node.scrollLeft = 0; });
        // Use an integral CSS pixel so browser hit-coordinate quantization
        // cannot turn the intended 1.5 s into a nearby sub-frame rational.
        await ruler.click({ position: { x: 120, y: 12 } });
        // Unedited boundary retains the exact public playhead.
        const result = await timeline("Split selection", form => form.getByRole("combobox", { name: prefix("Keep after split") }).selectOption("both"), { kind: "split", retain: "both", ripple: false });
        exact(result.planned.request.command.at, 3, 2);
        state.right_id = result.planned.request.command.targets[0].right_element_id;
        authored(find(result.snapshot, latin.id), latin, [2, 3], [5, 6], [1, 3], [11, 6], `${latin.name} (left)`);
        authored(find(result.snapshot, state.right_id), latin, [3, 2], [7, 6], [7, 6], [2, 3], `${latin.name} (right)`); state.split_project = result.snapshot.project; return result;
      }],
      ["Delete reopen Undo Redo", async () => {
        await select(state.right_id); const result = await timeline("Delete selection", async () => {}, { kind: "delete", ripple: false });
        expect(find(result.snapshot, state.right_id)).toBeUndefined(); expect(find(result.snapshot, chinese.id)).toEqual(chinese);
        await reopen(); expect((await inspect()).project).toEqual(result.snapshot.project);
        const undo = await history("undo", state.split_project), redo = await history("redo", result.snapshot.project);
        return { ...result, undo, redo };
      }],
      ["Chinese copy and paste", async () => {
        await select(chinese.id); const before = await inspect(), count = writeCount;
        const copied = await exchange("opencut_timeline_v4_copy", () => clipboard.getByRole("button", { name: "Copy elements", exact: true }).click());
        expect(copied.request).toEqual({ project_id: projectId, revision: before.revision, sequence_id: sequenceId, targets: [target(before, chinese.id)] });
        expect(copied.data.selection_digest).toMatch(/^v2:[a-f0-9]{64}$/u);
        expect((await inspect()).revision).toBe(before.revision); expect(writeCount).toBe(count);
        const result = await paste("Paste copied elements", chinese, 4); expect(result.planned.request.clipboard).toEqual(copied.data); state.pasted_id = result.id; return { ...result, copied };
      }],
      ["Duplicate and reopen", async () => {
        const source = find(await inspect(), state.pasted_id); await select(source.id);
        // Classic duplicate retains the selected source's start on a free track.
        const result = await paste("Duplicate elements", source, 4); state.duplicate_id = result.id;
        await reopen(); expect((await inspect()).project).toEqual(result.snapshot.project); expect(find(result.snapshot, chinese.id)).toEqual(chinese); return result;
      }],
      ["Physical text Move bilateral Trim and keyboard S", async () => {
        mutated = true; await restore(state.baseline_revision); await reopen();
        const panel = page.locator(".v4-timeline-panel"), zoom = panel.getByRole("slider", { name: "Timeline zoom" });
        await zoom.focus(); await zoom.press("Home");
        for (let index = 0; index < 9; index++) await zoom.press("ArrowRight");
        await expect(zoom).toHaveValue("1");
        await select(latin.id);
        const card = page.locator(`[data-element-id="${latin.id}"]`);
        const moved = await gesture(() => drag(card.locator(".v4-element-select"), 80), { kind: "move_selection", ripple: false });
        authored(find(moved.snapshot, latin.id), latin, [1], [3], [0], [0]);
        const left = await gesture(() => drag(card.locator('[data-trim-edge="left"]'), 16), { kind: "trim", edge: "left", ripple: false });
        authored(find(left.snapshot, latin.id), latin, [6, 5], [14, 5], [1, 5], [0]);
        const right = await gesture(() => drag(card.locator('[data-trim-edge="right"]'), -16), { kind: "trim", edge: "right", ripple: false });
        authored(find(right.snapshot, latin.id), latin, [6, 5], [13, 5], [1, 5], [1, 5]);
        await select(latin.id); await seek("2");
        const split = await gesture(async () => { await panel.focus(); await page.keyboard.press("s"); }, { kind: "split", retain: "both", ripple: false });
        exact(split.planned.request.command.at, 2);
        const rightId = split.planned.request.command.targets[0].right_element_id;
        authored(find(split.snapshot, latin.id), latin, [6, 5], [4, 5], [1, 5], [2], `${latin.name} (left)`);
        authored(find(split.snapshot, rightId), latin, [2], [9, 5], [1], [1, 5], `${latin.name} (right)`);
        expect(find(split.snapshot, chinese.id)).toEqual(chinese);
        await reopen(); expect((await inspect()).project).toEqual(split.snapshot.project);
        return { ...split, gesture_steps: [moved, left, right] };
      }],
    ];
    for (const [name, run] of steps) {
      if (evidence.some(item => item.case === name && item.passed)) continue;
      phase = name; await save({}); const result = await run();
      evidence.push({ case: name, passed: true, request: result.planned?.request, plan: result.planned?.data, receipt: result.applied?.data, revision: (await inspect()).revision, additional: { copied: result.copied, undo: result.undo, redo: result.redo, gesture_steps: result.gesture_steps } });
      await save({ last_revision: (await inspect()).revision, completed_revision: (await inspect()).revision });
    }
    expect(errors).toEqual([]);
  } catch (error) {
    failure = error;
    const report = { status: "failed", phase, project_id: projectId, candidate, error: String(error).slice(0, 4096), state, evidence, errors, network };
    try { report.ui = await page.evaluate(() => document.body.innerText.slice(0, 8192)); } catch {}
    const file = `f19-text-timeline-failure-${Date.now()}.json`;
    await fs.writeFile(path.join(directory, file), JSON.stringify(report, null, 2) + "\n");
    await page.screenshot({ path: path.join(directory, file.replace(/\.json$/u, ".png")), fullPage: false }).catch(() => {});
  } finally {
    if (mutated && original) {
      try { const restored = await restore(original.revision); expect(restored.project).toEqual(original.project); await save({ restored_from_revision: original.revision, restored_revision: restored.revision }); }
      catch (error) { await save({ restore_error: String(error) }); if (!failure) failure = error; }
    }
    await page.close();
  }
  if (failure) throw failure;
  const report = { status: "passed", project_id: projectId, sequence_id: sequenceId, candidate, evidence, restoration: { from_revision: state.restored_from_revision, revision: state.restored_revision }, scope: "Existing static text timeline UI Move, bilateral Trim, exact-frame Split, Delete, reference Copy/Paste/Duplicate, one reviewed commit, persistence and selective Undo/Redo. Includes physical text Move/bilateral Trim and keyboard S. No renderer, ripple or cross-scene rerun; animated trim belongs to unsupported F29." };
  await fs.writeFile(path.join(directory, "f19-text-timeline.json"), JSON.stringify(report, null, 2) + "\n"); check("F19 existing text timeline UI and original reviewed transactions", report);
}
