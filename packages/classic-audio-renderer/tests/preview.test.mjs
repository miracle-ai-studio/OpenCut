import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { previewBindingDigest,ensurePreviewSourceWindows } from "../src/preview.mjs";
import { mixPreviewBlock } from "../src/preview-mixing.mjs";
import { validatePreviewBegin, validatePreviewNext } from "../host/preview-runner.mjs";
import { rollingPreviewSource, validateRollingPreviewAdmission } from "../src/rolling-preview-source.mjs";
import { PcmBudget } from "../src/validation.mjs";
import { packPreviewBinding, expandPreviewBinding, compactPreviewReceipt } from "../host/preview-binding.mjs";
import { configurePreviewPage, preparePreviewRealm, nextPreviewRealm, previewEnvelope, unwrapPreviewEnvelope,encodePreviewFacts } from "../host/preview-browser.mjs";
import { PROTOCOL_LIMITS, PacketWriter } from "../host/protocol.mjs";
import { Writable } from "node:stream";
import { preparePreviewPage, nextPreviewPage } from "../host/preview-page.mjs";
import { readFile } from "node:fs/promises";
import { unpackPreviewBlock,readPreviewFactsText } from "../src/preview-packet.mjs";

const buffer = channels => ({ length: channels[0].length, numberOfChannels: channels.length, getChannelData: c => new Float32Array(channels[c]) });
const point = (segmentId, lower, upper = lower, fraction = 0) => ({ segmentId, lower: String(lower), upper: String(upper), fraction });
function densePacket(packet) {
  const positions = [], amplitudes = [];
  return { ...packet, block: { ...packet.block, voices: packet.block.voices.map((voice, index) => {
    const { positions: points, amplitudes: gains, ...rest } = voice;
    positions.push(points); amplitudes.push(gains);
    return { ...rest, positionRef: index, amplitudeRef: index };
  }) }, positionTables: positions, amplitudeTables: amplitudes };
}

test("binding identity ignores object spelling order and floating JSON spelling, preserving exact f64 PTS bits", async () => {
  const expected = createHash("sha256").update('{"a":{"$f64":"0000000000000000"},"b":{"$f64":"3eb0c6f7a0b5ed8d"}}').digest("hex");
  assert.equal(await previewBindingDigest({ b: 1e-6, a: -0 }), expected);
  assert.equal(await previewBindingDigest({ a: 0, b: .000001 }), expected);
});
test("PTS segments remain separate: null gaps stay silent and interpolation cannot join unrelated packets", () => {
  const sources = new Map([["ordinary", new Map([[0, buffer([[.25, .5]])], [1, buffer([[-.5, -.25], [.75, .25]])]])]]);
  const block = { voices: [{ elementId: "ordinary", blockOffset: 0, frameCount: 5, positions: [point(0, 0), point(0, 0, 1, .5), null, point(1, 0), point(1, 1)], amplitudes: [1, 2, 99, .5, 1] }] };
  const actual = mixPreviewBlock(block, sources, 5);
  assert.deepEqual([...actual[0]], [.25, .75, 0, -.25, -.25]);
  assert.deepEqual([...actual[1]], [.25, .75, 0, .375, .25]);
  block.voices[0].positions[1] = point(0, 1, 2, .5);
  assert.throws(() => mixPreviewBlock(block, sources, 5), { code: "INVALID_AUDIO_PLAN" });
});
test("prepared origin-zero samples use supplied amplitude ramps, with independent Float32 expected levels", () => {
  const sources = new Map([["prepared", new Map([[0, buffer([[.25, .25, .25]])]])]]);
  const low = 10 ** (-6 / 20), amplitudes = [1, (1 + low) / 2, low];
  const actual = mixPreviewBlock({ voices: [{ elementId: "prepared", blockOffset: 1, frameCount: 3, positions: [point(0, 0), point(0, 1), point(0, 2)], amplitudes }] }, sources, 4);
  assert.deepEqual([...actual[0]], [0, .25, Math.fround(.25 * (1 + low) / 2), Math.fround(.25 * low)]);
  assert.deepEqual(actual[1], actual[0]);
  assert.notEqual(actual[0][2], Math.fround(.25 * 10 ** (-3 / 20)));
});
test("preview rejects duplicate voices, missing segments, nonfinite amplitude and Float32 overflow", () => {
  const source = new Map([["v", new Map([[0, buffer([[.5]])]])]]);
  const voice = { elementId: "v", blockOffset: 0, frameCount: 1, positions: [point(0, 0)], amplitudes: [1] };
  for (const voices of [[voice, voice], [{ ...voice, amplitudes: [NaN] }], [{ ...voice, amplitudes: [1e100] }], [{ ...voice, positions: [point(2, 0)] }]]) assert.throws(() => mixPreviewBlock({ voices }, source, 1));
});
test("long-lived preview fixes profile, one-hour TTL, source branch and block credit independently of WAV range", () => {
  const begin = { executionId: "fixed", type: "begin", preparationDeadlineMs: 120000,
    plan: { identity: { projectId: "p", revision: "r", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000 }, frameCount: "24017", encodedSource: { mode: "verified-range", cacheBytes: 65536 }, resources: [{ assetId: "a", kind: "audio", contentHash: "a".repeat(64), size: 1 }], voices: [{ elementId: "e", assetId: "a", sourceMode: "preserve-pts" }] },
    resources: [{ assetId: "a", handle: "private" }], admission: { maxPcmBytes: 2097152, maxEncodedBytes: 1048576, maxRssBytes: "104857600", deadlineMs: 3600000, quotaProfile: "monitored-terminate" } };
  assert.equal(validatePreviewBegin(begin).plan.identity.profile, "classic-realtime-v31-web-audio");
  assert.throws(() => validatePreviewBegin({ ...begin, type: "unknown" }), { code: "INVALID_HOST_PROTOCOL" });
  const dense = structuredClone(begin); dense.plan.voices = Array.from({ length: 256 }, (_, i) => ({ elementId: `e${i}`, assetId: "a", sourceMode: "preserve-pts" }));
  assert.equal(validatePreviewBegin(dense).plan.voices.length, 256);
  dense.plan.voices.push({ elementId: "extra", assetId: "a", sourceMode: "preserve-pts" });
  assert.throws(() => validatePreviewBegin(dense), { code: "INVALID_HOST_PROTOCOL" });
  assert.equal(validatePreviewBegin({ ...begin, plan: { ...begin.plan, voices: [{ ...begin.plan.voices[0], sourceMode: "prepared-concat" }] } }).plan.voices[0].sourceMode, "prepared-concat");
  assert.throws(() => validatePreviewBegin({ ...begin, plan: { ...begin.plan, range: { startSample: "0", frameCount: "1" } } }));
  assert.throws(() => validatePreviewBegin({ ...begin, admission: { ...begin.admission, deadlineMs: 300000 } }));
  const command = { op: "next", id: "1", generation: "18446744073709551615", overlayEpoch: "0", startSample: "9007199254740993", frameCount: 4096 };
  assert.equal(validatePreviewNext(command).startSample, "9007199254740993");
  assert.throws(() => validatePreviewNext({ ...command, frameCount: 4097 }));
});


test("preview block union and sample work remain bounded when source metadata exceeds 32 voices", () => {
  const names = Array.from({ length: 33 }, (_, i) => `v${i}`);
  const sources = new Map(names.map(id => [id, new Map([[0, buffer([[.25]])]])]));
  const voice = (elementId, count) => ({ elementId, blockOffset: 0, frameCount: count, positions: Array(count).fill(point(0, 0)), amplitudes: Array(count).fill(1) });
  assert.throws(() => mixPreviewBlock({ voices: names.map(id => voice(id, 1)) }, sources, 1), { code: "INVALID_AUDIO_PLAN" });
  assert.throws(() => mixPreviewBlock({ voices: names.slice(0, 17).map(id => voice(id, 4096)) }, sources, 4096), { code: "INVALID_AUDIO_PLAN" });
  const actual = mixPreviewBlock({ voices: names.slice(0, 16).map(id => voice(id, 4096)) }, sources, 4096);
  assert(actual[0].every(value => value === 4));
  assert.deepEqual(actual[1], actual[0]);
});

