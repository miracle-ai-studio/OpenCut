import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import {
  GRAPHIC_ORACLE_MODEL,
  composeGraphicOracle,
  readGraphicOracleInputs,
} from "./graphic-oracle.mjs";
import {
  GRAPHIC_OUTPUT_ORACLE_MODEL,
  archiveRawMp4Observations,
  hasCurrentPixelObservation,
  loadGraphicOutputReference,
  measureMixedPixels,
} from "./graphic-output-oracle.mjs";
import {
  installScenePreviewWitness,
  trackScenePreviewPumps,
} from "./verify-scene-preview.mjs";

const execute = promisify(execFile);
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const readJson = async (filename) =>
  JSON.parse(await fs.readFile(filename, "utf8"));
const seconds = (time) => Number(time.ticks) / time.timebase;
const prefix = (label) =>
  new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
async function candidateIdentity() {
  expect(process.env.OPENCUT_WEB_BINARY).toBeTruthy();
  expect(process.env.OPENCUT_WEB_ROOT).toBeTruthy();
  const entries = [];
  async function visit(relative = "") {
    for (const item of await fs.readdir(
      path.join(process.env.OPENCUT_WEB_ROOT, relative),
      { withFileTypes: true },
    )) {
      const name = path.join(relative, item.name);
      if (item.isSymbolicLink())
        throw Error(`Unhashed candidate symlink ${name}`);
      if (item.isDirectory()) await visit(name);
      else if (/\.(js|mjs|cjs|css)$/u.test(name))
        entries.push([
          name,
          sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name))),
        ]);
    }
  }
  await visit();
  entries.sort((a, b) => a[0].localeCompare(b[0]));
  expect(entries.length).toBeGreaterThan(0);
  return {
    binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    web_js_css_tree_sha256: sha(JSON.stringify(entries)),
  };
}
async function ffmpeg(args) {
  const { stdout } = await execute(
    process.env.OPENCUT_FFMPEG ?? "ffmpeg",
    ["-hide_banner", "-loglevel", "error", ...args],
    { encoding: "buffer", maxBuffer: 32 * 1024 * 1024 },
  );
  return stdout;
}
const decodeImage = (file, time) =>
  ffmpeg([
    "-ss",
    String(time),
    "-i",
    file,
    "-frames:v",
    "1",
    "-f",
    "image2pipe",
    "-vcodec",
    "png",
    "pipe:1",
  ]);
const decodePcm = (file, channels) =>
  ffmpeg([
    "-i",
    file,
    "-map",
    "0:a:0",
    "-ar",
    "48000",
    "-ac",
    String(channels),
    "-f",
    "f32le",
    "pipe:1",
  ]);
const floatValues = (bytes) =>
  Array.from({ length: bytes.length / 4 }, (_, i) => bytes.readFloatLE(i * 4));

