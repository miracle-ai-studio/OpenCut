import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { composeGraphicOracle } from "./graphic-oracle.mjs";
import {
  GRAPHIC_OUTPUT_ORACLE_MODEL,
  archiveRawMp4Observations,
  hasCurrentPixelObservation,
  loadGraphicOutputReference,
  measureMixedPixels,
  observeYuv420Rgba,
  validateGraphicReferenceSeed,
  graphicOutputHash as hash,
} from "./graphic-output-oracle.mjs";

const frameBytes = 320 * 180 * 4;
const opaque = (length, color) =>
  Buffer.from(Array.from({ length }, (_, i) => color[i % 4]));
const specs = [
  { kind: "rectangle", x: -80, y: -45 },
  { kind: "ellipse", x: 80, y: -45 },
  { kind: "polygon", x: -80, y: 45 },
  { kind: "star", x: 80, y: 45 },
];
const passes = (m) =>
  m.alphaErrors === 0 &&
  m.mean <= m.limits.mean &&
  m.regions.every(
    (r) =>
      r.expectedPixels > 100 &&
      r.mean <= m.limits.regionMean &&
      r.iou >= m.limits.iou,
  );

test("raw MP4 success and failure survive migration verbatim; preview evidence is retained", () => {
  const before = { kind: "mp4", time: 0, passed: true, mean: 2.5 };
  const failed = {
    kind: "mp4",
    time: 0.5,
    passed: false,
    regions: [{ iou: 0.865 }],
  };
  const preview = { kind: "preview", time: 0, passed: true };
  const facts = { pixels: [before, failed, preview] };
  assert.equal(archiveRawMp4Observations(facts), true);
  assert.deepEqual(facts.pixels_raw_rgba, [before, failed]);
  assert.deepEqual(facts.pixels, [preview]);
  assert.equal(archiveRawMp4Observations(facts), false);
  assert.equal(facts.pixels_raw_rgba.length, 2);
});

test("MP4 resume requires model and current reference identity; existing preview remains reusable", () => {
  const sample = { kind: "mp4", time: 0.5 };
  const facts = {
    pixels: [
      { ...sample, passed: true },
      { kind: "preview", time: 0.5, passed: true },
    ],
  };
  assert.equal(hasCurrentPixelObservation(facts, sample, "reference-a"), false);
  facts.pixels[0].model = GRAPHIC_OUTPUT_ORACLE_MODEL;
  facts.pixels[0].reference_key = "reference-a";
  assert.equal(hasCurrentPixelObservation(facts, sample, "reference-a"), true);
  assert.equal(hasCurrentPixelObservation(facts, sample, "reference-b"), false);
  facts.pixels[0].model = "old-model";
  assert.equal(hasCurrentPixelObservation(facts, sample, "reference-a"), false);
  assert.equal(
    hasCurrentPixelObservation(
      facts,
      { kind: "preview", time: 0.5 },
      "reference-b",
    ),
    true,
  );
});

test("reference seeds reject changed fonts, authored fields, implementation or bytes", () => {
  const expected = opaque(frameBytes * 90, [0, 0, 0, 255]);
  const base = expected;
  const identity = {
    graphic_model: "graphic-model",
    source_sha256: "source",
    fonts: [{ id: "a", sha256: "font" }],
    titles_sha256: "titles",
    captions_sha256: "captions",
    specs_sha256: hash("[]"),
    graphic_oracle_source_sha256: "graphic-code",
    independent_base_source_sha256: "base-code",
    ffmpeg_version_sha256: "ffmpeg",
  };
  const metadata = {
    ...identity,
    model: "graphic-model",
    independent: true,
    width: 320,
    height: 180,
    frame_count: 90,
    fps: 30,
    expected_sha256: hash(expected),
    base_sha256: hash(base),
  };
  assert.doesNotThrow(() =>
    validateGraphicReferenceSeed(metadata, identity, expected, base, "[]"),
  );
  for (const key of [
    "source_sha256",
    "fonts",
    "titles_sha256",
    "captions_sha256",
    "specs_sha256",
    "graphic_oracle_source_sha256",
    "independent_base_source_sha256",
    "ffmpeg_version_sha256",
  ]) {
    assert.throws(
      () =>
        validateGraphicReferenceSeed(
          metadata,
          { ...identity, [key]: "changed" },
          expected,
          base,
          "[]",
        ),
      /identity differs/,
    );
  }
  assert.throws(
    () =>
      validateGraphicReferenceSeed(
        metadata,
        identity,
        expected.subarray(4),
        base,
        "[]",
      ),
    /content differs/,
  );
  assert.throws(
    () =>
      validateGraphicReferenceSeed(metadata, identity, expected, base, "[1]"),
    /content differs/,
  );
});