function sharedFixture(count = 100, segmentCount = 188) {
  const plan = { resources: [{ assetId: "fixed-source" }], voices: Array.from({ length: count }, (_, i) => ({ elementId: `clip-${i}`, assetId: "fixed-source", sourceMode: "preserve-pts" })) };
  const segments = Array.from({ length: segmentCount }, (_, i) => ({ segmentId: i, timestampSeconds: i * 1024 / 48000, frameCount: "1024" }));
  const formats = plan.voices.map(v => ({ elementId: v.elementId, sampleRate: 48000, frameCount: String(segmentCount * 1024), channels: 1, prepared: false, segments }));
  return { plan, formats };
}

test("100 voices share 188 observed segments in bounded request and receipt without changing the expanded binding hash", async () => {
  const { plan, formats } = sharedFixture();
  const binding = packPreviewBinding(plan, formats);
  assert.equal(binding.ordinarySources.length, 1);
  assert.equal(binding.sourceFormats.length, 100);
  assert(binding.sourceFormats.every(f => !Object.hasOwn(f, "segments") && f.sourceRef === "fixed-source"));
  const expanded = expandPreviewBinding(plan, binding);
  assert.deepEqual(expanded, formats);
  const fullFacts = { decoders: [{ assetId: "fixed-source", primary: { trackId: 2, codec: "aac", sampleRate: 48000, channels: 1 } }], sourceFormats: formats, preparation: [] };
  const sourceBindingSha256 = await previewBindingDigest(fullFacts);
  assert.equal(await previewBindingDigest({ ...fullFacts, sourceFormats: expanded }), sourceBindingSha256);
  const receipt = { ...fullFacts, sourceBindingSha256, identity: { revision: "fixed" }, engineIdentity: "pinned", peakAccountedBytes: 768000 };
  const compact = compactPreviewReceipt(plan, receipt, binding);
  assert(!Object.hasOwn(compact, "sourceFormats"));
  assert.equal(compact.sourceBindingSha256, sourceBindingSha256);
  let wireBytes = 0;
  const writer = new PacketWriter(new Writable({ write(bytes, encoding, done) { wireBytes = Math.max(wireBytes, bytes.length - 1); done(); } }), "fixed-execution");
  assert.throws(() => writer.send({ type: "request", id: "1", op: "bindPreviewSources", args: { sourceFormats: formats } }), { code: "HOST_PACKET_TOO_LARGE" });
  await writer.send({ type: "request", id: "1", op: "bindPreviewSourcesShared", args: binding });
  await writer.send({ type: "prepared", receipt: compact });
  assert(wireBytes < PROTOCOL_LIMITS.packetBytes);
});

test("shared transport rejects wrong references, coverage order, duplicates, orphan sources, modes and unknown fields", () => {
  const { plan, formats } = sharedFixture(2, 2);
  const binding = packPreviewBinding(plan, formats);
  const malformed = [
    b => { b.schemaVersion = 2; },
    b => { b.sourceFormats.reverse(); },
    b => { b.sourceFormats[1].elementId = b.sourceFormats[0].elementId; },
    b => { b.sourceFormats[0].sourceRef = "other-source"; },
    b => { b.sourceFormats[0].sourceRef = null; },
    b => { b.sourceFormats[0].prepared = true; b.sourceFormats[0].sourceRef = null; },
    b => { b.sourceFormats[1].sampleRate = 44100; },
    b => { b.ordinarySources.push(structuredClone(b.ordinarySources[0])); },
    b => { b.ordinarySources.push({ assetId: "unused", segments: b.ordinarySources[0].segments }); },
    b => { b.ordinarySources[0].segments[1].segmentId = 0; },
    b => { b.ordinarySources[0].segments[0].timestampSeconds = NaN; },
    b => { b.sourceFormats[0].extra = "hidden"; },
  ];
  for (const change of malformed) { const bad = structuredClone(binding); change(bad); assert.throws(() => expandPreviewBinding(plan, bad)); }
  const changed = structuredClone(formats); changed[1].segments = structuredClone(changed[1].segments); changed[1].segments[0].timestampSeconds = .1;
  assert.throws(() => packPreviewBinding(plan, changed), { code: "INVALID_HOST_CALLBACK" });
  const altered = structuredClone(formats); altered[0].segments[0].frameCount = "512";
  assert.throws(() => compactPreviewReceipt(plan, { sourceFormats: altered }, binding));
  const mixedPlan = structuredClone(plan); mixedPlan.voices[1].sourceMode = "prepared-concat";
  const mixedFormats = structuredClone(formats); mixedFormats[1].prepared = true; mixedFormats[1].segments = [];
  const mixed = packPreviewBinding(mixedPlan, mixedFormats);
  assert.equal(mixed.sourceFormats[1].sourceRef, null);
  assert.deepEqual(expandPreviewBinding(mixedPlan, mixed), mixedFormats);
  mixed.sourceFormats[1].sourceRef = "fixed-source";
  assert.throws(() => expandPreviewBinding(mixedPlan, mixed));
});

test("a small source dictionary cannot amplify expanded source facts past the existing 16 MiB fact budget", () => {
  const { plan, formats } = sharedFixture(256, 2000);
  const binding = { ordinarySources: [{ assetId: "fixed-source", segments: formats[0].segments }], sourceFormats: formats.map(({ segments, ...f }) => ({ ...f, sourceRef: "fixed-source" })) };
  assert(Buffer.byteLength(JSON.stringify(binding)) < PROTOCOL_LIMITS.packetBytes);
  assert.throws(() => packPreviewBinding(plan, formats), { code: "AUDIO_BUDGET_EXCEEDED" });
  assert.throws(() => expandPreviewBinding(plan, binding), { code: "AUDIO_BUDGET_EXCEEDED" });
});

test("Node preview bridge preserves exact safe failure codes and only captures accepted shared facts", async () => {
  const handlers = new Map(), { plan, formats } = sharedFixture(2, 2), requests = [];
  const fullReceipt = { sourceFormats: formats, sourceBindingSha256: "unchanged", decoders: [], preparation: [] };
  const command = { id: "1", generation: "3", overlayEpoch: "7", startSample: "0", frameCount: 1 };
  const realm = { begin: { plan, resources: [], admission: {} }, identity: { engine: "pinned" }, signal: new AbortController().signal,
    writer: { send: async () => {} },
    page: { exposeFunction: async (name, fn) => handlers.set(name, fn), evaluate: async fn => ({ ok: true, value: fn.name === "preparePreviewPage" ? fullReceipt : { receipt: { completed: true }, frameCount: 1, channelsBase64: ["AAAAAA==", "AAAAAA=="] } }) },
    broker: { request: async (op, args) => { requests.push({ op, args }); return { bound: true }; } } };
  await configurePreviewPage(realm);
  const callback = handlers.get("__opencutPreviewCallback");
  assert.deepEqual(await callback("bindPreviewSources", { sourceFormats: formats, window: null }), { ok: true, value: { bound: true } });
  assert.equal(requests[0].op, "bindPreviewSourcesShared");
  assert.deepEqual(requests[0].args.binding, packPreviewBinding(plan, formats));
  const allocation = { resource: plan.resources[0], primary: { trackId: 1, codec: "aac", sampleRate: 44100, channels: 2 } };
  // One allocation operation selects the fixed content's execution semantics.
  assert.deepEqual(await callback("admitPreviewDecode", allocation), { ok: true, value: { bound: true } });
  assert.deepEqual(await callback("admitPreviewDecode", { ...allocation, resource: { ...allocation.resource, contentHash: "changed" } }), { ok: false, error: { code: "NOT_AUTHORIZED" } });
  assert.deepEqual(await callback("releaseDecodedSource", { resource: plan.resources[0] }), { ok: true, value: { bound: true } });
  assert.deepEqual(requests.at(-1), { op: "releaseDecodedSource", args: { resource: plan.resources[0] } });
  assert.deepEqual(await callback("releaseDecodedSource", { resource: { ...plan.resources[0], contentHash: "changed" } }), { ok: false, error: { code: "NOT_AUTHORIZED" } });
  assert(!Object.hasOwn(await preparePreviewRealm(realm), "sourceFormats"));
  assert.deepEqual(await nextPreviewRealm(realm, command), { completed: true });
  assert(!handlers.has("__opencutPreviewPcm"));
  realm.previewBinding = undefined;
  realm.broker.request = async () => { throw Object.assign(new Error("private host path"), { code: "AUDIO_BUDGET_EXCEEDED" }); };
  const failure = await callback("bindPreviewSources", { sourceFormats: formats, window: null });
  assert.deepEqual(failure, { ok: false, error: { code: "AUDIO_BUDGET_EXCEEDED" } });
  assert.equal(realm.previewBinding, undefined);
  assert.throws(() => unwrapPreviewEnvelope(failure), { code: "AUDIO_BUDGET_EXCEEDED", message: "Isolated preview operation failed" });
  realm.page.evaluate = async () => failure;
  await assert.rejects(nextPreviewRealm(realm, command), { code: "AUDIO_BUDGET_EXCEEDED" });
  assert.deepEqual(await previewEnvelope(async () => { throw new Error("HOST_PACKET_TOO_LARGE private path"); }), { ok: false, error: { code: "AUDIO_PREVIEW_FAILED" } });
  assert.deepEqual(await previewEnvelope(async () => { throw { code: "invalid/path" }; }), { ok: false, error: { code: "AUDIO_PREVIEW_FAILED" } });
  assert.deepEqual(await previewEnvelope(async () => undefined), { ok: true, value: undefined });
  assert.throws(() => unwrapPreviewEnvelope({ ok: false, error: { code: "CANCELLED", path: "hidden" } }), { code: "AUDIO_PROTOCOL_ERROR" });
});