// This fixed fixture deliberately reuses the independently verified B2/B3 white
// text styles. It reads original font bytes and authored fields, never layouts,
// GPU receipts or a PNG emitted by the product as an expected image.
export async function independentBase({ png, time, titles, captions, fonts }) {
  const image = new Image();
  image.src = `data:image/png;base64,${png}`;
  await image.decode();
  const canvas = new OffscreenCanvas(320, 180),
    ctx = canvas.getContext("2d");
  ctx.drawImage(image, 0, 0, 320, 180);
  const faces = [],
    families = new Map();
  try {
    for (const [i, font] of fonts.entries()) {
      const family = `GraphicOutputIndependent${i}`;
      const face = new FontFace(
        family,
        Uint8Array.from(atob(font.bytes), (c) => c.charCodeAt(0)),
      );
      await face.load();
      document.fonts.add(face);
      faces.push(face);
      families.set(font.id, family);
    }
    for (const title of titles) {
      const size = 2 * title.text.font_size.value,
        lines = title.text.content.split("\n"),
        lineHeight = size * title.text.line_height_multiplier;
      ctx.font = `${size}px "${families.get(title.text.font.font_id)}"`;
      ctx.textBaseline = "middle";
      ctx.textAlign = "center";
      ctx.fillStyle = "white";
      const x = 160 + title.visual.transform.position_x_px,
        y = 90 + title.visual.transform.position_y_px;
      lines.forEach((line, i) =>
        ctx.fillText(line, x, y + (i - (lines.length - 1) / 2) * lineHeight),
      );
    }
    for (const cue of captions) {
      const start =
        Number(cue.timeline_range.start.ticks) /
        cue.timeline_range.start.timebase;
      const duration =
        Number(cue.timeline_range.duration.ticks) /
        cue.timeline_range.duration.timebase;
      if (time < start || time >= start + duration) continue;
      ctx.font = `24px "${families.get(cue.style.font.font_id)}"`;
      ctx.textBaseline = "middle";
      ctx.textAlign = "center";
      ctx.fillStyle = "white";
      const lines = [];
      for (const paragraph of cue.text.replace(/\r\n?/g, "\n").split("\n")) {
        const words = paragraph.trim().split(/\s+/u).filter(Boolean);
        if (!words.length) {
          lines.push("");
          continue;
        }
        let line = "";
        for (const word of words) {
          const next = line ? `${line} ${word}` : word;
          if (line && ctx.measureText(next).width > 256) {
            lines.push(line);
            line = word;
          } else line = next;
        }
        lines.push(line);
      }
      const lineHeight = 28.8,
        top = 171 - lines.length * lineHeight;
      lines.forEach((line, i) =>
        ctx.fillText(line, 160, top + (i + 0.5) * lineHeight),
      );
    }
    return Array.from(ctx.getImageData(0, 0, 320, 180).data);
  } finally {
    for (const face of faces) document.fonts.delete(face);
  }
}
export async function graphicOutputReference(page, directory, facts, fonts) {
  const specs = facts.graphics.map((g) => g.spec);
  const identity = {
    model: GRAPHIC_OUTPUT_ORACLE_MODEL,
    graphic_model: GRAPHIC_ORACLE_MODEL,
    width: 320,
    height: 180,
    frame_count: 90,
    fps: 30,
    source_sha256: sha(await fs.readFile(path.join(directory, "sample.mp4"))),
    fonts: facts.fonts.map((font) => ({
      id: font.id,
      sha256: font.content.content_hash,
    })),
    specs_sha256: sha(JSON.stringify(specs)),
    titles_sha256: sha(JSON.stringify(facts.titles)),
    captions_sha256: sha(JSON.stringify(facts.captions)),
    graphic_oracle_source_sha256: sha(
      await fs.readFile(new URL("./graphic-oracle.mjs", import.meta.url)),
    ),
    independent_base_source_sha256: sha(independentBase.toString()),
    observation_source_sha256: sha(
      await fs.readFile(
        new URL("./graphic-output-oracle.mjs", import.meta.url),
      ),
    ),
    ffmpeg_version_sha256: sha(await ffmpeg(["-version"])),
  };
  return loadGraphicOutputReference({
    directory,
    identity,
    seedDirectory: process.env.OPENCUT_B4_GRAPHIC_REFERENCE,
    ffmpeg: process.env.OPENCUT_FFMPEG ?? "ffmpeg",
    generate: async () => {
      const expectedFrames = [],
        baseFrames = [],
        frames = [],
        cache = new Map();
      let layers;
      for (let frame = 0; frame < 90; frame++) {
        const time = frame / 30;
        const source = await decodeImage(
          path.join(directory, "sample.mp4"),
          time,
        );
        if (!layers)
          layers = (
            await page.evaluate(readGraphicOracleInputs, {
              png: source.toString("base64"),
              specs,
            })
          ).layers;
        const active = facts.captions
          .filter(
            (cue) =>
              time >= seconds(cue.timeline_range.start) &&
              time <
                seconds(cue.timeline_range.start) +
                  seconds(cue.timeline_range.duration),
          )
          .map((cue) => cue.id);
        const key = sha(source) + ":" + active.join(",");
        let cached = cache.get(key);
        if (!cached) {
          const base = await page.evaluate(independentBase, {
            png: source.toString("base64"),
            time,
            titles: facts.titles,
            captions: facts.captions,
            fonts,
          });
          cached = {
            base: Buffer.from(base),
            expected: Buffer.from(composeGraphicOracle(layers, 320, 180, base)),
          };
          cache.set(key, cached);
        }
        expectedFrames.push(cached.expected);
        baseFrames.push(cached.base);
        frames.push({
          frame,
          time,
          source_png_sha256: sha(source),
          active_captions: active,
          expected_sha256: sha(cached.expected),
          base_sha256: sha(cached.base),
        });
      }
      return {
        expected: Buffer.concat(expectedFrames),
        base: Buffer.concat(baseFrames),
        frames,
        unique_bases: cache.size,
      };
    },
  });
}
function pcmDifference(actual, expected) {
  expect(actual.length).toBe(expected.length);
  expect(actual.every(Number.isFinite) && expected.every(Number.isFinite)).toBe(
    true,
  );
  let square = 0,
    peak = 0,
    xy = 0,
    xx = 0,
    yy = 0;
  for (let i = 0; i < actual.length; i++) {
    const d = actual[i] - expected[i];
    square += d * d;
    peak = Math.max(peak, Math.abs(d));
    xy += actual[i] * expected[i];
    xx += actual[i] ** 2;
    yy += expected[i] ** 2;
  }
  return {
    frames: actual.length,
    rmse: Math.sqrt(square / actual.length),
    peak_error: peak,
    correlation: xy / Math.sqrt(xx * yy),
    rms: Math.sqrt(xx / actual.length),
    expected_rms: Math.sqrt(yy / expected.length),
  };
}

