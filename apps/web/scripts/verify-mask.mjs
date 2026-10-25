import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { MASK_ORACLE_MODEL, makeMaskReference } from "./mask-oracle.mjs";
import { BLEND_ORACLE_MODEL, compositeClassicRgba8 } from "./blend-oracle.mjs";
import { installScenePreviewWitness } from "./verify-scene-preview.mjs";
import { verifyClassicMaskImport } from "./verify-classic-mask.mjs";

const execute = promisify(execFile), sha = bytes => createHash("sha256").update(bytes).digest("hex");
const read = async file => JSON.parse(await fs.readFile(file, "utf8"));
const W = 320, H = 180, PROFILE = "classic-v31-gpu-scene-frame-v1";
const tiles = [
  { x: 80, y: 45, kind: "rectangle", feather: 0, back: 32, front: 224, mode: "normal" },
  { x: 240, y: 45, kind: "ellipse", feather: 0, back: 32, front: 224, mode: "normal" },
  { x: 80, y: 135, kind: "rectangle", feather: 24, back: 32, front: 224, mode: "normal" },
  { x: 240, y: 135, kind: "ellipse", feather: 24, back: 192, front: 96, mode: "multiply" },
];
const gray = n => ({ encoding: "srgb", red: n / 255, green: n / 255, blue: n / 255, alpha: 1 });
const geometry = (extent = 0.5) => ({ center_x: 0, center_y: 0, width: extent, height: extent, rotation_degrees: 0, scale: 1 });
const elementIn = (project, sequenceId, id) => project.sequences.find(s => s.id === sequenceId).tracks.flatMap(t => t.elements).find(e => e.id === id);
const maskFor = (tile, id = "independent") => ({ id, enabled: true, inverted: false,
  shape: { kind: tile.kind, geometry: geometry(tile.extent ?? 0.5) },
  feather: { unit: "classic_pixels", value: tile.feather },
  stroke: { color: gray(255), width_px: 0, align: "center" } });

function tileTracks(template) {
  return tiles.flatMap((tile, i) => ["back", "front"].map(part => {
    const track = structuredClone(template), element = structuredClone(template.elements[0]);
    track.id = `d2b-track-${part}-${i}`; track.name = `D2b ${tile.kind} ${tile.feather}px ${part}`;
    track.channels = []; track.opacity = 1; track.enabled = true; track.locked = false;
    const source = part === "front" ? { width: 96, height: 64 } : { width: 120, height: 80 };
    const contain = Math.min(W / source.width, H / source.height);
    element.id = `d2b-${part}-${i}`; element.name = track.name; element.channels = []; element.enabled = true;
    element.timeline_range = { start: { ticks: "0", timebase: 1 }, duration: { ticks: "3", timebase: 1 } };
    element.content.graphic = { geometry: { kind: "rectangle", corner_radius_percent: 0 }, source_size: source,
      fill: gray(tile[part]), stroke: { width_px: 0, align: "inside", color: gray(0) } };
    element.visual = { profile: "classic_canvas_v31", blend_mode: part === "front" ? tile.mode : "normal", opacity: 1,
      effects: [], masks: [], transform: { position_x_px: tile.x - W / 2, position_y_px: tile.y - H / 2,
        rotation_degrees: 0, scale_x: 1 / contain, scale_y: 1 / contain } };
    track.elements = [element];
    return track;
  }));
}