test("only compiled preview facts cross the page bridge as bounded JSON text without changing sample facts", async () => {
  const handlers = new Map(), { plan, formats } = sharedFixture(1, 1);
  const block = { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000, channels: 2 }, startSample: "9007199254740993", frameCount: 2,
    voices: [{ elementId: "clip-0", blockOffset: 0, frameCount: 2,
      positions: [point(17, "9007199254740993", "9007199254740994", .125), null], amplitudes: [.001, 10] }], receipts: [{ trackId: "track", elementId: "clip-0", decision: "ready" }] };
  const packed = { block: { ...block, voices: [{ elementId: "clip-0", blockOffset: 0, frameCount: 2, positionRef: 0, amplitudeRef: 0 }] },
    positionTables: [[[17, "9007199254740993", "9007199254740994", .125], null]], amplitudeTables: [[.001, 10]] };
  const packedText = JSON.stringify(packed);
  const operations = [];
  const realm = { previewCommand: { startSample: block.startSample, frameCount: block.frameCount }, begin: { plan, resources: [], admission: {} }, signal: new AbortController().signal,
    page: { exposeFunction: async (name, fn) => handlers.set(name, fn) }, broker: { request: async op => { operations.push(op); return ["compilePreviewBlockPacked", "bindAndCompilePreviewBlockPacked"].includes(op) ? packedText : block; } } };
  await configurePreviewPage(realm);
  const callback = handlers.get("__opencutPreviewCallback"), args = { startSample: block.startSample, frameCount: 2 };
  const envelope = await callback("compilePreviewBlock", args);
  assert.equal(envelope.ok, true);
  assert.equal(envelope.value.codec, "deflate-base64");
  assert.deepEqual(JSON.parse(await readPreviewFactsText(envelope.value)), packed);
  assert.equal(operations[0], "compilePreviewBlockPacked");
  const combined = await callback("bindAndCompilePreviewBlock", { sourceFormats: formats, window: args });
  assert.equal(combined.ok,true);assert.equal(await readPreviewFactsText(combined.value),packedText);
  assert.equal(operations[1], "bindAndCompilePreviewBlockPacked");
  assert.deepEqual(await callback("bindAndCompilePreviewBlock", { sourceFormats: formats, window: { ...args, frameCount: 3 } }), { ok: false, error: { code: "INVALID_HOST_CALLBACK" } });
  assert.equal(operations.length, 2);
  assert.equal((await realm.nodePreviewRequest("compilePreviewBlock", args)).value, packedText);
  assert.equal((await realm.nodePreviewRequest("bindAndCompilePreviewBlock", {sourceFormats:formats,window:args})).value, packedText);
  assert.deepEqual(await realm.nodePreviewRequest("unknown", {}), {ok:false,error:{code:"INVALID_HOST_CALLBACK"}});

  assert.deepEqual(await callback("compileSourcePositions", { elementId: "clip-0", frameCount: 2 }), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });

  // Route only the browser module import to a tiny consumer. The serialized
  // production page function, including its callback parser, runs unchanged.
  const moduleUrl = "data:text/javascript," + encodeURIComponent("export async function prepareClassicPreview(plan,host){return {receipt:await host.compilePreviewBlock({startSample:'9007199254740993',frameCount:2})}}");
  const pageSource = preparePreviewPage.toString()
    .replace('import("/renderer/src/preview.mjs")', `import(${JSON.stringify(moduleUrl)})`)
    .replace('import("/renderer/src/preview-packet.mjs")', `import(${JSON.stringify(new URL("../src/preview-packet.mjs", import.meta.url).href)})`);
  const page = new Function("return (" + pageSource + ")")();
  const prior = globalThis.__opencutPreviewCallback;
  const priorProducer = globalThis.__opencutPreviewProducer;
  try {
    globalThis.__opencutPreviewCallback = callback;
    assert.deepEqual(await page({ plan, engineIdentity: "fixed", admission: {} }), { ok: true, value: block });
    for (const value of [block, undefined, "{bad-json"]) {
      globalThis.__opencutPreviewCallback = async () => ({ ok: true, value });
      assert.deepEqual(await page({ plan, engineIdentity: "fixed", admission: {} }), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
    }
    globalThis.__opencutPreviewCallback = async () => ({ ok: false, error: { code: "NOT_AUTHORIZED" } });
    assert.deepEqual(await page({ plan, engineIdentity: "fixed", admission: {} }), { ok: false, error: { code: "NOT_AUTHORIZED" } });
  } finally {
    if (prior === undefined) delete globalThis.__opencutPreviewCallback; else globalThis.__opencutPreviewCallback = prior;
    if (priorProducer === undefined) delete globalThis.__opencutPreviewProducer; else globalThis.__opencutPreviewProducer = priorProducer;
  }
  realm.broker.request = async () => "é".repeat(PROTOCOL_LIMITS.replyBytes / 2 + 1);
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "AUDIO_BUDGET_EXCEEDED" } });
  assert.deepEqual(await realm.nodePreviewRequest("compilePreviewBlock", args), { ok: false, error: { code: "AUDIO_BUDGET_EXCEEDED" } });
  realm.broker.request = async () => undefined;
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
  realm.broker.request = async () => packed;
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
  const rawText = packedText.replace(",0.125]", ",-0.0]").replace('"schemaVersion":2', '"schemaVersion": 2');
  realm.broker.request = async () => rawText;
  assert.equal(await readPreviewFactsText((await callback("compilePreviewBlock", args)).value), rawText, "Node does not normalize native JSON spelling or signed zero");
  assert(Object.is(unpackPreviewBlock(JSON.parse(rawText)).voices[0].positions[0].fraction, -0));
  const direct=await realm.nodePreviewRequest("compilePreviewBlock",args);assert.equal(direct.value,rawText);assert(Object.is(unpackPreviewBlock(JSON.parse(direct.value)).voices[0].positions[0].fraction,-0));
  realm.broker.request = async () => { throw Object.assign(new Error("private path"), { code: "NOT_AUTHORIZED" }); };
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "NOT_AUTHORIZED" } });
});

