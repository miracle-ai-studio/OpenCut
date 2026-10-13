// Direct recovery of original v31 text, using the existing owned font workspace.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";

const sha = bytes => createHash("sha256").update(bytes).digest("hex");
export async function verifyClassicV4(browser, endpoint, directory, check) {
  const sourceProject = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8")).project_id;
  const checkpoint = path.join(directory, "classic-v4-import-checkpoint.json");
  const candidate = { binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)) };
  const webFiles = [];
  async function visit(relative = "") {
    for (const item of await fs.readdir(path.join(process.env.OPENCUT_WEB_ROOT, relative), { withFileTypes: true })) {
      const file = path.join(relative, item.name);
      if (item.isDirectory()) await visit(file);
      else if (item.isFile() && /\.(js|css|mjs)$/u.test(file)) webFiles.push([file, sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, file)))]);
    }
  }
  await visit(); webFiles.sort((a, b) => a[0].localeCompare(b[0]));
  candidate.web_js_css_tree_sha256 = sha(JSON.stringify(webFiles));
  let facts = { candidate, source_project_id: sourceProject };
  try { const previous = JSON.parse(await fs.readFile(checkpoint, "utf8")); expect(previous.candidate).toEqual(candidate); facts = previous; }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
  page.setDefaultTimeout(30000);
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  const errors = [], calls = [];
  page.on("pageerror", error => { if (errors.length < 16) errors.push(error.message); });
  page.on("request", request => { const name = new URL(request.url()).pathname.split("/").at(-1); if (name.startsWith("opencut_") && calls.length < 256) calls.push(name); });
  let phase = "prepare original source";
  const save = async values => { facts = { ...facts, ...values, phase }; await fs.writeFile(checkpoint, JSON.stringify(facts, null, 2) + "\n"); };
  const tool = async (name, input) => {
    const reply = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { name, input });
    expect(reply.error, JSON.stringify(reply.error)).toBeNull(); return reply.data;
  };
  const exchange = async (name, action) => {
    const waiting = page.waitForResponse(response => response.url().endsWith(`/v1/tools/${name}`));
    await action(); const response = await waiting, reply = await response.json();
    const result = { request: response.request().postDataJSON(), reply };
    await save({ last_exchange: { operation: name, ...result } }); expect(reply.error, JSON.stringify(reply.error)).toBeNull();
    return { request: result.request, data: reply.data };
  };
  const inspect = () => tool("opencut_project_v4_inspect", { project_id: facts.imported.target_project_id, revision: null });
  const open = async () => { await page.goto(`${endpoint}/editor/${encodeURIComponent(facts.imported.target_project_id)}?scene=scene`); await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible(); };
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button", { name: "Recover Classic projects", exact: true })).toBeEnabled();
    const authority = await tool("opencut_project_v4_inspect", { project_id: sourceProject, revision: null });
    const families = ["Lato", "Ma Shan Zheng"], contents = ["OpenCut\nTitle", "中文\n标题"];
    const selectedFonts = families.map(family => authority.project.fonts.find(font => font.family === family && font.weight === 400 && font.style === "normal"));
    expect(selectedFonts.every(Boolean)).toBe(true);
    const template = JSON.parse(await fs.readFile(new URL("../../../crates/editor-application/src/classic_import_v4/fixture.json", import.meta.url), "utf8"));
    const raw = structuredClone(template);
    raw.metadata = { id: "classic-v4-text-fonts", name: "Classic direct V4 bilingual titles" };
    raw.settings.background.color = "#000000";
    const positions = [[32, -30, 1.25, .75], [-20, 35, .9, 1.1]];
    raw.scenes[0].tracks.overlay = families.map((family, index) => {
      const element = structuredClone(template.scenes[0].tracks.overlay[0].elements[0]);
      Object.assign(element, { id: `title-${index}`, name: `Imported ${family}`, startTime: 0, duration: 360000, trimStart: 12000, trimEnd: 24000, sourceDuration: 396000 });
      Object.assign(element.params, { content: contents[index], fontFamily: family, fontSize: 12, color: "#ffffff", textDecoration: "none", letterSpacing: 0, lineHeight: 1.2, "background.enabled": false, "transform.positionX": positions[index][0], "transform.positionY": positions[index][1], "transform.scaleX": positions[index][2], "transform.scaleY": positions[index][3], "transform.rotate": 0, opacity: 1 });
      return { id: `titles-${index}`, name: `Titles ${family}`, type: "text", hidden: false, elements: [element] };
    });
    const snapshot = { origin: "http://classic.invalid", sourceId: raw.metadata.id, raw, context: { legacyTracksBySceneId: {}, mediaTypesById: {} }, media: [] };
    const header = Buffer.from(JSON.stringify({ format: "opencut.classic-recovery.v1", snapshot, media: [] })), length = Buffer.alloc(4);
    length.writeUInt32LE(header.length);
    const bundle = path.join(directory, "classic-v4-text-fonts.occlassic");
    await fs.writeFile(bundle, Buffer.concat([Buffer.from("OPENCUT-CLASSIC-RECOVERY\n"), length, header]));
    if (!facts.imported) {
      phase = "actual recovery UI and fixed font mapping";
      await page.getByRole("button", { name: "Recover Classic projects", exact: true }).click();
      await page.getByLabel("Classic recovery bundle", { exact: true }).setInputFiles(bundle);
      const first = await exchange("opencut_classic_analyze_v4", () => page.getByRole("button", { name: "Prepare recovery", exact: true }).click());
      expect(first.data.blockers).toEqual([]); expect(first.data.unresolved_fonts).toHaveLength(2);
      expect(JSON.parse(first.request.content)).toEqual(raw);
      await page.getByRole("combobox", { name: /^Font source project\b/iu }).selectOption(sourceProject);
      for (const [index, family] of families.entries()) await page.getByRole("combobox", { name: new RegExp(`^Font for ${family} \\(normal, normal\\)`, "iu") }).selectOption(selectedFonts[index].id);
      const mapped = await exchange("opencut_classic_analyze_v4", () => page.getByRole("button", { name: "Review font mappings", exact: true }).click());
      expect(mapped.data.unresolved_fonts).toEqual([]); expect(mapped.data.blockers).toEqual([]);
      for (const selector of mapped.request.font_mappings) expect(selector).toMatchObject({ source_project_id: sourceProject, source_revision: authority.revision });
      if (mapped.data.losses.length) await page.getByRole("checkbox", { name: "I accept these listed differences for the recovered copy.", exact: true }).check();
      const imported = await exchange("opencut_classic_import_v4", () => page.getByRole("button", { name: "Recover project", exact: true }).click());
      await save({ analysis: mapped.data, import_request: imported.request, imported: imported.data, original_source: raw });
      await page.getByRole("button", { name: "Open recovered project", exact: true }).click();
    } else await open();
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    const firstSaved = await tool("opencut_project_v4_inspect", { project_id: facts.imported.target_project_id, revision: facts.imported.revision });
    expect(firstSaved.project).toEqual(facts.analysis.proposed_project);
    expect(firstSaved.project.sequences[0].captions).toEqual([]);
    expect(firstSaved.project.fonts).toEqual(expect.arrayContaining(selectedFonts));
    expect(firstSaved.project.assets).toEqual([]);
    for (const [index, expected] of raw.scenes[0].tracks.overlay.entries()) {
      const item = firstSaved.project.sequences[0].tracks.find(track => track.id === expected.id).elements[0];
      expect(item.content.kind).toBe("text"); expect(item.content.text.content).toBe(contents[index]);
      expect(item.content.text.font.font_id).toBe(selectedFonts[index].id);
      expect(item.content.text.font_size).toEqual({ unit: "classic_height90", value: 12 });
      const seconds = value => Number(value.ticks) / value.timebase;
      expect(seconds(item.source_span.trim_start)).toBe(.1); expect(seconds(item.source_span.trim_end)).toBe(.2); expect(seconds(item.source_span.source_duration)).toBe(3.3);
      expect(item.visual.transform).toEqual({ position_x_px: positions[index][0], position_y_px: positions[index][1], scale_x: positions[index][2], scale_y: positions[index][3], rotation_degrees: 0 });
    }
    expect(await tool("opencut_classic_import_v4", facts.import_request)).toEqual(facts.imported);
    if (!facts.edit_passed) {
      phase = "imported text edit reopen undo redo";
      const before = await inspect();
      await page.getByRole("button", { name: "Edit text on canvas: OpenCut\nTitle", exact: true }).dblclick();
      const textbox = page.getByRole("textbox", { name: "Edit text on canvas", exact: true });
      await textbox.fill("OpenCut\nRecovered");
      const edit = await exchange("opencut_edit_v4_apply", () => page.getByRole("button", { name: "Confirm text", exact: true }).click());
      const after = await inspect();
      await open(); expect((await inspect()).project).toEqual(after.project);
      const history = page.locator(".v4-history");
      for (const [direction, expected] of [["undo", before.project], ["redo", after.project], ["undo", before.project]]) {
        const label = direction === "undo" ? "Undo" : "Redo";
        await exchange(`opencut_edit_v4_${direction}_plan`, () => history.getByRole("button", { name: label, exact: true }).click());
        await exchange(`opencut_edit_v4_${direction}_apply`, () => history.getByRole("button", { name: `Apply ${direction}`, exact: true }).click());
        expect((await inspect()).project).toEqual(expected);
      }
      await open(); expect((await inspect()).project).toEqual(before.project);
      await save({ edit_passed: true, edit, render_revision: (await inspect()).revision });
    }
    if (!facts.submission) {
      phase = "one imported bilingual PNG";
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5"); await page.getByRole("button", { name: "Go to time", exact: true }).click();
      const submission = await exchange("opencut_job_v4_text_frame_png", () => page.getByRole("button", { name: "Render scene frame", exact: true }).click());
      expect(submission.request.revision).toBe(facts.render_revision); await save({ submission });
    }
    let job;
    await expect.poll(async () => { job = await tool("opencut_job_view", { job_id: facts.submission.data.job_id }); return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending"; }, { timeout: 120000, intervals: [200, 500] }).not.toBe("pending");
    await save({ job }); expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    const artifact = job.artifacts.find(artifact => artifact.mime_type === "image/png"), output = path.join(directory, "classic-v4-text-fonts.png");
    expect(artifact).toBeTruthy();
    let png;
    try { png = await fs.readFile(output); if (sha(png) !== artifact.content_hash) png = null; } catch (error) { if (error.code !== "ENOENT") throw error; }
    if (!png) {
      await page.getByRole("button", { name: "Tasks", exact: true }).click(); await page.getByLabel("Refresh tasks", { exact: true }).click();
      const task = page.locator(".task-card").filter({ hasText: facts.render_revision }); await expect(task).toHaveCount(1);
      const download = page.waitForEvent("download"); await task.getByRole("button", { name: "Download", exact: true }).click(); await (await download).saveAs(output); png = await fs.readFile(output);
    }
    expect(sha(png)).toBe(artifact.content_hash);
    const fontDirectory = process.env.OPENCUT_B1_FONT_SOURCES;
    const manifest = JSON.parse(await fs.readFile(path.join(fontDirectory, "manifest.json"), "utf8"));
    const fontData = await Promise.all(selectedFonts.map(async font => { const file = manifest.files.find(file => file.sha256 === font.content.content_hash); expect(file).toBeTruthy(); const bytes = await fs.readFile(path.join(fontDirectory, path.basename(file.file))); expect(sha(bytes)).toBe(file.sha256); return bytes.toString("base64"); }));
    phase = "independent two-font pixel oracle";
    const oracle = await page.evaluate(async ({ png, fontData, contents, positions }) => {
      const image = new Image(); image.src = `data:image/png;base64,${png}`; await image.decode();
      const expected = new OffscreenCanvas(320, 180), actual = new OffscreenCanvas(320, 180);
      const ctx = expected.getContext("2d"); ctx.fillStyle = "black"; ctx.fillRect(0, 0, 320, 180);
      for (let index = 0; index < fontData.length; index++) {
        const face = new FontFace(`ClassicImportOracle${index}`, Uint8Array.from(atob(fontData[index]), c => c.charCodeAt(0))); await face.load(); document.fonts.add(face);
        try { ctx.save(); ctx.font = `24px ClassicImportOracle${index}`; ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.fillStyle = "white";
          const [x, y, sx, sy] = positions[index]; ctx.translate(160 + x, 90 + y); ctx.scale(sx, sy);
          contents[index].split("\n").forEach((line, n) => ctx.fillText(line, 0, (n - .5) * 28.8)); ctx.restore();
        } finally { document.fonts.delete(face); }
      }
      actual.getContext("2d").drawImage(image, 0, 0);
      const a = actual.getContext("2d").getImageData(0, 0, 320, 180).data, e = ctx.getImageData(0, 0, 320, 180).data;
      let intersection = 0, union = 0, error = 0, blankError = 0;
      for (let i = 0; i < a.length; i += 4) { intersection += +(a[i] > 32 && e[i] > 32); union += +(a[i] > 32 || e[i] > 32); for (let c = 0; c < 4; c++) error += Math.abs(a[i + c] - e[i + c]); if (!e[i] && !e[i + 1] && !e[i + 2]) blankError = Math.max(blankError, a[i], a[i + 1], a[i + 2]); }
      return { width: image.naturalWidth, height: image.naturalHeight, intersection, union, iou: intersection / union, meanChannelError: error / a.length, blankError };
    }, { png: png.toString("base64"), fontData, contents, positions });
    await save({ oracle, png_sha256: sha(png) });
    expect(oracle.width).toBe(320); expect(oracle.height).toBe(180); expect(oracle.union).toBeGreaterThan(150); expect(oracle.iou).toBeGreaterThan(.995); expect(oracle.meanChannelError).toBeLessThan(.1); expect(oracle.blankError).toBeLessThanOrEqual(2);
    expect(await tool("opencut_project_v4_inspect", { project_id: sourceProject, revision: null })).toEqual(authority);
    expect(calls).not.toContain("opencut_project_v3_promote"); expect(calls).not.toContain("opencut_classic_recovery_import_v1"); expect(calls).not.toContain("opencut_font_v1_import"); expect(errors).toEqual([]);
    await save({ status: "passed", scope: "Original v31 independent bilingual text and exact fixed font mapping, real import/edit/reopen/Undo/Redo, one independently checked PNG. No complete Classic feature/package/S1 claim." });
    await fs.writeFile(path.join(directory, "classic-v4-import.json"), JSON.stringify(facts, null, 2) + "\n"); check("Classic direct V4 import and fixed-font text recovery", facts);
  } catch (error) {
    await fs.writeFile(path.join(directory, `classic-v4-import-failure-${Date.now()}.json`), JSON.stringify({ phase, error: String(error), facts, errors, calls, ui: await page.locator("body").innerText().catch(() => "") }, null, 2));
    throw error;
  } finally { await page.close(); }
}
