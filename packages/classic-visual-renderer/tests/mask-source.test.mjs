import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { createHash } from "node:crypto";
import { maskFactV1 } from "./mask-fixture.mjs";
import { maskPathV1, maskSourceLayoutV1, prepareMaskFrameSourcesV1, maskFrameSourcesV1, rasterizeMaskSourceV1, releaseMaskFrameSourcesV1 } from "../src/mask-source-v1.mjs";
import { byteBudget } from "../src/validation.mjs";

const quad = { centerX: 6, centerY: 4, width: 5.6, height: 3.4, rotationRadians: 0.3, flipX: true, flipY: false, opacity: 0.4 };
const identity = { projectId: "p", revision: "r", sequenceId: "s", profile: "classic-v31-gpu-mask-frame-v1", width: 12, height: 8, time: { ticks: "0", timebase: 30 } };
const admission = { maxDecodedPixels: "1000", maxRgbaBytes: "10000" };
function drawing() {
  const canvases = [], ops = [];
  const createCanvas = (width, height) => {
    const canvas = { width, height }; canvases.push(canvas);
    canvas.getContext = () => new Proxy({ canvas }, { get: (ctx, key) => key in ctx ? ctx[key] : (...args) => ops.push([canvases.indexOf(canvas), key, ...args]), set: (ctx, key, value) => { ops.push([canvases.indexOf(canvas), key, value]); ctx[key] = value; return true; } });
    return canvas;
  };
  class Path { constructor() { return new Proxy({}, { get: (_value, key) => (...args) => ops.push(["path", key, ...args]) }); } }
  return { createCanvas, Path, ops, canvases };
}
function prepare(mask = maskFactV1(), q = quad, draw = drawing(), limits = admission) {
  const frame = { identity: structuredClone(identity), masks: [{ elementId: "a", mask: structuredClone(mask), quad: structuredClone(q) }] };
  const state = { budget: byteBudget(limits.maxRgbaBytes), releases: [], sources: new Map(), retainedDecoded: 0 };
  prepareMaskFrameSourcesV1(frame, state, limits, null, draw.createCanvas, draw.Path);
  return { frame, state, draw, source: maskFrameSourcesV1(frame).get("a") };
}
test("Classic rectangle/ellipse preserve minimum dimensions and ignore retained box scale", () => {
  const rectangle = maskFactV1(); rectangle.shape.geometry.width = 0.001; rectangle.shape.geometry.rotation_degrees = 90;
  const path = maskPathV1(rectangle, 100, 50);
  assert.deepEqual(path.map(command => command[0]), ["moveTo", "lineTo", "lineTo", "lineTo", "closePath"]);
  assert.ok(Math.abs(path[0][1] - 65) < 1e-12); assert.ok(Math.abs(path[0][2] - 24.5) < 1e-12);
  rectangle.shape.geometry.scale = 5; assert.deepEqual(maskPathV1(rectangle, 100, 50), path);
  rectangle.shape.geometry.scale = 0; assert.deepEqual(maskPathV1(rectangle, 100, 50), path);
  rectangle.shape.geometry.scale = 1e100; assert.deepEqual(maskPathV1(rectangle, 100, 50), path);
  const ellipse = maskFactV1("ellipse"); ellipse.shape.geometry.center_x = 0.1; ellipse.shape.geometry.center_y = -0.1;
  assert.deepEqual(maskPathV1(ellipse, 100, 50), [["ellipse", 60, 20, 30, 15, 0, 0, Math.PI * 2]]);
});
test("mask uses rounded local clipping, unrounded path/draw size, then element transform without opacity", () => {
  const { frame, state, draw, source } = prepare();
  assert.equal(state.budget.used, 4n * (96n + 18n));
  const canvas = rasterizeMaskSourceV1(source);
  assert.equal(canvas.width, 12); assert.equal(canvas.height, 8);
  assert.equal(draw.canvases[1].width, 0);
  assert.deepEqual(draw.ops.filter(op => op[0] === 0).map(op => op.slice(1, 2)[0]), ["save", "translate", "rotate", "scale", "translate", "drawImage", "restore"]);
  const image = draw.ops.find(op => op[1] === "drawImage");
  assert.deepEqual(image.slice(3), [3.2, 2.3, 5.6, 3.4]);
  assert.equal(draw.ops.some(op => op[1] === "globalAlpha"), false);
  assert.ok(Math.abs(draw.ops.find(op => op[0] === "path")[2] - 1.12) < 1e-12);
  assert.throws(() => { source.facts.mask.shape.geometry.width = 0.9; }, TypeError);
  releaseMaskFrameSourcesV1(frame); assert.throws(() => maskFrameSourcesV1(frame), error => error.code === "INVALID_VISUAL_PLAN");
  canvas.width = canvas.height = 0; state.releases.forEach(release => release()); assert.equal(state.budget.used, 0n);
});
test("MaskQuadV1 retains native f64 size and rotation through Canvas allocation/path/draw", () => {
  const radians = 27.5000009 * Math.PI / 180;
  const highPrecision = { ...quad, width: 10.4999999, rotationRadians: radians };
  const { source, draw } = prepare(maskFactV1(), highPrecision);
  assert.equal(source.localWidth, 10); assert.equal(Math.round(Math.fround(highPrecision.width)), 11);
  const canvas = rasterizeMaskSourceV1(source);
  assert.equal(draw.ops.find(op => op[1] === "rotate")[2], radians);
  assert.equal(draw.ops.find(op => op[1] === "drawImage")[5], 10.4999999);
  canvas.width = canvas.height = 0;
});
test("same static scope reuses keys across time, and every geometry/revision/quad change invalidates", () => {
  const first = prepare(), other = prepare();
  other.frame.identity.time.ticks = "1";
  assert.throws(() => maskFrameSourcesV1(other.frame), error => error.code === "INVALID_VISUAL_PLAN");
  prepareMaskFrameSourcesV1(other.frame, other.state, admission, null, other.draw.createCanvas, other.draw.Path);
  assert.equal(maskFrameSourcesV1(other.frame).get("a").key, first.source.key);
  for (const change of [f => f.identity.revision = "next", f => f.masks[0].mask.feather.value = 5, f => f.masks[0].quad.width = 4]) {
    const next = prepare(); change(next.frame); prepareMaskFrameSourcesV1(next.frame, next.state, admission, null, next.draw.createCanvas, next.draw.Path);
    assert.notEqual(maskFrameSourcesV1(next.frame).get("a").key, first.source.key);
  }
});
test("all CPU/aggregate reservations precede allocation, null quads allocate nothing, and failed/cancelled paints clear canvases", () => {
  const draw = drawing(); assert.throws(() => prepare(maskFactV1(), quad, draw, { ...admission, maxRgbaBytes: "455" }), error => error.code === "VISUAL_BUDGET_EXCEEDED");
  assert.equal(draw.canvases.length, 0);
  const skipped = prepare(maskFactV1(), null); assert.equal(skipped.source, undefined); assert.equal(skipped.state.budget.used, 0n);
  assert.throws(() => maskSourceLayoutV1({ mask: maskFactV1(), quad: { ...quad, width: 20000 } }, identity, admission), error => error.code === "INVALID_VISUAL_PLAN");
});
test("unsupported and forged mask sources never allocate", () => {
  for (const change of [m => m.inverted = true, m => m.enabled = false, m => m.feather.unit = "legacy_canvas_fraction", m => m.feather.value = 2 ** -149, m => m.stroke.width_px = 1, m => m.shape.kind = "heart", m => m.shape.geometry.width = NaN, m => m.extra = true]) {
    const mask = maskFactV1(); change(mask); const draw = drawing();
    assert.throws(() => prepare(mask, quad, draw), error => error.code === "INVALID_VISUAL_PLAN"); assert.equal(draw.canvases.length, 0);
  }
  assert.throws(() => rasterizeMaskSourceV1({}), error => error.code === "INVALID_VISUAL_PLAN");
  const ready = prepare(), reason = Error("cancelled");
  assert.throws(() => rasterizeMaskSourceV1(ready.source, { aborted: true, reason }), error => error === reason); assert.equal(ready.draw.canvases.length, 0);
  const broken = drawing(), create = broken.createCanvas;
  broken.createCanvas = (w, h) => { const c = create(w, h); if (broken.canvases.length === 2) c.getContext = () => null; return c; };
  const failed = prepare(maskFactV1(), quad, broken);
  assert.throws(() => rasterizeMaskSourceV1(failed.source), error => error.code === "CAPABILITY_UNAVAILABLE"); assert.ok(broken.canvases.every(canvas => canvas.width === 0 && canvas.height === 0));
});
test("all four mask shaders are byte-for-byte Classic sources with pinned provenance", async () => {
  const vendor = new URL("../vendor/classic-v31/", import.meta.url), metadata = JSON.parse(await fs.readFile(new URL("source.json", vendor), "utf8"));
  for (const name of ["mask.wgsl", "jfa_init.wgsl", "jfa_step.wgsl", "jfa_distance.wgsl"]) {
    const bytes = await fs.readFile(new URL(name, vendor));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), metadata.sources[name].sha256);
    const original = new URL(`../../tmp/opencut-classic/${metadata.sources[name].source}`, new URL("../", import.meta.url));
    // Classic checkout is a local provenance aid, not a shipped dependency.
    try { assert.deepEqual(bytes, await fs.readFile(original)); } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
});
