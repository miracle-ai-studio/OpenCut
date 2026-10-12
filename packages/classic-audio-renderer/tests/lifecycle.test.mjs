import assert from "node:assert/strict";
import test, { mock } from "node:test";
import { createHash } from "node:crypto";
import { setImmediate } from "node:timers/promises";
import { renderClassicExport } from "../src/index.mjs";

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
function buffer(channels, frames) {
  const data = Array.from({ length: channels }, () => new Float32Array(frames));
  return { length: frames, numberOfChannels: channels, sampleRate: 44100, getChannelData: c => data[c], copyToChannel: (values, c) => data[c].set(values) };
}
const position = { lower: "0", fractionNumerator: "0", fractionDenominator: "1" };
function fixture() {
  const bytes = new Uint8Array([1]);
  const contentHash = createHash("sha256").update(bytes).digest("hex");
  const identity = { projectId: "p", sequenceId: "s", revision: "r", profile: "classic-export-v31-web-audio", sampleRate: 44100 };
  const plan = { identity, frameCount: "1", resources: [{ assetId: "a", contentHash, size: 1, kind: "audio" }], voices: [{ elementId: "e", assetId: "a" }] };
  const counters = { terminate: 0, decode: 0, close: 0 };
  const facts = { mode: "unprepared-linear", decodedSampleRate: 44100, targetSampleRate: 44100, numChannels: 1, outputFrames: "1", tempo: { numerator: "1", denominator: "1" } };
  const context = {
    sampleRate: 44100, state: "running", createBuffer: buffer,
    async decodeAudioData() { counters.decode++; const decoded = buffer(1, 1); decoded.getChannelData(0)[0] = 0.25; return decoded; },
    async close() { counters.close++; this.state = "closed"; },
  };
  const host = {
    engineIdentity: "pinned-engine", resolveVerifiedBytes: async () => bytes,
    admitDecode: async () => ({ maxFrames: 1, maxChannels: 1 }),
    compilePitchPreparation: async () => facts,
    compileSourcePositions: async () => [position],
    compileBlock: async request => ({ identity, startSample: request.startSample, frameCount: 1, voices: [{ elementId: "e", blockOffset: 0, frameCount: 1, positions: [position], amplitudes: [1] }] }),
    createAudioContext: () => context,
    createOfflineContext() { throw new Error("Low peak must not start offline mastering"); },
    async abortRendering() { counters.terminate++; },
  };
  return { plan, host, facts, context, counters, contentHash };
}

test("abort during hash waiting terminates exactly once and waits for acknowledgement", async () => {
  const f = fixture(), controller = new AbortController();
  const started = deferred(), hash = deferred(), ack = deferred();
  mock.method(crypto.subtle, "digest", () => { started.resolve(); return hash.promise; });
  f.host.abortRendering = async () => { f.counters.terminate++; await ack.promise; };
  try {
    let settled = false;
    const work = renderClassicExport(f.plan, f.host, { signal: controller.signal, maxPcmBytes: 1024 });
    const observed = work.then(() => { settled = true; }, () => { settled = true; });
    await started.promise; controller.abort(); await setImmediate();
    assert.equal(f.counters.terminate, 1); assert.equal(settled, false);
    ack.resolve(); await assert.rejects(work, { code: "CANCELLED" }); await observed;
    assert.equal(f.counters.terminate, 1); assert.equal(f.counters.decode, 0);
  } finally {
    hash.resolve(new Uint8Array(Buffer.from(f.contentHash, "hex")).buffer);
    mock.restoreAll(); await setImmediate();
  }
});