// Independent authored source facts, never product plans/bounds/raster receipts.
function references(tracks) {
  return tiles.map((tile, i) => {
    const e = tracks.find(t => t.id === `d2b-track-front-${i}`).elements[0], s = e.content.graphic.source_size, t = e.visual.transform;
    const contain = Math.min(W / s.width, H / s.height), sx = s.width * contain * t.scale_x, sy = s.height * contain * t.scale_y;
    const spec = { width: W, height: H, mask: maskFor(tile), quad: { centerX: W / 2 + t.position_x_px, centerY: H / 2 + t.position_y_px,
      width: Math.abs(sx), height: Math.abs(sy), rotationRadians: t.rotation_degrees * Math.PI / 180,
      flipX: sx < 0, flipY: sy < 0, opacity: e.visual.opacity } };
    const reference = makeMaskReference(spec);
    // Cardinal rows avoid Canvas/analytic scan-conversion disagreements at curved
    // AA pixels. No global threshold is relaxed and no product pixel selects probes.
    const offsets = tile.feather ? [-38, -35, -30, -25, -24, -19, -12, 0, 11, 18, 23, 24, 29, 35, 38] : [-34, -28, -20, 0, 19, 27, 33];
    const points = offsets.map(dx => [tile.x + dx, tile.y]);
    points.push([tile.x, tile.y - 27], [tile.x, tile.y + 26], [tile.x, tile.y - 10], [tile.x, tile.y + 9]);
    // These pixels have zero hard ellipse coverage, but full hard rectangle
    // coverage. They distinguish the two shapes; their feather alpha is exact
    // seed distance, not a sampled Canvas/JFA expected image.
    points.push([tile.x + 20, tile.y + 12], [tile.x - 21, tile.y - 13]);
    const samples = points.map(([x, y]) => {
      const alpha = reference.alpha[y * W + x];
      return { x, y, alpha, expected: [...compositeClassicRgba8([tile.back, tile.back, tile.back, 255], [tile.front, tile.front, tile.front, alpha], tile.mode)] };
    });
    return { element_id: e.id, spec, alpha_sha256: sha(reference.alpha), samples };
  });
}
function measure(raw, refs) {
  expect(raw.length).toBe(W * H * 4);
  return refs.map(ref => ({ element_id: ref.element_id, samples: ref.samples.map(p => {
    const actual = [...raw.subarray((p.y * W + p.x) * 4, (p.y * W + p.x) * 4 + 4)];
    return { ...p, actual, max_channel_error: Math.max(...actual.map((n, c) => Math.abs(n - p.expected[c]))) };
  }) }));
}
function assertPixels(observation, tolerance) {
  for (const tile of observation) for (const p of tile.samples)
    expect(p.max_channel_error, `${tile.element_id} (${p.x},${p.y}) alpha=${p.alpha} actual=${p.actual} expected=${p.expected}`).toBeLessThanOrEqual(tolerance);
}
async function decode(file, seconds) {
  return (await execute(process.env.OPENCUT_FFMPEG ?? "ffmpeg", ["-hide_banner", "-loglevel", "error",
    ...(seconds === null ? [] : ["-ss", String(seconds)]), "-i", file, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"],
  { encoding: "buffer", maxBuffer: 32 * 1024 * 1024 })).stdout;
}
async function candidateIdentity() {
  if (!process.env.OPENCUT_WEB_ROOT || !process.env.OPENCUT_WEB_BINARY) throw Error("Explicit OPENCUT_WEB_ROOT and OPENCUT_WEB_BINARY are required");
  const entries = [];
  async function walk(relative = "") {
    for (const e of await fs.readdir(path.join(process.env.OPENCUT_WEB_ROOT, relative), { withFileTypes: true })) {
      const name = path.join(relative, e.name);
      if (e.isSymbolicLink()) throw Error(`Unhashed web symlink ${name}`);
      if (e.isDirectory()) await walk(name);
      else if (/\.(js|mjs|css)$/u.test(name)) entries.push([name, sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name)))]);
    }
  }
  await walk(); entries.sort((a, b) => a[0].localeCompare(b[0]));
  return { binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)), web_js_css_tree_sha256: sha(JSON.stringify(entries)) };
}

// The hot window only clones three presented bitmaps. Pixel readback/EDT/asserts
// happen after playback, so verifier CPU work is outside the 3-second fixture.
function installMaskPictures() {
  window.__maskPictures = [];
  const captured = new Set(), original = ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
  ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function(bitmap) {
    if (bitmap && this.canvas.classList.contains("editor-scene-frame")) {
      const time = Number(this.canvas.dataset.time), slot = Math.floor(time), session = window.__scenePreviewWitness?.session;
      if (session?.state === "playing" && slot >= 0 && slot < 3 && time - slot > 0.1 && time - slot < 0.85 && !captured.has(slot)) {
        captured.add(slot);
        window.__maskPictures.push({ slot, time, session_id: session.session_id, generation: this.canvas.dataset.generation, runtime: this.canvas.dataset.runtime,
          bitmap: createImageBitmap(bitmap).then(value => ({ value }), error => ({ error: String(error) })) });
      }
    }
    return original.call(this, bitmap);
  };
  window.__readMaskPictures = async () => Promise.all(window.__maskPictures.map(async p => {
    const result = await p.bitmap;
    if (result.error) throw Error(result.error);
    const b = result.value, canvas = new OffscreenCanvas(b.width, b.height), ctx = canvas.getContext("2d");
    try {
      ctx.drawImage(b, 0, 0);
      return { slot: p.slot, time: p.time, session_id: p.session_id, generation: p.generation, runtime: p.runtime, bytes: [...ctx.getImageData(0, 0, canvas.width, canvas.height).data] };
    } finally { b.close(); canvas.width = canvas.height = 0; }
  }));
}
function assertClock(clock) {
  expect(clock.overflows).toBe(0);
  expect(clock.frames.length).toBeGreaterThan(0);
  expect(clock.frames.every(f => Number.isFinite(f.actual) && f.actual - f.time <= 0.150001 && f.time <= f.actual + 0.000001)).toBe(true);
}

