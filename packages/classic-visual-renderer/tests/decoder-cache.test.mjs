import { clearGraphicSourceCacheV1 } from "../src/graphic-source.mjs";
import { test, mock } from "node:test";
import assert from "node:assert/strict";

const opened = [], iterators = [];
let checkSources = sources => assert.equal(sources.get("first"), sources.get("second"));
const values = [
  { timestamp: 0, duration: .5, canvas: { width: 4, height: 2 } },
  { timestamp: .5, duration: .5, canvas: { width: 4, height: 2 } },
  { timestamp: 2, duration: .5, canvas: { width: 4, height: 2 } },
];
class Input {
  constructor() { this.disposed = false; opened.push(this); }
  async getVideoTracks() { return [{}]; }
  async getPrimaryVideoTrack() { return { displayWidth: 4, displayHeight: 2,
    canBeTransparent: async () => false, canDecode: async () => true }; }
  dispose() { this.disposed = true; }
}
class CanvasSink {
  frames = structuredClone(values);
  canvases(start) {
    const frames = this.frames;
    let index = values.findIndex(v => v.timestamp + v.duration > start);
    if (index < 0) index = values.length;
    const iterator = { returned: false, next: async () => ({ value: frames[index++], done: index > frames.length }),
      return: async () => { iterator.returned = true; return { done: true }; } };
    iterators.push(iterator); return iterator;
  }
}
mock.module("mediabunny", { namedExports: { Input, CanvasSink, BlobSource: class {}, ALL_FORMATS: [] } });
mock.module("../src/gpu.mjs", { namedExports: {
  openGpuRealm: async () => ({}),
    clearGpuFrameCaches: clearGraphicSourceCacheV1,
    closeGpuRealm: clearGraphicSourceCacheV1,
  renderGpuFrame: async (_frame, sources, _admission, emit) => {
    checkSources(sources);
    await emit(0, new Uint8Array([42, 31, 19, 255])); return { peakGpuBytes: "4" };
  },
} });
globalThis.OffscreenCanvas = class {
  constructor(width, height) { this.width = width; this.height = height; }
  getContext() { return { drawImage() {} }; }
};
const { VisualDecoderCache } = await import("../src/decoder.mjs");
const admission = { maxWidth: 4, maxHeight: 2, maxPixels: "8", maxDecodedBytes: "128" };
const decode = requestedTimeSeconds => ({ selectedVideoStream: null, requestedTimeSeconds });
const signal = new AbortController().signal;

test("serial cached decoder observes covering PTS, preserves gaps/EOF and restarts backward requests", async () => {
  const cache = new VisualDecoderCache(), count = opened.length, sinks = iterators.length;
  const read = t => cache.decode("verified", new Uint8Array([1]), decode(t), admission, signal);
  try {
    const first = await read(.25), repeated = await read(.4);
    assert.equal(first.actualTimestampSeconds, 0);
    assert.equal(repeated.actualTimestampSeconds, 0);
    assert.equal(first.canvas, repeated.canvas);
    assert.equal(first.borrowed, true);
    assert.equal((await read(.75)).actualTimestampSeconds, .5);
    assert.deepEqual(await read(1.25), { reason: "timestamp_gap" });
    assert.equal((await read(2.25)).actualTimestampSeconds, 2);
    assert.deepEqual(await read(3), { reason: "eof" });
    assert.equal((await read(.25)).actualTimestampSeconds, 0);
    assert.equal(opened.length - count, 1);
    assert.equal(iterators.length - sinks, 2);
    assert.equal(iterators[sinks].returned, true);
    assert.equal(cache.retainedBytes(), 96);
  } finally { cache.close(); }
  assert.equal(opened.at(-1).disposed, true);
  assert.equal(cache.retainedBytes(), 0);
});

