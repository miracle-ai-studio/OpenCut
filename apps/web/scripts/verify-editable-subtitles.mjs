import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
const hash = value => createHash("sha256").update(value).digest("hex");
const prefix = label => new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
async function identity() {
  const root = process.env.OPENCUT_WEB_ROOT, binary = process.env.OPENCUT_WEB_BINARY;
  expect(root).toBeTruthy(); expect(binary).toBeTruthy(); const files = [];
  async function visit(dir = "") { for (const entry of await fs.readdir(path.join(root, dir), { withFileTypes: true })) { const name = path.join(dir, entry.name); if (entry.isSymbolicLink()) throw new Error("Candidate symlink"); if (entry.isDirectory()) await visit(name); else if (/\.(js|mjs|cjs|css)$/u.test(name)) files.push([name, hash(await fs.readFile(path.join(root, name)))]); } }
  await visit(); files.sort((a, b) => a[0].localeCompare(b[0])); expect(files.length).toBeGreaterThan(0);
  return { binary_sha256: hash(await fs.readFile(binary)), web_js_css_tree_sha256: hash(JSON.stringify(files)) };
}
export async function verifyEditableSubtitles(browser, endpoint, directory, check) {
  const { project_id: projectId } = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const checkpoint = path.join(directory, "f23-editable-subtitles-checkpoint.json"), candidate = await identity();
  let facts; try { facts = JSON.parse(await fs.readFile(checkpoint, "utf8")); } catch (error) { if (error.code !== "ENOENT") throw error; }
  if (facts) { expect(facts.project_id).toBe(projectId); expect(facts.candidate).toEqual(candidate); } else facts = { project_id: projectId, candidate, transactions: {}, downloads: {}, source_mode: "file", phase: "bootstrap" };
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true }); page.setDefaultTimeout(30000);
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  const errors = []; page.on("pageerror", error => { if (errors.length < 16) errors.push(error.message.slice(0, 800)); });
  const save = async () => { const bytes = JSON.stringify(facts, null, 2); if (Buffer.byteLength(bytes) > 2 * 1024 * 1024) throw new Error("Bounded subtitle checkpoint exceeded"); await fs.writeFile(checkpoint, bytes + "\n"); };
  const tool = async (name, input) => { const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { name, input }); expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data; };
  const inspect = () => editorSnapshot(tool("opencut_project_inspect", { project_id: projectId, revision: null }));
  const reopen = async () => { await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}${facts.sequence_id ? `?scene=${encodeURIComponent(facts.sequence_id)}` : ""}`); await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible(); const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true }); if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click(); };
  // Persist the exact original request before accepting its response. Restart
  // retries only that durable transaction; successful stages are never replayed.
  const transaction = async (stage, name, action) => {
    if (facts.transactions[stage]?.receipt) return facts.transactions[stage].receipt;
    if (facts.transactions[stage]?.request) { const receipt = await tool(name, facts.transactions[stage].request); facts.transactions[stage].receipt = receipt; await save(); return receipt; }
    let journal;
    const listener = request => { if (request.url().endsWith(`/api/tools/${name}`)) { facts.transactions[stage] = { operation: name, request: request.postDataJSON() }; journal = save(); } };
    page.on("request", listener);
    try { const response = page.waitForResponse(r => r.url().endsWith(`/api/tools/${name}`)); await action(); const received = await response; await journal; const envelope = await received.json(); facts.transactions[stage].envelope = envelope; await save(); expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); facts.transactions[stage].receipt = envelope.data; await save(); return envelope.data; } finally { page.off("request", listener); }
  };
  const scene = snapshot => snapshot.project.sequences.find(s => s.id === facts.sequence_id);
  const track = snapshot => scene(snapshot).tracks.find(t => t.id === facts.track_id);
  try {
    await reopen();
    if (!facts.original) { facts.original = await inspect(); facts.sequence_id = facts.original.project.default_sequence_id; await save(); }
    const original = facts.original;
    expect(original.project.settings.profile).toBe("classic_global"); expect(original.project.settings.settings.canvas_size).toEqual({ width: 320, height: 180 });
    if (facts.phase === "passed") { expect((await inspect()).project).toEqual(original.project); expect(errors).toEqual([]); check("F23 retained editable subtitle evidence", facts); return facts; }
    const font = original.project.fonts.find(f => f.family === "Lato" && f.weight === 400 && f.style === "normal"); expect(font).toBeTruthy();
    const long = "Editable subtitles wrap this English sentence into clear lines near the bottom margin.";
    facts.phase = "import two editable cues"; await save();
    const panel = page.getByRole("region", { name: "Captions", exact: true });
    if (!facts.transactions.import?.request) {
      await expect(panel.getByRole("combobox", { name: prefix("Caption track") })).toHaveValue("__new_subtitle_text_track__");
      await panel.getByRole("combobox", { name: prefix("Pinned font") }).selectOption(font.id); await panel.getByLabel(prefix("Size (px)")).fill("24");
      await panel.getByRole("combobox", { name: prefix("Format") }).selectOption("srt");
      await panel.getByRole("textbox", { name: prefix("Subtitle content") }).fill(`1\n00:00:00,000 --> 00:00:02,000\n${long}\n\n2\n00:00:02,000 --> 00:00:04,000\nSecond English subtitle.\n\n`);
    }
    await transaction("import", "opencut_edit_apply", () => panel.getByRole("button", { name: "Import subtitles", exact: true }).click());
    const imported = await inspect();
    if (!facts.imported) {
      const added = scene(imported).tracks.filter(t => !scene(original).tracks.some(old => old.id === t.id)); expect(added).toHaveLength(1); facts.track_id = added[0].id; facts.element_id = added[0].elements[0].id; facts.imported = imported;
      expect(added[0].kind).toBe("text"); expect(added[0].elements).toHaveLength(2); expect(added[0].elements.every(e => e.content.kind === "text")).toBe(true);
      expect(added[0].elements[0].content.text.content).toContain("\n"); expect(added[0].elements[0].content.text.content.replace(/\s+/g, " ")).toBe(long);
      expect(added[0].elements[0].content.text.font_size).toEqual({ unit: "canvas_pixels", value: 24 });
      expect(scene(imported).tracks.filter(t => t.id !== facts.track_id)).toEqual(scene(original).tracks);
      const remainder = structuredClone(imported.project); remainder.sequences.find(s => s.id === facts.sequence_id).tracks = scene(original).tracks; expect(remainder).toEqual(original.project);
      expect(facts.transactions.import.request.operations).toHaveLength(1); expect(facts.transactions.import.request.operations[0].type).toBe("sequence_put"); await save();
    }
    if (!facts.png_checked) {
      facts.phase = "one isolated normal PNG"; await save();
      if (!facts.isolation_request) {
        const isolatedScene = structuredClone(scene(facts.imported)); isolatedScene.captions = [];
        isolatedScene.tracks.forEach(t => { if (t.id !== facts.track_id) t.enabled = false; });
        const settings = structuredClone(facts.imported.project.settings);
        expect(settings.profile).toBe("classic_global");
        settings.settings.background = { kind: "color", color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 } };
        facts.isolation_request = { project_id: projectId, expected_revision: (await inspect()).revision, idempotency_key: randomUUID(), label: "F23 isolate one subtitle PNG", operations: [{ type: "settings_set", settings }, { type: "sequence_put", sequence: isolatedScene }] }; await save();
      }
      if (!facts.isolation) { facts.isolation = await tool("opencut_edit_apply", facts.isolation_request); await save(); }
      await reopen();
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5"); await page.getByRole("button", { name: "Go to time", exact: true }).click();
      const submission = await transaction("png", "opencut_render_submit", () => page.getByRole("button", { name: "Render scene frame", exact: true }).click());
      expect(facts.transactions.png.request).toMatchObject({ project_id: projectId, revision: facts.isolation.revision, sequence_id: facts.sequence_id });
      let job; await expect.poll(async () => { job = await tool("opencut_job_view", { job_id: submission.job_id }); return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending"; }, { timeout: 120000, intervals: [100, 250, 500] }).not.toBe("pending");
      facts.png_job = job; await save(); expect(job.state, JSON.stringify(job.failure)).toBe("succeeded"); expect(job.result.kind).toBe("render.text_frame");
      const result = job.result.data; expect(result.identity).toMatchObject({ project_id: projectId, revision: facts.isolation.revision, sequence_id: facts.sequence_id, profile: "classic-v31-gpu-normal-text-frame-v2" }); expect(result.texts).toHaveLength(1); expect(result.font_lock).toEqual([{ font_id: font.id, content_hash: font.content.content_hash, size: font.content.size }]);
      const artifact = job.artifacts.find(a => a.mime_type === "image/png"); expect(artifact).toBeTruthy(); const output = path.join(directory, "f23-editable-subtitle.png");
      let png; try { png = await fs.readFile(output); if (hash(png) !== artifact.content_hash) png = null; } catch (error) { if (error.code !== "ENOENT") throw error; }
      if (!png) {
        await page.getByRole("button", { name: "Tasks", exact: true }).click(); await page.getByLabel("Refresh tasks", { exact: true }).click(); const card = page.locator(".task-card").filter({ hasText: facts.isolation.revision }); await expect(card).toHaveCount(1); const download = page.waitForEvent("download"); await card.getByRole("button", { name: "Download", exact: true }).click(); await (await download).saveAs(output); png = await fs.readFile(output);
      }
      expect(hash(png)).toBe(artifact.content_hash); facts.png_output = { path: output, sha256: hash(png) }; await save();
      const fontsDirectory = process.env.OPENCUT_B1_FONT_SOURCES; expect(fontsDirectory).toBeTruthy(); const manifest = JSON.parse(await fs.readFile(path.join(fontsDirectory, "manifest.json"), "utf8")); const source = manifest.files.find(f => f.sha256 === font.content.content_hash); expect(source).toBeTruthy(); const fontBytes = await fs.readFile(path.join(fontsDirectory, path.basename(source.file))); expect(hash(fontBytes)).toBe(font.content.content_hash);
      facts.oracle = await page.evaluate(async ({ png, font, sentence, width, height }) => {
        const face = new FontFace("F23IndependentLato", Uint8Array.from(atob(font), c => c.charCodeAt(0))); await face.load(); document.fonts.add(face);
        try {
          const expected = new OffscreenCanvas(width, height), ctx = expected.getContext("2d"); ctx.fillStyle = "black"; ctx.fillRect(0, 0, width, height); ctx.font = "24px F23IndependentLato"; ctx.textBaseline = "middle"; ctx.textAlign = "center";
          const maxWidth = width * .8, lines = []; let line = "";
          for (const word of sentence.split(" ")) { const candidate = line ? `${line} ${word}` : word; if (line && ctx.measureText(candidate).width > maxWidth) { lines.push(line); line = word; } else line = candidate; } if (line) lines.push(line);
          const lineHeight = 24 * 1.2, blockHeight = lines.length * lineHeight, center = { x: width / 2, y: height * .95 - blockHeight / 2 };
          ctx.fillStyle = "white"; lines.forEach((text, i) => ctx.fillText(text, center.x, center.y + (i - (lines.length - 1) / 2) * lineHeight));
          const image = new Image(); image.src = `data:image/png;base64,${png}`; await image.decode(); const actual = new OffscreenCanvas(width, height), ac = actual.getContext("2d"); ac.drawImage(image, 0, 0); const a = ac.getImageData(0, 0, width, height).data, e = ctx.getImageData(0, 0, width, height).data;
          const bounds = data => { let left = width, top = height, right = -1, bottom = -1, count = 0; for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) if (data[(y * width + x) * 4] > 32) { left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y); count++; } return { left, top, right, bottom, count }; };
          let union = 0, intersection = 0, error = 0; for (let i = 0; i < a.length; i += 4) { union += Number(a[i] > 32 || e[i] > 32); intersection += Number(a[i] > 32 && e[i] > 32); for (let c = 0; c < 4; c++) error += Math.abs(a[i + c] - e[i + c]); }
          return { width: image.naturalWidth, height: image.naturalHeight, lines, widths: lines.map(t => ctx.measureText(t).width), maxWidth, bottomMargin: height * .05, center, lineHeight, expectedBounds: bounds(e), actualBounds: bounds(a), iou: intersection / union, meanChannelError: error / a.length };
        } finally { document.fonts.delete(face); }
      }, { png: png.toString("base64"), font: fontBytes.toString("base64"), sentence: long, width: result.identity.width, height: result.identity.height }); await save();
      const oracle = facts.oracle, first = track(facts.imported).elements.find(e => e.id === facts.element_id); expect(oracle.lines.length).toBeGreaterThan(1); expect(first.content.text.content).toBe(oracle.lines.join("\n")); expect(first.visual.transform.position_x_px).toBeCloseTo(0, 6); expect(first.visual.transform.position_y_px).toBeCloseTo(oracle.center.y - oracle.height / 2, 6);
      expect(result.texts[0].layout.fontSizePx).toBe(24); expect(result.texts[0].layout.lineHeightPx).toBeCloseTo(oracle.lineHeight, 6); expect(result.texts[0].layout.lines.map(l => l.content)).toEqual(oracle.lines); oracle.widths.forEach((width, i) => expect(result.texts[0].layout.lines[i].width).toBeCloseTo(width, 4));
      expect(oracle.expectedBounds.count).toBeGreaterThan(100); expect(oracle.iou).toBeGreaterThan(.97); expect(oracle.meanChannelError).toBeLessThan(1); for (const edge of ["left", "right", "top", "bottom"]) expect(Math.abs(oracle.actualBounds[edge] - oracle.expectedBounds[edge])).toBeLessThanOrEqual(1);
      if (!facts.after_png_restore_request) { facts.after_png_restore_request = { project_id: projectId, expected_revision: (await inspect()).revision, target_revision: facts.imported.revision, idempotency_key: randomUUID(), asset_versions: {} }; await save(); }
      if (!facts.after_png_restore) { facts.after_png_restore = await tool("opencut_project_restore", facts.after_png_restore_request); await save(); }
      expect((await inspect()).project).toEqual(facts.imported.project); facts.png_checked = true; await save(); await reopen();
    }
    facts.phase = "edit cue through existing text review"; await save();
    if (!facts.transactions.edit?.request) {
      await page.locator(`[data-element-id="${facts.element_id}"] .editor-element-select`).click({ position: { x: 12, y: 12 } }); await page.getByRole("button", { name: "Edit text", exact: true }).click();
      const form = page.getByRole("form", { name: "Edit text", exact: true }); await form.getByRole("textbox", { name: prefix("Text content") }).fill("Edited subtitle\nSecond line"); await form.getByRole("combobox", { name: prefix("Text decoration") }).selectOption("underline"); await form.getByRole("textbox", { name: prefix("Letter spacing (pixels)") }).fill("1.5");
      const planned = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_text_plan")); await form.getByRole("button", { name: "Save text", exact: true }).click(); const envelope = await (await planned).json(); expect(envelope.error).toBeNull(); expect((await inspect()).project).toEqual(facts.imported.project);
    }
    await transaction("edit", "opencut_edit_apply", () => page.getByRole("region", { name: "Review text changes", exact: true }).getByRole("button", { name: "Apply text changes", exact: true }).click());
    if (!facts.edited) { facts.edited = await inspect(); const text = track(facts.edited).elements.find(e => e.id === facts.element_id).content.text; expect(text).toMatchObject({ content: "Edited subtitle\nSecond line", decoration: "underline", letter_spacing_px: 1.5 }); const expected = structuredClone(facts.imported.project); const owned = expected.sequences.find(s => s.id === facts.sequence_id).tracks.find(t => t.id === facts.track_id).elements.find(e => e.id === facts.element_id); Object.assign(owned.content.text, { content: "Edited subtitle\nSecond line", decoration: "underline", letter_spacing_px: 1.5 }); expect(facts.edited.project).toEqual(expected); await save(); }
    await reopen();
    const history = page.locator(".editor-history");
    for (const [direction, expected] of [["undo", facts.imported.project], ["redo", facts.edited.project]]) {
      facts.phase = direction; await save();
      if (!facts.transactions[direction]?.request) { const response = page.waitForResponse(r => r.url().endsWith(`/api/tools/opencut_edit_${direction}_plan`)); await history.getByRole("button", { name: direction === "undo" ? "Undo" : "Redo", exact: true }).click(); expect((await (await response).json()).error).toBeNull(); }
      await transaction(direction, `opencut_edit_${direction}_apply`, () => history.getByRole("button", { name: `Apply ${direction}`, exact: true }).click());
      if (!facts[`${direction}_checked`]) { expect((await inspect()).project).toEqual(expected); facts[`${direction}_checked`] = true; await save(); }
    }
    await reopen(); expect((await inspect()).project).toEqual(facts.edited.project);
    for (const format of ["srt", "ass"]) {
      facts.phase = `download ${format}`; await save();
      if (!facts.downloads[format]) {
        await panel.getByRole("combobox", { name: prefix("Caption track") }).selectOption(facts.track_id); await panel.getByRole("combobox", { name: prefix("Format") }).selectOption(format);
        const response = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_subtitle_text_export")), download = page.waitForEvent("download"); await panel.getByRole("button", { name: "Export subtitles", exact: true }).click(); const receipt = (await (await response).json()).data, file = await download; const target = path.join(directory, `f23-editable-subtitles.${format}`); await file.saveAs(target); const text = await fs.readFile(target, "utf8"); expect(text).toBe(receipt.content); expect(receipt.cue_count).toBe(2); expect(receipt.losses.length).toBeGreaterThan(0); expect(text).toContain(format === "ass" ? "Edited subtitle\\NSecond line" : "Edited subtitle\nSecond line"); if (format === "ass") { expect(text).toContain("\\fnLato"); expect(text).toContain("\\u1"); expect(text).toContain("\\fsp1.5"); }
        facts.downloads[format] = { receipt, path: target, sha256: hash(text) }; await save();
      } else expect(hash(await fs.readFile(facts.downloads[format].path))).toBe(facts.downloads[format].sha256);
    }
    facts.phase = "restore exact original aggregate";
    if (!facts.restore_request) facts.restore_request = { project_id: projectId, expected_revision: (await inspect()).revision, target_revision: original.revision, idempotency_key: randomUUID(), asset_versions: {} }; await save();
    if (!facts.restored) { facts.restored = await tool("opencut_project_restore", facts.restore_request); await save(); }
    expect((await inspect()).project).toEqual(original.project); expect(errors).toEqual([]); facts.phase = "passed"; facts.pageerrors = errors; await save(); check("F23 editable subtitle UI/import/edit/download/restore", facts); return facts;
  } catch (error) { facts.failure = { message: String(error).slice(0, 2000), pageerrors: errors, ui: (await page.locator("body").innerText().catch(() => "")).slice(0, 6000) }; await save(); throw error; } finally { await page.close(); }
}
