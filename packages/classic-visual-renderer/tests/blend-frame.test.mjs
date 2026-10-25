import { clearGraphicSourceCache } from "../src/graphic-source.mjs";
import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { blendBegin } from "./blend-fixture.mjs";
import { validateBlendFramePlan } from "../src/blend-plan.mjs";
import { BLEND_MODES, blendModeIndex } from "../src/blend-modes.mjs";

const canvases = [],
  gpuCalls = [],
  decodes = [];
globalThis.Path2D = class {
  roundRect() {}
  ellipse() {}
  moveTo() {}
  lineTo() {}
  arcTo() {}
  closePath() {}
};
globalThis.OffscreenCanvas = class {
  constructor(width, height) {
    this.width = width;
    this.height = height;
    canvases.push(this);
  }
  getContext() {
    return {
      canvas: this,
      clearRect() {},
      fill() {},
      stroke() {},
      save() {},
      restore() {},
      clip() {},
      translate() {},
      drawImage() {},
      scale() {
        throw Error("quad applied twice");
      },
      rotate() {
        throw Error("quad applied twice");
      },
    };
  }
};
mock.module("../src/decoder.mjs", {
  namedExports: {
    VisualDecoderCache: class {},
    decodeImage: async () => {
      decodes.push(1);
      return { width: 2, height: 1, canvas: new OffscreenCanvas(2, 1) };
    },
    decodeVideo: async () => {
      throw Error("unexpected video");
    },
  },
});
mock.module("../src/gpu.mjs", {
  namedExports: {
    openGpuRealm: async () => ({}),
    clearGpuFrameCaches: clearGraphicSourceCache,
    closeGpuRealm: clearGraphicSourceCache,
    renderGpuFrame: async (bound, sources, _admission, emit) => {
      gpuCalls.push({
        bound: structuredClone(bound),
        sourceIds: [...sources.keys()],
      });
      for (const layer of bound.layers)
        assert.ok(sources.get(layer.elementId).canvas.width > 0);
      await emit(
        0,
        new Uint8Array(bound.identity.width * bound.identity.height * 4),
      );
      return { peakGpuBytes: "32" };
    },
  },
});
const { renderClassicBlendFrame } = await import("../src/blend-frame.mjs");
assert.equal(
  (await import("../src/index.mjs")).renderClassicBlendFrame,
  renderClassicBlendFrame,
);

function host(packet, change = () => {}) {
  const p = packet.plan.execution,
    calls = [],
    rgba = [],
    bounds = {};
  return {
    admission: packet.admission,
    calls,
    rgbaPackets: rgba,
    bounds,
    async callback(op, args) {
      calls.push(op);
      if (op === "resolveVerifiedBytes")
        return { transferId: "read", byteLength: "4" };
      if (op === "readVerifiedBytes")
        return { offset: "0", bytesHex: "01020304", eof: true };
      if (op === "releaseVerifiedBytes") return {};
      if (op === "admitDecode") return { maxDecodedBytes: "32" };
      if (op === "bindFrame")
        return (bounds.media = {
          identity: p.execution.execution.media.identity,
          clear: [0, 0, 0, 0],
          background: [0, 0, 0, 1],
          decoded: args.decoded,
          layers: [
            {
              elementId: "back",
              assetId: "media",
              quad: {
                centerX: 4,
                centerY: 2,
                width: 8,
                height: 4,
                rotationRadians: 0,
                opacity: 1,
                flipX: false,
                flipY: false,
              },
            },
          ],
        });
      if (op === "bindCaptionFrameV3")
        return (bounds.child = {
          ...bounds.media,
          identity: p.execution.execution.identity,
          rasters: args.rasters,
        });
      if (op === "bindGraphicFrameV1")
        return (bounds.graphic = {
          ...bounds.child,
          identity: p.identity,
          captions: [],
          graphics: args.graphics,
          layers: p.order.map((id) =>
            id === "back"
              ? bounds.child.layers[0]
              : {
                  elementId: id,
                  assetId: `graphic:${id}`,
                  quad: p.graphics.find((layer) => layer.elementId === id).quad,
                },
          ),
        });
      if (op === "bindBlendFrameV1") {
        assert.deepEqual(args, {});
        const result = {
          identity: packet.plan.identity,
          execution: structuredClone(bounds.graphic),
          blends: structuredClone(packet.plan.blends),
        };
        change(result);
        return result;
      }
      throw Error(op);
    },
    async rgba(offset, data) {
      rgba.push({ offset, count: data.length });
    },
  };
}

