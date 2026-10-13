import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { drawClassicText, measureClassicText, rasterizeClassicText, validateClassicText } from "../src/text-layout.mjs";

const code = expected => error => error.code === expected;
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const bytes = Uint8Array.of(1, 2, 3, 4); // Font loading is mocked; this is not glyph evidence.
const text = () => ({
  content: "AV To\n中文编辑", fontSizePx: 30, lineHeightPx: 36, letterSpacingPx: 2,
  align: "center", decoration: "none", color: [1, 0.25, 0, 1],
  background: { enabled: true, color: [0, 0.5, 1, 0.5], paddingXPx: 10, paddingYPx: 6, offsetXPx: 3, offsetYPx: -4, cornerRadiusPercent: 50 },
  transform: { centerX: 320, centerY: 180, scaleX: 2, scaleY: 0.5, rotationRadians: Math.PI / 2 },
});
const font = content => ({ fontId: "fixed-font", contentHash: hash(bytes), validatedTextHash: hash(content), weight: 400, style: "normal", faceIndex: 0, glyphCoverageValidated: true });
const dimensions = { width: 640, height: 360, maxPixels: "230400", maxRasterBytes: "3686400" };

function context() {
  const calls = [], measured = [], state = [];
  const ctx = {
    calls, measured, font: "", textAlign: "start", textBaseline: "alphabetic", letterSpacing: "0px", fillStyle: "#000",
    save() { state.push(Object.fromEntries(["font", "textAlign", "textBaseline", "letterSpacing", "fillStyle"].map(key => [key, this[key]]))); calls.push(["save"]); },
    restore() { Object.assign(this, state.pop()); calls.push(["restore"]); },
    measureText(content) {
      measured.push({ content, font: this.font, baseline: this.textBaseline, spacing: this.letterSpacing });
      return content === "AV To" ? { width: 73.5, actualBoundingBoxAscent: 11, actualBoundingBoxDescent: 9 } : { width: content ? 120 : 0, actualBoundingBoxAscent: 13, actualBoundingBoxDescent: 8 };
    },
  };
  for (const method of ["translate", "scale", "rotate", "beginPath", "roundRect", "fill", "fillText", "fillRect"]) ctx[method] = (...args) => calls.push([method, ...args]);
  return ctx;
}

test("sampled size, explicit lines, middle baselines and padding retain independent Classic geometry", () => {
  const t = text(), ctx = context(), result = measureClassicText(ctx, t, "locked", font(t.content));
  assert.deepEqual(result.block, { maxWidth: 120, height: 72, visualCenterOffset: 18 });
  assert.deepEqual(result.lines.map(line => [line.content, line.baselineY]), [["AV To", -18], ["中文编辑", 18]]);
  assert.deepEqual(result.background, { left: -67, top: -46, width: 140, height: 84, radius: 21 });
  assert.deepEqual(ctx.measured, ["AV To", "中文编辑"].map(content => ({ content, font: 'normal 400 30px "locked"', baseline: "middle", spacing: "2px" })));
  assert.equal(ctx.font, "");
  assert.equal(ctx.textBaseline, "alphabetic");
  const long = { ...t, content: "an explicit long line stays on one line\n\n尾行" };
  assert.deepEqual(measureClassicText(context(), long, "locked", font(long.content)).lines.map(line => line.baselineY), [-36, 0, 36]);
});

test("underline and strike follow actual middle-baseline ascent/descent and per-line alignment", () => {
  const t = { ...text(), align: "right", decoration: "underline" };
  const under = measureClassicText(context(), t, "locked", font(t.content));
  assert.deepEqual(under.lines[0].decoration, { left: -73.5, top: -6.9, width: 73.5, height: 2.1 });
  assert.deepEqual(under.lines[1].decoration, { left: -120, top: 28.1, width: 120, height: 2.1 });
  const strike = measureClassicText(context(), { ...t, align: "left", decoration: "lineThrough" }, "locked", font(t.content));
  assert.deepEqual(strike.lines[0].decoration, { left: 0, top: -18.7, width: 73.5, height: 2.1 });
  assert.equal(strike.background.left, -7);
  assert.equal(under.background.left, -127);
});

