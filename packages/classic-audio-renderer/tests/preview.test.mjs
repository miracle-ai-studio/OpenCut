import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { previewBindingDigest } from "../src/preview.mjs";
import { mixPreviewBlock } from "../src/preview-mixing.mjs";
import { validatePreviewBegin, validatePreviewNext } from "../host/preview-runner.mjs";
import { packPreviewBinding, expandPreviewBinding, compactPreviewReceipt } from "../host/preview-binding.mjs";
import { configurePreviewPage, preparePreviewRealm, nextPreviewRealm, previewEnvelope, unwrapPreviewEnvelope } from "../host/preview-browser.mjs";
import { PROTOCOL_LIMITS, PacketWriter } from "../host/protocol.mjs";
import { Writable } from "node:stream";
import { preparePreviewPage } from "../host/preview-page.mjs";
import { readFile } from "node:fs/promises";
import { unpackPreviewBlockV1, unpackPreviewBlockV2 } from "../src/preview-packet.mjs";

const buffer = channels => ({ length: channels[0].length, numberOfChannels: channels.length, getChannelData: c => new Float32Array(channels[c]) });
const point = (segmentId, lower, upper = lower, fraction = 0) => ({ segmentId, lower: String(lower), upper: String(upper), fraction });
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
  const begin = { protocolVersion: 1, executionId: "fixed", type: "begin", preparationDeadlineMs: 120000,
    plan: { identity: { projectId: "p", revision: "r", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000 }, frameCount: "24017", resources: [{ assetId: "a", kind: "audio", contentHash: "a".repeat(64), size: 1 }], voices: [{ elementId: "e", assetId: "a", sourceMode: "preserve-pts" }] },
    resources: [{ assetId: "a", handle: "private" }], admission: { maxPcmBytes: 1024, maxEncodedBytes: 1024, maxRssBytes: "104857600", deadlineMs: 3600000, quotaProfile: "monitored-terminate" } };
  assert.equal(validatePreviewBegin(begin).plan.identity.profile, "classic-realtime-v31-web-audio");
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
  await writer.send({ type: "request", id: "1", op: "bindPreviewSourcesSharedV1", args: binding });
  await writer.send({ type: "prepared", receipt: compact });
  assert(wireBytes < PROTOCOL_LIMITS.packetBytes);
});

test("shared transport rejects wrong references, coverage order, duplicates, orphan sources, modes and unsupported versions", () => {
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
  const binding = { schemaVersion: 1, ordinarySources: [{ assetId: "fixed-source", segments: formats[0].segments }], sourceFormats: formats.map(({ segments, ...f }) => ({ ...f, sourceRef: "fixed-source" })) };
  assert(Buffer.byteLength(JSON.stringify(binding)) < PROTOCOL_LIMITS.packetBytes);
  assert.throws(() => packPreviewBinding(plan, formats), { code: "AUDIO_BUDGET_EXCEEDED" });
  assert.throws(() => expandPreviewBinding(plan, binding), { code: "AUDIO_BUDGET_EXCEEDED" });
});