test("synchronous block boundary abort cannot let completed work win or invoke termination twice", async () => {
  const f = fixture(), controller = new AbortController(), ack = deferred();
  f.host.abortRendering = async () => { f.counters.terminate++; await ack.promise; };
  const compile = f.host.compileBlock;
  f.host.compileBlock = request => { controller.abort(); return compile(request); };
  let settled = false;
  const work = renderClassicExport(f.plan, f.host, { signal: controller.signal, maxPcmBytes: 1024 });
  const observed = work.then(() => { settled = true; }, () => { settled = true; });
  await setImmediate(); assert.equal(f.counters.terminate, 1); assert.equal(settled, false);
  ack.resolve(); await assert.rejects(work, { code: "CANCELLED" }); await observed; await setImmediate();
  assert.equal(f.counters.terminate, 1); assert.equal(f.counters.close, 1);
});

test("asynchronous host edits cannot mutate prepared facts or change the execution identity", async () => {
  const f = fixture(); f.facts.mode = "prepared-linear";
  f.host.compileSourcePositions = async () => {
    await Promise.resolve();
    f.facts.mode = "unprepared-linear"; f.facts.tempo.numerator = "5";
    return [position];
  };
  const compile = f.host.compileBlock;
  f.host.compileBlock = request => { assert.equal(request.sourceFormats[0].prepared, true); return compile(request); };
  const result = await renderClassicExport(f.plan, f.host, { maxPcmBytes: 1024 });
  assert.equal(result.buffer.getChannelData(0)[0], 0.25);
  assert.equal(result.receipt.preparation[0].facts.mode, "prepared-linear");
  assert.equal(result.receipt.preparation[0].facts.tempo.numerator, "1");
  assert.equal(Object.isFrozen(result.receipt.preparation[0].facts.tempo), true);
  assert.equal(result.receipt.engineIdentity, "pinned-engine");
  const changed = fixture();
  changed.context.close = async () => { await Promise.resolve(); changed.host.engineIdentity = "replacement-engine"; changed.context.state = "closed"; };
  await assert.rejects(renderClassicExport(changed.plan, changed.host, { maxPcmBytes: 1024 }), { code: "AUDIO_ENGINE_CHANGED" });
});


test("256 fixed voice descriptors share one decode and short compiled blocks cover the scene without skipping or repeating samples", async () => {
  const f = fixture(); f.plan.frameCount = "7";
  f.plan.voices = Array.from({ length: 256 }, (_, i) => ({ elementId: `e${i}`, assetId: "a" }));
  const requests = [];
  f.host.compileBlock = async request => {
    requests.push([request.startSample, request.frameCount]);
    assert.equal(request.sourceFormats.length, 256);
    const count = Math.min(2, request.frameCount);
    const silent = request.startSample === "2";
    return { identity: f.plan.identity, startSample: request.startSample, frameCount: count,
      voices: silent ? [] : [{ elementId: request.startSample === "0" ? "e0" : "e255", blockOffset: 0, frameCount: count,
        positions: Array(count).fill(position), amplitudes: Array(count).fill(1) }] };
  };
  const result = await renderClassicExport(f.plan, f.host, { maxPcmBytes: 4096 });
  assert.deepEqual(requests, [["0", 7], ["2", 5], ["4", 3], ["6", 1]]);
  assert.deepEqual([...result.buffer.getChannelData(0)], [.25, .25, 0, 0, .25, .25, .25]);
  assert.deepEqual(result.buffer.getChannelData(1), result.buffer.getChannelData(0));
  assert.equal(f.counters.decode, 1);
  assert.equal(result.receipt.preparation.length, 256);
});

test("compiled WAV blocks cannot return zero, noninteger, oversized or a different start", async () => {
  for (const change of [{ frameCount: 0 }, { frameCount: "1" }, { frameCount: NaN }, { frameCount: 5 }, { startSample: "1" }]) {
    const f = fixture(); f.plan.frameCount = "4";
    f.host.compileBlock = async request => ({ identity: f.plan.identity, startSample: request.startSample, frameCount: request.frameCount, voices: [], ...change });
    await assert.rejects(renderClassicExport(f.plan, f.host, { maxPcmBytes: 1024 }), { code: "INVALID_AUDIO_PLAN" });
    assert.equal(f.counters.close, 1);
  }
});
