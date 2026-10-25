import { editorSnapshot } from "./project-inspection.mjs";
// Root owns browser/service lifecycle. This module only defines the scenario;
// importing it does not acquire fonts, copy fixtures, submit jobs or launch tools.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import { constants } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomUUID } from "node:crypto";

const W = 640, H = 360, SIZE = 48, TEXT = "AV To fj 0123";
const PROVIDER = "opencut-chromium-font-synthesis-v1";
const CACHE = "/private/tmp/opencut-b3-font-cache";
const STATIC = "google-lato-regular-static-v1-synthetic-italic-v1";
const DERIVED = "google-assistant-regular-derived-v2-derived-v2-synthetic-italic-v1";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const time = (n) => ({ ticks: String(n), timebase: 2 });
const gray = (n) => ({ encoding: "srgb", red: n, green: n, blue: n, alpha: 1 });
const readJson = async (file) => JSON.parse(await fs.readFile(file, "utf8"));
export { sourceLocations as fixedFontCacheLocations, pinnedFile as readFixedFontCache };
const canonical = (value) => JSON.stringify(sort(value));
function sort(value) {
  if (Array.isArray(value)) return value.map(sort);
  if (value && typeof value === "object")
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, sort(value[key])]));
  return value;
}
async function optionalJson(file) {
  try { return await readJson(file); }
  catch (error) { if (error.code !== "ENOENT") throw error; return null; }
}
async function candidateIdentity(browser) {
  const root = process.env.OPENCUT_WEB_ROOT;
  const binary = process.env.OPENCUT_WEB_BINARY;
  const chrome = process.env.OPENCUT_CHROME;
  if (!root || !binary || !chrome)
    throw Error("Font synthesis acceptance requires explicit OPENCUT_WEB_ROOT, OPENCUT_WEB_BINARY and OPENCUT_CHROME");
  expect(browser.browserType().name()).toBe("chromium");
  const files = [];
  async function walk(relative = "") {
    for (const entry of await fs.readdir(path.join(root, relative), { withFileTypes: true })) {
      const name = path.join(relative, entry.name);
      if (entry.isSymbolicLink()) throw Error(`Unhashed candidate symlink: ${name}`);
      if (entry.isDirectory()) await walk(name);
      else if (/\.(?:js|mjs|cjs|css)$/u.test(name)) files.push([name, sha(await fs.readFile(path.join(root, name)))]);
    }
  }
  await walk();
  expect(files.length).toBeGreaterThan(0);
  files.sort(([a], [b]) => a.localeCompare(b));
  const repository = fileURLToPath(new URL("../../../", import.meta.url));
  return {
    binary_sha256: sha(await fs.readFile(binary)),
    web_js_css_tree_sha256: sha(JSON.stringify(files)),
    chrome_sha256: sha(await fs.readFile(chrome)),
    chrome_version: browser.version(),
    scenario_sha256: sha(await fs.readFile(fileURLToPath(import.meta.url))),
    classic_loader_sha256: sha(await fs.readFile(path.join(repository, "tmp/opencut-classic/apps/web/src/fonts/google-fonts.ts"))),
  };
}
// This only maps immutable reviewed source URLs into the existing external
// cache. It never downloads, copies, changes metadata, or uses a system font.
function sourceLocations(cache, url, commit) {
  expect(commit).toMatch(/^[a-f0-9]{40}$/u);
  const parsed = new URL(url);
  expect(parsed.origin).toBe("https://raw.githubusercontent.com");
  expect(parsed.search + parsed.hash).toBe("");
  const pieces = decodeURIComponent(parsed.pathname).split("/").filter(Boolean);
  const [owner, repository, revision, ...relative] = pieces;
  expect(revision).toBe(commit);
  expect(relative.length).toBeGreaterThan(1);
  for (const part of relative) {
    if (!part || part === "." || part === ".." || /[\\\x00-\x1f]/u.test(part)) throw Error("Unsafe fixed font source path");
  }
  if (owner === "google" && repository === "fonts")
    return [path.join(cache, "fixed", commit, ...relative), path.join(cache, ...relative)];
  if (owner === "google" && repository === "material-design-icons")
    return [path.join(cache, "official/google-material-design-icons", commit, ...relative)];
  throw Error("This acceptance helper has no independent mapping for the fixed source repository");
}
async function pinnedFile(cache, locations, expectedHash, expectedSize) {
  expect(expectedHash).toMatch(/^[a-f0-9]{64}$/u);
  const size = Number(expectedSize);
  expect(Number.isSafeInteger(size) && size > 0 && size <= 64 * 1024 * 1024).toBe(true);
  for (const file of locations) {
    const relative = path.relative(cache, file);
    if (relative.startsWith("..") || path.isAbsolute(relative)) throw Error("Font cache path escaped its fixed root");
    try {
      let current = cache;
      for (const piece of relative.split(path.sep)) {
        current = path.join(current, piece);
        if ((await fs.lstat(current)).isSymbolicLink()) throw Error(`Font cache symlink: ${current}`);
      }
      const handle = await fs.open(file, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
      try {
        const stat = await handle.stat();
        expect(stat.isFile()).toBe(true);
        expect(stat.size).toBe(size);
        const bytes = await handle.readFile();
        expect(bytes.length).toBe(size);
        expect(sha(bytes), `Pinned original font bytes changed: ${file}`).toBe(expectedHash);
        return { file, bytes, sha256: expectedHash, size };
      } finally { await handle.close(); }
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  throw Error(`Required fixed font cache is missing: ${locations.join(" or ")}; this is not a passed fixture-required case`);
}
function validateSource(source, route) {
  const { binding, source: outer } = source;
  expect(binding).toMatchObject({ schema_version: 1, profile: "classic_chromium_italic_v1", actual: { style: "normal" }, requested: { style: "italic" } });
  expect([400, 700]).toContain(binding.actual.weight);
  expect(binding.requested.weight).toBe(binding.actual.weight);
  const proof = JSON.parse(outer.proof_json);
  expect(canonical(proof)).toBe(outer.proof_json);
  expect(proof).toMatchObject({ schema_version: 3, kind: "browser_font_synthesis", provider: PROVIDER, source_bytes_unchanged: true, actual: binding.actual, requested: binding.requested });
  const unsigned = { ...proof }; delete unsigned.derivation_id;
  expect(binding.binding_id).toBe(`font-synthesis-v1_${sha(canonical(unsigned))}`);
  expect(proof.derivation_id).toBe(binding.binding_id);
  expect(outer.face).toMatchObject({ family: binding.actual.family, weight: binding.actual.weight, style: "normal", derivation_id: binding.binding_id, content_hash: binding.actual.content_hash, size_bytes: binding.actual.size_bytes });
  expect(outer.face.id).not.toBe(binding.actual.id);
  expect(outer.source_sha256).toBe(binding.actual.content_hash);
  expect(outer.source_size_bytes).toBe(binding.actual.size_bytes);
  expect(sha(outer.license_notice)).toBe(proof.license_sha256);
  if (route === "derived_v2") {
    expect(proof.base?.route).toBe("derived_v2");
    expect(proof.base.render_profile).toBe("static_outline");
    expect(binding.actual.family).toMatch(/^OCDF [a-f0-9]+$/u);
    expect(proof.base.source.face.content_hash).toBe(binding.actual.content_hash);
    expect(proof.base.source.face.derivation_id).toMatch(/^font-derived-v2_[a-f0-9]{64}$/u);
  } else {
    expect(proof.base?.route ?? "static_v1").toBe("static_v1");
    expect(proof.base?.render_profile ?? "static_outline").toBe("static_outline");
  }
  return proof;
}
async function independentFont(source, route, cache) {
  const proof = validateSource(source, route), actual = source.binding.actual;
  if (route === "static_v1") {
    const output = await pinnedFile(cache, sourceLocations(cache, actual.source_url, actual.source_commit), actual.content_hash, actual.size_bytes);
    return { output, provenance: { route, source_url: actual.source_url, source_commit: actual.source_commit, file: output.file, sha256: output.sha256, size: output.size } };
  }
  const base = proof.base.source, baseProof = JSON.parse(base.proof_json);
  expect(baseProof.source.render_profile).toBe("static_outline");
  expect(baseProof.output_sha256).toBe(actual.content_hash);
  expect(String(baseProof.output_size)).toBe(actual.size_bytes);
  const raw = await pinnedFile(cache, sourceLocations(cache, base.source_url, base.source_commit), base.source_sha256, base.source_size_bytes);
  expect(decodeURIComponent(new URL(base.source_url).pathname)).toBe(`/google/fonts/${base.source_commit}/${baseProof.source.source_path}`);
  const output = await pinnedFile(cache, [path.join(cache, "derived", actual.content_hash)], actual.content_hash, actual.size_bytes);
  return { output, provenance: { route, source_url: base.source_url, source_commit: base.source_commit, raw_file: raw.file, raw_sha256: raw.sha256, raw_size: raw.size, file: output.file, sha256: output.sha256, size: output.size, base_derivation_id: base.face.derivation_id, base_proof_sha256: sha(base.proof_json) } };
}
function authoredScene(original, cases) {
  const sequence = structuredClone(original);
  const main = structuredClone(sequence.tracks.find((track) => track.id === sequence.main_track_id));
  expect(main?.kind).toBe("video");
  Object.assign(main, { elements: [], channels: [], enabled: true, locked: false, muted: true, opacity: 1 });
  const track = (id, kind) => ({ id, name: id, kind, enabled: true, muted: false, locked: false, opacity: 1, gain: { unit: "decibels", value: 0 }, channels: [], elements: [] });
  const titles = track("synthesis-titles", "text"), masks = track("synthesis-masks", "graphic");
  for (const [index, c] of cases.entries()) {
    const common = { enabled: true, timeline_range: { start: time(index * 2), duration: time(2) }, source_span: { profile: "classic_trimmed", trim_start: time(0), trim_end: time(0), source_duration: null }, channels: [] };
    const style = { profile: "classic_canvas_v31", transform: { position_x_px: 0, position_y_px: 0, scale_x: 1, scale_y: 1, rotation_degrees: 0 }, opacity: 1, blend_mode: "normal", effects: [], masks: [] };
    const font = { kind: "locked", font_id: c.font_id }, font_weight = c.source.binding.actual.weight === 700 ? "bold" : "normal";
    titles.elements.push({ ...structuredClone(common), id: c.title_id, name: `${c.label} italic title`, visual: { ...structuredClone(style), transform: { ...style.transform, position_y_px: -90 } }, content: { kind: "text", text: { content: TEXT, font, font_weight, font_style: "italic", decoration: "none", align: "center", font_size: { unit: "classic_height90", value: 12 }, color: gray(1), letter_spacing_px: 0, line_height_multiplier: 1.2, background: { enabled: false, color: gray(0), corner_radius_percent: 0, padding_x_at_size15: 0, padding_y_at_size15: 0, offset_x_px: 0, offset_y_px: 0 } } } });
    masks.elements.push({ ...structuredClone(common), id: c.graphic_id, name: `${c.label} shared font mask`, content: { kind: "graphic", graphic: { source_size: { width: W, height: H }, fill: gray(1), stroke: { color: gray(0), width_px: 0, align: "inside" }, geometry: { kind: "rectangle", corner_radius_percent: 0 } } }, visual: { ...structuredClone(style), masks: [{ id: c.mask_id, enabled: true, inverted: false, feather: { unit: "classic_pixels", value: 0 }, stroke: { width_px: 0, align: "center", color: gray(0) }, shape: { kind: "text", mask: { content: TEXT, font, font_size: { unit: "classic_height90", value: 12 }, font_weight, font_style: "italic", decoration: "none", letter_spacing_px: 0, line_height_multiplier: 1.2, center_x: 0, center_y: 0.25, rotation_degrees: 0, scale: 1 } } }] } });
  }
  sequence.tracks = [main, masks, titles];
  sequence.captions = []; sequence.transitions = [];
  return sequence;
}
// Independent authority: only normal font bytes, authored constants and native
// Canvas APIs enter here. Product plans/layout metrics/raster code do not.
async function independentPixels(page, png, bytes, weight) {
  return page.evaluate(async ({ png, font, weight, width, height, size, content }) => {
    const family = "SynthesisIndependentNormalSource";
    const face = new FontFace(family, Uint8Array.from(atob(font), (c) => c.charCodeAt(0)), { weight: String(weight), style: "normal" });
    await face.load(); document.fonts.add(face);
    try {
      const make = (style) => {
        const canvas = new OffscreenCanvas(width, height), ctx = canvas.getContext("2d");
        if (!ctx || !("letterSpacing" in ctx)) throw Error("Independent Canvas text capability missing");
        ctx.fillStyle = "black"; ctx.fillRect(0, 0, width, height);
        ctx.font = `${style} ${weight} ${size}px "${family}"`;
        if (!ctx.font.includes(family)) throw Error("Independent fixed font request was rejected");
        ctx.textAlign = "center"; ctx.textBaseline = "middle"; ctx.letterSpacing = "0px"; ctx.fillStyle = "white";
        const metrics = ctx.measureText(content);
        for (const y of [90, 270]) ctx.fillText(content, width / 2, y);
        return { raw: ctx.getImageData(0, 0, width, height).data, font: ctx.font, width: metrics.width };
      };
      const expected = make("italic"), negative = make("normal");
      const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(png), (c) => c.charCodeAt(0))], { type: "image/png" }));
      if (bitmap.width !== width || bitmap.height !== height) throw Error("Product PNG dimensions changed");
      const canvas = new OffscreenCanvas(width, height), ctx = canvas.getContext("2d");
      ctx.drawImage(bitmap, 0, 0); bitmap.close();
      const actual = ctx.getImageData(0, 0, width, height).data;
      const digest = async (data) => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", data)), (v) => v.toString(16).padStart(2, "0")).join("");
      function measure(a, b, fromY, toY) {
        let intersection = 0, union = 0, error = 0, maximum = 0, foregroundError = 0, differing = 0;
        for (let y = fromY; y < toY; y++) for (let x = 0; x < width; x++) {
          const i = (y * width + x) * 4, aa = a[i] > 32, bb = b[i] > 32;
          intersection += Number(aa && bb); union += Number(aa || bb);
          if (a[i] !== b[i]) differing++;
          for (let k = 0; k < 4; k++) { const delta = Math.abs(a[i + k] - b[i + k]); error += delta; maximum = Math.max(maximum, delta); if (aa || bb) foregroundError += delta; }
        }
        return { intersection, union, iou: union ? intersection / union : 0, mean_channel_error: error / ((toY - fromY) * width * 4), foreground_mean_channel_error: union ? foregroundError / (union * 4) : 0, max_channel_error: maximum, differing_pixels: differing };
      }
      return { width, height, actual_descriptor: { weight: face.weight, style: face.style }, requested_font: expected.font, independent_text_width: expected.width, rgba_sha256: await digest(actual), expected_rgba_sha256: await digest(expected.raw), normal_negative_rgba_sha256: await digest(negative.raw), title: measure(actual, expected.raw, 0, 180), mask: measure(actual, expected.raw, 180, height), normal_negative: measure(expected.raw, negative.raw, 0, height) };
    } finally { document.fonts.delete(face); }
  }, { png: png.toString("base64"), font: bytes.toString("base64"), weight, width: W, height: H, size: SIZE, content: TEXT });
}