test("Node preview bridge preserves exact safe failure codes and only captures accepted shared facts", async () => {
  const handlers = new Map(), { plan, formats } = sharedFixture(2, 2), requests = [];
  const fullReceipt = { sourceFormats: formats, sourceBindingSha256: "unchanged", decoders: [], preparation: [] };
  const realm = { begin: { plan, resources: [], admission: {} }, identity: { engine: "pinned" }, signal: new AbortController().signal,
    page: { exposeFunction: async (name, fn) => handlers.set(name, fn), evaluate: async fn => ({ ok: true, value: fn.name === "preparePreviewPage" ? fullReceipt : { completed: true } }) },
    broker: { request: async (op, args) => { requests.push({ op, args }); return { bound: true }; } } };
  await configurePreviewPage(realm);
  const callback = handlers.get("__opencutPreviewCallback");
  assert.deepEqual(await callback("bindPreviewSources", { sourceFormats: formats }), { ok: true, value: { bound: true } });
  assert.equal(requests[0].op, "bindPreviewSourcesSharedV1");
  assert.equal(requests[0].args.schemaVersion, 1);
  assert(!Object.hasOwn(await preparePreviewRealm(realm), "sourceFormats"));
  assert.deepEqual(await nextPreviewRealm(realm, {}), { completed: true });
  realm.previewBinding = undefined;
  realm.broker.request = async () => { throw Object.assign(new Error("private host path"), { code: "AUDIO_BUDGET_EXCEEDED" }); };
  const failure = await callback("bindPreviewSources", { sourceFormats: formats });
  assert.deepEqual(failure, { ok: false, error: { code: "AUDIO_BUDGET_EXCEEDED" } });
  assert.equal(realm.previewBinding, undefined);
  assert.throws(() => unwrapPreviewEnvelope(failure), { code: "AUDIO_BUDGET_EXCEEDED", message: "Isolated preview operation failed" });
  realm.page.evaluate = async () => failure;
  await assert.rejects(nextPreviewRealm(realm, {}), { code: "AUDIO_BUDGET_EXCEEDED" });
  assert.deepEqual(await previewEnvelope(async () => { throw new Error("HOST_PACKET_TOO_LARGE private path"); }), { ok: false, error: { code: "AUDIO_PREVIEW_FAILED" } });
  assert.deepEqual(await previewEnvelope(async () => { throw { code: "invalid/path" }; }), { ok: false, error: { code: "AUDIO_PREVIEW_FAILED" } });
  assert.deepEqual(await previewEnvelope(async () => undefined), { ok: true, value: undefined });
  assert.throws(() => unwrapPreviewEnvelope({ ok: false, error: { code: "CANCELLED", path: "hidden" } }), { code: "AUDIO_PROTOCOL_ERROR" });
});

test("only compiled preview facts cross the page bridge as bounded JSON text without changing sample facts", async () => {
  const handlers = new Map(), { plan } = sharedFixture(1, 1);
  const block = { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000, channels: 2 }, startSample: "9007199254740993", frameCount: 2,
    voices: [{ elementId: "clip-0", blockOffset: 0, frameCount: 2,
      positions: [point(17, "9007199254740993", "9007199254740994", .125), null], amplitudes: [.001, 10] }], receipts: [{ trackId: "track", elementId: "clip-0", decision: "ready" }] };
  const packed = { schemaVersion: 2, block: { ...block, voices: [{ elementId: "clip-0", blockOffset: 0, frameCount: 2, positionRef: 0, amplitudeRef: 0 }] },
    positionTables: [[[17, "9007199254740993", "9007199254740994", .125], null]], amplitudeTables: [[.001, 10]] };
  const packedText = JSON.stringify(packed);
  const operations = [];
  const realm = { begin: { plan, resources: [], admission: {} }, signal: new AbortController().signal,
    page: { exposeFunction: async (name, fn) => handlers.set(name, fn) }, broker: { request: async op => { operations.push(op); return op === "compilePreviewBlockPackedV2" ? packedText : block; } } };
  await configurePreviewPage(realm);
  const callback = handlers.get("__opencutPreviewCallback"), args = { startSample: block.startSample, frameCount: 2 };
  const envelope = await callback("compilePreviewBlock", args);
  assert.equal(envelope.ok, true);
  assert.equal(typeof envelope.value, "string");
  assert.deepEqual(JSON.parse(envelope.value), packed);
  assert.equal(operations[0], "compilePreviewBlockPackedV2");
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
  realm.broker.request = async () => undefined;
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
  realm.broker.request = async () => packed;
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "AUDIO_PROTOCOL_ERROR" } });
  const rawText = packedText.replace(",0.125]", ",-0.0]").replace('"schemaVersion":2', '"schemaVersion": 2');
  realm.broker.request = async () => rawText;
  assert.equal((await callback("compilePreviewBlock", args)).value, rawText, "Node does not normalize native JSON spelling or signed zero");
  assert(Object.is(unpackPreviewBlockV2(JSON.parse(rawText)).voices[0].positions[0].fraction, -0));
  realm.broker.request = async () => { throw Object.assign(new Error("private path"), { code: "NOT_AUTHORIZED" }); };
  assert.deepEqual(await callback("compilePreviewBlock", args), { ok: false, error: { code: "NOT_AUTHORIZED" } });
});