export async function verifyMask(browser, endpoint, directory, check, { outputsOnly = false } = {}) {
  const checkpoint = path.join(directory, "d2b-mask-checkpoint.json"), source = await read(path.join(directory, "b4-graphic-output-checkpoint.json"));
  const candidate = await candidateIdentity();
  let facts = { candidate, project_id: randomUUID(), sequence_id: source.sequence_id, stages: {}, jobs: {} };
  try { const old = await read(checkpoint); expect(old.candidate).toEqual(candidate); facts = old; }
  catch (e) { if (e.code !== "ENOENT") throw e; }
  // Atomic replacement preserves the previous complete checkpoint if interrupted.
  const save = async () => { await fs.writeFile(`${checkpoint}.next`, JSON.stringify(facts, null, 2) + "\n"); await fs.rename(`${checkpoint}.next`, checkpoint); };
  facts.status = "running"; delete facts.failure; await save();
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 }, acceptDownloads: true }), errors = [];
  page.on("pageerror", e => errors.push(e.message));
  await page.addInitScript(() => Object.defineProperty(window, "showSaveFilePicker", { value: undefined, configurable: true }));
  await page.addInitScript(installScenePreviewWitness, { deferReadback: true, exactConsumerOrigin: true });
  await page.addInitScript(installMaskPictures);
  const tool = async (name, input) => {
    const reply = await page.evaluate(async ({ name, input }) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) })).json(), { name, input });
    expect(reply.error, JSON.stringify(reply.error)).toBeNull(); return reply.data;
  };
  const inspect = (project_id = facts.project_id, revision = null) => editorSnapshot(tool("opencut_project_inspect", { project_id, revision }));
  const open = async () => {
    await page.goto(`${endpoint}/editor/${facts.project_id}?scene=${facts.sequence_id}`);
    await expect(page.getByRole("main", { name: "OpenCut editor", exact: true })).toBeVisible();
    const welcome = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await welcome.isVisible()) await welcome.getByRole("button", { name: "Close", exact: true }).click();
  };
  const operation = async (stage, name, input) => {
    let r = facts.stages[stage];
    if (!r) { r = facts.stages[stage] = { name, input }; await save(); }
    expect(r.name).toBe(name);
    if (!r.data) { r.data = await tool(name, r.input); await save(); }
    return r.data;
  };
  const exchange = async (stage, name, click) => {
    if (facts.stages[stage]) return operation(stage, name, facts.stages[stage].input);
    const pattern = `**/api/tools/${name}`;
    const handler = async route => { facts.stages[stage] = { name, input: route.request().postDataJSON() }; await save(); await route.continue(); };
    await page.route(pattern, handler, { times: 1 });
    try {
      const response = page.waitForResponse(r => r.url().endsWith(`/api/tools/${name}`));
      response.catch(() => {});
      await click(); const reply = await (await response).json();
      facts.stages[stage].reply = reply; await save();
      expect(reply.error, JSON.stringify(reply.error)).toBeNull();
      facts.stages[stage].data = reply.data; await save(); return reply.data;
    } finally { await page.unroute(pattern, handler); }
  };
  const seek = async seconds => {
    await page.getByRole("textbox", { name: /^Playhead \(seconds\)/i }).fill(String(seconds));
    await page.getByRole("button", { name: "Go to time", exact: true }).click();
  };
  const clock = () => page.evaluate(() => ({ frames: window.__scenePreviewWitness.frames, progresses: window.__scenePreviewWitness.progresses,
    controls: window.__scenePreviewWitness.controls, overflows: window.__scenePreviewWitness.overflows }));
  const moveDraft = async () => {
    await open();
    await page.locator('[data-element-id="d2b-front-0"] .editor-element-select').click({ position: { x: 10, y: 10 } });
    await page.getByRole("button", { name: "Edit mask", exact: true }).click();
    const canvas = page.getByRole("group", { name: "Mask canvas controls", exact: true }), hit = canvas.getByRole("button", { name: "Move mask", exact: true });
    await hit.scrollIntoViewIfNeeded();
    const bounds = await hit.boundingBox(), viewport = await canvas.boundingBox(); expect(bounds).toBeTruthy(); expect(viewport).toBeTruthy();
    const from = { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 },
      to = { x: from.x + viewport.width * 9.6 / W, y: from.y + viewport.height * 6.4 / H };
    await page.evaluate(() => {
      window.__maskGestureTrace = [];
      for (const name of ["pointerdown", "pointermove", "pointerup", "pointercancel", "lostpointercapture"]) document.addEventListener(name, event => {
        if (window.__maskGestureTrace.length < 40) window.__maskGestureTrace.push({name, target:event.target?.getAttribute?.("aria-label"), tag:event.target?.tagName, x:event.clientX, y:event.clientY});
      }, true);
    });
    facts.gesture_probe = { from, to, bounds, viewport, hit: await page.evaluate(from => { const e=document.elementFromPoint(from.x,from.y); return {tag:e?.tagName,label:e?.getAttribute("aria-label"),class:e?.getAttribute("class")}; },from), capability: (await tool("opencut_render_capability", {})).frame }; await save();
    await page.mouse.move(from.x, from.y); await page.mouse.down(); await page.mouse.move(to.x, to.y, { steps: 5 });
    const form = page.getByRole("form", { name: "Edit mask", exact: true });
    try { await expect.poll(async () => Number(await form.getByRole("spinbutton", { name: "Mask X (%)", exact: true }).inputValue())).toBeGreaterThan(1); }
    catch(error) { facts.gesture_probe.trace = await page.evaluate(() => window.__maskGestureTrace); facts.gesture_probe.after = await page.getByRole("region", {name:"Canvas notes",exact:true}).ariaSnapshot(); await save(); throw error; }
    return { from, to, viewport, draft_x_percent: Number(await form.getByRole("spinbutton", { name: "Mask X (%)", exact: true }).inputValue()),
      draft_y_percent: Number(await form.getByRole("spinbutton", { name: "Mask Y (%)", exact: true }).inputValue()) };
  };
  try {
    await page.goto(`${endpoint}/projects`);
    await expect(page.getByRole("button", { name: "Restore project package", exact: true })).toBeEnabled();
    if (!facts.original) { facts.original = await inspect(source.project_id); await save(); }
    await operation("copy", "opencut_project_copy", { source_project_id: source.project_id, source_revision: source.revision,
      project_id: facts.project_id, name: "D2b Classic mask acceptance", idempotency_key: `d2b-copy-${facts.project_id}` });
    if (!facts.setup) {
      const before = await inspect(), scene = before.project.sequences.find(s => s.id === facts.sequence_id);
      const tracks = tileTracks(scene.tracks.find(t => t.kind === "graphic"));
      const video = scene.tracks.flatMap(t => t.elements).find(e => e.enabled && e.content.kind === "video");
      expect(video, "The reused D0 scene must contain its real video").toBeTruthy();
      facts.fixture_tracks ??= tracks; facts.video_id ??= video.id; await save();
      const receipt = await operation("setup", "opencut_edit_apply", { project_id: facts.project_id, expected_revision: before.revision,
        idempotency_key: `d2b-tiles-v2-${facts.project_id}`, label: "D2b independent mask tiles",
        operations: facts.fixture_tracks.map(track => ({ type: "track_put", sequence_id: facts.sequence_id, track })) });
      facts.setup = receipt.revision; await save();
    }
    const targets = [{ id: facts.video_id, kind: "rectangle", feather: 8, extent: 0.8 }, ...tiles.map((t, i) => ({ ...t, id: `d2b-front-${i}` }))];
    for (const [i, target] of targets.entries()) {
      const stage = `mask-${i}`;
      if (facts.stages[stage]?.verified) continue;
      await open();
      await page.locator(`[data-element-id="${target.id}"] .editor-element-select`).click({ position: { x: 10, y: 10 } });
      if (!facts.stages[stage]) {
        await page.getByRole("button", { name: `Add ${target.kind} mask`, exact: true }).click();
        const form = page.getByRole("form", { name: "Edit mask", exact: true });
        await expect(form).toBeVisible();
        // Escape is a draft-only action: no revision may be created.
        if (i === 0 && !facts.cancel_verified) {
          const before = await inspect();
          await form.getByRole("spinbutton", { name: "Mask X (%)", exact: true }).fill("13");
          await form.getByRole("spinbutton", { name: "Mask X (%)", exact: true }).press("Escape");
          await expect(form).not.toBeVisible(); expect(await inspect()).toEqual(before);
          facts.cancel_verified = true; await save();
          await page.getByRole("button", { name: `Add ${target.kind} mask`, exact: true }).click();
        }
        for (const [name, value] of [["Mask X (%)", 0], ["Mask Y (%)", 0], ["Mask width (%)", (target.extent ?? 0.5) * 100],
          ["Mask height (%)", (target.extent ?? 0.5) * 100], ["Mask rotation (degrees)", 0], ["Mask feather (pixels)", target.feather]])
          await form.getByRole("spinbutton", { name, exact: true }).fill(String(value));
        await exchange(stage, "opencut_edit_apply", () => form.getByRole("button", { name: "Save mask", exact: true }).click());
      } else await operation(stage, "opencut_edit_apply", facts.stages[stage].input);
      const snapshot = await inspect(), element = elementIn(snapshot.project, facts.sequence_id, target.id);
      expect(element.visual.masks).toHaveLength(1);
      expect(element.visual.masks[0]).toEqual(maskFor(target, element.visual.masks[0].id));
      facts.stages[stage].verified = true; facts.stages[stage].mask = element.visual.masks[0]; await save();
    }
    if (!facts.before_history) { facts.before_history = await inspect(); await save(); }
    for (const direction of ["undo", "redo"]) {
      const stage = `history-${direction}`;
      if (facts.stages[stage]?.verified) continue;
      await open(); const history = page.locator(".editor-history");
      if (!facts.stages[stage]) {
        await history.getByRole("button", { name: direction === "undo" ? "Undo" : "Redo", exact: true }).click();
        await exchange(stage, `opencut_edit_${direction}_apply`, () => history.getByRole("button", { name: `Apply ${direction}`, exact: true }).click());
      } else await operation(stage, `opencut_edit_${direction}_apply`, facts.stages[stage].input);
      const current = await inspect(), expected = structuredClone(facts.before_history.project);
      if (direction === "undo") elementIn(expected, facts.sequence_id, "d2b-front-3").visual.masks = [];
      expect(current.project).toEqual(expected); facts.revision = current.revision;
      facts.stages[stage].verified = true; await save();
    }
    // Keep canvas gestures after the main history pair, then restore the exact
    // final authored aggregate. Their own original requests/Undo are resumable.
    if (!facts.gesture_cancel?.verified) {
      const before = await inspect(), requests = [];
      const observe = request => { if (request.url().endsWith("/api/tools/opencut_edit_apply")) requests.push(request.postDataJSON()); };
      page.on("request", observe);
      try {
        const movement = await moveDraft();
        expect(await inspect()).toEqual(before); expect(requests).toEqual([]);
        await page.keyboard.press("Escape"); await page.mouse.up();
        await expect(page.getByRole("form", { name: "Edit mask", exact: true })).not.toBeVisible();
        expect(await inspect()).toEqual(before); expect(requests).toEqual([]);
        facts.gesture_cancel = { ...movement, before_revision: before.revision, apply_requests: requests, verified: true }; await save();
      } finally { page.off("request", observe); await page.keyboard.press("Escape").catch(() => {}); await page.mouse.up(); }
    }
    if (!facts.gesture_restore?.verified) {
      if (!facts.gesture_before) { facts.gesture_before = await inspect(); await save(); }
      if (!facts.stages["gesture-commit"]) {
        const requests = [], observe = request => { if (request.url().endsWith("/api/tools/opencut_edit_apply")) requests.push(request.postDataJSON()); };
        page.on("request", observe);
        try {
          facts.gesture_movement = await moveDraft();
          expect(await inspect()).toEqual(facts.gesture_before); expect(requests).toEqual([]); await save();
          await exchange("gesture-commit", "opencut_edit_apply", () => page.mouse.up());
          await expect(page.getByRole("form", { name: "Edit mask", exact: true })).not.toBeVisible();
          expect(requests).toHaveLength(1); facts.stages["gesture-commit"].observed_apply_requests = requests; await save();
        } finally { page.off("request", observe); await page.keyboard.press("Escape").catch(() => {}); await page.mouse.up(); }
      } else await operation("gesture-commit", "opencut_edit_apply", facts.stages["gesture-commit"].input);
      if (!facts.gesture_committed) {
        const current = await inspect(), expected = structuredClone(facts.gesture_before.project);
        const actual = elementIn(current.project, facts.sequence_id, "d2b-front-0").visual.masks[0].shape.geometry;
        // One CSS-pixel event rounding is allowed here, not in pixel assertions.
        expect(Math.abs(actual.center_x - 0.1)).toBeLessThanOrEqual(1 / 96);
        expect(Math.abs(actual.center_y - 0.1)).toBeLessThanOrEqual(1 / 64);
        const previous = elementIn(expected, facts.sequence_id, "d2b-front-0").visual.masks[0].shape.geometry;
        previous.center_x = actual.center_x; previous.center_y = actual.center_y;
        expect(current.project).toEqual(expected); expect(current.revision).not.toBe(facts.gesture_before.revision);
        facts.gesture_committed = current; await save();
      }
      await open();
      if (!facts.stages["gesture-undo"]) {
        const history = page.locator(".editor-history"); await history.getByRole("button", { name: "Undo", exact: true }).click();
        await exchange("gesture-undo", "opencut_edit_undo_apply", () => history.getByRole("button", { name: "Apply undo", exact: true }).click());
      } else await operation("gesture-undo", "opencut_edit_undo_apply", facts.stages["gesture-undo"].input);
      const current = await inspect(); expect(current.project).toEqual(facts.before_history.project);
      facts.revision = current.revision; facts.gesture_restore = { revision: current.revision, verified: true }; await save();
    }
    await open();
    const reopened = await inspect(); expect(reopened.revision).toBe(facts.revision); expect(reopened.project).toEqual(facts.before_history.project);
    // Prove the independent source facts still describe the authored shapes.
    for (const track of facts.fixture_tracks) {
      const expected = structuredClone(track.elements[0]), actual = elementIn(reopened.project, facts.sequence_id, expected.id);
      expected.visual.masks = actual.visual.masks; expect(actual).toEqual(expected);
    }
    const refs = references(facts.fixture_tracks);
    const referenceKey = sha(JSON.stringify({ refs, mask: sha(await fs.readFile(new URL("./mask-oracle.mjs", import.meta.url))),
      blend: sha(await fs.readFile(new URL("./blend-oracle.mjs", import.meta.url))) }));
    facts.oracle = { model: MASK_ORACLE_MODEL, blend_model: BLEND_ORACLE_MODEL, key: referenceKey, references: refs,
      scope: "Four static graphic masks, cardinal interior/exterior and feather ramps; video decoder-mask execution. No image, curved AA pixel equality or independent PCM claim." };
    await save();

    if (!outputsOnly) {
      if (!facts.preview?.captured) {
        if (facts.preview || facts.preview_observation) {
          (facts.preview_attempts ??= []).push({ preview: facts.preview, observation: facts.preview_observation });
          delete facts.preview; delete facts.preview_observation; await save();
        }
        await seek(0);
        await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
        await expect.poll(() => page.evaluate(() => window.__maskPictures.length === 3 || window.__scenePreviewWitness.endedSession !== null), { timeout: 45000 }).toBe(true);
        facts.preview_observation = await page.evaluate(() => ({ captured: window.__maskPictures.map(({ slot, time, runtime, generation }) => ({ slot, time, runtime, generation })),
          frames: window.__scenePreviewWitness.frames, ended: window.__scenePreviewWitness.endedSession })); await save();
        const pause = page.getByRole("button", { name: "Pause scene audio", exact: true });
        if (await pause.isEnabled()) { await pause.click(); await expect(page.getByText("Scene sound paused", { exact: true })).toBeVisible(); }
        const pictures = await page.evaluate(() => window.__readMaskPictures());
        facts.preview = { captured: false, pictures: [], clock: await clock() };
        for (const p of pictures) {
          const raw = Buffer.from(p.bytes); delete p.bytes;
          const file = path.join(directory, `d2b-preview-${p.slot}.rgba`); await fs.writeFile(file, raw);
          facts.preview.pictures.push({ ...p, file, sha256: sha(raw) }); await save();
        }
        expect(facts.preview.pictures.map(p => p.slot).sort()).toEqual([0, 1, 2]);
        assertClock(facts.preview.clock); expect(new Set(facts.preview.clock.frames.map(f => f.time)).size).toBeGreaterThan(5);
        facts.preview.captured = true; await save();
      }
      for (const picture of facts.preview.pictures) {
        expect(facts.preview.clock.frames.some(f => f.time === picture.time && f.session_id === picture.session_id && f.generation === picture.generation)).toBe(true);
        const raw = await fs.readFile(picture.file); expect(sha(raw)).toBe(picture.sha256);
        picture.pixels = measure(raw, refs); picture.reference_key = referenceKey; await save(); assertPixels(picture.pixels, 2);
      }
      assertClock(facts.preview.clock);
      if (!facts.preview_control?.verified) {
        // A separate owned session prevents a natural 3s end racing live seek.
        await open(); await seek(0.25);
        await page.getByRole("button", { name: "Play scene preview", exact: true }).click();
        await expect.poll(() => page.evaluate(() => { const w = window.__scenePreviewWitness; return w.session?.state === "playing" && Number(w.progress?.consumedFrames) > 0 && w.frames.length > 0; }), { timeout: 15000 }).toBe(true);
        const previous = await page.evaluate(() => ({ session_id: window.__scenePreviewWitness.session.session_id, generation: window.__scenePreviewWitness.session.generation }));
        const head = page.getByRole("textbox", { name: /^Playhead \(seconds\)/i }); await head.fill("0.25"); await head.press("Enter");
        await expect.poll(() => page.evaluate(previous => {
          const w = window.__scenePreviewWitness, s = w.session;
          return s?.session_id === previous.session_id && s.generation !== previous.generation && new Set(w.frames.filter(f => f.session_id === s.session_id && f.generation === s.generation).map(f => f.time)).size >= 2;
        }, previous), { timeout: 15000 }).toBe(true);
        const generation = await page.evaluate(() => window.__scenePreviewWitness.session.generation);
        facts.preview_control = { previous, seek_generation: generation, target_seconds: 0.25 }; await save();
        await page.getByRole("button", { name: "Pause scene audio", exact: true }).click();
        await expect.poll(() => page.evaluate(() => { const s = window.__scenePreviewWitness.session; return s?.state === "paused" && Number(document.querySelector("canvas.editor-scene-frame")?.dataset.time) === Number(s.presentation_time.ticks) / s.presentation_time.timebase; })).toBe(true);
        facts.preview_control.pause = await page.evaluate(async () => {
          const sample = () => { const w = window.__scenePreviewWitness, c = document.querySelector("canvas.editor-scene-frame");
            return { state: w.session.state, session_id: w.session.session_id, generation: w.session.generation, consumed_frames: w.progress.consumedFrames,
              authority: w.session.presentation_time, frame_count: w.frames.length, frame_time: c?.dataset.time, frame_generation: c?.dataset.generation }; };
          const before = sample(), at = performance.now(); await new Promise(resolve => setTimeout(resolve, 300));
          return { before, after: sample(), elapsed_ms: performance.now() - at };
        });
        facts.preview_control.clock = await clock(); await save();
        const { before, after, elapsed_ms } = facts.preview_control.pause;
        expect(before).toEqual(after); expect(elapsed_ms).toBeGreaterThanOrEqual(250); expect(before.session_id).toBe(previous.session_id);
        expect(before.frame_generation).toBe(before.generation); expect(before.generation).not.toBe(generation); assertClock(facts.preview_control.clock);
        facts.preview_control.verified = true; await save();
      }
    }

    for (const kind of ["png", "mp4"]) {
      const rec = facts.jobs[kind] ??= {}, name = "opencut_render_submit";
      if (!rec.submission) {
        await open();
        if (kind === "png") await seek(0.25);
        else await page.getByRole("button", { name: "Export video MP4", exact: true }).click();
        rec.submission = await exchange(`job-${kind}`, name, () => page.getByRole("button", { name: kind === "png" ? "Render scene frame" : "Start video export", exact: true }).click());
        await save();
      }
      let job;
      await expect.poll(async () => { job = await tool("opencut_job_view", { job_id: rec.submission.job_id }); return ["succeeded", "failed", "cancelled"].includes(job.state) ? job.state : "pending"; },
        { timeout: 300000, intervals: [500, 1000] }).not.toBe("pending");
      rec.job = job; await save(); expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      expect(job.kind).toBe(kind === "png" ? "render.frame" : "render.video");
      expect(job.result?.kind).toBe(job.kind);
      const result = job.result?.data;
      expect(result?.identity).toMatchObject({ project_id: facts.project_id, revision: facts.revision, sequence_id: facts.sequence_id,
        width: W, height: H, ...(kind === "png" ? { profile: PROFILE } : { visual_profile: "scene_v1" }) });
      if (kind === "png") {
        const byId = (a, b) => a.elementId.localeCompare(b.elementId);
        expect(result.stacks.filter(stack => stack.masks.length > 0).map(({ elementId, masks }) => ({ elementId, masks })).sort(byId))
          .toEqual(targets.map((target, i) => ({ elementId: target.id, masks: [facts.stages[`mask-${i}`].mask] })).sort(byId));
      }
      const artifact = job.artifacts.find(a => a.mime_type === (kind === "png" ? "image/png" : "video/mp4")); expect(artifact).toBeTruthy();
      const file = rec.output?.file ?? path.join(directory, `d2b-${job.job_id}.${kind}`);
      let bytes; try { bytes = await fs.readFile(file); } catch (e) { if (e.code !== "ENOENT") throw e; }
      if (!bytes || sha(bytes) !== artifact.content_hash) {
        await open(); await page.getByRole("button", { name: "Tasks", exact: true }).click(); await page.getByLabel("Refresh tasks", { exact: true }).click();
        const cards = page.locator(".task-card").filter({ hasText: facts.revision }); await expect(cards.first()).toBeVisible();
        let found = false;
        for (let i = 0; i < await cards.count(); i++) {
          const download = page.waitForEvent("download"); await cards.nth(i).getByRole("button", { name: "Download", exact: true }).click();
          const d = await download;
          if (d.suggestedFilename() === `opencut-${job.job_id}.${kind}`) { await d.saveAs(file); found = true; break; }
        }
        expect(found).toBe(true); bytes = await fs.readFile(file);
      }
      expect(sha(bytes)).toBe(artifact.content_hash); rec.output = { file, sha256: sha(bytes) }; await save();
      if (kind === "mp4") {
        const probe = await execute(process.env.OPENCUT_FFPROBE ?? "ffprobe", ["-v", "error", "-count_frames", "-show_streams", "-show_format", "-of", "json", file],
          { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 });
        rec.probe = JSON.parse(probe.stdout); rec.probe_artifact_sha256 = artifact.content_hash; await save();
        const video = rec.probe.streams.filter(s => s.codec_type === "video"), audio = rec.probe.streams.filter(s => s.codec_type === "audio");
        expect(video).toHaveLength(1); expect(audio).toHaveLength(1);
        expect(video[0]).toMatchObject({ codec_name: "h264", width: W, height: H, r_frame_rate: "30/1", nb_frames: "90", nb_read_frames: "90" });
        expect(Number(video[0].duration)).toBeCloseTo(3, 6);
        expect(audio[0]).toMatchObject({ codec_name: "aac", sample_rate: "48000", channels: 2 });
      }
      rec.observations ??= [];
      for (const seconds of kind === "png" ? [null] : [0.25, 1.5, 2.9]) {
        const previous = rec.observations.find(o => o.passed && o.seconds === seconds && o.reference_key === referenceKey && o.artifact_sha256 === artifact.content_hash);
        if (previous) {
          const raw = await fs.readFile(previous.file); expect(sha(raw)).toBe(previous.sha256);
          assertPixels(measure(raw, refs), kind === "png" ? 2 : 6); continue;
        }
        // Re-read the original artifact; reference updates never submit another job.
        const raw = await decode(file, seconds), rawFile = path.join(directory, `d2b-${job.job_id}-${seconds ?? "still"}.rgba`);
        await fs.writeFile(rawFile, raw);
        const observation = { seconds, reference_key: referenceKey, artifact_sha256: artifact.content_hash, file: rawFile, sha256: sha(raw), pixels: measure(raw, refs), passed: false };
        rec.observations.push(observation); await save();
        assertPixels(observation.pixels, kind === "png" ? 2 : 6); observation.passed = true; await save();
      }
    }
    facts.package_path ??= path.join(directory, `d2b-mask-package-${facts.project_id}`); facts.restored_id ??= randomUUID(); await save();
    if (!facts.stages["package-export"]?.data) {
      let exists = true; try { await fs.access(facts.package_path); } catch (e) { if (e.code !== "ENOENT") throw e; exists = false; }
      if (exists) {
        const inspected = await tool("opencut_project_package_inspect", { package_path: facts.package_path });
        expect(inspected).toMatchObject({ source_project_id: facts.project_id, source_revision: facts.revision, mode: "self_contained", scope: "saved_revision" });
        facts.stages["package-export"] = { name: "opencut_project_package_export", input: { project_id: facts.project_id, revision: facts.revision, destination: facts.package_path, mode: "self_contained", scope: "saved_revision" },
          data: { package_path: facts.package_path, package: inspected }, recovered_published_package: true }; await save();
      }
    }
    const exported = await operation("package-export", "opencut_project_package_export", { project_id: facts.project_id, revision: facts.revision, destination: facts.package_path, mode: "self_contained", scope: "saved_revision" });
    const restored = await operation("package-restore", "opencut_project_package_import", { package_path: facts.package_path, target_project_id: facts.restored_id,
      expected_manifest_sha256: exported.package.manifest_sha256, idempotency_key: `d2b-restore-${facts.restored_id}` });
    expect((await inspect(facts.restored_id, restored.revision)).project).toEqual({ ...facts.before_history.project, id: facts.restored_id });
    facts.package = { exported, restored, all_masks_preserved: true };
    facts.classic_import = await verifyClassicMaskImport(page, endpoint, directory, tool); await save();
    // Work was confined to a copy: the actual source head and entire project stay intact.
    expect(await inspect(source.project_id)).toEqual(facts.original); facts.original_preserved = true;
    facts.pageerrors = errors; expect(errors).toEqual([]);
    facts.profile = PROFILE; facts.status = outputsOnly ? "outputs_passed_preview_pending" : "passed"; await save();
    check(outputsOnly ? "D2b mask editor/history, independent PNG/MP4 and package; preview pending" : "D2b rectangle/ellipse hard+feather: UI/history, consumer preview, independent PNG/MP4, video mask and package", facts);
  } catch (error) {
    facts.status = "failed"; facts.failure = String(error); facts.pageerrors = errors; await save();
    await fs.writeFile(path.join(directory, "d2b-mask-failure-ui.txt"), await page.locator("body").ariaSnapshot()).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
