import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { byteBudget } from "../src/validation.mjs";
import {
  validateSceneTextMaskV1,
  sceneFontResourcesV1,
  resolveSceneTextMaskFontsV1,
  prepareSceneTextMasksV1,
  sceneTextMaskSourceV1,
  releaseSceneTextMasksV1,
  sceneSynthesisEntriesV1,
  createSceneSynthesisV1,
} from "../src/scene-text-v1.mjs";
const hash = (value) => createHash("sha256").update(value).digest("hex");
// Byte/font/Canvas mocks below test adapter structure only. They are explicitly
// not native glyph evidence or expected-pixel proof for any real font.
const bytes = Uint8Array.of(1, 2, 3, 4);
const mask = () => ({
  id: "mask",
  enabled: true,
  inverted: false,
  feather: { unit: "classic_pixels", value: 12 },
  stroke: {
    color: { encoding: "srgb", red: 0.2, green: 0.4, blue: 0.8, alpha: 1 },
    width_px: 2,
    align: "inside",
  },
  shape: {
    kind: "text",
    mask: {
      content: "AB\nC",
      font: { kind: "locked", font_id: "font" },
      font_size: { unit: "classic_height90", value: 15 },
      font_weight: "normal",
      font_style: "normal",
      decoration: "underline",
      letter_spacing_px: 1.25,
      line_height_multiplier: 1.2,
      center_x: 0.1,
      center_y: -0.2,
      rotation_degrees: 30,
      scale: 1.5,
    },
  },
});
function input(m = mask()) {
  const font = {
      fontId: "font",
      contentHash: hash(bytes),
      size: String(bytes.length),
      weight: 400,
      style: "normal",
      faceIndex: 0,
      glyphCoverageValidated: true,
      validatedTextHash: hash(m.shape.mask.content),
    },
    quad = {
      centerX: 50,
      centerY: 30,
      width: 80.49,
      height: 44.6,
      rotationRadians: 0.3,
      flipX: false,
      flipY: true,
      opacity: 0.7,
    };
  const plan = {
    execution: { execution: { execution: { execution: { texts: [] } } } },
    stacks: [{ elementId: "e", masks: [m], effects: [] }],
    maskFonts: [{ maskId: m.id, font }],
    fontSynthesis: [],
  };
  const frame = {
    identity: {
      projectId: "p",
      revision: "r",
      sequenceId: "s",
      profile: "classic-v31-gpu-scene-frame-v1",
      width: 100,
      height: 60,
    },
    scene: { layers: [{ stack: plan.stacks[0], quad }] },
  };
  plan.identity = structuredClone(frame.identity);
  return { m, font, quad, plan, frame };
}
function globals() {
  const saved = {
    FontFace: globalThis.FontFace,
    OffscreenCanvas: globalThis.OffscreenCanvas,
    fonts: globalThis.fonts,
  };
  const faces = new Set(), createdFaces = [],
    canvases = [];
  class Face {
    constructor(family, content, options) {
      this.family = family;
      this.content = content;
      this.options = options;
      this.status = "unloaded";
      createdFaces.push(this);
    }
    async load() {
      this.status = "loaded";
      return this;
    }
  }
  class Canvas {
    constructor(width, height) {
      this.width = width;
      this.height = height;
      this.calls = [];
      this.context = {
        font: "",
        letterSpacing: "0px",
        save: () => this.calls.push(["save"]),
        restore: () => this.calls.push(["restore"]),
        measureText: (content) => ({
          width: content.length * 5,
          actualBoundingBoxAscent: 4,
          actualBoundingBoxDescent: 2,
        }),
        ...Object.fromEntries(
          [
            "translate",
            "scale",
            "rotate",
            "fillText",
            "fillRect",
            "strokeText",
            "drawImage",
          ].map((method) => [
            method,
            (...args) => this.calls.push([method, ...args]),
          ]),
        ),
      };
      canvases.push(this);
    }
    getContext() {
      return this.context;
    }
  }
  globalThis.FontFace = Face;
  globalThis.OffscreenCanvas = Canvas;
  globalThis.fonts = {
    add: (f) => faces.add(f),
    delete: (f) => faces.delete(f),
  };
  return {
    canvases,
    faces,
    createdFaces,
    restore() {
      for (const [name, value] of Object.entries(saved)) {
        if (value === undefined) delete globalThis[name];
        else globalThis[name] = value;
      }
    },
  };
}
const state = () => ({
  budget: byteBudget("10000000"),
  releases: [],
  canvases: new Set(),
  encoded: new Map(),
  resolveFont: async () => ({ bytes, release() {} }),
});
const admission = { maxDecodedPixels: "1000000", maxRgbaBytes: "10000000" };
test("strict typography rejects unresolved system fonts, invalid sizes and unbounded content", () => {
  validateSceneTextMaskV1(mask().shape.mask);
  for (const mutate of [
    (m) =>
      (m.font = {
        kind: "unresolved_family",
        family: "Arial",
        policy: { mode: "strict" },
      }),
    (m) => (m.font_size.value = 0),
    (m) => (m.scale = Infinity),
    (m) => (m.content = "x".repeat(65537)),
    (m) => (m.surprise = true),
  ]) {
    const raw = mask().shape.mask;
    mutate(raw);
    assert.throws(
      () => validateSceneTextMaskV1(raw),
      (e) => e.code === "INVALID_VISUAL_PLAN",
    );
  }
});
test("font union requires exact mask ownership, matching sampled face and one immutable shared resource", () => {
  const { plan } = input();
  assert.equal(sceneFontResourcesV1(plan).get("font").contentHash, hash(bytes));
  for (const mutate of [
    (p) => (p.maskFonts = []),
    (p) => (p.maskFonts[0].font.glyphCoverageValidated = false),
    (p) => (p.maskFonts[0].maskId = "other"),
    (p) => (p.maskFonts[0].font.weight = 700),
  ]) {
    const bad = structuredClone(plan);
    mutate(bad);
    assert.throws(
      () => sceneFontResourcesV1(bad),
      (e) => e.code === "INVALID_VISUAL_PLAN",
    );
  }
});
test("local rounded Canvas layout uses Classic height, T S R and matching typed-font stroke without system fallback", async () => {
  const g = globals();
  try {
    const { plan, frame } = input(),
      s = state();
    await prepareSceneTextMasksV1(plan, frame, s, admission, null);
    const source = sceneTextMaskSourceV1(frame, "mask");
    assert.equal(source.localWidth, 80);
    assert.equal(source.localHeight, 45);
    assert.equal(source.body.width, 80);
    assert.equal(source.body.height, 45);
    assert.ok(source.body.context.font.includes("7.5px"));
    assert.deepEqual(
      source.body.calls.find((c) => c[0] === "translate"),
      ["translate", 48, 13.5],
    );
    assert.deepEqual(
      source.body.calls.find((c) => c[0] === "scale"),
      ["scale", 1.5, 1.5],
    );
    assert.ok(
      source.stroke.calls.some((c) => c[0] === "strokeText" && c[1] === "AB"),
    );
    assert.ok(
      source.stroke.calls.some(
        (c) => c[0] === "drawImage" && c[1] === source.body,
      ),
    );
    assert.equal(source.stroke.context.lineJoin, "round");
    assert.equal(g.faces.size, 0);
    frame.scene.layers[0].quad.width = 81;
    assert.throws(
      () => sceneTextMaskSourceV1(frame, "mask"),
      (e) => e.code === "INVALID_VISUAL_PLAN",
    );
    releaseSceneTextMasksV1(frame);
    s.releases.forEach((fn) => fn());
  } finally {
    g.restore();
  }
});
test("changed text hash fails before a Canvas glyph can claim the native proof", async () => {
  const g = globals();
  try {
    const { plan, frame } = input(),
      s = state();
    plan.maskFonts[0].font.validatedTextHash = hash("different content");
    await assert.rejects(
      prepareSceneTextMasksV1(plan, frame, s, admission, null),
      (e) => e.code === "INVALID_VISUAL_PLAN",
    );
    assert.equal(g.canvases.length, 0);
    s.releases.forEach((fn) => fn());
  } finally {
    g.restore();
  }
});
test("mask font resolution deduplicates faces and keeps source records scoped to one fixed frame", async () => {
  const { plan, frame } = input(),
    s = state();
  let reads = 0;
  s.resolveFont = async () => {
    reads++;
    return { bytes, release() {} };
  };
  const second = structuredClone(plan.stacks[0].masks[0]);
  second.id = "second";
  plan.stacks[0].masks.push(second);
  plan.maskFonts.push({
    ...structuredClone(plan.maskFonts[0]),
    maskId: "second",
  });
  await resolveSceneTextMaskFontsV1(plan, s, null);
  assert.equal(reads, 1);
  assert.throws(
    () => sceneTextMaskSourceV1(structuredClone(frame), "mask"),
    (e) => e.code === "INVALID_VISUAL_PLAN",
  );
});