/** Run only inside the integrator's existing authenticated synthetic workspace.
 * options.sourceDirectory selects its existing B4/B1 checkpoint directory;
 * staticCatalogId may explicitly select the original Gudea700 source instead.
 * Both normal static + V2 OCDF cases are mandatory; missing inputs throw. */
export async function verifyFontSynthesis(browser, endpoint, directory, check, options = {}) {
  const candidate = await candidateIdentity(browser);
  const checkpoint = path.join(directory, "f20-font-synthesis-checkpoint.json");
  const sourceDirectory = options.sourceDirectory ?? directory;
  const cache = await fs.realpath(options.fontCache ?? CACHE);
  const selection = [{ label: "static", route: "static_v1", catalog_id: options.staticCatalogId ?? STATIC }, { label: "ocdf-v2", route: "derived_v2", catalog_id: options.derivedCatalogId ?? DERIVED }];
  let facts = await optionalJson(checkpoint);
  if (facts) { expect(facts.candidate).toEqual(candidate); expect(facts.selection).toEqual(selection); expect(facts.cache).toBe(cache); }
  else facts = { schema_version: 1, candidate, selection, cache, project_id: randomUUID(), stages: {}, cases: [], phase: "initial", status: "running" };
  const save = async () => { await fs.writeFile(`${checkpoint}.next`, JSON.stringify(facts, null, 2) + "\n"); await fs.rename(`${checkpoint}.next`, checkpoint); };
  await save();
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [], writes = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (request) => { if (request.url().endsWith("/api/tools/opencut_edit_apply")) writes.push(request.postDataJSON()); });
  const tool = async (name, input) => {
    const result = await page.evaluate(async ({ name, input }) => {
      const response = await fetch(`/api/tools/${name}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) });
      return { status: response.status, envelope: await response.json() };
    }, { name, input });
    expect(result.envelope.error, JSON.stringify(result)).toBeNull();
    return result.envelope.data;
  };
  const operation = async (stage, name, build) => {
    if (!facts.stages[stage]) { facts.stages[stage] = { name, input: await build() }; await save(); }
    const record = facts.stages[stage]; expect(record.name).toBe(name);
    if (!record.data) { record.data = await tool(name, record.input); await save(); }
    return record.data;
  };
  const inspect = (id = facts.project_id, revision = null) => editorSnapshot(tool("opencut_project_inspect", { project_id: id, revision }));
  try {
    await page.goto(`${endpoint}/projects`);
    await expect(page.getByRole("button", { name: "Restore project package", exact: true })).toBeEnabled();
    if (!facts.source) {
      const b4File = path.join(sourceDirectory, "b4-graphic-output-checkpoint.json");
      const b4 = await optionalJson(b4File);
      const file = b4 ? b4File : path.join(sourceDirectory, "b1-text-project.json");
      const seed = b4 ?? await readJson(file);
      const original = await inspect(seed.project_id, b4 ? seed.revision : null);
      expect(original.project.id).toBe(seed.project_id);
      if (b4) expect(original.revision).toBe(seed.revision);
      facts.source = { checkpoint: file, checkpoint_sha256: sha(await fs.readFile(file)), project_id: original.project.id, revision: original.revision, project: original.project };
      facts.sequence_id = b4?.sequence_id ?? original.project.default_sequence_id;
      facts.source_head_before = await inspect(original.project.id);
      await save();
    }
    // Reuse the source revision without changing its head, resource collection or files.
    expect((await inspect(facts.source.project_id, facts.source.revision)).project).toEqual(facts.source.project);
    await operation("copy", "opencut_project_copy", () => ({ source_project_id: facts.source.project_id, source_revision: facts.source.revision, project_id: facts.project_id, name: "Explicit italic synthesis acceptance", idempotency_key: `synthesis-copy-${facts.project_id}` }));
    for (const [index, selected] of selection.entries()) {
      let c = facts.cases[index];
      const selectedSource = await tool("opencut_font_source", { catalog_id: selected.catalog_id });
      expect(selectedSource.details.kind).toBe("synthesis");
      const source=selectedSource.details.source;
      if (c) expect(c.source).toEqual(source);
      else { c = { ...selected, source, font_id: randomUUID(), title_id: randomUUID(), graphic_id: randomUUID(), mask_id: randomUUID(), time: time(index * 2 + 1) }; facts.cases.push(c); await save(); }
      facts.phase = `source-${c.label}`; await save();
      const independent = await independentFont(source, c.route, cache);
      if (c.provenance) expect(c.provenance).toEqual(independent.provenance);
      c.provenance = independent.provenance; await save();
      const stage = `import-${c.label}`;
      c.import_receipt = await operation(stage, "opencut_font_import", async () => ({ project_id: facts.project_id, expected_revision: (await inspect()).revision, idempotency_key: `synthesis-import-${facts.project_id}-${c.label}`, catalog_id: c.catalog_id, font_id: c.font_id, required_text: TEXT }));
      expect(await tool("opencut_font_import", facts.stages[stage].input)).toEqual(c.import_receipt);
      const imported = await inspect(facts.project_id, c.import_receipt.revision);
      const resource = imported.project.fonts.find((font) => font.id === c.font_id);
      expect(resource).toMatchObject({ family: source.binding.actual.family, weight: source.binding.actual.weight, style: "normal", face_index: 0, content: { content_hash: independent.output.sha256, size: independent.output.size }, origin: { kind: "generated", provider: PROVIDER, generation_id: source.binding.binding_id } });
      const proof = await tool("opencut_font_synthesis_proof", { project_id: facts.project_id, revision: imported.revision, font_id: c.font_id });
      expect(proof).toEqual({ project_id: facts.project_id, revision: imported.revision, font_id: c.font_id, source });
      c.proof = proof;
      const descriptor = await tool("opencut_project_resource_inspect", { project_id: facts.project_id, revision: imported.revision, kind: "font", resource_id: c.font_id });
      const query = (value) => encodeURIComponent(value).replace(/[!'()*]/g, (v) => `%${v.charCodeAt(0).toString(16).toUpperCase()}`);
      const contentPath = `/api/project-resources/content?project_id=${query(facts.project_id)}&revision=${query(imported.revision)}&kind=font&resource_id=${query(c.font_id)}`;
      expect(descriptor.content_path).toBe(contentPath);
      const response = await page.request.get(`${endpoint}${contentPath}`);
      expect(response.ok()).toBe(true);
      const bytes = await response.body();
      expect(bytes.equals(independent.output.bytes)).toBe(true);
      c.resource_bytes = { descriptor, sha256: sha(bytes), size: bytes.length, equals_original_normal_output: true };
      await save();
    }
    const originalScene = facts.source.project.sequences.find((scene) => scene.id === facts.sequence_id);
    expect(originalScene).toBeTruthy();
    const sequence = authoredScene(originalScene, facts.cases), settings = structuredClone(facts.source.project.settings);
    expect(settings.profile).toBe("classic_global");
    settings.settings.canvas_size = { width: W, height: H }; settings.settings.background = { kind: "color", color: gray(0) };
    const setup = await operation("setup", "opencut_edit_apply", async () => ({ project_id: facts.project_id, expected_revision: (await inspect()).revision, idempotency_key: `synthesis-setup-${facts.project_id}`, label: "Explicit normal source and requested italic", operations: [{ type: "settings_set", settings }, { type: "sequence_put", sequence }] }));
    facts.revision = setup.revision; await save();
    const snapshot = await inspect();
    expect(snapshot.revision).toBe(facts.revision);
    expect(snapshot.project.sequences.find((s) => s.id === sequence.id)).toEqual(sequence);
    expect(snapshot.project.assets).toEqual(facts.source.project.assets);
    for (const c of facts.cases) {
      facts.phase = `canvas-and-png-${c.label}`; await save();
      const independent = await independentFont(c.source, c.route, cache);
      const plan = await tool("opencut_text_canvas_plan", { project_id: facts.project_id, revision: facts.revision, sequence_id: facts.sequence_id, element_id: c.title_id, time: c.time });
      expect(plan).toMatchObject({ identity: { profile: "classic-canvas-text", projectId: facts.project_id, revision: facts.revision, sequenceId: facts.sequence_id, time: c.time }, layer: { elementId: c.title_id, font: { fontId: c.font_id, contentHash: c.source.binding.actual.content_hash, weight: c.source.binding.actual.weight, style: "normal", glyphCoverageValidated: true } }, font_synthesis: c.source.binding });
      c.canvas_plan = plan; await save();
      if (!c.canvas_ui) {
        await page.goto(`${endpoint}/editor/${encodeURIComponent(facts.project_id)}?scene=${encodeURIComponent(facts.sequence_id)}`);
        await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
        const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
        if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
        await page.getByLabel("Playhead (seconds)", { exact: true }).fill(String(Number(c.time.ticks) / c.time.timebase));
        await page.getByRole("button", { name: "Go to time", exact: true }).click();
        const before = writes.length;
        await page.getByRole("button", { name: `Edit text on canvas: ${TEXT}`, exact: true }).dblclick();
        const input = page.getByRole("textbox", { name: "Edit text on canvas", exact: true });
        await expect(input).toBeVisible();
        await expect(input).toHaveValue(TEXT);
        const observation = await input.evaluate((node) => {
          const style = getComputedStyle(node), family = style.fontFamily.replaceAll('"', "");
          return { style: style.fontStyle, weight: style.fontWeight, family, faces: Array.from(document.fonts).filter((face) => face.family.replaceAll('"', "") === family).map((face) => ({ family: face.family, style: face.style, weight: face.weight, status: face.status })) };
        });
        expect(observation.style).toBe("italic"); expect(observation.weight).toBe(String(c.source.binding.actual.weight));
        expect(observation.family).toBe(`OpenCutCanvas_${c.source.binding.actual.content_hash}`);
        expect(observation.faces.length).toBeGreaterThan(0);
        for (const face of observation.faces) expect(face).toMatchObject({ style: "normal", weight: String(c.source.binding.actual.weight), status: "loaded" });
        await input.press("Escape"); await expect(input).toHaveCount(0);
        expect(writes.length).toBe(before);
        expect((await inspect()).revision).toBe(facts.revision);
        c.canvas_ui = { ...observation, cancelled_without_write: true }; await save();
      }
      const stage = `png-${c.label}`;
      const submission = await operation(stage, "opencut_render_submit", () => ({ kind: "frame", request: ({ project_id: facts.project_id, revision: facts.revision, sequence_id: facts.sequence_id, time: c.time, idempotency_key: `synthesis-png-${facts.project_id}-${c.label}` }) }));
      let job;
      await expect.poll(async () => { job = await tool("opencut_job_view", { job_id: submission.job_id }); return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending"; }, { timeout: 180000, intervals: [250, 500, 1000] }).not.toBe("pending");
      c.job = job; await save();
      expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      expect(job.result.kind).toBe("render.frame");
      const result = job.result.data;
      expect(result.identity).toMatchObject({ project_id: facts.project_id, revision: facts.revision, sequence_id: facts.sequence_id, time: c.time, width: W, height: H, profile: "classic-v31-gpu-scene-frame-v1" });
      expect(result.execution.browser_sha256).toBe(candidate.chrome_sha256);
      expect(result.execution.browser_version).toBe(candidate.chrome_version);
      for (const field of ["host_sha256", "module_sha256", "shader_sha256"]) expect(result.execution[field]).toMatch(/^[a-f0-9]{64}$/u);
      expect(result.font_lock).toEqual([{ font_id: c.font_id, content_hash: c.source.binding.actual.content_hash, size: Number(c.source.binding.actual.size_bytes) }]);
      expect(result.texts).toHaveLength(1);
      expect(result.stacks.find((stack) => stack.elementId === c.graphic_id)?.masks[0]?.shape.mask.font.font_id).toBe(c.font_id);
      const artifact = job.artifacts.find((a) => a.mime_type === "image/png"); expect(artifact).toBeTruthy();
      expect(artifact.artifact_id).toBe(`artifact:${job.job_id}:${artifact.content_hash}`);
      const file = path.join(directory, `f20-synthesis-${c.label}-${job.job_id}.png`);
      let png;
      try { png = await fs.readFile(file); expect(sha(png)).toBe(artifact.content_hash); }
      catch (error) { if (error.code !== "ENOENT") throw error; const response = await page.request.get(`${endpoint}/api/artifacts/${encodeURIComponent(artifact.artifact_id)}`); expect(response.ok()).toBe(true); png = await response.body(); expect(sha(png)).toBe(artifact.content_hash); await fs.writeFile(file, png); }
      c.output = { file, sha256: sha(png), artifact };
      c.oracle = await independentPixels(page, png, independent.output.bytes, c.source.binding.actual.weight); await save();
      expect(c.oracle.actual_descriptor).toEqual({ weight: String(c.source.binding.actual.weight), style: "normal" });
      expect(c.oracle.requested_font).toContain("italic");
      for (const region of [c.oracle.title, c.oracle.mask]) {
        expect(region.union).toBeGreaterThan(500);
        expect(region.iou).toBeGreaterThan(0.99);
        expect(region.mean_channel_error).toBeLessThan(0.5);
        expect(region.foreground_mean_channel_error).toBeLessThan(2);
        expect(region.max_channel_error).toBeLessThanOrEqual(3);
      }
      expect(c.oracle.normal_negative.iou).toBeLessThan(0.95);
      expect(c.oracle.normal_negative.differing_pixels).toBeGreaterThan(500);
      expect(result.texts[0].layout.lines[0].width).toBeCloseTo(c.oracle.independent_text_width, 4);
      c.passed = true; await save();
    }
    expect(await inspect(facts.source.project_id)).toEqual(facts.source_head_before);
    expect(errors).toEqual([]);
    facts.phase = "passed"; facts.status = "passed";
    facts.scope = "Two exact cached normal sources, same-weight italic, Canvas V2 descriptors and shared title/text-mask Scene PNG independent pixels. No MP4, all-font catalog, cross-browser or performance claim.";
    await save();
    check("F20 explicit normal-source italic synthesis: static + V2 OCDF, Canvas V2 and independent shared-font Scene PNG", facts);
    return facts;
  } catch (error) {
    facts.status = "failed"; facts.failure = { phase: facts.phase, message: String(error).slice(0, 6000), pageerrors: errors.slice(-20) }; await save();
    await page.screenshot({ path: path.join(directory, "f20-font-synthesis-failure.png"), fullPage: false }).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