function tablePacket() {
  return { block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000, channels: 2 },
      startSample: "0", frameCount: 3,
      voices: [{ elementId: "a", blockOffset: 0, frameCount: 3, positionRef: 0, amplitudeRef: 0 },
        { elementId: "b", blockOffset: 0, frameCount: 3, positionRef: 0, amplitudeRef: 0 },
        { elementId: "c", blockOffset: 0, frameCount: 3, positionRef: 1, amplitudeRef: 1 },
        { elementId: "d", blockOffset: 0, frameCount: 3, positionRef: 0, amplitudeRef: 1 }],
      receipts: ["a", "b", "c", "d"].map(elementId => ({ trackId: "track", elementId, decision: "ready" })) },
    positionTables: [[[7, "0", "1", .5], null, [7, "2", "2", -0]], [[7, "1", "2", 0], [7, "0", "1", .5], null]],
    amplitudeTables: [[1, 2, .5], [-1, .5, 3]],
  };
}
test("Packed transport shares only readonly sample tables and mixed independent facts still reach the expected PCM", () => {
  const packet = tablePacket(), block = unpackPreviewBlock(packet);
  assert.deepEqual(block.identity, packet.block.identity);
  assert.deepEqual(block.receipts, packet.block.receipts);
  assert.equal(block.startSample, "0"); assert.equal(block.frameCount, 3);
  assert.deepEqual(block.voices.map(v => [v.elementId, v.blockOffset, v.frameCount]), [["a", 0, 3], ["b", 0, 3], ["c", 0, 3], ["d", 0, 3]]);
  assert.deepEqual(block.voices[0].positions, [point(7, 0, 1, .5), null, point(7, 2, 2, -0)]);
  assert.deepEqual(block.voices[2].positions, [point(7, 1, 2, 0), point(7, 0, 1, .5), null]);
  assert.equal(block.voices[0].positions, block.voices[1].positions);
  assert.equal(block.voices[0].positions, block.voices[3].positions);
  assert.equal(block.voices[0].amplitudes, block.voices[1].amplitudes);
  assert.equal(block.voices[2].amplitudes, block.voices[3].amplitudes);
  assert.notEqual(block.voices[0].positions, block.voices[2].positions);
  assert.notEqual(block.voices[0].amplitudes, block.voices[2].amplitudes);
  assert(Object.isFrozen(block.voices[0].positions) && Object.isFrozen(block.voices[0].positions[0]) && Object.isFrozen(block.voices[0].amplitudes));
  assert.throws(() => { block.voices[0].positions[0].lower = "1"; }, TypeError);
  assert.throws(() => { block.voices[0].amplitudes[0] = 2; }, TypeError);
  const data = new Float32Array([-.5, .25, .5]);
  const samples = { length: 3, numberOfChannels: 1, getChannelData: () => data };
  const sources = new Map(["a", "b", "c", "d"].map(id => [id, new Map([[7, samples]])]));
  const pcm = mixPreviewBlock(block, sources, 3);
  assert.deepEqual([...pcm[0]], [-.375, -.0625, 2]);
  assert.deepEqual(pcm[1], pcm[0]);
  assert.deepEqual(unpackPreviewBlock({ block: { ...packet.block, voices: [], receipts: [] }, positionTables: [], amplitudeTables: [] }).voices, []);
});

test("Packed transport rejects orphan or out-of-order tables, bad references and facts without discounting shared execution work", () => {
  const changes = [
    p => { p.schemaVersion = 1; }, p => { p.extra = true; }, p => { p.block.startSample = "18446744073709551615"; },
    p => { p.block.voices[0].positionRef = 1; }, p => { p.block.voices[0].amplitudeRef = 1; },
    p => { p.block.voices[0].positionRef = -1; }, p => { p.block.voices[0].amplitudeRef = .5; },
    p => { p.block.voices[0].positionRef = 2; }, p => { p.block.voices[0].amplitudeRef = 2; },
    p => { p.block.voices.reverse(); }, p => { p.block.voices[0].extra = true; },
    p => { p.positionTables.push([null]); }, p => { p.amplitudeTables.push([1]); },
    p => { p.positionTables[0].pop(); }, p => { p.amplitudeTables[0].pop(); },
    p => { p.positionTables[0][0].pop(); }, p => { p.positionTables[0][0][0] = 0x100000000; },
    p => { p.positionTables[0][0][1] = "01"; }, p => { p.positionTables[0][0][2] = "18446744073709551616"; },
    p => { p.positionTables[0][0][3] = NaN; }, p => { p.positionTables[0][0][3] = -1; },
    p => { p.amplitudeTables[0][0] = Infinity; }, p => { p.positionTables = Array(33).fill([null]); },
    p => { p.amplitudeTables = Array(33).fill([1]); },
  ];
  for (const change of changes) { const packet = tablePacket(); change(packet); assert.throws(() => unpackPreviewBlock(packet), { code: "AUDIO_PROTOCOL_ERROR" }); }
  const dense = tablePacket(); dense.block.frameCount = 4096;
  dense.positionTables = [Array(4096).fill(null)]; dense.amplitudeTables = [Array(4096).fill(1)];
  dense.block.voices = Array.from({ length: 17 }, (_, i) => ({ elementId: `e${i}`, blockOffset: 0, frameCount: 4096, positionRef: 0, amplitudeRef: 0 }));
  dense.block.receipts = dense.block.voices.map(v => ({ trackId: "track", elementId: v.elementId, decision: "ready" }));
  assert.throws(() => unpackPreviewBlock(dense), { code: "AUDIO_PROTOCOL_ERROR" });
  dense.block.voices.pop(); dense.block.receipts.pop();
  assert.equal(unpackPreviewBlock(dense).voices.length, 16, "65536 actual voice samples remain admitted despite transport sharing");
  const exact = tablePacket(); exact.positionTables[0][0] = [0xffffffff, "18446744073709551615", "18446744073709551615", -0];
  assert.deepEqual(unpackPreviewBlock(exact).voices[0].positions[0], point(0xffffffff, "18446744073709551615", "18446744073709551615", -0));
});