test("17 mode indices are exact and unknown values never fall through to normal", () => {
  assert.deepEqual(BLEND_MODES, [
    "normal",
    "darken",
    "multiply",
    "color-burn",
    "lighten",
    "screen",
    "plus-lighter",
    "color-dodge",
    "overlay",
    "soft-light",
    "hard-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
  ]);
  assert.deepEqual(
    BLEND_MODES.map(blendModeIndex),
    Array.from({ length: 17 }, (_, index) => index),
  );
  for (const value of [undefined, null, 0, "Normal", "unknown"])
    assert.throws(
      () => blendModeIndex(value),
      (error) => error.code === "INVALID_VISUAL_PLAN",
    );
});
test("blend wrapper requires identical fixed child scope and an exact ordered complete mode map", () => {
  for (const change of [
    (p) => (p.identity.profile = "classic-v31-gpu-normal-graphic-frame-v1"),
    (p) => (p.execution.identity.revision = "other"),
    (p) => p.blends.pop(),
    (p) => p.blends.reverse(),
    (p) => (p.blends[1] = p.blends[0]),
    (p) => (p.blends[0].blendMode = "unknown"),
    (p) => (p.blends[0].quad = null),
    (p) => (p.extra = true),
  ]) {
    const packet = blendBegin();
    change(packet.plan);
    assert.throws(
      () => validateBlendFramePlan(packet.plan, packet.admission),
      (error) => error.code === "INVALID_VISUAL_PLAN",
    );
  }
  const packet = blendBegin();
  packet.plan.execution.graphics[0].quad = null;
  validateBlendFramePlan(packet.plan, packet.admission);
  packet.plan.blends.shift();
  assert.throws(() => validateBlendFramePlan(packet.plan, packet.admission));
});
test("one decode/GPU executes after all four binds, preserving child DTOs and source accounting", async () => {
  canvases.length = gpuCalls.length = decodes.length = 0;
  const packet = blendBegin(),
    original = structuredClone(packet.plan),
    h = host(packet);
  const receipt = await renderClassicBlendFrame(packet.plan, h, {}, null);
  assert.equal(decodes.length, 1);
  assert.equal(gpuCalls.length, 1);
  assert.deepEqual(
    h.calls.filter((op) => op.startsWith("bind")),
    [
      "bindFrame",
      "bindCaptionFrameV3",
      "bindGraphicFrameV1",
      "bindBlendFrameV1",
    ],
  );
  assert.deepEqual(
    gpuCalls[0].bound.layers.map(({ elementId, blendMode }) => ({
      elementId,
      blendMode,
    })),
    packet.plan.blends,
  );
  assert.equal(
    gpuCalls[0].bound.identity.profile,
    "classic-v31-gpu-blend-frame-v1",
  );
  assert.ok(h.bounds.graphic.layers.every((layer) => !("blendMode" in layer)));
  assert.equal(receipt.engineIdentity, "classic-v31-gpu-blend-frame-v1");
  assert.equal(
    receipt.execution.engineIdentity,
    "classic-v31-gpu-normal-graphic-frame-v1",
  );
  assert.deepEqual(receipt.execution.execution.execution.bound, h.bounds.child);
  assert.deepEqual(receipt.blends, packet.plan.blends);
  assert.equal(receipt.execution.execution.execution.rgbaBytes, "128");
  assert.equal(receipt.execution.execution.execution.peakAccountedBytes, "236");
  assert.deepEqual(h.rgbaPackets, [{ offset: 0, count: 128 }]);
  assert.deepEqual(packet.plan, original);
  assert.ok(
    canvases.every((canvas) => canvas.width === 0 && canvas.height === 0),
  );
});
test("final wrapper cannot change identity, geometry, source observations, mode or order before GPU", async () => {
  for (const change of [
    (b) => (b.identity.revision = "other"),
    (b) => (b.execution.decoded = []),
    (b) => (b.execution.graphics[0].width = 4),
    (b) => (b.execution.layers[0].quad.width = 9),
    (b) => b.execution.layers.reverse(),
    (b) => b.blends.reverse(),
    (b) => (b.blends[0].blendMode = "normal"),
    (b) => (b.extra = true),
  ]) {
    canvases.length = gpuCalls.length = 0;
    const packet = blendBegin();
    await assert.rejects(
      renderClassicBlendFrame(packet.plan, host(packet, change), {}, null),
      (error) => error.code === "INVALID_VISUAL_PLAN",
    );
    assert.equal(gpuCalls.length, 0);
    assert.ok(canvases.every((canvas) => canvas.width === 0));
  }
});
test("cancellation during final bind releases sources and prevents pixels", async () => {
  const packet = blendBegin(),
    cancel = new AbortController(),
    reason = Error("cancelled");
  gpuCalls.length = canvases.length = 0;
  const h = host(packet, () => cancel.abort(reason));
  await assert.rejects(
    renderClassicBlendFrame(packet.plan, h, {}, cancel.signal),
    (error) => error === reason,
  );
  assert.equal(gpuCalls.length, 0);
  assert.equal(h.rgbaPackets.length, 0);
  assert.ok(canvases.every((canvas) => canvas.width === 0));
});
test("blend wrapper retains aggregate source budget rejection and does not add surfaces", async () => {
  const packet = blendBegin();
  packet.admission.maxDecodedPixels = "5";
  gpuCalls.length = canvases.length = 0;
  const h = host(packet);
  await assert.rejects(
    renderClassicBlendFrame(packet.plan, h, {}, null),
    (error) => error.code === "VISUAL_BUDGET_EXCEEDED",
  );
  assert.ok(!h.calls.includes("bindBlendFrameV1"));
  assert.equal(gpuCalls.length, 0);
  assert.equal(canvases.length, 1);
  assert.ok(canvases.every((canvas) => canvas.width === 0));
});

test('warm blend cache is cleared when a new wrapper fails validation before rendering, and can retry',async()=>{
  canvases.length=gpuCalls.length=0;let destroys=0;
  const packet=blendBegin(),realm={device:{destroy(){destroys++;}}};
  await renderClassicBlendFrame(packet.plan,host(packet),realm,null);
  const warm=canvases.find(canvas=>canvas.width>0);
  assert.ok(warm);
  const invalid=structuredClone(packet);invalid.plan.blends=[];
  const h=host(invalid);
  await assert.rejects(renderClassicBlendFrame(invalid.plan,h,realm,null),error=>error.code==='INVALID_VISUAL_PLAN');
  assert.equal(warm.width,0);assert.equal(h.calls.length,0);assert.equal(gpuCalls.length,1);assert.equal(destroys,0);
  await renderClassicBlendFrame(packet.plan,host(packet),realm,null);
  assert.equal(gpuCalls.length,2);assert.ok(canvases.some(canvas=>canvas.width>0));clearGraphicSourceCache(realm);
});
