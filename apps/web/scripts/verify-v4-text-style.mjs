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

// Owned existing Latin title/fonts only. Isolation changes scene background
// and other visibility; every value under test is entered through product UI.
// A retained successful fixed-revision PNG is downloaded/rechecked, never rerendered.
export async function verifyV4TextStyle(browser, endpoint, directory, check, options = {}) {
  const { project_id: projectId } = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8"));
  const candidate = await candidateIdentity(), checkpointPath = path.join(directory, "f21-text-style-checkpoint.json");
  let prior = null;
  try { prior = JSON.parse(await fs.readFile(checkpointPath, "utf8")); } catch (error) { if (error.code !== "ENOENT") throw error; }
  if (options.freshRun) prior = null;
  if (prior?.styled_revision) { expect(prior.project_id).toBe(projectId); expect(prior.candidate, "Retained style evidence belongs to another candidate; root must decide a fresh run").toEqual(candidate); }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
  page.setDefaultTimeout(30000);
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  let phase = "open", original, failure, changed = false, facts = prior?.styled_revision ? prior : { project_id: projectId, candidate, evidence: [] };
  const errors = [], network = [];
  let applyCount = 0;
  page.on("pageerror", error => { if (errors.length < 32) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 256) network.push({ operation });
    if (operation === "opencut_edit_v4_apply") applyCount++;
  });
  const save = async values => {
    facts = { ...facts, ...values, phase };
    const encoded = JSON.stringify(facts, null, 2) + "\n";
    if (Buffer.byteLength(encoded, "utf8") > 2 * 1024 * 1024) throw new Error("F21 checkpoint exceeds bounded 2 MiB evidence envelope");
    await fs.writeFile(checkpointPath, encoded);
  };
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({ name, input }) => (await fetch(`/v1/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { name, input });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const inspect = revision => tool("opencut_project_v4_inspect", { project_id: projectId, revision: revision ?? null });
  const exchange = async (name, action) => {
    const pending = page.waitForResponse(response => response.url().endsWith(`/v1/tools/${name}`));
    await action(); const response = await pending, envelope = await response.json();
    const value = { request: response.request().postDataJSON(), envelope };
    await save({ last_exchange: { operation: name, ...value } });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return { request: value.request, data: envelope.data };
  };
  const reopen = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(projectId)}${facts.sequence_id ? `?scene=${encodeURIComponent(facts.sequence_id)}` : ""}`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    if (facts.sequence_id) await page.getByRole("combobox", { name: /^V4 scene(?:\s|$)/ }).selectOption(facts.sequence_id);
  };
  const scene = snapshot => snapshot.project.sequences.find(scene => scene.id === facts.sequence_id);
  const element = snapshot => scene(snapshot).tracks.flatMap(track => track.elements).find(element => element.id === facts.element_id);
  const select = async () => { await page.locator(`[data-element-id="${facts.element_id}"] .v4-element-select`).click({ position: { x: 12, y: 12 } }); };
  const isolateDuplicate = async snapshot => {
    const track = scene(snapshot).tracks.find(track => track.elements.some(item => item.id === facts.duplicate_id));
    const copy = track.elements.find(item => item.id === facts.duplicate_id);
    expect(copy.content).toEqual(facts.expected_title.content);
    expect(copy.visual).toEqual(facts.expected_title.visual);
    expect(copy.source_span).toEqual(facts.expected_title.source_span);
    expect(copy.channels).toEqual(facts.expected_title.channels);
    expect(copy.timeline_range).toEqual(facts.expected_title.timeline_range);
    expect(scene(snapshot).captions).toEqual(facts.compatibility_captions);
    const committed = await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: snapshot.revision, idempotency_key: randomUUID(), label: "F21 isolate duplicated text for one PNG", operations: [{ type: "element_put", sequence_id: facts.sequence_id, track_id: track.id, element: { ...copy, enabled: false } }] });
    await save({ render_revision: committed.revision });
  };
  const submitFrame = async () => {
    phase = "one fixed native PNG";
    await reopen();
    await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0.5"); await page.getByRole("button", { name: "Go to time", exact: true }).click();
    // The retained caption record selects the existing caption-capable frame
    // protocol even though its track is disabled for this isolated PNG.
    facts.frame_protocol = "caption";
    const submitted = await exchange("opencut_job_v4_caption_frame_png", () => page.getByRole("button", { name: "Render scene frame", exact: true }).click());
    await save({ submission: submitted });
    expect(submitted.request).toMatchObject({ project_id: projectId, revision: facts.render_revision, sequence_id: facts.sequence_id });
  };
  try {
    await reopen(); original = await inspect();
    if (!facts.submission && prior?.observed_duplicate) {
      phase = "resume saved duplicate after verifier correction";
      const duplicated = await inspect(prior.last_exchange.envelope.data.revision);
      expect(hash(JSON.stringify(duplicated.project))).toBe(prior.observed_duplicate_project_sha256);
      changed = true;
      await tool("opencut_project_v4_restore", { project_id: projectId, expected_revision: original.revision, target_revision: duplicated.revision, idempotency_key: randomUUID(), asset_versions: {} });
      facts.evidence.push({ case: "duplicate full styled text through UI", apply: prior.last_exchange, duplicate_id: prior.duplicate_id, resumed_from_saved_revision: duplicated.revision });
      await isolateDuplicate(await inspect());
      await submitFrame();
    }
    if (!facts.submission) {
      phase = "isolate existing free-text output";
      facts.sequence_id = original.project.default_sequence_id;
      const track = scene(original).tracks.find(track => track.enabled && !track.locked && track.elements.some(element => element.id === "cf25b163-cc52-46f7-ab1a-9719bb1e6180"));
      const title = track?.elements.find(element => element.id === "cf25b163-cc52-46f7-ab1a-9719bb1e6180");
      expect(title?.content.kind).toBe("text"); facts.element_id = title.id; facts.track_id = track.id;
      const font = original.project.fonts.find(font => font.id === title.content.text.font.font_id);
      expect(font).toMatchObject({ family: "Lato", weight: 400, style: "normal" });
      expect(title.content.text.font_size.unit).toBe("classic_height90");
      expect(title.channels).toEqual([]);
      expect(title.visual.transform).toEqual({ position_x_px: 0, position_y_px: -45, scale_x: 1, scale_y: 1, rotation_degrees: 0 });
      expect(title.visual.opacity).toBe(1); expect(title.visual.effects).toEqual([]); expect(title.visual.masks).toEqual([]);
      expect(title.content.text).toMatchObject({ align: "center", decoration: "none", letter_spacing_px: 0, line_height_multiplier: 1.2, color: { red: 1, green: 1, blue: 1, alpha: 1 }, background: { enabled: false } });
      const settings = structuredClone(original.project.settings);
      settings.settings.canvas_size = { width: 320, height: 180 }; settings.settings.canvas_size_mode = "custom";
      settings.settings.last_custom_canvas_size = { width: 320, height: 180 };
      settings.settings.background = { kind: "color", color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 } };
      const centered = structuredClone(title);
      centered.visual.transform.position_y_px = 0;
      const operations = [{ type: "settings_set", settings }, { type: "element_put", sequence_id: facts.sequence_id, track_id: track.id, element: centered }];
      for (const owned of scene(original).tracks) {
        if (owned.kind === "caption" && owned.enabled) operations.push({ type: "track_put", sequence_id: facts.sequence_id, track: { ...structuredClone(owned), enabled: false } });
        for (const other of owned.elements) if (other.id !== title.id && other.enabled) operations.push({ type: "element_put", sequence_id: facts.sequence_id, track_id: owned.id, element: { ...structuredClone(other), enabled: false } });
      }
      // Reuse the already-accepted caption revision. The current restored scene
      // has no captions; comparing two empty arrays would not prove coexistence.
      const captionReceipt = JSON.parse(await fs.readFile(path.join(directory, "b3-captions.json"), "utf8"));
      expect(captionReceipt.project_id).toBe(projectId);
      const captionSource = await inspect(captionReceipt.final_revision);
      const captions = scene(captionSource).captions;
      expect(captions.length).toBeGreaterThan(0);
      for (const caption of captions) {
        const existing = scene(original).tracks.find(item => item.id === caption.track_id);
        if (!existing) {
          const sourceTrack = scene(captionSource).tracks.find(item => item.id === caption.track_id);
          expect(sourceTrack.kind).toBe("caption");
          operations.push({ type: "track_put", sequence_id: facts.sequence_id, track: { ...structuredClone(sourceTrack), enabled: false } });
        }
        operations.push({ type: "caption_put", sequence_id: facts.sequence_id, caption });
      }
      facts.compatibility_captions = captions;
      facts.caption_source_revision = captionReceipt.final_revision;
      changed = true;
      await tool("opencut_edit_v4_apply", { project_id: projectId, expected_revision: original.revision, idempotency_key: randomUUID(), label: "F21 existing text output isolation", operations });
      await save({ original_revision: original.revision, baseline_title: title, font }); await reopen();
      phase = "actual complete style and precise face UI";
      const bold = original.project.fonts.find(face => face.family === "Lato" && face.weight === 700 && face.style === "normal");
      const boldItalic = original.project.fonts.find(face => face.family === "Lato" && face.weight === 700 && face.style === "italic");
      await save({ required_faces: { regular: font, bold, boldItalic } });
      expect(bold).toBeTruthy(); expect(boldItalic).toBeTruthy(); facts.font = boldItalic;
      await select(); await page.getByRole("button", { name: "Edit text", exact: true }).click();
      const form = page.getByRole("form", { name: "Edit text", exact: true });
      await form.getByRole("textbox", { name: prefix("Text content") }).fill("OpenCut\nStyle");
      await form.getByRole("textbox", { name: prefix("Font size (Classic)") }).fill("12");
      await form.getByRole("combobox", { name: prefix("Font weight") }).selectOption("700");
      await expect(form.getByRole("combobox", { name: prefix("Font family") })).toHaveValue(bold.id);
      await form.getByRole("combobox", { name: prefix("Font style") }).selectOption("italic");
      await expect(form.getByRole("combobox", { name: prefix("Font family") })).toHaveValue(boldItalic.id);
      await form.getByRole("combobox", { name: prefix("Text decoration") }).selectOption("line_through");
      await form.getByLabel("Enable text background", { exact: true }).check();
      await form.getByLabel(prefix("Background color")).fill("#cc3311");
      for (const [label, value] of [["Letter spacing (pixels)", "1.5"], ["Line height", "1.6"], ["Background opacity (%)", "45"], ["Background corner radius (%)", "37.5"], ["Background horizontal padding (Classic size 15)", "7.25"], ["Background vertical padding (Classic size 15)", "11.5"], ["Background horizontal offset (pixels)", "-13.75"], ["Background vertical offset (pixels)", "9.5"]])
        await form.getByRole("textbox", { name: prefix(label) }).fill(value);
      const before = await inspect(), count = applyCount;
      const expected = structuredClone(element(before));
      Object.assign(expected.content.text, { content: "OpenCut\nStyle", font: { kind: "locked", font_id: boldItalic.id }, font_weight: "bold", font_style: "italic", decoration: "line_through", font_size: { unit: "classic_height90", value: 12 }, letter_spacing_px: 1.5, line_height_multiplier: 1.6 });
      Object.assign(expected.content.text.background, { enabled: true, color: { encoding: "srgb", red: 204 / 255, green: 51 / 255, blue: 17 / 255, alpha: .45 }, corner_radius_percent: 37.5, padding_x_at_size15: 7.25, padding_y_at_size15: 11.5, offset_x_px: -13.75, offset_y_px: 9.5 });
      const planned = await exchange("opencut_text_v1_plan", () => form.getByRole("button", { name: "Save text", exact: true }).click());
      expect(planned.request).toMatchObject({ project_id: projectId, expected_revision: before.revision, sequence_id: facts.sequence_id, command: { kind: "edit", target: { track_id: track.id, element_id: title.id }, values: { text: expected.content.text } } });
      expect(planned.data.transaction.operations).toEqual([{ type: "element_put", sequence_id: facts.sequence_id, track_id: track.id, element: expected }]);
      expect((await inspect()).revision).toBe(before.revision); expect(applyCount).toBe(count);
      const review = page.getByRole("region", { name: "Review text changes", exact: true });
      const edited = await exchange("opencut_edit_v4_apply", () => review.getByRole("button", { name: "Apply text changes", exact: true }).click());
      expect(edited.request).toEqual(planned.data.transaction); expect(applyCount).toBe(count + 1);
      await expect(review).toBeHidden(); const styled = await inspect();
      await save({ observed_style: element(styled), observed_project_sha256: hash(JSON.stringify(styled.project)) });
      expect(element(styled)).toEqual(expected);
      const expectedProject = structuredClone(before.project);
      expectedProject.sequences.find(scene => scene.id === facts.sequence_id).tracks.find(track => track.id === facts.track_id).elements = scene(before).tracks.find(track => track.id === facts.track_id).elements.map(item => item.id === expected.id ? expected : item);
      expect(styled.project).toEqual(expectedProject);
      expect(scene(styled).captions).toEqual(facts.compatibility_captions);
      facts.evidence.push({ case: phase, plan: planned, apply: edited });
      await save({ styled_revision: styled.revision, expected_title: expected });
      phase = "reopen and selective style Undo/Redo";
      await reopen(); expect((await inspect()).project).toEqual(styled.project);
      const history = page.locator(".v4-history"), historyEvidence = [];
      for (const [direction, expectedGraph] of [["undo", before.project], ["redo", styled.project]]) {
        const proposal = await exchange(`opencut_edit_v4_${direction}_plan`, () => history.getByRole("button", { name: direction === "undo" ? "Undo" : "Redo", exact: true }).click());
        const committed = await exchange(`opencut_edit_v4_${direction}_apply`, () => history.getByRole("button", { name: `Apply ${direction}`, exact: true }).click());
        expect(committed.request).toEqual(proposal.request); await expect(history.getByRole("button", { name: `Apply ${direction}`, exact: true })).toBeHidden();
        const observed = await inspect();
        await save({ last_history: { direction, proposal, committed, observed_title: element(observed), observed_project_sha256: hash(JSON.stringify(observed.project)) } });
        expect(observed.project).toEqual(expectedGraph); historyEvidence.push({ direction, proposal, committed });
      }
      facts.evidence.push({ case: phase, history: historyEvidence }); await save({});
      phase = "duplicate full styled text through UI";
      await select();
      await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0"); await page.getByRole("button", { name: "Go to time", exact: true }).click();
      const duplicateBase = await inspect(), duplicateCount = applyCount;
      const duplicatePlan = await exchange("opencut_timeline_v4_paste_plan", () => page.locator('[aria-label="Timeline clipboard"]').getByRole("button", { name: "Duplicate elements", exact: true }).click());
      expect(duplicatePlan.request.expected_revision).toBe(duplicateBase.revision);
      expect((await inspect()).revision).toBe(duplicateBase.revision); expect(applyCount).toBe(duplicateCount);
      const duplicateReview = page.getByRole("dialog", { name: "Review paste", exact: true });
      const duplicateApply = await exchange("opencut_edit_v4_apply", () => duplicateReview.getByRole("button", { name: "Apply copied elements", exact: true }).click());
      expect(duplicateApply.request).toEqual(duplicatePlan.data.transaction); expect(applyCount).toBe(duplicateCount + 1);
      await expect(duplicateReview.getByRole("status")).toHaveText("Pasted elements saved.");
      await duplicateReview.getByRole("button", { name: "Done", exact: true }).click();
      const duplicated = await inspect(), duplicateId = duplicatePlan.data.decision.selection[0].element_id;
      const copy = scene(duplicated).tracks.flatMap(track => track.elements).find(element => element.id === duplicateId);
      await save({ observed_duplicate: copy, duplicate_id: duplicateId, observed_duplicate_project_sha256: hash(JSON.stringify(duplicated.project)) });
      expect(copy.content).toEqual(expected.content); expect(copy.visual).toEqual(expected.visual); expect(copy.source_span).toEqual(expected.source_span); expect(copy.channels).toEqual(expected.channels);
      expect(Number(copy.timeline_range.start.ticks) / copy.timeline_range.start.timebase).toBe(0);
      expect(copy.timeline_range.duration).toEqual(expected.timeline_range.duration);
      expect(element(duplicated)).toEqual(expected); expect(scene(duplicated).captions).toEqual(facts.compatibility_captions);
      await reopen(); expect((await inspect()).project).toEqual(duplicated.project);
      facts.evidence.push({ case: phase, plan: duplicatePlan, apply: duplicateApply, duplicate_id: duplicateId });
      await isolateDuplicate(duplicated);
      await submitFrame();
    }
    phase = "original PNG job completion/download";
    let job;
    await expect.poll(async () => {
      job = await tool("opencut_job_view", { job_id: facts.submission.data.job_id });
      return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending";
    }, { timeout: 120000, intervals: [100, 250, 500] }).not.toBe("pending");
    await save({ job }); expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.extended_result.kind).toBe("render.project_caption_frame.v4");
    const result = job.extended_result.data;
    expect(result.identity).toMatchObject({ project_id: projectId, revision: facts.render_revision, sequence_id: facts.sequence_id, width: 320, height: 180, profile: "classic-v31-gpu-normal-caption-frame-v3" });
    expect(result.captions).toEqual([]);
    expect(result.texts).toHaveLength(1); expect(result.font_lock).toEqual([{ font_id: facts.font.id, content_hash: facts.font.content.content_hash, size: facts.font.content.size }]);
    const artifact = job.artifacts.find(artifact => artifact.mime_type === "image/png"); expect(artifact).toBeTruthy();
    const output = path.join(directory, "f21-text-style.png");
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
    phase = "independent precise-face full-style Canvas oracle";
    const fontsDirectory = process.env.OPENCUT_B1_FONT_SOURCES; expect(fontsDirectory).toBeTruthy();
    const manifest = JSON.parse(await fs.readFile(path.join(fontsDirectory, "manifest.json"), "utf8"));
    const source = manifest.files.find(source => source.sha256 === facts.font.content.content_hash); expect(source).toBeTruthy();
    const fontBytes = await fs.readFile(path.join(fontsDirectory, path.basename(source.file))); expect(hash(fontBytes)).toBe(facts.font.content.content_hash);
    const oracle = await page.evaluate(async ({ png, font }) => {
      const image = new Image(); image.src = `data:image/png;base64,${png}`; await image.decode();
      const face = new FontFace("F21IndependentFont", Uint8Array.from(atob(font), character => character.charCodeAt(0)), { weight: "700", style: "italic" }); await face.load(); document.fonts.add(face);
      try {
        const expected = new OffscreenCanvas(320, 180), ctx = expected.getContext("2d"); ctx.fillStyle = "black"; ctx.fillRect(0, 0, 320, 180);
        // Independent Canvas glyph raster under T*S*R. Font size
        // is12*180/90=24; baselines +/-19.2; identity transform center160/90.
        ctx.font = "italic 700 24px F21IndependentFont"; ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.fillStyle = "white"; ctx.letterSpacing = "1.5px";
        const lines = ["OpenCut", "Style"].map((text, index) => ({ text, baselineY: (index - .5) * 38.4, width: ctx.measureText(text).width, ascent: ctx.measureText(text).actualBoundingBoxAscent, descent: ctx.measureText(text).actualBoundingBoxDescent }));
        // Classic authored padding scales by size12/15, independently of
        // output glyph24px. Two line boxes total76.8px; radius=min/2*37.5%.
        const maxWidth = Math.max(...lines.map(line => line.width));
        const rectangle = { left: -maxWidth / 2 - 5.8 - 13.75, top: -38.4 - 9.2 + 9.5, width: maxWidth + 11.6, height: 76.8 + 18.4 };
        rectangle.radius = Math.min(rectangle.width, rectangle.height) / 2 * .375;
        ctx.translate(160, 90);
        ctx.fillStyle = "rgba(204,51,17,.45)"; ctx.beginPath(); ctx.roundRect(rectangle.left, rectangle.top, rectangle.width, rectangle.height, rectangle.radius); ctx.fill();
        ctx.fillStyle = "white";
        lines.forEach(line => { ctx.fillText(line.text, 0, line.baselineY); ctx.fillRect(-line.width / 2, line.baselineY - (line.ascent - line.descent) * .35, line.width, 1.68); });
        const actual = new OffscreenCanvas(320, 180), actualCtx = actual.getContext("2d"); actualCtx.drawImage(image, 0, 0);
        const e = expected.getContext("2d").getImageData(0, 0, 320, 180).data, a = actualCtx.getImageData(0, 0, 320, 180).data;
        const bounds = data => { let left = 320, top = 180, right = -1, bottom = -1, count = 0; for (let y = 0; y < 180; y++) for (let x = 0; x < 320; x++) if (data[(y * 320 + x) * 4] > 32) { count++; left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y); } return { left, top, right, bottom, count }; };
        let union = 0, intersection = 0, error = 0, blankError = 0;
        let glyphUnion = 0, glyphIntersection = 0, expectedGlyphs = 0, backgroundUnion = 0, backgroundIntersection = 0;
        for (let i = 0; i < a.length; i += 4) {
          const onA = a[i] > 32, onE = e[i] > 32; union += Number(onA || onE); intersection += Number(onA && onE);
          const glyphA = Math.min(a[i], a[i + 1], a[i + 2]) > 180, glyphE = Math.min(e[i], e[i + 1], e[i + 2]) > 180;
          glyphUnion += Number(glyphA || glyphE); glyphIntersection += Number(glyphA && glyphE); expectedGlyphs += Number(glyphE);
          const bgA = a[i] - a[i + 1] > 30, bgE = e[i] - e[i + 1] > 30;
          backgroundUnion += Number(bgA || bgE); backgroundIntersection += Number(bgA && bgE);
          for (let c = 0; c < 4; c++) error += Math.abs(a[i + c] - e[i + c]);
          if (!e[i] && !e[i + 1] && !e[i + 2]) blankError = Math.max(blankError, a[i], a[i + 1], a[i + 2], 255 - a[i + 3]);
        }
        return { width: image.naturalWidth, height: image.naturalHeight, lines, rectangle, center: { x: 160, y: 90 }, fontSize: 24, lineHeight: 38.4, expectedBounds: bounds(e), actualBounds: bounds(a), iou: intersection / union, glyphIou: glyphIntersection / glyphUnion, expectedGlyphs, backgroundIou: backgroundIntersection / backgroundUnion, meanChannelError: error / a.length, blankError };
      } finally { document.fonts.delete(face); }
    }, { png: png.toString("base64"), font: fontBytes.toString("base64") });
    await save({ oracle });
    const layout = result.texts[0].layout;
    expect(layout.fontSizePx).toBe(24); expect(layout.lineHeightPx).toBeCloseTo(38.4, 8);
    expect(layout.lines.map(line => line.content)).toEqual(["OpenCut", "Style"]);
    for (const [index, line] of oracle.lines.entries()) {
      expect(layout.lines[index].width).toBeCloseTo(line.width, 4);
      expect(layout.lines[index].baselineY).toBeCloseTo(line.baselineY, 8);
      expect(layout.lines[index].decoration.top).toBeCloseTo(line.baselineY - (line.ascent - line.descent) * .35, 4);
      expect(layout.lines[index].decoration.height).toBeCloseTo(1.68, 4);
    }
    expect(oracle.width).toBe(320); expect(oracle.height).toBe(180); expect(oracle.expectedBounds.count).toBeGreaterThan(100);
    expect(oracle.expectedGlyphs).toBeGreaterThan(100); expect(oracle.glyphIou).toBeGreaterThan(.995); expect(oracle.backgroundIou).toBeGreaterThan(.995);
    expect(oracle.iou).toBeGreaterThan(.995); expect(oracle.meanChannelError).toBeLessThan(.1); expect(oracle.blankError).toBeLessThanOrEqual(2);
    for (const coordinate of ["left", "top", "right", "bottom"]) expect(Math.abs(oracle.actualBounds[coordinate] - oracle.expectedBounds[coordinate])).toBeLessThanOrEqual(1);
    expect(errors).toEqual([]); await save({ pixels_passed: true });
  } catch (error) {
    failure = error;
    const report = { status: "failed", phase, error: String(error).slice(0, 4096), checkpoint: facts, errors, network };
    try { report.ui = await page.evaluate(() => document.body.innerText.slice(0, 8192)); } catch {}
    const file = `f21-text-style-failure-${Date.now()}.json`;
    await fs.writeFile(path.join(directory, file), JSON.stringify(report, null, 2) + "\n");
    await page.screenshot({ path: path.join(directory, file.replace(/\.json$/u, ".png")), fullPage: false }).catch(() => {});
  } finally {
    if (changed && original) {
      try {
        const current = await inspect(); await tool("opencut_project_v4_restore", { project_id: projectId, expected_revision: current.revision, target_revision: original.revision, idempotency_key: randomUUID(), asset_versions: {} });
        const restored = await inspect(); expect(restored.project).toEqual(original.project); await save({ restored_from_revision: original.revision, restored_revision: restored.revision });
      } catch (error) { await save({ restore_error: String(error) }); if (!failure) failure = error; }
    }
    await page.close();
  }
  if (failure) throw failure;
  const report = { ...facts, status: "passed", reused_job: prior?.submission?.data.job_id ?? null, scope: "Actual UI Lato precise BoldItalic face, multiline/Classic size, strikethrough/spacing/line height and complete semi-transparent background combination; original atomic plan/apply, reopen/UndoRedo/duplicate, caption arrays unchanged, one fixed PNG with independent external-face Canvas pixels. One combination/face only; ordinary/underline retained prior evidence, no all-font or animation claim." };
  await fs.writeFile(path.join(directory, "f21-text-style.json"), JSON.stringify(report, null, 2) + "\n"); check("F21 complete text style UI and independent pixels", report);
}
