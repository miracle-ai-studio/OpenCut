import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";

const hash = value => createHash("sha256").update(value).digest("hex");
const prefix = label => new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
async function candidateIdentity() {
  const root = process.env.OPENCUT_WEB_ROOT, binary = process.env.OPENCUT_WEB_BINARY;
  if (!root || !binary) throw new Error("Explicit OPENCUT_WEB_ROOT and OPENCUT_WEB_BINARY are required");
  const files = [];
  async function visit(relative = "") {
    for (const item of await fs.readdir(path.join(root, relative), { withFileTypes: true })) {
      const name = path.join(relative, item.name);
      if (item.isDirectory()) await visit(name);
      else if (item.isFile() && /\.(js|mjs|cjs|css)$/u.test(name)) files.push([name, hash(await fs.readFile(path.join(root, name)))]);
      else if (item.isSymbolicLink()) throw new Error(`Unhashed candidate symlink ${name}`);
    }
  }
  await visit(); files.sort((a, b) => a[0].localeCompare(b[0])); expect(files.length).toBeGreaterThan(0);
  return { binary_sha256: hash(await fs.readFile(binary)), web_js_css_tree_sha256: hash(JSON.stringify(files)) };
}

// Scope: the serialized TextElement emitted by Classic's subtitle builder, not
// V4 CaptionStyle, ASS style preservation, ASR, a pixel oracle or system IME.
export async function verifyClassicSubtitleStyle(browser, endpoint, directory, check) {
  const sourceProject = JSON.parse(await fs.readFile(path.join(directory, "b1-text-project.json"), "utf8")).project_id;
  const candidate = await candidateIdentity(), checkpoint = path.join(directory, "classic-v4-subtitle-style-checkpoint.json");
  let facts = { candidate, source_project_id: sourceProject };
  try {
    const previous = JSON.parse(await fs.readFile(checkpoint, "utf8"));
    expect(previous.candidate, "Retained evidence belongs to another candidate; root must decide a new run").toEqual(candidate);
    expect(previous.source_project_id).toBe(sourceProject); facts = previous;
  } catch (error) { if (error.code !== "ENOENT") throw error; }
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [], network = [], forbiddenCalls = [];
  let forbiddenCount = 0;
  page.on("pageerror", error => { if (errors.length < 16) errors.push(error.message.slice(0, 1024)); });
  page.on("request", request => {
    const operation = new URL(request.url()).pathname.split("/").at(-1);
    if (operation.startsWith("opencut_") && network.length < 128) network.push(operation);
    if (/font.*import|upload|frame_png|video_mp4/u.test(operation)) {
      forbiddenCount++;
      if (forbiddenCalls.length < 16) forbiddenCalls.push(operation);
    }
  });
  let phase = "capture fixed font authority";
  const save = async values => {
    facts = { ...facts, ...values, phase };
    const encoded = JSON.stringify(facts, null, 2) + "\n";
    if (Buffer.byteLength(encoded) > 2 * 1024 * 1024) throw new Error("Subtitle style checkpoint exceeds 2 MiB");
    await fs.writeFile(checkpoint, encoded);
  };
  const tool = async (operation, input, key) => {
    if (key) await save({ [`${key}_request`]: input });
    const envelope = await page.evaluate(async ({ operation, input }) => (await fetch(`/v1/tools/${operation}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { operation, input });
    if (key) await save({ [key]: { request: input, envelope } });
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return envelope.data;
  };
  const exchange = async (operation, action, key) => {
    await save({ [`${key}_attempted`]: true });
    const waiting = page.waitForResponse(response => response.url().endsWith(`/v1/tools/${operation}`));
    await action(); const response = await waiting, envelope = await response.json();
    const value = { request: response.request().postDataJSON(), envelope };
    await save({ [key]: value }); expect(envelope.error, JSON.stringify(envelope.error)).toBeNull(); return value;
  };
  const inspect = revision => tool("opencut_project_v4_inspect", { project_id: facts.import_request.source.target_project_id, revision: revision ?? null });
  const scene = snapshot => snapshot.project.sequences.find(item => item.id === "scene");
  const element = snapshot => scene(snapshot).tracks.flatMap(track => track.elements).find(item => item.id === "classic-subtitle-cue-1");
  const open = async () => {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(facts.import_request.source.target_project_id)}?scene=scene`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    const welcome = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await welcome.isVisible()) await welcome.getByRole("button", { name: "Close", exact: true }).click();
  };
  const select = () => page.locator('[data-element-id="classic-subtitle-cue-1"] .v4-element-select').click({ position: { x: 12, y: 12 } });
  // A saved public UI transaction is the only replay input after a lost response.
  const recoverApply = async (operation, key, request) => {
    if (!facts[key]?.envelope?.data) await tool(operation, request, key);
    expect(facts[key].envelope.error).toBeNull(); return facts[key].envelope.data;
  };
  try {
    await page.goto(`${endpoint}/editor/${encodeURIComponent(sourceProject)}`);
    await expect(page.getByRole("main", { name: "V4 editor", exact: true })).toBeVisible();
    if (!facts.import_request) {
      const authority = await tool("opencut_project_v4_inspect", { project_id: sourceProject, revision: null });
      const font = authority.project.fonts.find(item => item.family === "Lato" && item.weight === 700 && item.style === "italic");
      expect(font, "Reuse the already imported Lato bold italic face").toBeTruthy();
      const builderPath = new URL("../../../tmp/opencut-classic/apps/web/src/subtitles/build-subtitle-text-element.ts", import.meta.url);
      const builderSource = await fs.readFile(builderPath, "utf8");
      expect(builderSource).toContain('name: `Caption ${index + 1}`');
      expect(builderSource).toContain('"background.paddingX":');
      // Explicit short lines need no automatic wrap. For middle placement,
      // positive padding covers the text; the visual rectangle center is the
      // background offset. Classic resolvePositionX/Y therefore yields (-3,2).
      const cue = { text: "Legacy\nSubtitle", startTime: .2, duration: 1.4, style: {
        fontFamily: "Lato", fontSize: 12, fontWeight: "bold", fontStyle: "italic", textDecoration: "line-through", textAlign: "center", color: "#eecc88", letterSpacing: 1.25, lineHeight: 1.3,
        background: { enabled: true, color: "#20406099", cornerRadius: 30, paddingX: 8, paddingY: 10, offsetX: 3, offsetY: -2 }, placement: { verticalAlign: "middle" },
      } };
      const style = cue.style, background = style.background;
      // Reproduce the builder's serialized return shape + placement-assigned ID,
      // not a preexisting independent title with its name changed.
      const subtitle = { id: "classic-subtitle-cue-1", type: "text", name: "Caption 1", startTime: 24000, duration: 168000, trimStart: 0, trimEnd: 0, params: {
        content: cue.text, fontSize: style.fontSize, fontFamily: style.fontFamily, fontWeight: style.fontWeight, fontStyle: style.fontStyle, textDecoration: style.textDecoration, textAlign: style.textAlign, color: style.color, letterSpacing: style.letterSpacing, lineHeight: style.lineHeight,
        "background.enabled": background.enabled, "background.color": background.color, "background.cornerRadius": background.cornerRadius, "background.paddingX": background.paddingX, "background.paddingY": background.paddingY, "background.offsetX": background.offsetX, "background.offsetY": background.offsetY,
        "transform.positionX": -3, "transform.positionY": 2, "transform.scaleX": 1, "transform.scaleY": 1, "transform.rotate": 0, opacity: 1, blendMode: "normal",
      } };
      const raw = JSON.parse(await fs.readFile(new URL("../../../crates/editor-application/src/classic_import_v4/fixture.json", import.meta.url), "utf8"));
      raw.metadata = { id: "classic-subtitle-style-f21", name: "Classic generated subtitle style compatibility" };
      raw.scenes[0].tracks.overlay = [{ id: "classic-subtitle-track", name: "Subtitles", type: "text", hidden: false, elements: [subtitle] }];
      const source = { content: JSON.stringify(raw), target_project_id: `classic-subtitle-f21-${hash(JSON.stringify([raw, font.content.content_hash, authority.revision])).slice(0, 24)}`, media_mappings: [], font_mappings: [{ classic_family: "Lato", font_weight: "bold", font_style: "italic", source_project_id: sourceProject, source_revision: authority.revision, font_id: font.id }] };
      const analysis = await tool("opencut_classic_analyze_v4", source, "analysis");
      await save({ builder: { path: builderPath.pathname, sha256: hash(builderSource), input: cue, serialized_output: subtitle, scope: "Reviewed serialized builder output; Classic code is not executed by this helper" }, authority_revision: authority.revision, font });
      expect(analysis.blockers).toEqual([]); expect(analysis.unresolved_fonts).toEqual([]); expect(analysis.proposed_project).toBeTruthy();
      await save({ import_request: { source, expected_source_content_id: analysis.source_content_id, accepted_loss_codes: [...new Set(analysis.losses.map(item => item.code))].sort(), idempotency_key: `classic-subtitle-f21-${hash(JSON.stringify(source))}` } });
    }
    phase = "one atomic import or original-key acknowledgement recovery";
    if (!facts.imported?.envelope?.data) await tool("opencut_classic_import_v4", facts.import_request, "imported");
    expect(facts.imported.envelope.error).toBeNull();
    const imported = await inspect(facts.imported.envelope.data.revision);
    expect(imported.project).toEqual(facts.analysis.envelope.data.proposed_project);
    const original = element(imported);
    expect(original.name).toBe("Caption 1"); expect(original.content.kind).toBe("text");
    expect(original.content.text).toMatchObject({ content: "Legacy\nSubtitle", font: { kind: "locked", font_id: facts.font.id }, font_weight: "bold", font_style: "italic", decoration: "line_through", letter_spacing_px: 1.25, line_height_multiplier: 1.3, background: { enabled: true, corner_radius_percent: 30, padding_x_at_size15: 8, padding_y_at_size15: 10, offset_x_px: 3, offset_y_px: -2 } });
    expect(Number(original.timeline_range.start.ticks) / original.timeline_range.start.timebase).toBe(.2);
    expect(scene(imported).tracks.flatMap(item => item.elements)).toHaveLength(1);
    expect(imported.project.assets).toEqual([]);
    await save({ imported_revision: imported.revision, original_element: original });
    await open();
    if (!facts.edited?.envelope?.data) {
      phase = "real existing text style editor";
      if (facts.edited_attempted && facts.edit_plan?.envelope?.data) await recoverApply("opencut_edit_v4_apply", "edited", facts.edit_plan.envelope.data.transaction);
      else {
        expect((await inspect()).project).toEqual(imported.project);
        await select(); await page.getByRole("button", { name: "Edit text", exact: true }).click();
        const form = page.getByRole("form", { name: "Edit text", exact: true });
        await expect(form.getByRole("combobox", { name: prefix("Font family") })).toHaveValue(facts.font.id);
        await expect(form.getByRole("combobox", { name: prefix("Font weight") })).toHaveValue("700");
        await expect(form.getByRole("combobox", { name: prefix("Font style") })).toHaveValue("italic");
        await expect(form.getByRole("checkbox", { name: "Enable text background", exact: true })).toBeChecked();
        await form.getByRole("textbox", { name: prefix("Text content") }).fill("Legacy\nEdited subtitle");
        await form.getByRole("combobox", { name: prefix("Text decoration") }).selectOption("underline");
        for (const [label, value] of [["Letter spacing (pixels)", "2"], ["Line height", "1.5"], ["Background corner radius (%)", "45"], ["Background horizontal padding (Classic size 15)", "9"], ["Background vertical padding (Classic size 15)", "12"], ["Background horizontal offset (pixels)", "-4"], ["Background vertical offset (pixels)", "5"]]) await form.getByRole("textbox", { name: prefix(label) }).fill(value);
        const planned = await exchange("opencut_text_v1_plan", () => form.getByRole("button", { name: "Save text", exact: true }).click(), "edit_plan");
        const expected = structuredClone(original);
        Object.assign(expected.content.text, { content: "Legacy\nEdited subtitle", decoration: "underline", letter_spacing_px: 2, line_height_multiplier: 1.5 });
        Object.assign(expected.content.text.background, { corner_radius_percent: 45, padding_x_at_size15: 9, padding_y_at_size15: 12, offset_x_px: -4, offset_y_px: 5 });
        await save({ expected_edited: expected });
        expect(planned.request.expected_revision).toBe((await inspect()).revision);
        expect(planned.envelope.data.transaction.operations).toEqual([{ type: "element_put", sequence_id: "scene", track_id: "classic-subtitle-track", element: expected }]);
        await exchange("opencut_edit_v4_apply", () => page.getByRole("region", { name: "Review text changes", exact: true }).getByRole("button", { name: "Apply text changes", exact: true }).click(), "edited");
      }
    }
    expect(facts.edited.request).toEqual(facts.edit_plan.envelope.data.transaction);
    const styled = await inspect(facts.edited.envelope.data.revision);
    expect(element(styled)).toEqual(facts.expected_edited);
    const expectedProject = structuredClone(imported.project);
    scene({ project: expectedProject }).tracks.find(item => item.id === "classic-subtitle-track").elements = [facts.expected_edited];
    expect(styled.project).toEqual(expectedProject);
    await save({ styled_revision: styled.revision });
    if (!facts.history_passed) {
      phase = "reopen Undo/Redo original styled subtitle";
      await open();
      const history = page.locator(".v4-history");
      for (const [direction, expectedGraph] of [["undo", imported.project], ["redo", styled.project]]) {
        const key = `history_${direction}`;
        if (!facts[key]?.envelope?.data) {
          if (facts[`${key}_attempted`] && facts[`${key}_plan`]?.request) await recoverApply(`opencut_edit_v4_${direction}_apply`, key, facts[`${key}_plan`].request);
          else {
            await exchange(`opencut_edit_v4_${direction}_plan`, () => history.getByRole("button", { name: direction === "undo" ? "Undo" : "Redo", exact: true }).click(), `${key}_plan`);
            await exchange(`opencut_edit_v4_${direction}_apply`, () => history.getByRole("button", { name: `Apply ${direction}`, exact: true }).click(), key);
          }
        }
        expect(facts[key].request).toEqual(facts[`${key}_plan`].request);
        expect((await inspect(facts[key].envelope.data.revision)).project).toEqual(expectedGraph);
      }
      expect((await inspect()).project).toEqual(styled.project); await open();
      await save({ history_passed: true });
    }
    if (!facts.duplicate?.envelope?.data) {
      phase = "duplicate original subtitle style through UI";
      if (facts.duplicate_attempted && facts.duplicate_plan?.envelope?.data) await recoverApply("opencut_edit_v4_apply", "duplicate", facts.duplicate_plan.envelope.data.transaction);
      else {
        await select();
        await page.getByLabel("Playhead (seconds)", { exact: true }).fill("0"); await page.getByRole("button", { name: "Go to time", exact: true }).click();
        await exchange("opencut_timeline_v4_paste_plan", () => page.locator('[aria-label="Timeline clipboard"]').getByRole("button", { name: "Duplicate elements", exact: true }).click(), "duplicate_plan");
        await exchange("opencut_edit_v4_apply", () => page.getByRole("dialog", { name: "Review paste", exact: true }).getByRole("button", { name: "Apply copied elements", exact: true }).click(), "duplicate");
        const dialog = page.getByRole("dialog", { name: "Review paste", exact: true });
        await expect(dialog.getByRole("status")).toHaveText("Pasted elements saved."); await dialog.getByRole("button", { name: "Done", exact: true }).click();
      }
    }
    expect(facts.duplicate.request).toEqual(facts.duplicate_plan.envelope.data.transaction);
    const duplicated = await inspect(facts.duplicate.envelope.data.revision), duplicateId = facts.duplicate_plan.envelope.data.decision.selection[0].element_id;
    const copy = scene(duplicated).tracks.flatMap(item => item.elements).find(item => item.id === duplicateId);
    await save({ duplicate_revision: duplicated.revision, duplicate_id: duplicateId, observed_duplicate: copy });
    expect(copy.content).toEqual(facts.expected_edited.content); expect(copy.visual).toEqual(facts.expected_edited.visual);
    expect(copy.source_span).toEqual(facts.expected_edited.source_span); expect(copy.channels).toEqual(facts.expected_edited.channels);
    expect(copy.timeline_range).toEqual(facts.expected_edited.timeline_range);
    expect(element(duplicated)).toEqual(facts.expected_edited);
    await open(); expect((await inspect()).project).toEqual(duplicated.project);
    const sourceNow = await tool("opencut_project_v4_inspect", { project_id: sourceProject, revision: null });
    expect(sourceNow.revision).toBe(facts.authority_revision);
    await save({ forbidden_count: forbiddenCount, forbidden_calls: forbiddenCalls });
    expect(forbiddenCount).toBe(0); expect(errors).toEqual([]);
    await save({ status: "passed", scope: "Classic generated subtitle TextElement → V4 existing full text style editor → durable edit/reopen/Undo/Redo/Duplicate. No V4 CaptionStyle full style/ASS/render claim." });
    await fs.writeFile(path.join(directory, "classic-v4-subtitle-style.json"), JSON.stringify(facts, null, 2) + "\n");
    check("Classic subtitle TextElement full-style editing compatibility", facts);
  } catch (error) {
    await fs.writeFile(path.join(directory, `classic-v4-subtitle-style-failure-${Date.now()}.json`), JSON.stringify({ phase, error: String(error), facts, errors, network, forbiddenCount, forbiddenCalls, ui: (await page.locator("body").innerText().catch(() => "")).slice(0, 64000) }, null, 2));
    throw error;
  } finally { await page.close(); }
}