/** Reuses one owned B1/CAS workspace, B3 mixed source revision, D0a graphic
 * parameters and original requests/jobs. Failed stages remain inspectable.
 * Successful completion restores the complete entering project graph. */
export async function verifyGraphicOutput(
  browser,
  endpoint,
  directory,
  check,
) {
  const graphicSource = await readJson(
    path.join(directory, "b4-graphic-checkpoint.json"),
  );
  const captionSource = await readJson(
    path.join(directory, "b3-captions.json"),
  );
  expect(graphicSource.pixels_passed).toBe(true);
  expect(graphicSource.variant_frame.pixels_passed).toBe(true);
  expect(graphicSource.project_id).toBe(captionSource.project_id);
  const candidate = await candidateIdentity(),
    checkpointPath = path.join(directory, "b4-graphic-output-checkpoint.json");
  let facts;
  try {
    facts = await readJson(checkpointPath);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  if (facts) {
    expect(
      facts.candidate,
      "Retained output belongs to another candidate; preserve it and explicitly choose a fresh run",
    ).toEqual(candidate);
    expect(facts.project_id).toBe(graphicSource.project_id);
  } else
    facts = {
      candidate,
      project_id: graphicSource.project_id,
      sequence_id: captionSource.sequence_id,
      stages: {},
      phase: "bootstrap",
    };
  if (facts.phase === "passed") {
    expect(
      [0, 0.5, 1.5, 2.9].every((time) =>
        hasCurrentPixelObservation(
          facts,
          { kind: "mp4", time },
          facts.pixel_reference?.key,
        ),
      ),
      "Completed observations must match the current MP4 oracle model",
    ).toBe(true);
    check(
      "D0b graphic continuous preview and MP4 independent pixels/PCM",
      facts,
    );
    return facts;
  }
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
    acceptDownloads: true,
  });
  page.setDefaultTimeout(30000);
  const errors = [],
    calls = [],
    writes = [];
  let phase = "open",
    queue = Promise.resolve();
  const save = (values) => {
    Object.assign(facts, values, { active_phase: phase });
    const encoded = JSON.stringify(facts, null, 2) + "\n";
    queue = queue.then(() => fs.writeFile(checkpointPath, encoded));
    return queue;
  };
  page.on("pageerror", (error) => {
    if (errors.length < 32) errors.push(error.message);
  });
  page.on("request", (request) => {
    if (
      request.url().endsWith("opencut_visual_preview_frame") &&
      calls.length < 256
    )
      calls.push(request.postDataJSON());
    if (request.url().endsWith("opencut_edit_apply"))
      writes.push(request.postDataJSON());
  });
  await page.addInitScript(() =>
    Object.defineProperty(window, "showSaveFilePicker", {
      value: undefined,
      configurable: true,
    }),
  );
  await page.addInitScript(installScenePreviewWitness, { deferReadback: true });
  const readWitness = trackScenePreviewPumps(page);
  await page.addInitScript(() => {
    const samples = (window.__graphicOutputSamples = []);
    window.__graphicOutputCapture = {
      slot: "before",
      previousGeneration: null,
    };
    const transfer =
      ImageBitmapRenderingContext.prototype.transferFromImageBitmap;
    ImageBitmapRenderingContext.prototype.transferFromImageBitmap = function (
      bitmap,
    ) {
      const time = Number(this.canvas.dataset.time);
      const slot =
        time < 0.1
          ? "before"
          : time >= 0.5 && time < 1
            ? "during"
            : time >= 1.5 && time < 2.5
              ? "after"
              : null;
      if (
        bitmap &&
        this.canvas.classList.contains("editor-scene-frame") &&
        slot === window.__graphicOutputCapture.slot &&
        this.canvas.dataset.generation !==
          window.__graphicOutputCapture.previousGeneration &&
        !samples.some((s) => s.slot === slot)
      )
        samples.push({
          slot,
          time,
          generation: this.canvas.dataset.generation,
          bitmap: createImageBitmap(bitmap),
        });
      return transfer.call(this, bitmap);
    };
    window.__readGraphicOutputSamples = async () => {
      const values = [];
      for (const sample of samples) {
        const bitmap = await sample.bitmap;
        try {
          const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
          canvas.getContext("2d").drawImage(bitmap, 0, 0);
          const blob = await canvas.convertToBlob({ type: "image/png" }),
            bytes = new Uint8Array(await blob.arrayBuffer());
          let binary = "";
          for (const b of bytes) binary += String.fromCharCode(b);
          values.push({
            slot: sample.slot,
            time: sample.time,
            generation: sample.generation,
            width: bitmap.width,
            height: bitmap.height,
            png: btoa(binary),
          });
        } finally {
          bitmap.close();
        }
      }
      return values;
    };
  });
  const tool = async (name, input) => {
    const envelope = await page.evaluate(
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
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return envelope.data;
  };
  const inspect = (revision = null) =>
    editorSnapshot(tool("opencut_project_inspect", {
      project_id: facts.project_id,
      revision,
    }));
  const replay = async (stage) => {
    const item = facts.stages[stage];
    if (!item.receipt) {
      item.receipt = await tool(item.operation, item.request);
      await save({});
    }
    return item.receipt;
  };
  const reopen = async () => {
    await page.goto(
      `${endpoint}/editor/${encodeURIComponent(facts.project_id)}?scene=${encodeURIComponent(facts.sequence_id)}`,
    );
    await expect(
      page.getByRole("main", { name: "OpenCut editor", exact: true }),
    ).toBeVisible();
    const guide = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await guide.isVisible())
      await guide.getByRole("button", { name: "Close", exact: true }).click();
    await page
      .getByRole("combobox", { name: prefix("Scene") })
      .selectOption(facts.sequence_id);
  };
  try {
    await reopen();
    if (!facts.original) await save({ original: await inspect() });
    phase =
      "compose existing media text captions and accepted graphic elements";
    if (!facts.stages.compose) {
      const source = await inspect(captionSource.final_revision),
        scene = structuredClone(
          source.project.sequences.find((s) => s.id === facts.sequence_id),
        );
      expect(scene).toBeTruthy();
      expect(source.project.settings).toMatchObject({
        profile: "classic_global",
        settings: {
          canvas_size: { width: 320, height: 180 },
          fps: { numerator: 30, denominator: 1 },
        },
      });
      const graphics = structuredClone(graphicSource.graphics);
      expect(graphics.map((g) => g.spec.kind)).toEqual([
        "rectangle",
        "ellipse",
        "polygon",
        "star",
      ]);
      for (const graphic of graphics) {
        expect(scene.tracks.some((t) => t.id === graphic.track_id)).toBe(false);
        scene.tracks.push({
          id: graphic.track_id,
          name: `D0b ${graphic.spec.kind}`,
          kind: "graphic",
          enabled: true,
          muted: false,
          locked: false,
          opacity: 1,
          gain: { unit: "decibels", value: 0 },
          channels: [],
          elements: [graphic.element],
        });
      }
      const elements = scene.tracks
        .filter((t) => t.enabled)
        .flatMap((t) => t.elements)
        .filter((e) => e.enabled);
      const media = elements.filter((e) =>
        ["audio", "video"].includes(e.content.kind),
      );
      expect(media.map((e) => e.name).sort()).toEqual([
        "sample.mp4",
        "sample.wav",
      ]);
      for (const element of media) {
        expect(seconds(element.timeline_range.start)).toBe(0);
        expect(seconds(element.timeline_range.duration)).toBe(3);
        expect(seconds(element.source_span.trim_start)).toBe(0);
        expect(element.content.retime.rate).toEqual({
          numerator: 1,
          denominator: 1,
        });
        expect(element.content.audio).toMatchObject({
          gain: { unit: "decibels", value: 0 },
          muted: false,
        });
        const track = scene.tracks.find((t) =>
          t.elements.some((e) => e.id === element.id),
        );
        expect(track.gain).toEqual({ unit: "decibels", value: 0 });
        expect(track.muted).toBe(false);
        if (element.content.kind === "video")
          expect(element.content.source_audio_enabled).toBe(false);
        const asset = source.project.assets.find(
          (a) => a.id === element.content.asset_id,
        );
        expect(sha(await fs.readFile(path.join(directory, element.name)))).toBe(
          asset.version.content_hash,
        );
      }
      const titles = elements
        .filter((e) => e.content.kind === "text")
        .map((e) => ({ id: e.id, text: e.content.text, visual: e.visual }));
      expect(titles).toHaveLength(2);
      expect(scene.captions).toHaveLength(1);
      for (const title of titles) {
        expect(title.text.font_size).toEqual({
          unit: "classic_height90",
          value: 15,
        });
        expect(title.text).toMatchObject({
          align: "center",
          decoration: "none",
          letter_spacing_px: 0,
          background: { enabled: false },
          color: { red: 1, green: 1, blue: 1, alpha: 1 },
        });
        expect(title.visual).toMatchObject({
          opacity: 1,
          blend_mode: "normal",
          transform: { scale_x: 1, scale_y: 1, rotation_degrees: 0 },
        });
      }
      expect(scene.captions[0].style).toMatchObject({
        font_size: { unit: "canvas_pixels", value: 24 },
        layout: "pinned_caption_v4",
        text_align: "center",
        line_height_px: null,
        max_width_px: null,
        background_color: null,
        color: { red: 1, green: 1, blue: 1, alpha: 1 },
      });
      const fontIds = new Set([
        ...titles.map((t) => t.text.font.font_id),
        ...scene.captions.map((c) => c.style.font.font_id),
      ]);
      const fonts = source.project.fonts.filter((f) => fontIds.has(f.id));
      for (const font of fonts)
        expect(
          facts.original.project.fonts.find((f) => f.id === font.id)?.content,
        ).toEqual(font.content);
      const current = await inspect();
      facts.stages.compose = {
        operation: "opencut_edit_apply",
        request: {
          project_id: facts.project_id,
          expected_revision: current.revision,
          idempotency_key: randomUUID(),
          label: "D0b shared graphic preview and video output",
          operations: [
            { type: "settings_set", settings: source.project.settings },
            { type: "sequence_put", sequence: scene },
          ],
        },
      };
      await save({
        graphics,
        titles,
        captions: scene.captions,
        fonts: fonts.map((f) => ({
          id: f.id,
          family: f.family,
          content: f.content,
        })),
        expected_scene: scene,
      });
    }
    const composition = await replay("compose");
    await save({ revision: composition.revision });
    expect(
      (await inspect(composition.revision)).project.sequences.find(
        (s) => s.id === facts.sequence_id,
      ),
    ).toEqual(facts.expected_scene);
    if (!facts.preview_passed) {
      if (!facts.preview_capture_complete) {
        // Source decoding and arithmetic are outside the live three-second
        // control window. Only bounded observations are copied during playback.
        const pcmReference = facts.preview_pcm
          ? null
          : floatValues(await decodePcm(path.join(directory, "sample.wav"), 1));
        expect((await inspect()).revision).toBe(facts.revision);
        await reopen();
        phase = "continuous mixed preview and seek";
        const initialWrites = writes.length;
        const retainGeneration = async (
          slot,
          generation,
          previousGeneration = null,
        ) => {
          const observed = await page.evaluate(
            ({ slot, generation, previousGeneration }) => {
              const w = window.__scenePreviewWitness;
              const sample = window.__graphicOutputSamples.find(
                (s) => s.slot === slot,
              );
              return {
                slot,
                generation,
                previousGeneration,
                frames: w.frames
                  .filter((f) => f.generation === generation)
                  .map((f) => ({ ...f })),
                sample: sample
                  ? {
                      slot: sample.slot,
                      time: sample.time,
                      generation: sample.generation,
                    }
                  : null,
                session_identity: w.session?.identity ?? null,
              };
            },
            { slot, generation, previousGeneration },
          );
          await save({
            preview_generations: [
              ...(facts.preview_generations ?? []).filter(
                (g) => g.slot !== slot,
              ),
              observed,
            ],
            graphic_frame_calls: calls,
          });
        };
        await page
          .getByRole("textbox", { name: prefix("Playhead (seconds)") })
          .fill("0");
        await page
          .getByRole("button", { name: "Go to time", exact: true })
          .click();
        const live = page.locator("canvas.editor-scene-frame");
        await page
          .getByRole("button", { name: "Play scene preview", exact: true })
          .click();
        await expect(live).toBeVisible();
        const firstGeneration = await live.getAttribute("data-generation");
        await expect
          .poll(() =>
            page.evaluate(() =>
              window.__graphicOutputSamples.some((s) => s.slot === "before"),
            ),
          )
          .toBe(true);
        await expect
          .poll(() =>
            page.evaluate(
              (generation) =>
                new Set(
                  window.__scenePreviewWitness.frames
                    .filter((f) => f.generation === generation)
                    .map((f) => f.time),
                ).size,
              firstGeneration,
            ),
          )
          .toBeGreaterThanOrEqual(3);
        await expect
          .poll(async () => (await readWitness()).pcm.length)
          .toBeGreaterThanOrEqual(4096);
        const firstWitness = await readWitness();
        await save({
          preview_initial_supply: {
            identity: firstWitness.session.identity,
            actual_start_sample: firstWitness.session.actual_start_sample,
            blocks: firstWitness.blocks,
            pump: firstWitness.pumpWitness,
          },
          preview_pcm_samples: facts.preview_pcm
            ? undefined
            : firstWitness.pcm.slice(0, 4096),
        });
        await retainGeneration("before", firstGeneration);
        let previousGeneration = firstGeneration;
        for (const time of [0.5, 1.5]) {
          const old = previousGeneration,
            slot = time === 0.5 ? "during" : "after";
          await page.evaluate(
            ({ slot, previousGeneration }) => {
              window.__graphicOutputCapture = { slot, previousGeneration };
            },
            { slot, previousGeneration: old },
          );
          await page
            .getByRole("textbox", { name: prefix("Playhead (seconds)") })
            .fill(String(time));
          await page
            .getByRole("textbox", { name: prefix("Playhead (seconds)") })
            .press("Enter");
          await expect
            .poll(() => live.getAttribute("data-generation"))
            .not.toBe(old);
          const generation = await live.getAttribute("data-generation");
          await expect
            .poll(() =>
              page.evaluate(
                (slot) =>
                  window.__graphicOutputSamples.some((s) => s.slot === slot),
                time === 0.5 ? "during" : "after",
              ),
            )
            .toBe(true);
          await expect
            .poll(() =>
              page.evaluate(
                (generation) =>
                  new Set(
                    window.__scenePreviewWitness.frames
                      .filter((f) => f.generation === generation)
                      .map((f) => f.time),
                  ).size,
                generation,
              ),
            )
            .toBeGreaterThanOrEqual(3);
          await retainGeneration(slot, generation, old);
          previousGeneration = generation;
        }
        await page
          .getByRole("button", { name: "Pause scene audio", exact: true })
          .click();
        await expect(
          page.getByText("Scene sound paused", { exact: true }),
        ).toBeVisible();
        // Pause may retire the dedicated audio worker. The actual supply was
        // already observed while it was alive; presentation/consumer history is
        // owned by the page and remains readable without rediscovering a worker.
        const witness = await page.evaluate(() => {
          const w = window.__scenePreviewWitness;
          return {
            frames: structuredClone(w.frames),
            controls: structuredClone(w.controls),
            progresses: structuredClone(w.progresses),
            overflows: w.overflows,
          };
        });
        await save({
          preview: witness,
          graphic_frame_calls: calls,
          preview_paused: true,
          preview_observed_revision: (await inspect()).revision,
          preview_project_writes: writes.length - initialWrites,
        });
        const pictures = await page.evaluate(() =>
          window.__readGraphicOutputSamples(),
        );
        for (const picture of pictures) {
          picture.output = path.join(
            directory,
            `b4-graphic-preview-${picture.slot}.png`,
          );
          const bytes = Buffer.from(picture.png, "base64");
          await fs.writeFile(picture.output, bytes);
          delete picture.png;
          picture.sha256 = sha(bytes);
          await save({
            pictures: [
              ...(facts.pictures ?? []).filter((p) => p.slot !== picture.slot),
              picture,
            ],
          });
        }
        if (!facts.preview_pcm) {
          await save({
            preview_pcm: pcmDifference(
              facts.preview_pcm_samples,
              pcmReference.slice(0, 4096),
            ),
          });
        }
        await save({ preview_capture_complete: true });
      }
      expect(facts.preview_initial_supply.identity).toMatchObject({
        project_id: facts.project_id,
        revision: facts.revision,
        sequence_id: facts.sequence_id,
        sample_rate: 48000,
      });
      expect(facts.preview_initial_supply.actual_start_sample).toBe("0");
      expect(facts.preview_pcm.frames).toBe(4096);
      expect(facts.preview_pcm.peak_error).toBeLessThan(1e-6);
      expect(facts.preview_generations.map((g) => g.slot).sort()).toEqual([
        "after",
        "before",
        "during",
      ]);
      for (const generation of facts.preview_generations) {
        expect(
          new Set(generation.frames.map((f) => f.time)).size,
        ).toBeGreaterThanOrEqual(3);
        expect(generation.sample.generation).toBe(generation.generation);
        expect(generation.generation).not.toBe(generation.previousGeneration);
      }
      const pictures = facts.pictures,
        witness = facts.preview,
        frameCalls = facts.graphic_frame_calls;
      expect(pictures.map((p) => p.slot).sort()).toEqual([
        "after",
        "before",
        "during",
      ]);
      expect(
        frameCalls.some(
          (c) => c.graphics?.length === 4 && c.rasters && c.captions,
        ),
      ).toBe(true);
      expect(witness.overflows).toBe(0);
      expect(
        witness.frames.every(
          (f) => f.actual - f.time <= 0.150001 && f.time <= f.actual + 0.000001,
        ),
      ).toBe(true);
      expect(facts.preview_paused).toBe(true);
      expect(facts.preview_observed_revision).toBe(facts.revision);
      expect(facts.preview_project_writes).toBe(0);
      for (const picture of pictures)
        expect(sha(await fs.readFile(picture.output))).toBe(picture.sha256);
      await save({ preview_passed: true });
    }
    phase = "original graphic MP4 UI submission";
    if (!facts.submission) {
      if (!facts.submit_request) {
        await reopen();
        await page
          .getByRole("button", { name: "Export video MP4", exact: true })
          .click();
        const dialog = page.getByRole("dialog", {
          name: "Export video MP4",
          exact: true,
        });
        const request = page.waitForRequest((r) =>
            r.url().endsWith("opencut_render_submit"),
          ),
          response = page.waitForResponse((r) =>
            r.url().endsWith("opencut_render_submit"),
          );
        const clicking = dialog
          .getByRole("button", { name: "Start video export", exact: true })
          .click();
        await save({ submit_request: (await request).postDataJSON() });
        await clicking;
        const envelope = await (await response).json();
        expect(envelope.error).toBeNull();
        await save({ submission: envelope.data });
      } else {
        if (facts.submit_request.kind !== "video") throw Error("Retained historical export has no observed job ID; resolve its original acceptance before current submission. Do not rewrite its request or guess encoding options.");
        await save({
          submission: await tool(
            "opencut_render_submit",
            facts.submit_request,
          ),
        });
      }
    }
    if (facts.submit_request.kind !== undefined) expect(facts.submit_request.kind).toBe("video");
    expect(facts.submit_request.kind === "video" ? facts.submit_request.request : facts.submit_request).toMatchObject({
      project_id: facts.project_id,
      revision: facts.revision,
      sequence_id: facts.sequence_id,
    });
    let job;
    await expect
      .poll(
        async () => {
          job = await tool("opencut_job_view", {
            job_id: facts.submission.job_id,
          });
          return ["succeeded", "failed", "cancelled"].includes(job.state)
            ? job.state
            : "pending";
        },
        { timeout: 300000, intervals: [250, 500, 1000] },
      )
      .not.toBe("pending");
    await save({ job });
    expect(job.state, JSON.stringify(job.failure)).toBe("succeeded");
    expect(job.kind).toBe("render.video");
    expect(job.result).toMatchObject({
      kind: "render.video",
      data: {
        identity: {
          project_id: facts.project_id,
          revision: facts.revision,
          sequence_id: facts.sequence_id,
          visual_profile: "scene_v1",
          width: 320,
          height: 180,
          frame_count: "90",
          audio_frame_count: "144000",
          options: { format: "mp4", include_audio: true },
        },
      },
    });
    const artifact = job.artifacts.find((a) => a.mime_type === "video/mp4");
    expect(artifact).toBeTruthy();
    phase = "task UI download original graphic MP4";
    const output = path.join(directory, "b4-graphic-mixed.mp4");
    let outputBytes;
    try {
      outputBytes = await fs.readFile(output);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
    if (
      !facts.download_completed ||
      !outputBytes ||
      sha(outputBytes) !== artifact.content_hash
    ) {
      await reopen();
      await page.getByRole("button", { name: "Tasks", exact: true }).click();
      await page.getByLabel("Refresh tasks", { exact: true }).click();
      const task = page
        .locator(".task-card")
        .filter({ hasText: facts.revision });
      await expect(task).toHaveCount(1);
      const download = page.waitForEvent("download");
      await task.getByRole("button", { name: "Download", exact: true }).click();
      await (await download).saveAs(output);
      outputBytes = await fs.readFile(output);
      expect(sha(outputBytes)).toBe(artifact.content_hash);
      await save({
        output,
        output_sha256: sha(outputBytes),
        download_completed: true,
      });
    }
    if (facts.stream_probe?.artifact_sha256 !== artifact.content_hash) {
      const probe = await execute(process.env.OPENCUT_FFPROBE ?? "ffprobe", ["-v", "error", "-show_streams", "-of", "json", output], { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 });
      await save({ stream_probe: { artifact_sha256: artifact.content_hash, streams: JSON.parse(probe.stdout).streams } });
    }
    const audioStreams = facts.stream_probe.streams.filter(stream => stream.codec_type === "audio");
    expect(audioStreams).toHaveLength(1);
    expect(audioStreams[0]).toMatchObject({ codec_name: "aac", sample_rate: "48000", channels: 2 });
    phase = "independent original-source pixels and PCM";
    const fontDirectory = process.env.OPENCUT_B1_FONT_SOURCES;
    expect(fontDirectory).toBeTruthy();
    const manifest = await readJson(path.join(fontDirectory, "manifest.json")),
      fonts = [];
    for (const font of facts.fonts) {
      const original = manifest.files.find(
        (f) =>
          f.sha256 === font.content.content_hash && f.file.endsWith(".ttf"),
      );
      expect(original).toBeTruthy();
      const bytes = await fs.readFile(original.file);
      expect(sha(bytes)).toBe(original.sha256);
      fonts.push({ ...font, bytes: bytes.toString("base64") });
    }
    facts.pixels ??= [];
    if (archiveRawMp4Observations(facts)) await save({});
    const reference = await graphicOutputReference(
      page,
      directory,
      facts,
      fonts,
    );
    await save({ pixel_reference: reference.metadata });
    const candidates = [
      ...facts.pictures.map((p) => ({ ...p, kind: "preview" })),
      ...[0, 0.5, 1.5, 2.9].map((time) => ({ time, kind: "mp4", output })),
    ];
    for (const sample of candidates) {
      if (hasCurrentPixelObservation(facts, sample, reference.metadata.key))
        continue;
      let actual, expected, base;
      if (sample.kind === "mp4") {
        actual = await ffmpeg([
          "-ss",
          String(sample.time),
          "-i",
          output,
          "-frames:v",
          "1",
          "-f",
          "rawvideo",
          "-pix_fmt",
          "rgba",
          "pipe:1",
        ]);
        const offset = Math.round(sample.time * 30) * 320 * 180 * 4;
        expected = reference.expected.subarray(offset, offset + 320 * 180 * 4);
        base = reference.base.subarray(offset, offset + 320 * 180 * 4);
        expect(actual.length).toBe(expected.length);
      } else {
        const png = await fs.readFile(sample.output);
        const source = await decodeImage(
          path.join(directory, "sample.mp4"),
          sample.time,
        );
        const input = await page.evaluate(readGraphicOracleInputs, {
          png: png.toString("base64"),
          specs: facts.graphics.map((g) => g.spec),
        });
        expect(input.width).toBe(320);
        expect(input.height).toBe(180);
        base = await page.evaluate(independentBase, {
          png: source.toString("base64"),
          time: sample.time,
          titles: facts.titles,
          captions: facts.captions,
          fonts,
        });
        expected = composeGraphicOracle(input.layers, 320, 180, base);
        actual = input.actual;
      }
      const observation = {
        kind: sample.kind,
        time: sample.time,
        ...(sample.kind === "mp4"
          ? {
              model: GRAPHIC_OUTPUT_ORACLE_MODEL,
              reference_key: reference.metadata.key,
              expected_sha256: sha(expected),
              base_sha256: sha(base),
            }
          : {}),
        ...measureMixedPixels(
          actual,
          expected,
          base,
          facts.graphics.map((g) => g.spec),
          sample.kind,
        ),
        passed: false,
      };
      facts.pixels = facts.pixels.filter(
        (p) =>
          p.kind !== sample.kind ||
          p.time !== sample.time ||
          p.model !== observation.model,
      );
      facts.pixels.push(observation);
      await save({});
      expect(observation.alphaErrors).toBe(0);
      expect(observation.mean).toBeLessThanOrEqual(observation.limits.mean);
      for (const region of observation.regions) {
        expect(region.expectedPixels).toBeGreaterThan(100);
        expect(region.mean).toBeLessThanOrEqual(observation.limits.regionMean);
        expect(region.iou).toBeGreaterThanOrEqual(observation.limits.iou);
      }
      observation.passed = true;
      await save({});
    }
    if (!facts.export_pcm?.passed) {
      const expected = floatValues(
          await decodePcm(path.join(directory, "sample.wav"), 1),
        ),
        actual = floatValues(await decodePcm(output, 2));
      expect(actual.length / 2).toBeGreaterThanOrEqual(144000);
      expect(actual.length / 2).toBeLessThanOrEqual(145024);
      const windows = [];
      for (const start of [9600, 72000, 120000])
        for (const channel of [0, 1]) {
          const values = Array.from(
            { length: 4096 },
            (_, i) => actual[(start + i) * 2 + channel],
          );
          const measured = {
            start,
            channel,
            ...pcmDifference(values, expected.slice(start, start + 4096)),
          };
          windows.push(measured);
          expect(measured.rmse).toBeLessThan(0.003);
          expect(measured.correlation).toBeGreaterThan(0.99);
        }
      await save({
        export_pcm: {
          passed: true,
          reference: "original sample.wav mono duplicated independently",
          decoded_frames: actual.length / 2,
          windows,
        },
      });
    }
    expect(errors).toEqual([]);
    phase = "restore entering project graph";
    if (!facts.stages.restore) {
      const current = await inspect();
      facts.stages.restore = {
        operation: "opencut_project_restore",
        request: {
          project_id: facts.project_id,
          expected_revision: current.revision,
          target_revision: facts.original.revision,
          idempotency_key: randomUUID(),
          asset_versions: {},
        },
      };
      await save({});
    }
    await replay("restore");
    const restored = await inspect();
    expect(restored.project).toEqual(facts.original.project);
    await save({
      phase: "passed",
      restored_revision: restored.revision,
      pageerrors: errors,
      scope:
        "Three-second fixed scene with existing media, two fonts, caption, four accepted graphics and sound; continuous consumer-clock preview, pause/seek, persistent MP4 task and actual UI download, independent source pixels and pre-master/AAC PCM. No full Classic parity or physical DAC claim.",
    });
    check(
      "D0b graphic continuous preview and MP4 independent pixels/PCM",
      facts,
    );
    return facts;
  } catch (error) {
    await save({
      phase: "failed",
      failure: String(error).slice(0, 4096),
      pageerrors: errors,
    });
    await page
      .screenshot({
        path: path.join(directory, "b4-graphic-output-failure.png"),
        fullPage: false,
      })
      .catch(() => {});
    throw error;
  } finally {
    await queue;
    await page.close();
  }
}