test("YUV420 observation preserves sequence length and alpha without H264 encoding", async () => {
  const rgba = Buffer.concat([
    opaque(4 * 4 * 4, [255, 0, 0, 255]),
    opaque(4 * 4 * 4, [0, 255, 0, 255]),
  ]);
  const result = await observeYuv420Rgba(rgba, { width: 4, height: 4 });
  assert.equal(result.length, rgba.length);
  for (let i = 0; i < result.length; i++)
    assert.ok(Math.abs(result[i] - rgba[i]) <= 2);
  for (let i = 3; i < result.length; i += 4) assert.equal(result[i], 255);
});

test("invalid or failed format conversion cannot produce an accepted reference", async () => {
  await assert.rejects(
    observeYuv420Rgba(Buffer.alloc(16), { width: 3, height: 2 }),
    /dimensions/,
  );
  await assert.rejects(
    observeYuv420Rgba(Buffer.alloc(16), { width: 2, height: 2 }),
    /opaque/,
  );
  await assert.rejects(
    observeYuv420Rgba(opaque(16, [0, 0, 0, 255]), {
      width: 2,
      height: 2,
      ffmpeg: "/no-such-graphic-oracle-ffmpeg",
    }),
  );
  assert.throws(
    () => measureMixedPixels([], [], [], specs, "mp4"),
    /dimensions/,
  );
});

test("original limits reject missing, shifted and wrong-color graphics after chroma projection", async () => {
  const base = opaque(frameBytes, [253, 0, 0, 255]);
  for (let y = 100; y < 170; y += 6)
    for (let x = 32; x < 288; x++)
      for (let c = 0; c < 3; c++) base[(y * 320 + x) * 4 + c] = 255;
  const layers = specs.map((spec, i) => ({
    source: {
      width: 12,
      height: 12,
      data: opaque(
        12 * 12 * 4,
        [
          [232, 64, 80, 210],
          [64, 192, 144, 210],
          [176, 96, 224, 210],
          [80, 160, 240, 210],
        ][i],
      ),
    },
    centerX: 160 + spec.x,
    centerY: 90 + spec.y,
    scaleX: 4,
    scaleY: 4,
    rotation: 0.1,
    opacity: 0.9,
  }));
  const expected = await observeYuv420Rgba(
    composeGraphicOracle(layers, 320, 180, base),
  );
  const observedBase = await observeYuv420Rgba(base);
  const positive = measureMixedPixels(
    expected,
    expected,
    observedBase,
    specs,
    "mp4",
  );
  assert.deepEqual(positive.limits, { mean: 5, regionMean: 6, iou: 0.9 });
  assert.equal(passes(positive), true);
  for (const mutation of ["missing-all", "missing-star", "shift", "color"]) {
    let changed = structuredClone(layers);
    if (mutation === "missing-all") changed = [];
    if (mutation === "missing-star") changed.pop();
    if (mutation === "shift") changed[3].centerX += 8;
    if (mutation === "color")
      for (let i = 0; i < changed[3].source.data.length; i += 4)
        for (let c = 0; c < 3; c++)
          changed[3].source.data[i + c] = 255 - changed[3].source.data[i + c];
    const actual = await observeYuv420Rgba(
      composeGraphicOracle(changed, 320, 180, base),
    );
    const result = measureMixedPixels(
      actual,
      expected,
      observedBase,
      specs,
      "mp4",
    );
    assert.equal(passes(result), false, mutation);
    assert.ok(
      result.regions[3].mean > 6 || result.regions[3].iou < 0.9,
      mutation,
    );
  }
});

test("cached sequences are reused only after identity, length and content validation", async () => {
  const directory = await fs.mkdtemp(
    path.join(os.tmpdir(), "graphic-output-reference-test-"),
  );
  try {
    const expected = opaque(frameBytes * 90, [253, 0, 0, 255]);
    const identity = {
      model: GRAPHIC_OUTPUT_ORACLE_MODEL,
      fixture: "local-unit-test",
    };
    let generated = 0;
    const generate = async () => {
      generated++;
      return { expected, base: expected, frames: [], unique_bases: 1 };
    };
    const first = await loadGraphicOutputReference({
      directory,
      identity,
      generate,
    });
    const second = await loadGraphicOutputReference({
      directory,
      identity,
      generate,
    });
    assert.equal(generated, 1);
    assert.equal(first.metadata.key, second.metadata.key);
    assert.deepEqual(first.expected, second.expected);
    const file = path.join(first.metadata.directory, "expected-observed.rgba");
    const corrupted = Buffer.from(first.expected);
    corrupted[0] ^= 1;
    await fs.writeFile(file, corrupted);
    await assert.rejects(
      loadGraphicOutputReference({ directory, identity, generate }),
      /cache content differs/,
    );
    assert.equal(generated, 1);
  } finally {
    await fs.rm(directory, { recursive: true, force: true });
  }
});