test("packed preview projection preserves every full field and rejects malformed or over-budget sample facts", () => {
  const packet = { block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 44100, channels: 2 },
    startSample: "18446744073709547519", frameCount: 4,
    receipts: [{ trackId: "track-a", elementId: "voice", decision: "requires_probe" }, { trackId: "track-b", elementId: "excluded", decision: "outside_block" }],
    voices: [{ elementId: "voice", blockOffset: 1, frameCount: 3, positions: [[0xffffffff, "18446744073709551615", "18446744073709551615", -0], null, [2, "9007199254740993", "9007199254740994", .125]], amplitudes: [-1, .001, 10] }] } };
  const expected = { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 44100, channels: 2 },
    startSample: "18446744073709547519", frameCount: 4,
    receipts: [{ trackId: "track-a", elementId: "voice", decision: "requires_probe" }, { trackId: "track-b", elementId: "excluded", decision: "outside_block" }],
    voices: [{ elementId: "voice", blockOffset: 1, frameCount: 3, positions: [point(0xffffffff, "18446744073709551615", "18446744073709551615", -0), null, point(2, "9007199254740993", "9007199254740994", .125)], amplitudes: [-1, .001, 10] }] };
  assert.deepEqual(unpackPreviewBlock(densePacket(packet)), expected);
  assert(Array.isArray(packet.block.voices[0].positions[0]), "Projection does not mutate the wire payload");
  const bad = [
    p => { p.schemaVersion = 2; }, p => { p.extra = true; }, p => { p.block.extra = true; },
    p => { p.block.identity.channels = 1; }, p => { p.block.startSample = "01"; }, p => { p.block.frameCount = 4097; },
    p => { p.block.voices[0].positions[0] = { segmentId: 0, lower: "0", upper: "0", fraction: 0 }; },
    p => { p.block.voices[0].positions[0].pop(); }, p => { p.block.voices[0].positions[0][0] = 0x100000000; },
    p => { p.block.voices[0].positions[0][1] = 0; }, p => { p.block.voices[0].positions[0][1] = "00"; },
    p => { p.block.voices[0].positions[0][2] = "18446744073709551616"; },
    p => { p.block.voices[0].positions[2][2] = "9007199254740995"; },
    p => { p.block.voices[0].positions[2][2] = "9007199254740992"; },
    p => { p.block.voices[0].positions[2][3] = NaN; }, p => { p.block.voices[0].positions[2][3] = 1.1; },
    p => { p.block.voices[0].amplitudes[0] = Infinity; }, p => { p.block.voices[0].amplitudes.pop(); },
    p => { p.block.voices[0].positions.pop(); }, p => { p.block.voices[0].blockOffset = 2; },
    p => { p.block.voices[0].frameCount = 0; }, p => { p.block.voices.push(p.block.voices[0]); },
    p => { p.block.receipts.pop(); p.block.receipts.pop(); },
  ];
  for (const change of bad) { const malformed = structuredClone(packet); change(malformed); assert.throws(() => unpackPreviewBlock(densePacket(malformed)), { code: "AUDIO_PROTOCOL_ERROR" }); }
  const dense = structuredClone(packet); dense.block.frameCount = 4096;
  dense.block.voices = Array.from({ length: 17 }, (_, i) => ({ elementId: `e${i}`, blockOffset: 0, frameCount: 4096, positions: Array(4096).fill(null), amplitudes: Array(4096).fill(1) }));
  dense.block.receipts = dense.block.voices.map(v => ({ trackId: "track", elementId: v.elementId, decision: "ready" }));
  assert.throws(() => unpackPreviewBlock(densePacket(dense)), { code: "AUDIO_PROTOCOL_ERROR" });
  dense.block.voices.forEach(v => { v.frameCount = 1; v.positions = [null]; v.amplitudes = [1]; });
  dense.block.voices = Array.from({ length: 33 }, (_, i) => ({ ...dense.block.voices[0], elementId: `e${i}` }));
  dense.block.receipts = dense.block.voices.map(v => ({ trackId: "track", elementId: v.elementId, decision: "ready" }));
  assert.throws(() => unpackPreviewBlock(densePacket(dense)), { code: "AUDIO_PROTOCOL_ERROR" });
});

test("packed identifiers match native UTF8 and whitespace bounds, active order follows receipts, and sample end cannot overflow", () => {
  const packet = { block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000, channels: 2 },
    startSample: "18446744073709551613", frameCount: 2,
    receipts: [{ trackId: "track", elementId: "a", decision: "ready" }, { trackId: "track", elementId: "excluded", decision: "non_audio" }, { trackId: "track", elementId: "b", decision: "ready" }],
    voices: ["a", "b"].map(elementId => ({ elementId, blockOffset: 0, frameCount: 1, positions: [null], amplitudes: [1] })) } };
  assert.equal(unpackPreviewBlock(densePacket(packet)).startSample, "18446744073709551613");
  for (const value of ["", " \u3000", "\u0085", "x\u009fy", "é".repeat(513), "😀".repeat(257)]) {
    const bad = structuredClone(packet); bad.block.identity.projectId = value;
    assert.throws(() => unpackPreviewBlock(densePacket(bad)), { code: "AUDIO_PROTOCOL_ERROR" });
  }
  // Rust accepts nonempty FEFF and preserves surrounding authored whitespace.
  for (const value of ["é".repeat(512), "😀".repeat(256), "\ufeff", " a "]) {
    const valid = structuredClone(packet); valid.block.identity.projectId = value;
    assert.equal(unpackPreviewBlock(densePacket(valid)).identity.projectId, value);
  }
  for (const change of [
    p => { p.block.startSample = "18446744073709551614"; },
    p => { p.block.voices.reverse(); },
    p => { p.block.receipts[0].trackId = "\u3000"; },
    p => { p.block.receipts[0].elementId = "é".repeat(513); },
  ]) {
    const bad = structuredClone(packet); change(bad);
    assert.throws(() => unpackPreviewBlock(densePacket(bad)), { code: "AUDIO_PROTOCOL_ERROR" });
  }
  const manyReceipts = structuredClone(packet);
  manyReceipts.block.receipts.unshift(...Array.from({ length: 300 }, (_, i) => ({ trackId: "track", elementId: `non-audio-${i}`, decision: "non_audio" })));
  assert.equal(unpackPreviewBlock(densePacket(manyReceipts)).receipts.length, 303, "Non-audio receipts do not consume the 256 participating voice bound");
});