function tablePacket() {
  return { schemaVersion: 2,
    block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000, channels: 2 },
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
test("V2 shares only readonly sample tables and mixed independent facts still reach the expected PCM", () => {
  const packet = tablePacket(), block = unpackPreviewBlockV2(packet);
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
  assert.deepEqual(unpackPreviewBlockV2({ schemaVersion: 2, block: { ...packet.block, voices: [], receipts: [] }, positionTables: [], amplitudeTables: [] }).voices, []);
});

test("V2 rejects orphan or out-of-order tables, bad references and facts without discounting shared execution work", () => {
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
  for (const change of changes) { const packet = tablePacket(); change(packet); assert.throws(() => unpackPreviewBlockV2(packet), { code: "AUDIO_PROTOCOL_ERROR" }); }
  const dense = tablePacket(); dense.block.frameCount = 4096;
  dense.positionTables = [Array(4096).fill(null)]; dense.amplitudeTables = [Array(4096).fill(1)];
  dense.block.voices = Array.from({ length: 17 }, (_, i) => ({ elementId: `e${i}`, blockOffset: 0, frameCount: 4096, positionRef: 0, amplitudeRef: 0 }));
  dense.block.receipts = dense.block.voices.map(v => ({ trackId: "track", elementId: v.elementId, decision: "ready" }));
  assert.throws(() => unpackPreviewBlockV2(dense), { code: "AUDIO_PROTOCOL_ERROR" });
  dense.block.voices.pop(); dense.block.receipts.pop();
  assert.equal(unpackPreviewBlockV2(dense).voices.length, 16, "65536 actual voice samples remain admitted despite transport sharing");
  const exact = tablePacket(); exact.positionTables[0][0] = [0xffffffff, "18446744073709551615", "18446744073709551615", -0];
  assert.deepEqual(unpackPreviewBlockV2(exact).voices[0].positions[0], point(0xffffffff, "18446744073709551615", "18446744073709551615", -0));
});

test("packed preview projection preserves every full field and rejects malformed or over-budget sample facts", () => {
  const packet = { schemaVersion: 1, block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 44100, channels: 2 },
    startSample: "18446744073709547519", frameCount: 4,
    receipts: [{ trackId: "track-a", elementId: "voice", decision: "requires_probe" }, { trackId: "track-b", elementId: "excluded", decision: "outside_block" }],
    voices: [{ elementId: "voice", blockOffset: 1, frameCount: 3, positions: [[0xffffffff, "18446744073709551615", "18446744073709551615", -0], null, [2, "9007199254740993", "9007199254740994", .125]], amplitudes: [-1, .001, 10] }] } };
  const expected = { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 44100, channels: 2 },
    startSample: "18446744073709547519", frameCount: 4,
    receipts: [{ trackId: "track-a", elementId: "voice", decision: "requires_probe" }, { trackId: "track-b", elementId: "excluded", decision: "outside_block" }],
    voices: [{ elementId: "voice", blockOffset: 1, frameCount: 3, positions: [point(0xffffffff, "18446744073709551615", "18446744073709551615", -0), null, point(2, "9007199254740993", "9007199254740994", .125)], amplitudes: [-1, .001, 10] }] };
  assert.deepEqual(unpackPreviewBlockV1(packet), expected);
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
  for (const change of bad) { const malformed = structuredClone(packet); change(malformed); assert.throws(() => unpackPreviewBlockV1(malformed), { code: "AUDIO_PROTOCOL_ERROR" }); }
  const dense = structuredClone(packet); dense.block.frameCount = 4096;
  dense.block.voices = Array.from({ length: 17 }, (_, i) => ({ elementId: `e${i}`, blockOffset: 0, frameCount: 4096, positions: Array(4096).fill(null), amplitudes: Array(4096).fill(1) }));
  dense.block.receipts = dense.block.voices.map(v => ({ trackId: "track", elementId: v.elementId, decision: "ready" }));
  assert.throws(() => unpackPreviewBlockV1(dense), { code: "AUDIO_PROTOCOL_ERROR" });
  dense.block.voices.forEach(v => { v.frameCount = 1; v.positions = [null]; v.amplitudes = [1]; });
  dense.block.voices = Array.from({ length: 33 }, (_, i) => ({ ...dense.block.voices[0], elementId: `e${i}` }));
  dense.block.receipts = dense.block.voices.map(v => ({ trackId: "track", elementId: v.elementId, decision: "ready" }));
  assert.throws(() => unpackPreviewBlockV1(dense), { code: "AUDIO_PROTOCOL_ERROR" });
});