function syntheticInput() {
  const fixture = input(), { plan, font, m } = fixture;
  font.weight = 700; m.shape.mask.font_weight = "bold"; m.shape.mask.font_style = "italic";
  const binding = {
    schema_version: 1, binding_id: `font-synthesis-v1_${"a".repeat(64)}`, profile: "classic_chromium_italic_v1",
    actual: { id: "fixture-actual-bold-v1", family: "Fixture normal", weight: 700, style: "normal", content_hash: font.contentHash, size_bytes: font.size,
      source_url: "https://example.invalid/fixed.ttf", source_commit: "1".repeat(40), license_id: "OFL-1.1", license_notice: "Mock only" },
    requested: { weight: 700, style: "italic" },
  };
  plan.fontSynthesis = [{ elementId: "e", ownerId: "mask", ownerKind: "text_mask", fontId: "font", binding }];
  return fixture;
}

test("Scene synthesis mapping requires exact active owners, project font identity and complete text-mask requests", () => {
  const { plan } = syntheticInput();
  assert.equal(sceneFontResourcesV1(plan).get("font").style, "normal");
  assert.equal(sceneSynthesisEntriesV1(plan).length, 1);
  for (const mutate of [
    p => { delete p.fontSynthesis; }, p => { p.fontSynthesis = []; },
    p => { p.fontSynthesis.push(structuredClone(p.fontSynthesis[0])); },
    p => { p.fontSynthesis[0].ownerId = "other-mask"; },
    p => { p.fontSynthesis[0].elementId = "other-element"; },
    p => { p.fontSynthesis[0].ownerKind = "text"; },
    p => { p.fontSynthesis[0].fontId = p.fontSynthesis[0].binding.actual.id; },
    p => { p.fontSynthesis[0].binding.actual.content_hash = "f".repeat(64); },
    p => { p.stacks[0].masks[0].enabled = false; },
    p => { p.stacks[0].masks[0].shape.mask.font_style = "normal"; },
  ]) {
    const bad = structuredClone(plan); mutate(bad);
    assert.throws(() => sceneFontResourcesV1(bad), error => error.code === "INVALID_VISUAL_PLAN");
  }
});