test("expanded source admission refuses before host binding and releases the prepared context on failure", async () => {
  // Redirect only the decoder import to an observed-facts fixture. Run the
  // production preparation orchestration and budget/cleanup without codecs or
  // a new injection seam in the renderer. This proves no PCM/render parity.
  const sourceUrl = new URL("../src/preview.mjs", import.meta.url);
  const decoderUrl = "data:text/javascript," + encodeURIComponent("export async function decodePreviewResource(resource,bytes,context,host){return host.observedForTest} export async function concatenatePreviewSource(){throw new Error('unexpected prepared branch')}");
  const source = (await readFile(sourceUrl, "utf8")).replace(/from "(\.\/[^\"]+)"/g, (match, relative) =>
    `from ${JSON.stringify(relative === "./preview-decoder.mjs" ? decoderUrl : new URL(relative, sourceUrl).href)}`);
  const { prepareClassicPreview } = await import("data:text/javascript," + encodeURIComponent(source));
  const { plan, formats } = sharedFixture(256, 2000);
  const bytes = new Uint8Array([1]);
  plan.identity = { profile: "classic-realtime-v31-web-audio", sampleRate: 48000 };
  plan.resources = [{ assetId: "fixed-source", kind: "audio", size: 1, contentHash: createHash("sha256").update(bytes).digest("hex") }];
  let bound = 0, released = 0, closed = 0;
  const host = { engineIdentity: "fixed", resolveRangeSource: async () => bytes,
    createAudioContext: () => ({ close: async () => { closed++; } }),
    observedForTest: { primary: { channels: 1 }, sampleRate: 48000, frameCount: formats[0].frameCount, sourceFrameCount: formats[0].frameCount, channels: 1,
      segments: new Map(), facts: formats[0].segments, release: () => { released++; } },
    observePreviewDecoded: async () => undefined,
    bindPreviewSources: async () => { bound++; throw new Error("budget must fail before binding"); },
  };
  await assert.rejects(prepareClassicPreview(plan, host, { maxPcmBytes: 4, maxEncodedBytes: 1024 }), { code: "AUDIO_BUDGET_EXCEEDED" });
  assert.equal(bound, 0);
  assert.equal(released, 1);
  assert.equal(closed, 1);
  // Reuse this orchestration case for resource-at-a-time retirement. Decoder
  // facts are fixtures;the real Float32 preparation,mixer,binding and cleanup
  // run unchanged. This is not codec,SoundTouch,WebAudio or RSS evidence.
  const observedDecoder = "data:text/javascript," + encodeURIComponent("export async function decodePreviewResource(resource,bytes,context,host,budget){return host.observed(resource,budget)} export async function concatenatePreviewSource(value,context,host,budget){return host.join(value,budget)}");
  const preparedSource = (await readFile(sourceUrl,"utf8")).replace(/from "(\.\/[^\"]+)"/g,(match,relative) =>
    `from ${JSON.stringify(relative === "./preview-decoder.mjs" ? observedDecoder : new URL(relative,sourceUrl).href)}`);
  const renderer = await import("data:text/javascript," + encodeURIComponent(preparedSource));
  const pcm = values => { const data = new Float32Array(values); return { sampleRate:48000,length:data.length,numberOfChannels:1,getChannelData:() => data }; };
  const inputs = { a:pcm([1,-1e8]), b:pcm([1e8,1e8]) }, owners = new Map(), joins = [], releasedInputs = [], rollingCloses = [];
  const mixed = { identity:{ profile:"classic-realtime-v31-web-audio",sampleRate:48000 },frameCount:"1",
    resources:[{ assetId:"b" },{ assetId:"a" }], voices:[
      { elementId:"a2",assetId:"a",sourceMode:"prepared-concat" },
      { elementId:"a1",assetId:"a",sourceMode:"prepared-concat" },
      { elementId:"b",assetId:"b",sourceMode:"prepared-concat" },
      { elementId:"ordinary",assetId:"a",sourceMode:"preserve-pts" },
    ] };
  let currentFormats, mixedClosed = 0;
  const preparedHost = {
    engineIdentity:"fixed",resolveRangeSource:async () => bytes,
    createAudioContext:() => ({ sampleRate:48000,createBuffer:(channels,frames) => {
      assert.equal(channels,1);return pcm(Array(frames).fill(0));
    },close:async () => { mixedClosed++; } }),
    observed(resource,budget) {
      if (resource.assetId === "a") {
        assert.deepEqual(releasedInputs,["b"]);
        assert.equal(owners.get("b").segments.size,0);
        assert.equal(joins[0].buffer,null);
      }
      const free = budget.reserve(16);let done = false;
      const owner = { resource,primary:{ channels:1 },sampleRate:48000,channels:1,frameCount:"2",sourceFrameCount:"2",
        rolling:resource.assetId === "a",segments:new Map([[0,inputs[resource.assetId]]]),facts:[],windowedConcatenated:inputs[resource.assetId],
        release() { if (!done) { done = true;free();releasedInputs.push(resource.assetId); } },
        close:async () => { rollingCloses.push(resource.assetId); },
        ensureWindows:async () => ({ frameCount:"2",segments:new Map([[9,pcm([1,1])]]),facts:[{ segmentId:9,timestampSeconds:0,frameCount:"2" }] }),
      };owners.set(resource.assetId,owner);return owner;
    },
    join(value,budget) { const free = budget.reserve(16),joined = { buffer:inputs[value.resource.assetId],release:free };joins.push(joined);return joined; },
    observePreviewDecoded:async () => undefined,
    compilePitchPreparation:async () => ({ mode:"prepared-linear",numChannels:1,decodedSampleRate:48000,targetSampleRate:48000,outputFrames:"1" }),
    compileSourcePositions:async args => [{ lower:args.elementId === "a2" ? "1" : "0",fractionNumerator:"0",fractionDenominator:"1" }],
    releaseDecodedSource:async resource => {
      assert.equal(rollingCloses.length,0);
      if (resource.assetId === "a") assert.equal(currentFormats,undefined,"All preparations finish before source binding");
      return { released:true };
    },
    bindPreviewSources:async args => { currentFormats = args.sourceFormats; },
    previewSourceWindows:async () => [{ assetId:"a",startSeconds:0,endSeconds:1/48000 }],
    compilePreviewBlock:async () => ({ identity:mixed.identity,startSample:"0",frameCount:1,receipts:[],
      voices:currentFormats.map(format => ({ elementId:format.elementId,blockOffset:0,frameCount:1,
        positions:[point(format.prepared ? 0 : 9,0)],amplitudes:[1] })) }),
  };
  const producer = await renderer.prepareClassicPreview(mixed,preparedHost,{ maxPcmBytes:1024,maxEncodedBytes:1024 });
  assert.deepEqual(releasedInputs,["b","a"]);
  assert(joins.every(joined => joined.buffer === null));
  assert.deepEqual(currentFormats.map(format => format.elementId),mixed.voices.map(voice => voice.elementId));
  const block = await producer.next({ startSample:"0",frameCount:1 });
  assert.deepEqual([...block.channels[0]],[1],"Stored Float32 voice order survives resource-grouped preparation");
  assert.deepEqual(block.channels[1],block.channels[0]);
  assert.equal(rollingCloses.length,0,"Retiring concat input does not close the ordinary decoder");
  await producer.close();
  assert.equal(mixedClosed,1);
  assert.deepEqual(releasedInputs,["b","a"],"Cleanup does not release an input twice");
  assert.deepEqual(rollingCloses,["b","a"]);
});


