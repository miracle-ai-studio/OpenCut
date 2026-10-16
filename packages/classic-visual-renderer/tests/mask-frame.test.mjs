import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { maskBeginV1, maskFactV1 } from "./mask-fixture.mjs";
import { validateMaskFramePlanV1 } from "../src/mask-plan-v1.mjs";
import { clearGraphicSourceCacheV1 } from "../src/graphic-source.mjs";
import { maskFrameSourcesV1, releaseMaskFrameSourcesV1 } from "../src/mask-source-v1.mjs";

const gpuCalls = [], canvases = [];
globalThis.Path2D = class { roundRect() {} ellipse() {} moveTo() {} lineTo() {} closePath() {} };
globalThis.OffscreenCanvas = class {
  constructor(width, height) { this.width = width; this.height = height; canvases.push(this); }
  getContext() { return { canvas: this, clearRect() {}, save() {}, restore() {}, translate() {}, fill() {}, stroke() {} }; }
};
mock.module("../src/decoder.mjs", { namedExports: { VisualDecoderCache: class {}, decodeImage: async () => ({ width: 2, height: 1, canvas: new OffscreenCanvas(2, 1) }), decodeVideo: async () => { throw Error("unexpected video"); } } });
mock.module("../src/gpu.mjs", { namedExports: {
  openGpuRealm: async () => ({}), closeGpuRealm: clearGraphicSourceCacheV1, clearGpuFrameCaches: clearGraphicSourceCacheV1,
  renderGpuFrame: async (bound, sources, _admission, emit) => {
    const masks = maskFrameSourcesV1(bound);
    gpuCalls.push({ bound: structuredClone(bound), sources: [...sources.keys()], masks: [...masks.keys()], maskLayouts: [...masks.values()].map(source => ({ localWidth: source.localWidth, localHeight: source.localHeight })) });
    releaseMaskFrameSourcesV1(bound); await emit(0, new Uint8Array(128)); return { peakGpuBytes: "512" };
  },
} });
const { renderClassicMaskFrameV1 } = await import("../src/mask-frame-v1.mjs");
assert.equal((await import("../src/index.mjs")).renderClassicMaskFrameV1, renderClassicMaskFrameV1);
function host(packet, change = () => {}) {
  const blend = packet.plan.execution, graphic = blend.execution, bounds = {}, calls = [], rgba = [];
  return { admission: packet.admission, calls, bounds, rgbaPackets: rgba,
    async callback(op, args) {
      calls.push(op);
      if (op === "resolveVerifiedBytes") return { transferId: "read", byteLength: "4" };
      if (op === "readVerifiedBytes") return { offset: "0", bytesHex: "01020304", eof: true };
      if (op === "releaseVerifiedBytes") return {};
      if (op === "admitDecode") return { maxDecodedBytes: "32" };
      if (op === "bindFrame") return bounds.media = { identity: graphic.execution.execution.media.identity, clear: [0, 0, 0, 0], background: [0, 0, 0, 1], decoded: args.decoded,
        layers: [{ elementId: "back", assetId: "media", quad: { centerX: 4, centerY: 2, width: 8, height: 4, rotationRadians: 0, opacity: 1, flipX: false, flipY: false } }] };
      if (op === "bindCaptionFrameV3") return bounds.text = { ...bounds.media, identity: graphic.execution.execution.identity, rasters: args.rasters };
      if (op === "bindGraphicFrameV1") return bounds.graphic = { ...bounds.text, identity: graphic.identity, captions: [], graphics: args.graphics,
        layers: graphic.order.map(id => id === "back" ? bounds.media.layers[0] : { elementId: id, assetId: `graphic:${id}`, quad: graphic.graphics.find(value => value.elementId === id).quad }) };
      if (op === "bindBlendFrameV1") return bounds.blend = { identity: blend.identity, execution: structuredClone(bounds.graphic), blends: structuredClone(blend.blends) };
      if (op === "bindMaskFrameV1") {
        assert.deepEqual(args, {});
        const result = { identity: packet.plan.identity, execution: structuredClone(bounds.blend), masks: packet.plan.masks.map(entry => {
          let quad = structuredClone(bounds.graphic.layers.find(layer => layer.elementId === entry.elementId).quad);
          if (entry.mask === null) quad = null;
          if (quad && entry.elementId === "shape") {
            const source = graphic.graphics[0].graphic, receipt = bounds.graphic.graphics[0];
            quad.width = Math.fround(quad.width * source.sourceWidth / receipt.width); quad.height = Math.fround(quad.height * source.sourceHeight / receipt.height);
          }
          return { ...structuredClone(entry), quad };
        }) };
        change(result); return result;
      }
      throw Error(op);
    }, async rgba(offset, bytes) { rgba.push([offset, bytes.length]); },
  };
}
test("mask wrapper requires exact ordered complete facts and rejects unsupported semantics before reads", () => {
  for (const change of [p => p.identity.profile = "other", p => p.execution.identity.revision = "other", p => p.masks.pop(), p => p.masks.reverse(), p => p.masks[1] = p.masks[0], p => p.masks[0].quad = null,
    p => p.masks[0].mask.feather.unit = "legacy_canvas_fraction", p => p.masks[0].mask.inverted = true, p => p.masks[0].mask.stroke.width_px = 1, p => p.extra = true]) {
    const packet = maskBeginV1(); change(packet.plan);
    assert.throws(() => validateMaskFramePlanV1(packet.plan, packet.admission), error => error.code === "INVALID_VISUAL_PLAN");
  }
});
test("one GPU run follows all five binds, carries logical sources internally, and preserves nested receipts and wire DTOs", async () => {
  const packet = maskBeginV1(), original = structuredClone(packet.plan), h = host(packet); gpuCalls.length = canvases.length = 0;
  const receipt = await renderClassicMaskFrameV1(packet.plan, h, {}, null);
  assert.deepEqual(h.calls.filter(op => op.startsWith("bind")), ["bindFrame", "bindCaptionFrameV3", "bindGraphicFrameV1", "bindBlendFrameV1", "bindMaskFrameV1"]);
  assert.equal(gpuCalls.length, 1); assert.deepEqual(gpuCalls[0].masks, ["shape"]);
  assert.deepEqual(gpuCalls[0].bound.layers.map(layer => layer.blendMode), ["multiply", "hue"]);
  assert.equal(gpuCalls[0].bound.identity.profile, "classic-v31-gpu-mask-frame-v1");
  assert.ok(!("masks" in h.bounds.graphic)); assert.ok(h.bounds.graphic.layers.every(layer => !("blendMode" in layer)));
  assert.equal(receipt.engineIdentity, "classic-v31-gpu-mask-frame-v1"); assert.equal(receipt.execution.engineIdentity, "classic-v31-gpu-blend-frame-v1");
  assert.deepEqual(receipt.masks, packet.plan.masks); assert.ok(receipt.masks.every(entry => !("quad" in entry)));
  assert.deepEqual(receipt.execution.blends, packet.plan.execution.blends);
  assert.equal(receipt.execution.execution.execution.execution.rgbaBytes, "128");
  assert.equal(receipt.execution.execution.execution.execution.peakAccountedBytes, "428");
  assert.deepEqual(packet.plan, original); assert.deepEqual(h.rgbaPackets, [[0, 128]]); assert.ok(canvases.every(canvas => canvas.width === 0));
});
test("final mask binding cannot change child facts, mask identity, order or precision-linked logical quad", async () => {
  for (const change of [b => b.identity.revision = "other", b => b.execution.blends[0].blendMode = "normal", b => b.execution.execution.graphics[0].width = 3, b => b.masks.reverse(), b => b.masks[0].mask.id = "another",
    b => b.masks[0].quad.centerX++, b => b.masks[0].quad.width /= 2, b => b.masks[0].quad.rotationRadians++, b => b.masks[0].quad.rotationRadians += 0.00001, b => b.masks[0].quad = null, b => b.masks[1].quad = b.masks[0].quad, b => b.masks[0].extra = true, b => b.extra = true]) {
    const packet = maskBeginV1(); gpuCalls.length = canvases.length = 0;
    await assert.rejects(renderClassicMaskFrameV1(packet.plan, host(packet, change), {}, null), error => error.code === "INVALID_VISUAL_PLAN");
    assert.equal(gpuCalls.length, 0); assert.ok(canvases.every(canvas => canvas.width === 0));
  }
});
test("f64 native mask quad keeps half-pixel extents and rotation while the old GPU child remains unchanged", async () => {
  const packet = maskBeginV1(), graphics = packet.plan.execution.execution.graphics;
  const degrees = 27.5000009, radians = degrees * Math.PI / 180;
  const oldRadians = Math.fround(Math.fround(degrees) * Math.fround(Math.PI / 180));
  assert.notEqual(Math.fround(radians), oldRadians); // Real degrees-first f32 rounding case.
  graphics[0].quad.width = 10.5; graphics[0].quad.rotationRadians = oldRadians;
  graphics[0].quad.opacity = 0.3; // Serde's shortest f32 decimal, parsed as a JS double.
  const h = host(packet, b => {
    b.masks[0].quad.width = 10.4999999;
    b.masks[0].quad.rotationRadians = radians;
    b.masks[0].quad.opacity = 0.30000000000000004;
  });
  await renderClassicMaskFrameV1(packet.plan, h, {}, null);
  const gpu = gpuCalls.at(-1);
  assert.equal(gpu.maskLayouts[0].localWidth, 10);
  assert.equal(gpu.bound.masks[0].quad.rotationRadians, radians);
  assert.equal(gpu.bound.layers[0].quad.width, 10.5);
  assert.equal(gpu.bound.layers[0].quad.rotationRadians, oldRadians);
  assert.equal(h.bounds.graphic.layers[0].quad.width, 10.5);
  await assert.rejects(renderClassicMaskFrameV1(packet.plan, host(packet, b => b.masks[0].quad.width = 11.4999999), {}, null), error => error.code === "INVALID_VISUAL_PLAN");
});
test("media logical quad is exact, graphics remove known padding, and null child quads skip mask allocation", async () => {
  const packet = maskBeginV1(); packet.plan.masks[1].mask = maskFactV1("ellipse", 0);
  await renderClassicMaskFrameV1(packet.plan, host(packet), {}, null); assert.deepEqual(gpuCalls.at(-1).masks, ["shape", "back"]);
  await assert.rejects(renderClassicMaskFrameV1(packet.plan, host(packet, b => b.masks[1].quad.width--), {}, null), error => error.code === "INVALID_VISUAL_PLAN");
  const padded = maskBeginV1(); padded.plan.execution.execution.graphics[0].graphic.stroke = { color: [0, 1, 0, 1], widthPx: 0.1, align: "center" };
  await renderClassicMaskFrameV1(padded.plan, host(padded), {}, null);
  assert.equal(gpuCalls.at(-1).bound.layers[0].quad.width, 4); assert.equal(gpuCalls.at(-1).bound.masks[0].quad.width, 2);
  await assert.rejects(renderClassicMaskFrameV1(padded.plan, host(padded, b => b.masks[0].quad.width = 4), {}, null), error => error.code === "INVALID_VISUAL_PLAN");
  const skipped = maskBeginV1(); skipped.plan.execution.execution.graphics[0].quad = null;
  await renderClassicMaskFrameV1(skipped.plan, host(skipped), {}, null); assert.deepEqual(gpuCalls.at(-1).masks, []);
});
test("invalid next wrapper clears retained graphic sources before any host request and a valid retry works", async () => {
  const packet = maskBeginV1(), realm = { device: { destroy() { throw Error("validation must not destroy reusable realm"); } } };
  canvases.length = gpuCalls.length = 0;
  await renderClassicMaskFrameV1(packet.plan, host(packet), realm, null);
  const retained = canvases.find(canvas => canvas.width > 0); assert.ok(retained);
  const invalid = structuredClone(packet); invalid.plan.masks.pop(); const h = host(invalid);
  await assert.rejects(renderClassicMaskFrameV1(invalid.plan, h, realm, null), error => error.code === "INVALID_VISUAL_PLAN");
  assert.equal(retained.width, 0); assert.equal(h.calls.length, 0); assert.equal(gpuCalls.length, 1);
  await renderClassicMaskFrameV1(packet.plan, host(packet), realm, null); assert.equal(gpuCalls.length, 2); clearGraphicSourceCacheV1(realm);
});
test("cancellation at final binding and CPU budget failure produce no pixels and release sources", async () => {
  const packet = maskBeginV1(), abort = new AbortController(), reason = Error("cancelled"); gpuCalls.length = 0;
  await assert.rejects(renderClassicMaskFrameV1(packet.plan, host(packet, () => abort.abort(reason)), {}, abort.signal), error => error === reason);
  packet.admission.maxRgbaBytes = "427";
  await assert.rejects(renderClassicMaskFrameV1(packet.plan, host(packet), {}, null), error => error.code === "VISUAL_BUDGET_EXCEEDED");
  assert.equal(gpuCalls.length, 0); assert.ok(canvases.every(canvas => canvas.width === 0));
});