test("different per-layer times or admission retain earlier borrowed observations until upload", async () => {
  const { renderClassicFrame } = await import("../src/index.mjs");
  const cache = new VisualDecoderCache(), resource = { assetId: "asset", contentHash: "b".repeat(64), size: "1" };
  const encoded = new Map([[`asset:${resource.contentHash}:1`, new Uint8Array([1])]]);
  const plan = { identity: { width: 1, height: 1, profile: "classic-v31-gpu-normal-frame" }, resources: [resource],
    layers: [0, .75, 2.25, 0].map((time, index) => ({ elementId: String(index), resource,
      decode: { kind: "video", ...decode(time), requestedTime: { ticks: String(time * 4), timebase: 4 } } })) };
  const observed = [];
  const host = { admission: { maxFramePixels: "1", maxRgbaBytes: "1024", maxEncodedBytes: "1" },
    callback: async (name, args) => name === "admitDecode" ? (args.elementId === "3" ? { ...admission, maxDecodedBytes: "129" } : admission)
      : { identity: plan.identity, decoded: args.decoded }, rgba: async () => {} };
  const originalCheck = checkSources;
  checkSources = sources => {
    for (let index = 0; index < 4; index++) observed.push(sources.get(String(index)));
    assert.deepEqual(observed.map(s => s.actualTimestampSeconds), [0, .5, 2, 0]);
    assert.equal(observed[0].borrowed, true);
    for (const source of observed.slice(1)) {
      assert.notEqual(source.canvas, observed[0].canvas);
      assert.equal(source.borrowed, undefined);
      assert.equal(source.canvas.width, 4);
    }
    assert.equal(observed[0].canvas.width, 4);
  };
  try {
    const count = opened.length;
    await renderClassicFrame(plan, host, {}, signal, encoded, cache);
    assert.equal(opened.length - count, 4);
    assert.equal(cache.retainedBytes(), 96);
    assert.equal(observed[0].canvas.width, 4);
    assert.ok(observed.slice(1).every(s => s.canvas.width === 0));
  } finally { checkSources = originalCheck; cache.close(); }
  assert.equal(observed[0].canvas.width, 0);
});

test("changed admission is rechecked before allocation; inactive and aborted resources release", async () => {
  const cache = new VisualDecoderCache();
  await cache.decode("verified", new Uint8Array([1]), decode(0), admission, signal);
  await assert.rejects(cache.decode("verified", new Uint8Array([1]), decode(0), { ...admission, maxPixels: "7" }, signal), e => e.code === "VISUAL_BUDGET_EXCEEDED");
  assert.equal(cache.retainedBytes(), 0);
  assert.equal(opened.at(-1).disposed, true);
  await cache.decode("other", new Uint8Array([1]), decode(0), admission, signal);
  cache.retain(new Set());
  assert.equal(opened.at(-1).disposed, true);
  const abort = new AbortController(); abort.abort();
  const count = opened.length;
  await assert.rejects(cache.decode("aborted", new Uint8Array([1]), decode(0), admission, abort.signal));
  assert.equal(opened.length, count);
  cache.close();
});

test("retained pool replaces temporary decode capacity within the unchanged whole-frame budget; identical observations share one source", async () => {
  const { renderClassicFrame } = await import("../src/index.mjs");
  const cache = new VisualDecoderCache(), resource = { assetId: "asset", contentHash: "a".repeat(64), size: "1", kind: "video" };
  const encoded = new Map([[`asset:${resource.contentHash}:1`, new Uint8Array([1])]]);
  const plan = { identity: { width: 1, height: 1, profile: "classic-v31-gpu-normal-frame", time: { ticks: "0", timebase: 1 } },
    resources: [resource], layers: ["first", "second"].map(elementId => ({ elementId, resource,
      decode: { kind: "video", ...decode(0), requestedTime: { ticks: "0", timebase: 1 } } })) };
  const host = { admission: { maxFramePixels: "1", maxRgbaBytes: "135", maxEncodedBytes: "1" },
    callback: async (name, args) => name === "admitDecode" ? admission : { identity: plan.identity, decoded: args.decoded }, rgba: async () => {} };
  try {
    const count = opened.length;
    assert.equal((await renderClassicFrame(plan, host, {}, signal, encoded, cache)).peakAccountedBytes, "135");
    assert.equal((await renderClassicFrame(plan, host, {}, signal, encoded, cache)).peakAccountedBytes, "135");
    assert.equal(opened.length - count, 1);
    await assert.rejects(renderClassicFrame(plan, { ...host, admission: { ...host.admission, maxRgbaBytes: "134" } }, {}, signal, encoded, cache), e => e.code === "VISUAL_BUDGET_EXCEEDED");
  } finally { cache.close(); }
});
