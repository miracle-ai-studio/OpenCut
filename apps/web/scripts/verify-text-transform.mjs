import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";

const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const prefix = label => new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
async function candidateIdentity() {
  expect(process.env.OPENCUT_WEB_BINARY).toBeTruthy(); expect(process.env.OPENCUT_WEB_ROOT).toBeTruthy();
  const files = [];
  const visit = async relative => {
    for (const item of await fs.readdir(path.join(process.env.OPENCUT_WEB_ROOT, relative), { withFileTypes: true })) {
      const name = path.join(relative, item.name);
      if (item.isDirectory()) await visit(name);
      else if (item.isFile() && /\.(js|mjs|cjs|css)$/u.test(name)) files.push([name, hash(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name)))]);
      else if (item.isSymbolicLink()) throw new Error(`Unhashed candidate symlink ${name}`);
    }
  };
  await visit(""); files.sort((a, b) => a[0].localeCompare(b[0])); expect(files.length).toBeGreaterThan(0);
  return { binary_sha256: hash(await fs.readFile(process.env.OPENCUT_WEB_BINARY)), web_js_css_tree_sha256: hash(JSON.stringify(files)) };
}

// Owned existing Latin title/fonts only. Isolation changes background and other
// visibility; all values under test must be entered through actual product UI.
export async function verifyTextTransform(browser, endpoint, directory, check, options = {}) {
  const { project_id: projectId } = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const candidate = await candidateIdentity(), checkpointPath = path.join(directory, "f19-text-transform-checkpoint.json");
  let prior = null;
  try { prior = JSON.parse(await fs.readFile(checkpointPath, "utf8")); } catch (error) { if (error.code !== "ENOENT") throw error; }
  if (options.freshRun) prior = null;
  if (prior?.submission) { expect(prior.project_id).toBe(projectId); expect(prior.candidate, "Retained PNG belongs to another candidate; root must decide a fresh run").toEqual(candidate); }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
  page.setDefaultTimeout(30000);
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  let phase = "open", original, failure, changed = false, facts = prior?.submission ? prior : { project_id: projectId, candidate, evidence: [] };
  const errors = [], network = [];
  let applyCount = 0;
  page.on("pageerror", error => { if (errors.length < 32) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 256) network.push({ operation });
    if (operation === "opencut_edit_apply") applyCount++;
  });
  const save = async values => {
    facts = { ...facts, ...values, phase };
    await fs.writeFile(checkpointPath, JSON.stringify(facts, null, 2) + "\n");
  };
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const inspect = revision => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: revision ?? null }));
  const exchange = async (name, action) => {
    const pending = page.waitForResponse(response => response.url().endsWith(`/api/tools/${name}`));
    await action(); const response = await pending, envelope = await response.json();
    const value = { request: response.request().postDataJSON(), envelope };
    await save({ last_exchange: { operation: name, ...value } });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return { request: value.request, data: envelope.data };
  };
  const reopen = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}${facts.sequence_id ? `?scene=${encodeURIComponent(facts.sequence_id)}` : ""}`);
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    if (facts.sequence_id) await page.getByRole("combobox", { name: "Scene", exact: true }).selectOption(facts.sequence_id);
  };
  const scene = snapshot => snapshot.project.sequences.find(scene => scene.id === facts.sequence_id);
  const element = snapshot => scene(snapshot).tracks.flatMap(track => track.elements).find(element => element.id === facts.element_id);
  const select = async () => { await page.locator(`[data-element-id="${facts.element_id}"] .editor-element-select`).click({ position: { x: 12, y: 12 } }); };
  try {
    await reopen(); original = await inspect();
    if (!facts.submission) {
      phase = "isolate existing free-text output";
      facts.sequence_id = original.project.default_sequence_id;
      const track = scene(original).tracks.find(track => track.enabled && !track.locked && track.elements.some(element => element.id === "cf25b163-cc52-46f7-ab1a-9719bb1e6180"));
      const title = track?.elements.find(element => element.id === "cf25b163-cc52-46f7-ab1a-9719bb1e6180");
      expect(title?.content.kind).toBe("text"); facts.element_id = title.id; facts.track_id = track.id;
      const font = original.project.fonts.find(font => font.id === title.content.text.font.font_id);
      expect(font).toMatchObject({ family: "Lato", weight: 400, style: "normal" });
      expect(title.content.text.font_size.unit).toBe("classic_height90");
      expect(title.channels).toEqual([]); expect(title.visual.transform.rotation_degrees).toBe(0);
      expect(title.content.text).toMatchObject({ align: "center", decoration: "none", letter_spacing_px: 0, line_height_multiplier: 1.2, color: { red: 1, green: 1, blue: 1, alpha: 1 }, background: { enabled: false } });
      const settings = structuredClone(original.project.settings);
      settings.settings.canvas_size = { width: 320, height: 180 }; settings.settings.canvas_size_mode = "custom";
      settings.settings.last_custom_canvas_size = { width: 320, height: 180 };
      settings.settings.background = { kind: "color", color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 } };
      const operations = [{ type: "settings_set", settings }];
      for (const owned of scene(original).tracks) {
        if (owned.kind === "caption" && owned.enabled) operations.push({ type: "track_put", sequence_id: facts.sequence_id, track: { ...structuredClone(owned), enabled: false } });
        for (const other of owned.elements) if (other.id !== title.id && other.enabled) operations.push({ type: "element_put", sequence_id: facts.sequence_id, track_id: owned.id, element: { ...structuredClone(other), enabled: false } });
      }
      changed = true;
      await tool("opencut_edit_apply", { project_id: projectId, expected_revision: original.revision, idempotency_key: randomUUID(), label: "F19 existing text output isolation", operations });
      await save({ original_revision: original.revision, baseline_title: title, font }); await reopen();
      phase = "actual text content and size UI";
      await select(); await page.getByRole("button", { name: "Edit text", exact: true }).click();
      const form = page.getByRole("form", { name: "Edit text", exact: true });
      await form.getByRole("textbox", { name: prefix("Text content") }).fill("OpenCut\nTitle");
      await form.getByRole("textbox", { name: prefix("Font size (Classic)") }).fill("12");
      const beforeText = await inspect(), count = applyCount;
      const planned = await exchange("opencut_text_plan", () => form.getByRole("button", { name: "Save text", exact: true }).click());
      expect(planned.request).toMatchObject({ project_id: projectId, expected_revision: beforeText.revision, sequence_id: facts.sequence_id, command: { kind: "edit", target: { track_id: track.id, element_id: title.id }, values: { text: { content: "OpenCut\nTitle", font_size: { unit: "classic_height90", value: 12 } } } } });
      expect((await inspect()).revision).toBe(beforeText.revision); expect(applyCount).toBe(count);
      const edited = await exchange("opencut_edit_apply", () => page.getByRole("region", { name: "Review text changes", exact: true }).getByRole("button", { name: "Apply text changes", exact: true }).click());
      expect(edited.request).toEqual(planned.data.transaction); expect(applyCount).toBe(count + 1);
      await expect(form).toBeHidden(); const textSaved = await inspect();
      const expectedText = structuredClone(element(beforeText)); expectedText.content.text.content = "OpenCut\nTitle"; expectedText.content.text.font_size.value = 12;
      expect(element(textSaved)).toEqual(expectedText);
      facts.evidence.push({ case: phase, plan: planned, apply: edited }); await save({ text_revision: textSaved.revision });
      phase = "actual position and nonuniform scale UI";
      await select(); await page.getByRole("button", { name: "Edit text position and scale", exact: true }).click();
      const transformForm = page.getByRole("form", { name: "Edit text position and scale", exact: true });
      for (const [label, value] of [["Horizontal center offset (pixels)", "32"], ["Vertical center offset (pixels)", "-12"], ["Horizontal scale (multiplier)", "1.25"], ["Vertical scale (multiplier)", ".75"]])
        await transformForm.getByRole("spinbutton", { name: prefix(label) }).fill(value);
      const before = await inspect(), transformCount = applyCount;
      const planPromise = exchange("opencut_edit_plan", async () => {});
      const applied = await exchange("opencut_edit_apply", () => transformForm.getByRole("button", { name: "Save text position and scale", exact: true }).click());
      const publicPlan = await planPromise;
      expect(publicPlan.request).toEqual(applied.request); expect(applyCount).toBe(transformCount + 1);
      expect(applied.request).toMatchObject({ project_id: projectId, expected_revision: before.revision, label: "Edit text position and scale" });
      const expected = structuredClone(element(before)); Object.assign(expected.visual.transform, { position_x_px: 32, position_y_px: -12, scale_x: 1.25, scale_y: .75 });
      expect(applied.request.operations).toEqual([{ type: "element_put", sequence_id: facts.sequence_id, track_id: track.id, element: expected }]);
      await expect(transformForm).toBeHidden(); const transformed = await inspect(); expect(element(transformed)).toEqual(expected);
      const expectedProject = structuredClone(before.project); expectedProject.sequences.find(scene => scene.id === facts.sequence_id).tracks.find(track => track.id === facts.track_id).elements = scene(before).tracks.find(track => track.id === facts.track_id).elements.map(element => element.id === expected.id ? expected : element);
      expect(transformed.project).toEqual(expectedProject);
      await save({ transformed_revision: transformed.revision, expected_title: expected });
      await reopen(); expect((await inspect()).project).toEqual(transformed.project);
      const history = page.locator(".editor-history"), historyEvidence = [];
      for (const [direction, expectedGraph] of [["undo", before.project], ["redo", transformed.project]]) {
        const proposal = await exchange(`opencut_edit_${direction}_plan`, () => history.getByRole("button", { name: direction === "undo" ? "Undo" : "Redo", exact: true }).click());
        const committed = await exchange(`opencut_edit_${direction}_apply`, () => history.getByRole("button", { name: `Apply ${direction}`, exact: true }).click());
        expect(committed.request).toEqual(proposal.request); await expect(history.getByRole("button", { name: `Apply ${direction}`, exact: true })).toBeHidden();
        expect((await inspect()).project).toEqual(expectedGraph); historyEvidence.push({ direction, proposal, committed });
      }
      facts.evidence.push({ case: phase, public_plan: publicPlan, apply: applied, history: historyEvidence });
      await save({ render_revision: (await inspect()).revision });
      phase = "one fixed native PNG";
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5"); await page.getByRole("button", { name: "Go to time", exact: true }).click();
      const submitted = await exchange("opencut_render_submit", () => page.getByRole("button", { name: "Render scene frame", exact: true }).click());
      await save({ submission: submitted });
      expect(submitted.request).toMatchObject({ project_id: projectId, revision: facts.render_revision, sequence_id: facts.sequence_id });
    }
    phase = "original PNG job completion/download";
    let job;
    await expect.poll(async () => {
      job = await tool("opencut_job_view", { job_id: facts.submission.data.job_id });
      return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending";
    }, { timeout: 120000, intervals: [100, 250, 500] }).not.toBe("pending");
    await save({ job }); expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.result.kind).toBe("render.text_frame");
    const result = job.result.data;
    expect(result.identity).toMatchObject({ project_id: projectId, revision: facts.render_revision, sequence_id: facts.sequence_id, width: 320, height: 180, profile: "classic-v31-gpu-normal-text-frame-v2" });
    expect(result.texts).toHaveLength(1); expect(result.font_lock).toEqual([{ font_id: facts.font.id, content_hash: facts.font.content.content_hash, size: facts.font.content.size }]);
    const artifact = job.artifacts.find(artifact => artifact.mime_type === "image/png"); expect(artifact).toBeTruthy();
    const output = path.join(directory, "f19-text-transform.png");
    let png;
    try { png = await fs.readFile(output); if (hash(png) !== artifact.content_hash) png = null; } catch (error) { if (error.code !== "ENOENT") throw error; }
    if (!png) {
      await page.getByRole("button", { name: "Tasks", exact: true }).click(); await page.getByLabel("Refresh tasks", { exact: true }).click();
      const task = page.locator(".task-card").filter({ hasText: facts.render_revision });
      await expect(task).toHaveCount(1); await expect(task).toContainText("succeeded");
      const downloadPromise = page.waitForEvent("download"); await task.getByRole("button", { name: "Download", exact: true }).click();
      await (await downloadPromise).saveAs(output); png = await fs.readFile(output);
    }
    await save({ output, png_sha256: hash(png) }); expect(hash(png)).toBe(artifact.content_hash);
    phase = "independent fixed-font T*S*R pixel oracle";
    const fontsDirectory = process.env.OPENCUT_B1_FONT_SOURCES; expect(fontsDirectory).toBeTruthy();
    const manifest = JSON.parse(await fs.readFile(path.join(fontsDirectory, "manifest.json"), "utf8"));
    const source = manifest.files.find(source => source.sha256 === facts.font.content.content_hash); expect(source).toBeTruthy();
    const fontBytes = await fs.readFile(path.join(fontsDirectory, path.basename(source.file))); expect(hash(fontBytes)).toBe(facts.font.content.content_hash);
    const oracle = await page.evaluate(async ({ png, font }) => {
      const image = new Image(); image.src = `data:image/png;base64,${png}`; await image.decode();
      const face = new FontFace("F19IndependentFont", Uint8Array.from(atob(font), character => character.charCodeAt(0))); await face.load(); document.fonts.add(face);
      try {
        const expected = new OffscreenCanvas(320, 180), ctx = expected.getContext("2d"); ctx.fillStyle = "black"; ctx.fillRect(0, 0, 320, 180);
        // Independent Canvas glyph raster under T*S*R. Font size
        // is12*180/90=24; baselines +/-14.4; center(160+32,90-12).
        ctx.font = "24px F19IndependentFont"; ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.fillStyle = "white";
        const lines = ["OpenCut", "Title"].map((text, index) => ({ text, baselineY: (index - .5) * 28.8, width: ctx.measureText(text).width }));
        ctx.translate(192, 78); ctx.scale(1.25, .75); ctx.rotate(0);
        lines.forEach(line => ctx.fillText(line.text, 0, line.baselineY));
        const actual = new OffscreenCanvas(320, 180), actualCtx = actual.getContext("2d"); actualCtx.drawImage(image, 0, 0);
        const e = expected.getContext("2d").getImageData(0, 0, 320, 180).data, a = actualCtx.getImageData(0, 0, 320, 180).data;
        const bounds = data => { let left = 320, top = 180, right = -1, bottom = -1, count = 0; for (let y = 0; y < 180; y++) for (let x = 0; x < 320; x++) if (data[(y * 320 + x) * 4] > 32) { count++; left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y); } return { left, top, right, bottom, count }; };
        let union = 0, intersection = 0, error = 0, blankError = 0;
        for (let i = 0; i < a.length; i += 4) {
          const onA = a[i] > 32, onE = e[i] > 32; union += Number(onA || onE); intersection += Number(onA && onE);
          for (let c = 0; c < 4; c++) error += Math.abs(a[i + c] - e[i + c]);
          if (!e[i] && !e[i + 1] && !e[i + 2]) blankError = Math.max(blankError, a[i], a[i + 1], a[i + 2], 255 - a[i + 3]);
        }
        return { width: image.naturalWidth, height: image.naturalHeight, lines, center: { x: 192, y: 78 }, fontSize: 24, lineHeight: 28.8, expectedBounds: bounds(e), actualBounds: bounds(a), iou: intersection / union, meanChannelError: error / a.length, blankError };
      } finally { document.fonts.delete(face); }
    }, { png: png.toString("base64"), font: fontBytes.toString("base64") });
    await save({ oracle });
    const layout = result.texts[0].layout;
    expect(layout.fontSizePx).toBe(24); expect(layout.lineHeightPx).toBeCloseTo(28.8, 8);
    expect(layout.lines.map(line => line.content)).toEqual(["OpenCut", "Title"]);
    for (const [index, line] of oracle.lines.entries()) {
      expect(layout.lines[index].width).toBeCloseTo(line.width, 4);
      expect(layout.lines[index].baselineY).toBeCloseTo(line.baselineY, 8);
    }
    expect(oracle.width).toBe(320); expect(oracle.height).toBe(180); expect(oracle.expectedBounds.count).toBeGreaterThan(100);
    expect(oracle.iou).toBeGreaterThan(.995); expect(oracle.meanChannelError).toBeLessThan(.1); expect(oracle.blankError).toBeLessThanOrEqual(2);
    for (const coordinate of ["left", "top", "right", "bottom"]) expect(Math.abs(oracle.actualBounds[coordinate] - oracle.expectedBounds[coordinate])).toBeLessThanOrEqual(1);
    expect(errors).toEqual([]); await save({ pixels_passed: true });
  } catch (error) {
    failure = error;
    const report = { status: "failed", phase, error: String(error).slice(0, 4096), checkpoint: facts, errors, network };
    try { report.ui = await page.evaluate(() => document.body.innerText.slice(0, 8192)); } catch {}
    const file = `f19-text-transform-failure-${Date.now()}.json`;
    await fs.writeFile(path.join(directory, file), JSON.stringify(report, null, 2) + "\n");
    await page.screenshot({ path: path.join(directory, file.replace(/\.json$/u, ".png")), fullPage: false }).catch(() => {});
  } finally {
    if (changed && original) {
      try {
        const current = await inspect(); await tool("opencut_project_restore", { project_id: projectId, expected_revision: current.revision, target_revision: original.revision, idempotency_key: randomUUID(), asset_versions: {} });
        const restored = await inspect(); expect(restored.project).toEqual(original.project); await save({ restored_from_revision: original.revision, restored_revision: restored.revision });
      } catch (error) { await save({ restore_error: String(error) }); if (!failure) failure = error; }
    }
    await page.close();
  }
  if (failure) throw failure;
  const report = { ...facts, status: "passed", reused_job: prior?.submission?.data.job_id ?? null, scope: "Actual UI multiline/Classic size and position/nonuniform scale, exact original public transaction, persisted graph and selective Undo/Redo, one fixed native PNG with independently hashed font glyph/position/T*S*R pixels. No new whole F27/F47 claim." };
  await fs.writeFile(path.join(directory, "f19-text-transform.json"), JSON.stringify(report, null, 2) + "\n"); check("F19 actual text size/position/scale UI and independent pixels", report);
}