test("full-canvas raster applies T S R, preserves unscaled offsets, and draws background before glyphs", () => {
  const t = { ...text(), decoration: "underline" }, ctx = context();
  const layout = measureClassicText(ctx, t, "locked", font(t.content));
  ctx.calls.length = 0;
  drawClassicText(ctx, t, layout, "locked", font(t.content));
  assert.deepEqual(ctx.calls.slice(0, 4), [["save"], ["translate", 320, 180], ["scale", 2, 0.5], ["rotate", Math.PI / 2]]);
  assert.deepEqual(ctx.calls.filter(call => call[0] === "roundRect"), [["roundRect", -67, -46, 140, 84, 21]]);
  assert.deepEqual(ctx.calls.filter(call => call[0] === "fillText"), [["fillText", "AV To", 0, -18], ["fillText", "中文编辑", 0, 18]]);
  assert.ok(ctx.calls.findIndex(call => call[0] === "fill") < ctx.calls.findIndex(call => call[0] === "fillText"));
  assert.equal(ctx.calls.at(-1)[0], "restore");
  // Independently, local (10, 4) becomes (-4, 10), (-8, 5), then (312, 185).
  // The order above is material: T R S would put it at (318, 200).
});

test("finite negative padding is preserved while radius alone follows Classic clamping", () => {
  const t = text();
  t.background = { ...t.background, paddingXPx: -10, paddingYPx: -6, cornerRadiusPercent: 120 };
  assert.deepEqual(measureClassicText(context(), t, "locked", font(t.content)).background, { left: -47, top: -34, width: 100, height: 60, radius: 30 });
  t.background.cornerRadiusPercent = -1;
  assert.equal(measureClassicText(context(), t, "locked", font(t.content)).background.radius, 0);
  t.background.color[3] = 0;
  assert.deepEqual(measureClassicText(context(), t, "locked", font(t.content)).background, { left: -47, top: -34, width: 100, height: 60, radius: 0 });
  t.background.enabled = false;
  assert.equal(measureClassicText(context(), t, "locked", font(t.content)).background, null);
});

test("invalid physical samples and unsupported letter-spacing capability fail explicitly", () => {
  for (const change of [{ fontSizePx: 0 }, { lineHeightPx: -1 }, { letterSpacingPx: Infinity }, { align: "start" }, { decoration: "overline" }, { color: [1, 0, 0, 2] }]) {
    assert.throws(() => validateClassicText({ ...text(), ...change }), code("INVALID_VISUAL_PLAN"));
  }
  const ctx = context(); delete ctx.letterSpacing;
  const t = text();
  assert.throws(() => measureClassicText(ctx, t, "locked", font(t.content)), code("CAPABILITY_UNAVAILABLE"));
});

function installRealm(options = {}) {
  const originals = new Map(["FontFace", "OffscreenCanvas", "fonts"].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  const faces = [], canvases = [], active = new Set(), additions = [], deletions = [];
  class FontFace {
    constructor(family, source, descriptors) { this.family = family; this.source = source; this.descriptors = descriptors; this.status = "unloaded"; faces.push(this); }
    async load() { if (options.load) await options.load(this); else this.status = "loaded"; return this; }
  }
  class OffscreenCanvas {
    constructor(width, height) { this.width = width; this.height = height; this.ctx = context(); canvases.push(this); }
    getContext(kind) { assert.equal(kind, "2d"); if (options.context) return options.context(this); return this.ctx; }
  }
  const fonts = { add(face) { active.add(face); additions.push(face); }, delete(face) { deletions.push(face); return active.delete(face); } };
  for (const [key, value] of Object.entries({ FontFace, OffscreenCanvas, fonts })) Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  return { faces, canvases, active, additions, deletions, restore() { for (const [key, descriptor] of originals) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; } } };
}

