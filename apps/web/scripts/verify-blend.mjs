import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  CLASSIC_BLEND_MODES,
  BLEND_ORACLE_MODEL,
  compositeClassicRgba8,
} from "./blend-oracle.mjs";
import { installScenePreviewWitness } from "./verify-scene-preview.mjs";
const execute = promisify(execFile),
  sha = (b) => createHash("sha256").update(b).digest("hex");
const read = async (p) => JSON.parse(await fs.readFile(p, "utf8"));
const decode = async (args) =>
  (
    await execute(
      process.env.OPENCUT_FFMPEG ?? "ffmpeg",
      ["-hide_banner", "-loglevel", "error", ...args],
      { encoding: "buffer", maxBuffer: 32 * 1024 * 1024 },
    )
  ).stdout;
const cases = [
  { back: [51, 102, 204], front: [204, 77, 26], opacity: 0.6 },
  { back: [51, 102, 204], front: [204, 77, 26], opacity: 1 },
  { back: [0, 0, 0], front: [255, 255, 255], opacity: 1 },
  { back: [255, 255, 255], front: [0, 0, 0], opacity: 1 },
  { back: [51, 102, 204], front: [204, 77, 26], opacity: 0 },
  { back: [193, 41, 100], front: [28, 211, 160], opacity: 0.25 },
];
const center = (i) => ({
  x: 30 + (i % 5) * 60,
  y: 22 + Math.floor(i / 5) * 40,
});
const rgba = (rgb) => ({
  encoding: "srgb",
  red: rgb[0] / 255,
  green: rgb[1] / 255,
  blue: rgb[2] / 255,
  alpha: 1,
});
const time = (n) => ({ ticks: String(n), timebase: 2 });
function tileTracks(template) {
  return CLASSIC_BLEND_MODES.flatMap((mode, i) =>
    ["back", "front"].map((part) => {
      const track = structuredClone(template);
      track.id = `d2-${part}-${i}`;
      track.name = `D2 ${part} ${mode}`;
      track.channels = [];
      track.opacity = 1;
      track.elements = cases.map((c, k) => {
        const e = structuredClone(template.elements[0]),
          p = center(i);
        e.id = `d2-${part}-${i}-${k}`;
        e.name = `D2 ${mode} ${part} ${k}`;
        e.channels = [];
        e.enabled = true;
        e.timeline_range = { start: time(k), duration: time(1) };
        e.content.graphic = {
          geometry: { kind: "rectangle", corner_radius_percent: 0 },
          source_size: { width: 56, height: 36 },
          fill: rgba(c[part]),
          stroke: { width_px: 0, align: "inside", color: rgba([0, 0, 0]) },
        };
        e.visual = {
          profile: "classic_canvas_v31",
          blend_mode: part === "front" && k > 0 ? mode : "normal",
          opacity: part === "front" ? c.opacity : 1,
          effects: [],
          masks: [],
          transform: {
            position_x_px: p.x - 160,
            position_y_px: p.y - 90,
            rotation_degrees: 0,
            scale_x: 0.2,
            scale_y: 0.2,
          },
        };
        return e;
      });
      return track;
    }),
  );
}
function checkPixels(raw, k, tolerance) {
  expect(raw.length).toBe(320 * 180 * 4);
  const c = cases[k];
  return CLASSIC_BLEND_MODES.map((mode, i) => {
    const p = center(i),
      expected = Array.from(
        compositeClassicRgba8(
          [...c.back, 255],
          [...c.front, Math.round(c.opacity * 255)],
          mode,
        ),
      );
    let max = 0;
    for (let y = p.y - 2; y <= p.y + 2; y++)
      for (let x = p.x - 2; x <= p.x + 2; x++)
        for (let ch = 0; ch < 4; ch++)
          max = Math.max(
            max,
            Math.abs(raw[(y * 320 + x) * 4 + ch] - expected[ch]),
          );
    expect(max, `${mode} case ${k} expected ${expected}`).toBeLessThanOrEqual(
      tolerance,
    );
    return { mode, case: k, expected, max_channel_error: max };
  });
}
const PREVIEW_EVIDENCE_VERSION = 3;
function validatePlaybackEvidence(facts) {
  expect(facts.preview_playback_version).toBe(PREVIEW_EVIDENCE_VERSION);
  expect(facts.preview.map((p) => p.case).sort()).toEqual([0, 1, 2, 3, 4, 5]);
  const w = facts.preview_playback_clock;
  expect(new Set(w.frames.map((f) => f.time)).size).toBeGreaterThan(5);
  expect(w.overflows).toBe(0);
  expect(
    w.frames.every(
      (f) =>
        Number.isFinite(f.actual) &&
        f.actual - f.time <= 0.150001 &&
        f.time <= f.actual + 0.000001,
    ),
  ).toBe(true);
  return w;
}
function validatePreviewEvidence(facts) {
  validatePlaybackEvidence(facts);
  expect(facts.preview_evidence_version).toBe(PREVIEW_EVIDENCE_VERSION);
  const w = facts.preview_clock;
  expect(w.overflows).toBe(0);
  expect(
    w.frames.every(
      (f) =>
        Number.isFinite(f.actual) &&
        f.actual - f.time <= 0.150001 &&
        f.time <= f.actual + 0.000001,
    ),
  ).toBe(true);
  expect(typeof facts.preview_seek.previous_session_id).toBe("string");
  expect(facts.preview_seek.new_session_id).toBe(
    facts.preview_seek.previous_session_id,
  );
  expect(facts.preview_seek.new_generation).not.toBe(
    facts.preview_seek.previous_generation,
  );
  expect(
    new Set(
      w.frames
        .filter(
          (f) =>
            f.session_id === facts.preview_seek.new_session_id &&
            f.generation === facts.preview_seek.new_generation,
        )
        .map((f) => f.time),
    ).size,
  ).toBeGreaterThanOrEqual(2);
  const { before, after, elapsed_ms } = facts.preview_pause;
  expect(before.state).toBe("paused");
  expect(after).toEqual(before);
  expect(elapsed_ms).toBeGreaterThanOrEqual(250);
  expect(before.frame_time).not.toBeNull();
  expect(before.generation).not.toBe(facts.preview_seek.new_generation);
  expect(before.generation).toBe(before.authoritative_generation);
  expect(Number(before.frame_time)).toBe(
    Number(before.presentation_time.ticks) /
      before.presentation_time.timebase,
  );
  expect(before.session_id).toBe(facts.preview_seek.new_session_id);
}
export async function verifyBlend(
  browser,
  endpoint,
  directory,
  check,
  { outputsOnly = false } = {},
) {
  const checkpoint = path.join(directory, "d2-blend-checkpoint.json"),
    source = await read(
      path.join(directory, "b4-graphic-output-checkpoint.json"),
    );
  const webEntries = [];
  async function webTree(relative = "") {
    for (const entry of await fs.readdir(
      path.join(process.env.OPENCUT_WEB_ROOT, relative),
      { withFileTypes: true },
    )) {
      const name = path.join(relative, entry.name);
      if (entry.isSymbolicLink())
        throw Error(`Unhashed web candidate symlink ${name}`);
      if (entry.isDirectory()) await webTree(name);
      else if (/\.(js|mjs|css)$/u.test(name))
        webEntries.push([
          name,
          sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name))),
        ]);
    }
  }
  await webTree();
  webEntries.sort((a, b) => a[0].localeCompare(b[0]));
  const candidate = {
    binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    web_js_css_tree_sha256: sha(JSON.stringify(webEntries)),
  };
  let facts = {
    candidate,
    stages: {},
    jobs: {},
    project_id: randomUUID(),
    sequence_id: source.sequence_id,
  };
  try {
    const old = await read(checkpoint);
    expect(old.candidate).toEqual(candidate);
    facts = old;
  } catch (e) {
    if (e.code !== "ENOENT") throw e;
  }
  facts.status = "running";
  delete facts.failure;
  const save = async () =>
    fs.writeFile(checkpoint, JSON.stringify(facts, null, 2) + "\n");
  await save();
  const page = await browser.newPage({
      locale: "en-US",
      viewport: { width: 1440, height: 1100 },
      acceptDownloads: true,
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.addInitScript(() =>
    Object.defineProperty(window, "showSaveFilePicker", {
      value: undefined,
      configurable: true,
    }),
  );
  await page.addInitScript(installScenePreviewWitness, {
    deferReadback: true,
    exactConsumerOrigin: true,
  });
  if (process.env.OPENCUT_D2_TIMING === "1")
    await page.addInitScript(() => {
      window.__blendTiming = [];
      window.__blendTimingOverflows = 0;
      window.__blendTimingScope = "first-six-render-attempts";
      let frame = 0;
      const record = (stage, extra = {}, owner = frame) => {
        if (owner > 6) return;
        if (window.__blendTiming.length >= 1536) {
          window.__blendTimingOverflows++;
          return;
        }
        window.__blendTiming.push({
          stage,
          frame: owner,
          at: performance.now(),
          consumed: window.__scenePreviewWitness?.progress?.consumedFrames,
          ...extra,
        });
      };
      const originalFetch = window.fetch;
      window.fetch = async (...args) => {
        const url = String(args[0]);
        const request = url.includes("opencut_visual_preview_frame")
          ? JSON.parse(args[1]?.body ?? "null")
          : null;
        const stage = request
          ? request.graphics
            ? "graphics"
            : request.rasters
              ? "text"
              : request.decoded
                ? "media"
                : "plan"
          : url.includes("opencut_visual_preview_font_authorize")
            ? "font-authorize"
            : null;
        if (stage === "plan") frame++;
        const owner = frame;
        if (stage) record(`${stage}-start`, { time: request?.time }, owner);
        const response = await originalFetch(...args);
        if (stage) {
          record(`${stage}-headers`, {}, owner);
          // Observe the SDK's one real read. Cloning/parsing a second copy can
          // distort the very frame latency this diagnostic is measuring.
          const text = response.text.bind(response);
          response.text = async () => {
            const value = await text();
            record(`${stage}-body`, { chars: value.length }, owner);
            return value;
          };
        }
        return response;
      };
      const digest = SubtleCrypto.prototype.digest;
      SubtleCrypto.prototype.digest = async function (algorithm, bytes) {
        const owner = frame,
          start = performance.now();
        try {
          return await digest.call(this, algorithm, bytes);
        } finally {
          record(
            "hash",
            { bytes: bytes.byteLength, ms: performance.now() - start },
            owner,
          );
        }
      };
      for (const method of ["measureText", "fillText", "drawImage"]) {
        const original = OffscreenCanvasRenderingContext2D.prototype[method];
        OffscreenCanvasRenderingContext2D.prototype[method] = function (
          ...args
        ) {
          const start = performance.now();
          try {
            return original.apply(this, args);
          } finally {
            record(method, { ms: performance.now() - start });
          }
        };
      }
      const push = GPUDevice.prototype.pushErrorScope;
      GPUDevice.prototype.pushErrorScope = function (...args) {
        if (args[0] === "out-of-memory") record("gpu-start");
        return push.apply(this, args);
      };
      const pop = GPUDevice.prototype.popErrorScope;
      GPUDevice.prototype.popErrorScope = async function (...args) {
        const owner = frame,
          start = performance.now();
        try {
          return await pop.apply(this, args);
        } finally {
          record("gpu-error-scope", { ms: performance.now() - start }, owner);
        }
      };
      const submit = GPUQueue.prototype.submit;
      GPUQueue.prototype.submit = function (...args) {
        record("gpu-submit");
        return submit.apply(this, args);
      };
      const upload = GPUQueue.prototype.copyExternalImageToTexture;
      GPUQueue.prototype.copyExternalImageToTexture = function (...args) {
        const start = performance.now();
        try {
          return upload.apply(this, args);
        } finally {
          record("gpu-upload", { ms: performance.now() - start });
        }
      };
      const transfer = OffscreenCanvas.prototype.transferToImageBitmap;
      OffscreenCanvas.prototype.transferToImageBitmap = function (...args) {
        record("bitmap-start");
        try {
          return transfer.apply(this, args);
        } finally {
          record("bitmap-ready");
        }
      };
    });
  await page.addInitScript(() => {
    window.__blendPictures = [];
    const captured = new Set(),
      original = ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (
      bitmap,
    ) {
      if (bitmap && this.canvas.classList.contains("editor-scene-frame")) {
        const t = Number(this.canvas.dataset.time),
          k = Math.floor(t * 2),
          fraction = t * 2 - k;
        if (
          k >= 0 &&
          k < 6 &&
          fraction > 0.15 &&
          fraction < 0.85 &&
          !captured.has(k)
        ) {
          captured.add(k);
          window.__blendPictures.push({
            case: k,
            time: t,
            generation: this.canvas.dataset.generation,
            runtime: this.canvas.dataset.runtime,
            bitmap: createImageBitmap(bitmap),
          });
        }
      }
      return original.call(this, bitmap);
    };
    window.__readBlendPictures = async () =>
      Promise.all(
        window.__blendPictures.map(async (p) => {
          const b = await p.bitmap,
            c = new OffscreenCanvas(b.width, b.height),
            ctx = c.getContext("2d");
          ctx.drawImage(b, 0, 0);
          const bytes = Array.from(
            ctx.getImageData(0, 0, c.width, c.height).data,
          );
          b.close();
          return {
            case: p.case,
            time: p.time,
            generation: p.generation,
            runtime: p.runtime,
            bytes,
          };
        }),
      );
  });
  const tool = async (name, input) => {
    const r = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/api/tools/${name}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { name, input },
    );
    expect(r.error, JSON.stringify(r.error)).toBeNull();
    return r.data;
  };
  const inspect = () =>
    editorSnapshot(tool("opencut_project_inspect", {
      project_id: facts.project_id,
      revision: null,
    }));
  const open = async () => {
    await page.goto(
      `${endpoint}/editor/${facts.project_id}?scene=${facts.sequence_id}`,
    );
    await expect(
      page.getByRole("main", { name: "OpenCut editor", exact: true }),
    ).toBeVisible();
    const w = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await w.isVisible())
      await w.getByRole("button", { name: "Close", exact: true }).click();
  };
  const operation = async (stage, name, input) => {
    let r = facts.stages[stage];
    if (!r) {
      r = facts.stages[stage] = { name, input };
      await save();
    }
    expect(r.name).toBe(name);
    if (!r.data) {
      r.data = await tool(name, r.input);
      await save();
    }
    return r.data;
  };
  const exchange = async (stage, name, click) => {
    const prior = facts.stages[stage];
    if (prior) return operation(stage, name, prior.input);
    const pattern = `**/api/tools/${name}`;
    await page.route(
      pattern,
      async (route) => {
        facts.stages[stage] = { name, input: route.request().postDataJSON() };
        await save();
        await route.continue();
      },
      { times: 1 },
    );
    const response = page.waitForResponse((r) =>
      r.url().endsWith(`/api/tools/${name}`),
    );
    await click();
    const reply = await (await response).json();
    expect(reply.error, JSON.stringify(reply.error)).toBeNull();
    facts.stages[stage].data = reply.data;
    await save();
    return reply.data;
  };
  try {
    await page.goto(`${endpoint}/projects`);
    await expect(
      page.getByRole("button", {
        name: "Restore project package",
        exact: true,
      }),
    ).toBeEnabled();
    await operation("copy", "opencut_project_copy", {
      source_project_id: source.project_id,
      source_revision: source.revision,
      project_id: facts.project_id,
      name: "D2 Classic blend acceptance",
      idempotency_key: `d2-copy-${facts.project_id}`,
    });
    if (!facts.setup) {
      const before = await inspect(),
        template = before.project.sequences
          .find((s) => s.id === facts.sequence_id)
          .tracks.find((t) => t.kind === "graphic");
      const tracks = tileTracks(template);
      const receipt = await operation("setup", "opencut_edit_apply", {
        project_id: facts.project_id,
        expected_revision: before.revision,
        idempotency_key: `d2-tiles-${facts.project_id}`,
        label: "D2 independent blend tiles",
        operations: tracks.map((track) => ({
          type: "track_put",
          sequence_id: facts.sequence_id,
          track,
        })),
      });
      facts.setup = receipt.revision;
      await save();
    }
    for (let i = 0; i < CLASSIC_BLEND_MODES.length; i++) {
      if (facts.stages[`edit-${i}`]?.verified) continue;
      await open();
      await page
        .locator(`[data-element-id="d2-front-${i}-0"] .editor-element-select`)
        .click({ position: { x: 10, y: 10 } });
      const form = page.getByRole("form", {
          name: "Edit blend mode",
          exact: true,
        }),
        select = form.getByRole("combobox", { name: /^Blend mode$/i });
      await expect(select.locator("option")).toHaveCount(17);
      if (i === 0) {
        await expect(select).toHaveValue("normal");
        facts.stages["edit-0"] = { verified: true };
        await save();
        continue;
      }
      if (!facts.stages[`edit-${i}`]) {
        await select.selectOption(CLASSIC_BLEND_MODES[i]);
        await exchange(`edit-${i}`, "opencut_edit_apply", () =>
          form
            .getByRole("button", { name: "Save blend mode", exact: true })
            .click(),
        );
      } else
        await operation(
          `edit-${i}`,
          "opencut_edit_apply",
          facts.stages[`edit-${i}`].input,
        );
      const snapshot = await inspect(),
        element = snapshot.project.sequences
          .find((s) => s.id === facts.sequence_id)
          .tracks.flatMap((t) => t.elements)
          .find((e) => e.id === `d2-front-${i}-0`);
      expect(element.visual.blend_mode).toBe(CLASSIC_BLEND_MODES[i]);
      facts.stages[`edit-${i}`].verified = true;
      await save();
    }
    if (!facts.before_history) {
      facts.before_history = await inspect();
      await save();
    }
    for (const direction of ["undo", "redo"]) {
      if (facts.stages[`history-${direction}`]?.verified) continue;
      await open();
      const history = page.locator(".editor-history");
      if (!facts.stages[`history-${direction}`]) {
        await history
          .getByRole("button", {
            name: direction === "undo" ? "Undo" : "Redo",
            exact: true,
          })
          .click();
        await exchange(
          `history-${direction}`,
          `opencut_edit_${direction}_apply`,
          () =>
            history
              .getByRole("button", { name: `Apply ${direction}`, exact: true })
              .click(),
        );
      } else
        await operation(
          `history-${direction}`,
          `opencut_edit_${direction}_apply`,
          facts.stages[`history-${direction}`].input,
        );
      const current = await inspect(),
        expected = structuredClone(facts.before_history.project);
      if (direction === "undo")
        expected.sequences
          .find((s) => s.id === facts.sequence_id)
          .tracks.flatMap((t) => t.elements)
          .find((e) => e.id === "d2-front-16-0").visual.blend_mode = "normal";
      expect(current.project).toEqual(expected);
      facts.stages[`history-${direction}`].verified = true;
      facts.revision = current.revision;
      await save();
    }
    await open();
    expect((await inspect()).revision).toBe(facts.revision);
    if (
      !outputsOnly &&
      (facts.preview_verified !== true ||
        facts.preview_evidence_version !== PREVIEW_EVIDENCE_VERSION ||
        facts.preview?.length !== 6)
    ) {
      delete facts.preview_verified;
      await save();
      if (facts.preview_playback_version !== PREVIEW_EVIDENCE_VERSION) {
        await page
          .getByRole("textbox", { name: /^Playhead \(seconds\)/i })
          .fill("0");
        await page
          .getByRole("button", { name: "Go to time", exact: true })
          .click();
        await page
          .getByRole("button", { name: "Play scene preview", exact: true })
          .click();
        await expect
          .poll(
            () =>
              page.evaluate(
                () =>
                  window.__blendPictures.length === 6 ||
                  window.__scenePreviewWitness.endedSession !== null,
              ),
            {
              timeout: 45000,
            },
          )
          .toBe(true);
        facts.preview_observation = await page.evaluate(() => ({
          captured: window.__blendPictures.map(({ case: k, time }) => ({
            case: k,
            time,
          })),
          frames: window.__scenePreviewWitness.frames,
          ended: window.__scenePreviewWitness.endedSession,
          timings: window.__blendTiming,
          timing_overflows: window.__blendTimingOverflows,
          timing_scope: window.__blendTimingScope,
        }));
        await save();
        expect(facts.preview_observation.captured).toHaveLength(6);
        const pausePlayback = page.getByRole("button", {
          name: "Pause scene audio",
          exact: true,
        });
        if (await pausePlayback.isEnabled()) {
          await pausePlayback.click();
          await expect(
            page.getByText("Scene sound paused", { exact: true }),
          ).toBeVisible();
        }
        const pictures = await page.evaluate(() =>
          window.__readBlendPictures(),
        );
        const validatedPreview = [];
        expect(pictures.map((p) => p.case).sort()).toEqual([0, 1, 2, 3, 4, 5]);
        for (const p of pictures) {
          const raw = Buffer.from(p.bytes);
          delete p.bytes;
          const file = path.join(directory, `d2-preview-${p.case}.rgba`);
          await fs.writeFile(file, raw);
          validatedPreview.push({
            ...p,
            sha256: sha(raw),
            file,
            pixels: checkPixels(raw, p.case, 2),
          });
        }
        facts.preview = validatedPreview;
        facts.preview_playback_clock = await page.evaluate(() => ({
          frames: window.__scenePreviewWitness.frames,
          progresses: window.__scenePreviewWitness.progresses,
          controls: window.__scenePreviewWitness.controls,
          overflows: window.__scenePreviewWitness.overflows,
        }));
        validatePlaybackEvidence({
          ...facts,
          preview_playback_version: PREVIEW_EVIDENCE_VERSION,
        });
        facts.preview_playback_version = PREVIEW_EVIDENCE_VERSION;
        await save();
      }
      validatePlaybackEvidence(facts);
      for (const picture of facts.preview) {
        const raw = await fs.readFile(picture.file);
        expect(sha(raw)).toBe(picture.sha256);
        picture.pixels = checkPixels(raw, picture.case, 2);
      }
      // A three-second playback may already have ended. Start a fresh owned
      // session, then seek while that SAME session is consuming sound. A new
      // session resetting generation to 1 is not evidence of a live seek.
      await open();
      await page
        .getByRole("textbox", { name: /^Playhead \(seconds\)/i })
        .fill("0.25");
      await page
        .getByRole("button", { name: "Go to time", exact: true })
        .click();
      await page
        .getByRole("button", { name: "Play scene preview", exact: true })
        .click();
      await expect
        .poll(
          () =>
            page.evaluate(() => {
              const w = window.__scenePreviewWitness;
              return (
                w.session?.state === "playing" &&
                Number(w.progress?.consumedFrames) > 0 &&
                w.frames.length > 0
              );
            }),
          { timeout: 15000 },
        )
        .toBe(true);
      const previous = await page.evaluate(() => ({
        session_id: window.__scenePreviewWitness.session.session_id,
        generation: window.__scenePreviewWitness.session.generation,
      }));
      const previousGeneration = previous.generation;
      await page
        .getByRole("textbox", { name: /^Playhead \(seconds\)/i })
        .fill("0.25");
      await page
        .getByRole("textbox", { name: /^Playhead \(seconds\)/i })
        .press("Enter");
      const readNewGeneration = () =>
        page.evaluate((previous) => {
          const w = window.__scenePreviewWitness;
          return w.session?.session_id === previous.session_id &&
            w.session.generation !== previous.generation
            ? (w.frames.find(
                (f) =>
                  f.session_id === previous.session_id &&
                  f.generation === w.session.generation,
              )?.generation ?? null)
            : null;
        }, previous);
      await expect.poll(readNewGeneration, { timeout: 15000 }).not.toBeNull();
      const newGeneration = await readNewGeneration();
      await expect
        .poll(
          () =>
            page.evaluate(
              ({ g, s }) =>
                new Set(
                  window.__scenePreviewWitness.frames
                    .filter((f) => f.session_id === s && f.generation === g)
                    .map((f) => f.time),
                ).size,
              { g: newGeneration, s: previous.session_id },
            ),
          { timeout: 15000 },
        )
        .toBeGreaterThanOrEqual(2);
      facts.preview_seek = {
        previous_session_id: previous.session_id,
        new_session_id: previous.session_id,
        previous_generation: previousGeneration,
        new_generation: newGeneration,
        target_seconds: 0.25,
      };
      const pause = page.getByRole("button", {
        name: "Pause scene audio",
        exact: true,
      });
      await expect(pause).toBeEnabled();
      await pause.click();
      await expect(
        page.getByText("Scene sound paused", { exact: true }),
      ).toBeVisible();
      // Let the final authorized paused frame settle, then prove that the
      // consumer, authority and presented picture stay fixed together.
      await expect
        .poll(() =>
          page.evaluate(() => {
            const s = window.__scenePreviewWitness.session;
            return (
              s?.state === "paused" &&
              Number(
                document.querySelector("canvas.editor-scene-frame")?.dataset.time,
              ) ===
                Number(s.presentation_time.ticks) /
                  s.presentation_time.timebase
            );
          }),
        )
        .toBe(true);
      facts.preview_pause = await page.evaluate(async () => {
        const sample = () => {
          const w = window.__scenePreviewWitness,
            canvas = document.querySelector("canvas.editor-scene-frame");
          return {
            state: w.session.state,
            session_id: w.session.session_id,
            authoritative_generation: w.session.generation,
            consumed_frames: w.progress.consumedFrames,
            presentation_time: w.session.presentation_time,
            frame_count: w.frames.length,
            frame_time: canvas?.dataset.time ?? null,
            generation: canvas?.dataset.generation ?? null,
          };
        };
        const before = sample(),
          start = performance.now();
        await new Promise((resolve) => setTimeout(resolve, 300));
        return {
          before,
          after: sample(),
          elapsed_ms: performance.now() - start,
        };
      });
      const w = await page.evaluate(() => ({
        frames: window.__scenePreviewWitness.frames,
        progresses: window.__scenePreviewWitness.progresses,
        controls: window.__scenePreviewWitness.controls,
        overflows: window.__scenePreviewWitness.overflows,
      }));
      facts.preview_clock = {
        frames: [...facts.preview_playback_clock.frames, ...w.frames],
        progresses: [
          ...facts.preview_playback_clock.progresses,
          ...w.progresses,
        ],
        controls: [...facts.preview_playback_clock.controls, ...w.controls],
        overflows: facts.preview_playback_clock.overflows + w.overflows,
      };
      expect((await inspect()).revision).toBe(facts.revision);
      facts.preview_evidence_version = PREVIEW_EVIDENCE_VERSION;
      validatePreviewEvidence(facts);
      facts.preview_verified = true;
      await save();
    }
    if (!outputsOnly) {
      validatePreviewEvidence(facts);
      for (const picture of facts.preview ?? []) {
        const raw = await fs.readFile(picture.file);
        expect(sha(raw)).toBe(picture.sha256);
        picture.pixels = checkPixels(raw, picture.case, 2);
      }
    }
    for (const kind of ["png", "mp4"]) {
      const rec = (facts.jobs[kind] ??= {}),
        name = "opencut_render_submit";
      if (!rec.submission) {
        await open();
        if (kind === "png") {
          await page
            .getByRole("textbox", { name: /^Playhead \(seconds\)/i })
            .fill(".25");
          await page
            .getByRole("button", { name: "Go to time", exact: true })
            .click();
        } else
          await page
            .getByRole("button", { name: "Export video MP4", exact: true })
            .click();
        rec.submission = await exchange(`job-${kind}`, name, () =>
          page
            .getByRole("button", {
              name:
                kind === "png" ? "Render scene frame" : "Start video export",
              exact: true,
            })
            .click(),
        );
        await save();
      }
      let job;
      await expect
        .poll(
          async () => {
            job = await tool("opencut_job_view", {
              job_id: rec.submission.job_id,
            });
            return ["succeeded", "failed", "cancelled"].includes(job.state)
              ? job.state
              : "pending";
          },
          { timeout: 300000, intervals: [500, 1000] },
        )
        .not.toBe("pending");
      rec.job = job;
      await save();
      expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
      const artifact = job.artifacts.find(
          (a) => a.mime_type === (kind === "png" ? "image/png" : "video/mp4"),
        ),
        file =
          rec.output?.file ?? path.join(directory, `d2-${job.job_id}.${kind}`);
      expect(artifact).toBeTruthy();
      let bytes;
      try {
        bytes = await fs.readFile(file);
      } catch (e) {
        if (e.code !== "ENOENT") throw e;
      }
      if (!bytes || sha(bytes) !== artifact.content_hash) {
        await open();
        await page.getByRole("button", { name: "Tasks", exact: true }).click();
        await page.getByLabel("Refresh tasks", { exact: true }).click();
        const cards = page
          .locator(".task-card")
          .filter({ hasText: facts.revision });
        await expect(cards.first()).toBeVisible();
        let found = false;
        for (let i = 0; i < (await cards.count()); i++) {
          const download = page.waitForEvent("download");
          await cards
            .nth(i)
            .getByRole("button", { name: "Download", exact: true })
            .click();
          const d = await download;
          if (d.suggestedFilename() === `opencut-${job.job_id}.${kind}`) {
            await d.saveAs(file);
            found = true;
            break;
          }
        }
        expect(found).toBe(true);
        bytes = await fs.readFile(file);
      }
      expect(sha(bytes)).toBe(artifact.content_hash);
      rec.output = { file, sha256: sha(bytes) };
      rec.pixels = [];
      for (const k of kind === "png" ? [0] : cases.map((_, i) => i)) {
        const raw = await decode([
          ...(kind === "mp4" ? ["-ss", String(k / 2 + 0.25)] : []),
          "-i",
          file,
          "-frames:v",
          "1",
          "-f",
          "rawvideo",
          "-pix_fmt",
          "rgba",
          "pipe:1",
        ]);
        rec.pixels.push(...checkPixels(raw, k, kind === "png" ? 2 : 6));
      }
      await save();
    }
    // The saved-revision package keeps the complete authored modes and resources.
    if (!facts.package_path) {
      facts.package_path = path.join(
        directory,
        `d2-blend-package-${facts.project_id}`,
      );
      facts.restored_id = randomUUID();
      await save();
    }
    if (!facts.stages["package-export"]?.data) {
      let exists = true;
      try {
        await fs.access(facts.package_path);
      } catch (error) {
        if (error.code !== "ENOENT") throw error;
        exists = false;
      }
      if (exists) {
        const inspected = await tool("opencut_project_package_inspect", {
          package_path: facts.package_path,
        });
        expect(inspected).toMatchObject({ source_project_id: facts.project_id,
          source_revision: facts.revision, mode: "self_contained", scope: "saved_revision" });
        facts.stages["package-export"] = {
          name: "opencut_project_package_export",
          input: {
            project_id: facts.project_id,
            revision: facts.revision,
            destination: facts.package_path,
            mode: "self_contained",
            scope: "saved_revision",
          },
          data: { package_path: facts.package_path, package: inspected },
          recovered_published_package: true,
        };
        await save();
      }
    }
    const exported = await operation(
      "package-export",
      "opencut_project_package_export",
      {
        project_id: facts.project_id,
        revision: facts.revision,
        destination: facts.package_path,
        mode: "self_contained",
        scope: "saved_revision",
      },
    );
    const restored = await operation(
      "package-restore",
      "opencut_project_package_import",
      {
        package_path: facts.package_path,
        target_project_id: facts.restored_id,
        expected_manifest_sha256: exported.package.manifest_sha256,
        idempotency_key: `d2-restore-${facts.restored_id}`,
      },
    );
    const restoredSnapshot = await editorSnapshot(tool("opencut_project_inspect", {
      project_id: facts.restored_id,
      revision: restored.revision,
    }));
    expect(restoredSnapshot.project).toEqual({
      ...facts.before_history.project,
      id: facts.restored_id,
    });
    facts.package = { exported, restored, all_modes_preserved: true };
    facts.pageerrors = errors;
    expect(errors).toEqual([]);
    facts.status = outputsOnly ? "outputs_passed_preview_pending" : "passed";
    facts.oracle = {
      model: BLEND_ORACLE_MODEL,
      cases,
      all_modes: CLASSIC_BLEND_MODES,
    };
    await save();
    check(
      outputsOnly
        ? "D2a independent PNG/MP4 and package restore only; consumer preview remains unverified"
        : "D2a Classic 17 blend modes: editor/history, consumer preview, independent PNG/MP4 and package restore",
      facts,
    );
  } catch (error) {
    await fs.writeFile(
      path.join(directory, "d2-blend-failure-ui.txt"),
      await page.locator("body").ariaSnapshot(),
    );
    facts.status = "failed";
    facts.failure = String(error);
    facts.pageerrors = errors;
    await save();
    throw error;
  } finally {
    await page.close();
  }
}