test("prepared source position bridge keeps exact integer fractions and rejects malformed replies", async () => {
  const positions = [
    { lower: "9007199254740993", fractionNumerator: "340282366920938463463374607431768211454", fractionDenominator: "340282366920938463463374607431768211455" },
    { lower: "1", fractionNumerator: "1", fractionDenominator: "3" },
  ];
  const handlers = new Map(), { plan } = sharedFixture(1, 1);
  const realm = { begin: { plan, resources: [], admission: {} }, signal: new AbortController().signal,
    page: { exposeFunction: async (name, fn) => handlers.set(name, fn) }, broker: { request: async () => positions } };
  await configurePreviewPage(realm);
  const callback = handlers.get("__opencutPreviewCallback"), args = { elementId: "clip-0", localStartSample: "0", frameCount: 2, sourceSampleRate: 48000 };
  const envelope = await callback("compileSourcePositions", args);
  assert.equal(typeof envelope.value, "string");
  assert.deepEqual(JSON.parse(envelope.value), positions);
  const moduleUrl = "data:text/javascript," + encodeURIComponent("export async function prepareClassicPreview(plan,host){return {receipt:await host.compileSourcePositions(" + JSON.stringify(args) + ")}}");
  const source = preparePreviewPage.toString()
    .replace('import("/renderer/src/preview.mjs")', `import(${JSON.stringify(moduleUrl)})`)
    .replace('import("/renderer/src/preview-packet.mjs")', `import(${JSON.stringify(new URL("../src/preview-packet.mjs", import.meta.url).href)})`);
  const page = new Function("return (" + source + ")")();
  const prior = globalThis.__opencutPreviewCallback, priorProducer = globalThis.__opencutPreviewProducer;
  try {
    globalThis.__opencutPreviewCallback = callback;
    assert.deepEqual(await page({ plan, engineIdentity: "fixed", admission: {} }), { ok: true, value: positions });
    for (const value of [positions, undefined, "{bad-json", "[]", "{}"]) {
      globalThis.__opencutPreviewCallback = async () => ({ ok: true, value });
      assert.deepEqual(await page({ plan, engineIdentity: "fixed", admission: {} }), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
    }
  } finally {
    if (prior === undefined) delete globalThis.__opencutPreviewCallback; else globalThis.__opencutPreviewCallback = prior;
    if (priorProducer === undefined) delete globalThis.__opencutPreviewProducer; else globalThis.__opencutPreviewProducer = priorProducer;
  }
  realm.broker.request = async () => [];
  assert.deepEqual(await callback("compileSourcePositions", args), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
});

test("one bounded page result retains exact stereo Float32 bytes and captured command fences", async () => {
  const prior = globalThis.__opencutPreviewProducer;
  try {
    for (const count of [1, 4096]) {
      const command = { id: "19", generation: "3", overlayEpoch: "7", startSample: "9007199254740993", frameCount: count };
      const channels = [new Float32Array(count), new Float32Array(count)];
      channels[0][0] = -0; channels[1][0] = .25;
      if (count > 1) { channels[0][count - 1] = -.5; channels[1][count - 1] = 1; }
      const receipt = { sourceBindingSha256: "fixed", compilation: { startSample: command.startSample, frameCount: count } };
      globalThis.__opencutPreviewProducer = { next: async input => { assert.equal(input, command); return { channels, receipt }; } };
      const packets = [], abort = new AbortController(); let calls = 0;
      const realm = { signal: abort.signal, page: { evaluate: async (fn, input) => { calls++; return fn(input); } }, writer: { send: async packet => packets.push(packet) } };
      assert.deepEqual(await nextPreviewRealm(realm, command), receipt);
      assert.equal(calls, 1); assert.equal(packets.length, 2); assert.equal(realm.previewCommand, undefined);
      for (let c = 0; c < 2; c++) {
        const bytes = Buffer.from(packets[c].bytesHex, "hex");
        assert.equal(bytes.length, count * 4);
        assert.equal(bytes.readUInt32LE(0), c === 0 ? 0x80000000 : 0x3e800000);
        if (count > 1) assert.equal(bytes.readUInt32LE(bytes.length - 4), c === 0 ? 0xbf000000 : 0x3f800000);
        assert.deepEqual({ ...packets[c], bytesHex: undefined }, { type: "pcm", commandId: "19", generation: "3", overlayEpoch: "7", startSample: "9007199254740993", channel: c, frames: count, bytesHex: undefined });
      }
      for (const bad of [
        { frameCount: count + 1 }, { channelsBase64: [Buffer.from(packets[0].bytesHex, "hex").toString("base64")] },
        { channelsBase64: [Buffer.from(packets[0].bytesHex, "hex").toString("base64"), "Z".repeat(count * 8)] },
        { channelsBase64: [Buffer.from(packets[0].bytesHex, "hex").toString("base64"), Buffer.from(packets[1].bytesHex + "00", "hex").toString("base64")] }, { extra: true },
      ]) {
        realm.page.evaluate = async () => ({ ok: true, value: { receipt, frameCount: count, channelsBase64: packets.map(p => Buffer.from(p.bytesHex, "hex").toString("base64")), ...bad } });
        const before = packets.length;
        await assert.rejects(nextPreviewRealm(realm, command), { code: "INVALID_OUTPUT_PCM" });
        assert.equal(packets.length, before); assert.equal(realm.previewCommand, undefined);
      }
      realm.page.evaluate = async () => { abort.abort(); return { ok: true, value: { receipt, frameCount: count, channelsBase64: packets.map(p => Buffer.from(p.bytesHex, "hex").toString("base64")) } }; };
      await assert.rejects(nextPreviewRealm(realm, command), { code: "CANCELLED" });
      assert.equal(packets.length, 2);
      const between = new AbortController(), published = [];
      realm.signal = between.signal;
      realm.page.evaluate = async () => ({ ok: true, value: { receipt, frameCount: count, channelsBase64: packets.map(p => Buffer.from(p.bytesHex, "hex").toString("base64")) } });
      realm.writer.send = async packet => { published.push(packet); between.abort(); };
      await assert.rejects(nextPreviewRealm(realm, command), { code: "CANCELLED" });
      assert.equal(published.length, 1); assert.equal(realm.previewCommand, undefined);
      realm.signal = new AbortController().signal;
      realm.page.evaluate = async () => { realm.previewCommand = { ...command }; return { ok: true, value: { receipt, frameCount: count, channelsBase64: packets.map(p => Buffer.from(p.bytesHex, "hex").toString("base64")) } }; };
      await assert.rejects(nextPreviewRealm(realm, command), { code: "INVALID_OUTPUT_PCM" });
      assert.equal(published.length, 1);
    }
    globalThis.__opencutPreviewProducer = { next: async () => ({ channels: [new Float32Array(2), new Float32Array(1)], receipt: {} }) };
    assert.deepEqual(await nextPreviewPage({ frameCount: 1 }), { ok: false, error: { code: "INVALID_OUTPUT_PCM" } });
  } finally {
    if (prior === undefined) delete globalThis.__opencutPreviewProducer; else globalThis.__opencutPreviewProducer = prior;
  }
});


function nativePtsBuffer(channels, rate = 44100) {
  const data = channels.map(values => Float32Array.from(values));
  return { length: data[0].length, sampleRate: rate, numberOfChannels: data.length,
    getChannelData: c => data[c], copyToChannel: (values, c) => data[c].set(values) };
}
const nativePtsContext = { sampleRate: 48000, createBuffer: (channels, frames, rate) =>
  nativePtsBuffer(Array.from({ length: channels }, () => new Float32Array(frames)), rate) };
const nativePtsPrimary = { sampleRate: 44100, channels: 2 };
const rollingAdmission = { workingFrames: 20, maxChannels: 2, maxChunkFrames: 4, sourceSampleRate: 44100, targetSampleRate: 48000 };

test("rolling PTS owns exact copies, reuses forward iteration, restarts backward and releases resident allocation", async () => {
  const budget = new PcmBudget(256), starts = []; let disposed = 0;
  class Sink {
    buffers(start) {
      starts.push(start);
      return (async function* () {
        const source = nativePtsBuffer([[.25,.5,.75,1],[-.25,-.5,-.75,-1]]);
        yield { buffer: source, timestamp: 0 };
        source.getChannelData(0).fill(9);
        yield { buffer: nativePtsBuffer([[.125,.25,.5],[-.125,-.25,-.5]]), timestamp: 4/44100 };
      })();
    }
  }
  validateRollingPreviewAdmission(rollingAdmission, nativePtsPrimary, 48000, { needsPts: true, needsConcat: false });
  const source = rollingPreviewSource({}, Sink, nativePtsPrimary, nativePtsContext, budget, rollingAdmission, undefined, async()=>{disposed++;});
  assert.equal(source.sourceFrameCount, null);
  const first = await source.ensureWindows([{startSeconds:0,endSeconds:4/44100}]);
  const result = await source.ensureWindows([{startSeconds:0,endSeconds:7/44100}]);
  assert.equal(starts.length,1);
  assert.equal(result.frameCount, "7");
  assert.deepEqual(result.facts, [{ segmentId:0,timestampSeconds:0,frameCount:"4" },{ segmentId:1,timestampSeconds:4/44100,frameCount:"3" }]);
  assert.deepEqual([...first.segments.get(0).getChannelData(0)], [.25,.5,.75,1]);
  assert.equal(budget.used, (7 + 4 * 4)*2*4);
  assert(budget.peak <= 256);
  await source.ensureWindows([{startSeconds:4/44100,endSeconds:7/44100}]);
  assert.equal(budget.used,(3+4*4)*2*4);
  const backward = await source.ensureWindows([{startSeconds:0,endSeconds:4/44100}]);
  assert.equal(starts.length,2);assert.deepEqual([...backward.segments.get(0).getChannelData(0)], [.25,.5,.75,1]);
  await source.close(); await source.close(); assert.equal(budget.used, 0);assert.equal(disposed,1);
});

test("rolling PTS shares a prepared source and rejects oversized packets or cancellation without leaking budget", async () => {
  validateRollingPreviewAdmission(rollingAdmission, nativePtsPrimary, 48000, { needsPts:true,needsConcat:true });
  assert.throws(() => validateRollingPreviewAdmission(rollingAdmission, nativePtsPrimary, 48000, { needsPts:false,needsConcat:true }), { code:"INVALID_AUDIO_PLAN" });
  assert.throws(() => validateRollingPreviewAdmission({ ...rollingAdmission, maxChunkFrames:65537 }, nativePtsPrimary, 48000, { needsPts:true,needsConcat:false }));
  const budget = new PcmBudget(256), controller = new AbortController(); let copied = 0;
  const context = { ...nativePtsContext, createBuffer(...args) { copied++; return nativePtsContext.createBuffer(...args); } };
  class Large { async *buffers() { yield { buffer:nativePtsBuffer([[1,2,3,4,5],[1,2,3,4,5]]),timestamp:0 }; } }
  const large = rollingPreviewSource({}, Large, nativePtsPrimary, context, budget, rollingAdmission, undefined, async()=>{});
  await assert.rejects(large.ensureWindows([{startSeconds:0,endSeconds:1}]), { code:"AUDIO_BUDGET_EXCEEDED" });
  assert.equal(copied,0); assert.equal(budget.used,0);
  class Cancelled { async *buffers() {
    yield { buffer:nativePtsBuffer([[.5,.5],[.25,.25]]),timestamp:0 };
    controller.abort(); yield { buffer:nativePtsBuffer([[1],[1]]),timestamp:2/44100 };
  } }
  const cancelled = rollingPreviewSource({}, Cancelled, nativePtsPrimary, context, budget, rollingAdmission, controller.signal, async()=>{});
  await assert.rejects(cancelled.ensureWindows([{startSeconds:0,endSeconds:1}]), { code:"CANCELLED" });
  assert.equal(budget.used,0);
});

test('shared tap tables keep exact Float32 accumulation and borrow channels once without sharing source bounds', () => {
  const positions = [point(0, 0, 1, .25), point(0, 1, 2, .75), point(0, 2)], amplitudes = [.5, -.25, 2];
  let reads = 0; const data = [new Float32Array([.1,.2,.3]), new Float32Array([-.3,.4,-.5])];
  const source = { length: 3, numberOfChannels: 2, getChannelData(c) { reads++; return data[c]; } };
  const voices = ['a','b','c','d','e'].map(elementId => ({ elementId, blockOffset: 0, frameCount: 3, positions, amplitudes }));
  const sources = new Map(voices.map(v => [v.elementId, new Map([[0,source]])]));
  const expected = [new Float32Array(3),new Float32Array(3)];
  for (const voice of voices) for (let i=0;i<3;i++) for(let c=0;c<2;c++) {
    const p=positions[i]; expected[c][i] += (data[c][Number(p.lower)]*(1-p.fraction)+data[c][Number(p.upper)]*p.fraction)*amplitudes[i];
  }
  const actual = mixPreviewBlock({ voices },sources,3);
  assert.deepEqual(actual,expected); assert.equal(reads,2);
  sources.set('e',new Map([[0,{ ...source,length:2 }]]));
  assert.throws(()=>mixPreviewBlock({ voices },sources,3),{code:'INVALID_AUDIO_PLAN'});
  const mono={ ...source,numberOfChannels:1 }; reads=0;
  const monoResult=mixPreviewBlock({voices:[voices[0]]},new Map([['a',new Map([[0,mono]])]]),3);
  assert.deepEqual(monoResult[0],monoResult[1]);assert.equal(reads,1);
});

test('compressed preview facts preserve exact spelling and enforce the original decompressed byte bound',async()=>{
 const text='{"value":-0.0,"text":"你好","frames":[1,2,3]}';const encoded=encodePreviewFacts(text);
 assert.equal(await readPreviewFactsText(encoded),text);
 await assert.rejects(readPreviewFactsText({...encoded,decodedBytes:encoded.decodedBytes-1}));
 await assert.rejects(readPreviewFactsText({...encoded,decodedBytes:16777217}));
 await assert.rejects(readPreviewFactsText({...encoded,data:encoded.data.slice(4)}));
 await assert.rejects(readPreviewFactsText({...encoded,unexpected:true}));
});
test('independent preview windows use four decoder credits, preserve source order and drain peers before failure',async()=>{
 const gates=new Map(),calls=[];let active=0,peak=0;
 const decoded=new Map(['a','b','c','d','e'].map(id=>[id,{rolling:true,async ensureWindows(windows){calls.push(id);peak=Math.max(peak,++active);await new Promise(resolve=>gates.set(id,resolve));active--;return windows;}}]));
 const pending=ensurePreviewSourceWindows(decoded,['a','b','c','d','e'].map(assetId=>({assetId})));
 await new Promise(resolve=>setImmediate(resolve));assert.deepEqual(calls,['a','b','c','d']);gates.get('d')();gates.get('c')();gates.get('b')();gates.get('a')();
 await new Promise(resolve=>setImmediate(resolve));assert.deepEqual(calls,['a','b','c','d','e']);gates.get('e')();
 assert.deepEqual([...(await pending).keys()],['a','b','c','d','e']);assert.equal(peak,4);
 let peer,settled=false;const error=new Error('source rejected');
 const failed=ensurePreviewSourceWindows(new Map([['a',{rolling:true,ensureWindows:async()=>{throw error;}}],['b',{rolling:true,ensureWindows:()=>new Promise(resolve=>{peer=resolve;})}],['c',{rolling:true,ensureWindows:async()=>({})}],['d',{rolling:true,ensureWindows:async()=>({})}],['e',{rolling:true,ensureWindows:()=>{throw Error('must not start');}}]]),[]).catch(reason=>{settled=true;return reason;});
 await new Promise(resolve=>setImmediate(resolve));assert.equal(settled,false);peer({});assert.equal(await failed,error);
});

test('native preview allocates standard PCM buffers without opening another physical AudioContext',async()=>{
 const oldContext=globalThis.AudioContext,oldBuffer=globalThis.AudioBuffer,oldCallback=globalThis.__opencutPreviewCallback;
 globalThis.AudioContext=class{constructor(){throw Error('source must not open a playback device');}};
 globalThis.AudioBuffer=class{constructor(options){Object.assign(this,options);}};
 const moduleUrl='data:text/javascript,'+encodeURIComponent('export async function prepareClassicPreview(plan,host){const context=host.createAudioContext(48000),buffer=context.createBuffer(2,4096,44100);await context.close();let closed=false;try{context.createBuffer(1,1,48000)}catch{closed=true}return {receipt:{sampleRate:context.sampleRate,buffer,closed}}}');
 const source=preparePreviewPage.toString().replace('import("/renderer/src/preview.mjs")',`import(${JSON.stringify(moduleUrl)})`).replace('import("/renderer/src/preview-packet.mjs")',`import(${JSON.stringify(new URL('../src/preview-packet.mjs',import.meta.url).href)})`);
 try{const result=await new Function('return ('+source+')')()({plan:{},admission:{},engineIdentity:'fixed'});assert.equal(result.ok,true);assert.deepEqual(result.value,{sampleRate:48000,buffer:new globalThis.AudioBuffer({numberOfChannels:2,length:4096,sampleRate:44100}),closed:true});}
 finally{if(oldContext===undefined)delete globalThis.AudioContext;else globalThis.AudioContext=oldContext;if(oldBuffer===undefined)delete globalThis.AudioBuffer;else globalThis.AudioBuffer=oldBuffer;if(oldCallback===undefined)delete globalThis.__opencutPreviewCallback;else globalThis.__opencutPreviewCallback=oldCallback;}
});

test("verified source transport preserves admitted bytes and rejects malformed native replies", async () => {
  const handlers = new Map(), {plan} = sharedFixture(1, 1), args={transferId:"fixed",offset:"0",count:4};
  let reply={offset:"0",eof:false,bytesHex:"0080ff01"};
  await configurePreviewPage({begin:{plan,resources:[]},signal:new AbortController().signal,
    page:{exposeFunction:async(name,fn)=>handlers.set(name,fn)},broker:{request:async()=>reply}});
  const call=()=>handlers.get("__opencutPreviewCallback")("readVerifiedBytes",args);
  assert.deepEqual(await call(),{ok:true,value:{offset:"0",eof:false,bytesBase64:"AID/AQ=="}});
  for (const changed of [{offset:"1"},{bytesHex:"0080FF01"},{bytesHex:"0080ff"},{extra:true},{eof:0}]){
    reply={offset:"0",eof:false,bytesHex:"0080ff01",...changed};
    assert.deepEqual(await call(),{ok:false,error:{code:"AUDIO_PROTOCOL_ERROR"}});
  }
});


test("Node verified-byte projection preserves bytes and shares the page callback checks",async()=>{
 const {plan}=sharedFixture(1,1);let reply={offset:"0",eof:true,bytesHex:"00ff80"};
 const realm={begin:{plan,resources:[],admission:{}},signal:new AbortController().signal,page:{exposeFunction:async()=>{}},broker:{request:async()=>reply}};
 await configurePreviewPage(realm);const args={transferId:"native-transfer",offset:"0",count:3};
 const node=await realm.nodePreviewRequest("readVerifiedBytes",args);assert.equal(node.ok,true);assert(node.value.bytes instanceof Uint8Array);assert.deepEqual([...node.value.bytes],[0,255,128]);
 const page=await realm.previewRequest("readVerifiedBytes",args);assert.deepEqual(page,{ok:true,value:{offset:"0",eof:true,bytesBase64:"AP+A"}});
 reply={...reply,bytesHex:"00FF80"};
 for(const callback of [realm.nodePreviewRequest,realm.previewRequest])assert.deepEqual(await callback("readVerifiedBytes",args),{ok:false,error:{code:"AUDIO_PROTOCOL_ERROR"}});
});