test("synthetic text masks use normal bytes with italic fill and stroke, without double style or skew transforms", async () => {
  const g = globals(), { plan, frame } = syntheticInput(), s = state();
  try {
    await prepareSceneTextMasksV1(plan, frame, s, admission, null);
    const source = sceneTextMaskSourceV1(frame, "mask");
    assert.deepEqual(g.createdFaces.map(face => face.options), [{ style: "normal", weight: "700" }]);
    assert.equal(g.createdFaces[0].content, bytes);
    assert.ok(source.body.context.font.startsWith("italic 700 "));
    assert.equal(source.stroke.context.font, source.body.context.font);
    assert.ok(source.body.context.font.includes(`opencut_${hash(bytes)}_700_normal`));
    assert.deepEqual(source.body.calls.filter(call => call[0] === "rotate"), [["rotate", Math.PI / 6]]);
    assert.equal(g.faces.size, 0);
  } finally { s.releases.forEach(release => release()); releaseSceneTextMasksV1(frame); g.restore(); }
});

test("one actual source shared by a title and text mask has one resource identity and one immutable synthesis binding", () => {
  const { plan, font } = syntheticInput(), binding = structuredClone(plan.fontSynthesis[0].binding);
  plan.execution.execution.execution.execution.texts.push({ elementId: "title", font: { ...font, validatedTextHash: hash("Other title") } });
  plan.fontSynthesis.push({ elementId: "title", ownerId: "title", ownerKind: "text", fontId: font.fontId, binding });
  assert.equal(sceneFontResourcesV1(plan).size, 1);
  assert.equal(sceneSynthesisEntriesV1(plan).length, 2);
  plan.fontSynthesis[1].binding.actual.family = "Unrelated family";
  assert.throws(() => sceneFontResourcesV1(plan), error => error.code === "INVALID_VISUAL_PLAN");
});

test("only the matched independent title uses the Scene hook; captions, other owners and subsequent normal text cannot inherit it", async () => {
  const g = globals(), { plan, font } = syntheticInput();
  const record = plan.fontSynthesis[0]; record.elementId = record.ownerId = "title"; record.ownerKind = "text";
  plan.stacks = []; plan.maskFonts = [];
  const layer = { elementId: "title", font, text: { content: "AB\nC", fontSizePx: 10, lineHeightPx: 12, letterSpacingPx: 0, align: "center", decoration: "none", color: [1, 1, 1, 1], background: { enabled: false, color: [0, 0, 0, 0], paddingXPx: 0, paddingYPx: 0, offsetXPx: 0, offsetYPx: 0, cornerRadiusPercent: 0 }, transform: { centerX: 50, centerY: 30, scaleX: 1, scaleY: 1, rotationRadians: 0 } } };
  plan.execution.execution.execution.execution.texts = [layer];
  const styles = createSceneSynthesisV1(plan, null); let delegated = 0;
  const hooks = styles.wrapTextHooks({ bind: () => "same-bind", rasterize: () => { delegated++; return "original raster"; } });
  const dims = { width: 100, height: 60, maxPixels: "6000", maxRasterBytes: "96000" };
  try {
    const raster = await hooks.rasterize(layer.text, font, bytes, dims, null, undefined, layer);
    assert.equal(raster.layout.fontStyle, "normal");
    assert.ok(raster.canvas.context.font.startsWith("italic 700 "));
    assert.equal(await hooks.rasterize(layer.text, font, bytes, dims, null, undefined, { ...layer, elementId: "ordinary" }), "original raster");
    assert.equal(delegated, 1); assert.equal(hooks.bind(), "same-bind");
    const bad = structuredClone(plan); bad.execution.execution.execution.captions = [{ elementId: "title" }];
    assert.throws(() => sceneSynthesisEntriesV1(bad), error => error.code === "INVALID_VISUAL_PLAN");
    styles.close();
    assert.throws(() => hooks.rasterize(layer.text, font, bytes, dims, null, undefined, layer), error => error.code === "INVALID_VISUAL_PLAN");
  } finally { styles.close(); g.restore(); }
});