test("packed identifiers match native UTF8 and whitespace bounds, active order follows receipts, and sample end cannot overflow", () => {
  const packet = { schemaVersion: 1, block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-realtime-v31-web-audio", sampleRate: 48000, channels: 2 },
    startSample: "18446744073709551613", frameCount: 2,
    receipts: [{ trackId: "track", elementId: "a", decision: "ready" }, { trackId: "track", elementId: "excluded", decision: "non_audio" }, { trackId: "track", elementId: "b", decision: "ready" }],
    voices: ["a", "b"].map(elementId => ({ elementId, blockOffset: 0, frameCount: 1, positions: [null], amplitudes: [1] })) } };
  assert.equal(unpackPreviewBlockV1(packet).startSample, "18446744073709551613");
  for (const value of ["", " \u3000", "\u0085", "x\u009fy", "é".repeat(513), "😀".repeat(257)]) {
    const bad = structuredClone(packet); bad.block.identity.projectId = value;
    assert.throws(() => unpackPreviewBlockV1(bad), { code: "AUDIO_PROTOCOL_ERROR" });
  }
  // Rust accepts nonempty FEFF and preserves surrounding authored whitespace.
  for (const value of ["é".repeat(512), "😀".repeat(256), "\ufeff", " a "]) {
    const valid = structuredClone(packet); valid.block.identity.projectId = value;
    assert.equal(unpackPreviewBlockV1(valid).identity.projectId, value);
  }
  for (const change of [
    p => { p.block.startSample = "18446744073709551614"; },
    p => { p.block.voices.reverse(); },
    p => { p.block.receipts[0].trackId = "\u3000"; },
    p => { p.block.receipts[0].elementId = "é".repeat(513); },
  ]) {
    const bad = structuredClone(packet); change(bad);
    assert.throws(() => unpackPreviewBlockV1(bad), { code: "AUDIO_PROTOCOL_ERROR" });
  }
  const manyReceipts = structuredClone(packet);
  manyReceipts.block.receipts.unshift(...Array.from({ length: 300 }, (_, i) => ({ trackId: "track", elementId: `non-audio-${i}`, decision: "non_audio" })));
  assert.equal(unpackPreviewBlockV1(manyReceipts).receipts.length, 303, "Non-audio receipts do not consume the 256 participating voice bound");
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
  const host = { engineIdentity: "fixed", resolveVerifiedBytes: async () => bytes,
    createAudioContext: () => ({ close: async () => { closed++; } }),
    observedForTest: { primary: { channels: 1 }, sampleRate: 48000, frameCount: formats[0].frameCount, channels: 1,
      segments: new Map(), facts: formats[0].segments, release: () => { released++; } },
    observePreviewDecoded: async () => undefined,
    bindPreviewSources: async () => { bound++; throw new Error("budget must fail before binding"); },
  };
  await assert.rejects(prepareClassicPreview(plan, host, { maxPcmBytes: 4, maxEncodedBytes: 1024 }), { code: "AUDIO_BUDGET_EXCEEDED" });
  assert.equal(bound, 0);
  assert.equal(released, 1);
  assert.equal(closed, 1);
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