test("verified bytes and exact-content coverage load one isolated face and release it after rasterization", async () => {
  const realm = installRealm(), t = text(), f = font(t.content);
  try {
    const raster = await rasterizeClassicText(t, f, bytes, dimensions);
    assert.equal(raster.canvas, realm.canvases[0]);
    assert.equal(raster.width, 640); assert.equal(raster.height, 360);
    assert.equal(realm.faces.length, 1);
    assert.equal(realm.faces[0].family, `opencut_${f.contentHash}_400_normal`);
    assert.equal(realm.faces[0].source, bytes);
    assert.deepEqual(realm.faces[0].descriptors, { style: "normal", weight: "400" });
    assert.equal(realm.active.size, 0);
    assert.deepEqual(realm.additions, realm.deletions);
    assert.equal(raster.layout.fontContentHash, f.contentHash);
    assert.equal(raster.layout.validatedTextHash, f.validatedTextHash);
    assert.deepEqual(raster.layout.transform, t.transform);
    assert.ok(realm.canvases[0].ctx.measured.every(item => !item.font.includes("sans-serif")));
  } finally { realm.restore(); }
});

test("changed bytes, mismatched coverage, unvalidated faces and canvas budgets fail before font allocation", async () => {
  const realm = installRealm(), t = text(), f = font(t.content);
  try {
    await assert.rejects(rasterizeClassicText(t, f, Uint8Array.of(5), dimensions), code("ASSET_HASH_MISMATCH"));
    await assert.rejects(rasterizeClassicText(t, { ...f, validatedTextHash: hash("other text") }, bytes, dimensions), code("INVALID_VISUAL_PLAN"));
    for (const change of [{ glyphCoverageValidated: false }, { faceIndex: 1 }, { weight: 500 }, { style: "oblique" }]) await assert.rejects(rasterizeClassicText(t, { ...f, ...change }, bytes, dimensions), code("INVALID_VISUAL_PLAN"));
    await assert.rejects(rasterizeClassicText(t, f, bytes, { ...dimensions, maxPixels: "230399" }), code("VISUAL_BUDGET_EXCEEDED"));
    await assert.rejects(rasterizeClassicText(t, f, bytes, { ...dimensions, maxRasterBytes: "3686399" }), code("VISUAL_BUDGET_EXCEEDED"));
    const missing = { ...dimensions }; delete missing.maxPixels;
    await assert.rejects(rasterizeClassicText(t, f, bytes, missing), code("INVALID_VISUAL_PLAN"));
    assert.equal(realm.faces.length, 0); assert.equal(realm.canvases.length, 0);
  } finally { realm.restore(); }
});

test("browser font rejection and cancellation never publish fallback glyphs or retain a font", async () => {
  const t = text(), f = font(t.content);
  let realm = installRealm({ load() { throw new Error("sanitizer rejected"); } });
  try {
    await assert.rejects(rasterizeClassicText(t, f, bytes, dimensions), code("FONT_LOAD_FAILED"));
    assert.equal(realm.canvases.length, 0); assert.equal(realm.additions.length, 0);
  } finally { realm.restore(); }
  const abort = new AbortController(), reason = new Error("cancelled by owner");
  realm = installRealm({ load(face) { face.status = "loaded"; abort.abort(reason); } });
  try {
    await assert.rejects(rasterizeClassicText(t, f, bytes, dimensions, abort.signal), error => error === reason);
    assert.equal(realm.canvases.length, 0); assert.equal(realm.active.size, 0);
  } finally { realm.restore(); }
});

test("raster failure releases loaded font and owned canvas", async () => {
  const t = text(), realm = installRealm({ context() { return null; } });
  try {
    await assert.rejects(rasterizeClassicText(t, font(t.content), bytes, dimensions), code("CAPABILITY_UNAVAILABLE"));
    assert.equal(realm.active.size, 0); assert.deepEqual(realm.additions, realm.deletions);
    assert.equal(realm.canvases[0].width, 0); assert.equal(realm.canvases[0].height, 0);
  } finally { realm.restore(); }
});

test("canvas ignoring a CSS font selection fails instead of measuring with the default family", async () => {
  const t = text(), realm = installRealm({ context(canvas) {
    Object.defineProperty(canvas.ctx, "font", { get: () => "10px sans-serif", set() {}, configurable: true });
    return canvas.ctx;
  } });
  try {
    await assert.rejects(rasterizeClassicText(t, font(t.content), bytes, dimensions), code("FONT_LOAD_FAILED"));
    assert.equal(realm.canvases[0].ctx.measured.length, 0);
    assert.equal(realm.active.size, 0);
    assert.equal(realm.canvases[0].width, 0); assert.equal(realm.canvases[0].height, 0);
  } finally { realm.restore(); }
});
