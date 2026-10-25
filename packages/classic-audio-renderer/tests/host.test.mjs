import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs/promises";
import { createHash } from "node:crypto";
import { PassThrough } from "node:stream";
import { deflateSync } from "node:zlib";
import { PacketReader, CallbackBroker, validateEnvelope, compiledFactsJson, PROTOCOL_LIMITS } from "../host/protocol.mjs";
import { validateBegin } from "../host/runner.mjs";
import { processTreeBytes, validateAdmission, QuotaMonitor } from "../host/limits.mjs";
import { BrowserRealm } from "../host/browser.mjs";
import { moduleRegistry, moduleResponse, ORIGIN } from "../host/modules.mjs";

const envelope = { executionId: "fixed-execution" };
test("compiled JSON text crosses the bridge once with exact values and the same budgets", async () => {
  const broker = new CallbackBroker({ send: () => {} });
  const text = '{"amplitudes":[-0.0,5e-324],"lower":"18446744073709551615","denominator":"340282366920938463463374607431768211455","text":"雪"}';
  const raw = Buffer.from(text), zipped = deflateSync(raw);
  const pending = broker.request("compileBlock", {}, { jsonText: true }); await Promise.resolve();
  broker.receive({ type: "reply_chunk", id: "1", sequence: 0, encoding: "zlib", decodedBytes: raw.length, bytesHex: zipped.toString("hex"), final: true });
  const wire = compiledFactsJson(await pending);
  assert.equal(wire, text);
  const facts = JSON.parse(wire);
  assert(Object.is(facts.amplitudes[0], -0)); assert.equal(facts.amplitudes[1], Number.MIN_VALUE);
  assert.equal(facts.denominator, "340282366920938463463374607431768211455");
  assert.equal(broker.chunkBytes, 0);
  assert.throws(() => compiledFactsJson("x".repeat(PROTOCOL_LIMITS.replyBytes + 1)), { code: "AUDIO_BUDGET_EXCEEDED" });
  const small = broker.request("compileBlock", {}, { jsonText: true }); await Promise.resolve();
  broker.receive({ type: "reply", id: "2", ok: true, value: { amplitudes: [-0] } });
  assert(Object.is(JSON.parse(await small).amplitudes[0], -0));
});
test("compressed callback facts retain exact values and enforce decoded/shared budgets", async () => {
  const sent = [], broker = new CallbackBroker({ send: packet => sent.push(packet) });
  const value = { text: 'quote"\n雪', positions: [{ lower: "18446744073709551615", fractionNumerator: "1", fractionDenominator: "18446744073709551616" }], gain: [-0, Number.MIN_VALUE, .1] };
  const raw = Buffer.from(compiledFactsJson(value)), zipped = deflateSync(raw);
  const result = broker.request("compileBlock", {}); await Promise.resolve();
  const split = Math.floor(zipped.length / 2);
  for (const [sequence, bytes] of [zipped.subarray(0, split), zipped.subarray(split)].entries()) broker.receive({ type: "reply_chunk", id: sent[0].id, sequence, encoding: "zlib", decodedBytes: raw.length, bytesHex: bytes.toString("hex"), final: sequence === 1 });
  assert.deepEqual(await result, value); assert.equal(broker.chunkBytes, 0);
  for (const packet of [
    { encoding: "unknown", decodedBytes: raw.length },
    { encoding: "zlib", decodedBytes: PROTOCOL_LIMITS.replyBytes + 1 },
    { encoding: "zlib", decodedBytes: raw.length - 1 },
    { encoding: "zlib", decodedBytes: raw.length + 1 },
    { encoding: "zlib", decodedBytes: raw.length, bytesHex: Buffer.concat([zipped.subarray(0, -1), Buffer.from([zipped.at(-1) ^ 255])]).toString("hex") },
  ]) {
    const invalid = new CallbackBroker({ send: () => {} });
    const pending = invalid.request("compileBlock", {}); await Promise.resolve();
    assert.throws(() => invalid.receive({ type: "reply_chunk", id: "1", sequence: 0, bytesHex: zipped.toString("hex"), final: true, ...packet }));
    invalid.close(); await assert.rejects(pending); assert.equal(invalid.chunkBytes, 0);
  }
});
test("compressed callback reservations retain the shared ceiling and release on cancellation", async () => {
  const broker = new CallbackBroker({ send: () => {} });
  const first = broker.request("compileBlock", {}), second = broker.request("compileBlock", {});
  await Promise.resolve();
  broker.receive({ type: "reply_chunk", id: "1", sequence: 0, encoding: "zlib", decodedBytes: PROTOCOL_LIMITS.replyBytes, bytesHex: "00", final: false });
  assert.throws(() => broker.receive({ type: "reply_chunk", id: "2", sequence: 0, encoding: "zlib", decodedBytes: PROTOCOL_LIMITS.replyBytes, bytesHex: "00", final: false }), { code: "HOST_REPLY_TOO_LARGE" });
  broker.close(); await assert.rejects(first); await assert.rejects(second);
  assert.equal(broker.chunkBytes, 0); assert.equal(broker.pending.size, 0);
});
test("export bulk callbacks cross the browser bridge as bounded JSON with exact sample facts", async () => {
  const callbacks = new Map();
  const positions = Array.from({ length: 4096 }, () => ({ lower: "18446744073709551615", fractionNumerator: "1", fractionDenominator: "18446744073709551616" }));
  const facts = { startSample: "0", frameCount: 4096, voices: [{ elementId: "voice", positions, amplitudes: [-0, Number.MIN_VALUE, .1] }] };
  const metadata = { offset: "0", bytesHex: "00", eof: true };
  const realm = new BrowserRealm({ resources: [], plan: { resources: [], voices: [] } },
    { request: async op => op === "compileBlock" ? facts : metadata }, {}, {}, new AbortController().signal);
  realm.page = { exposeFunction: async (name, callback) => callbacks.set(name, callback) };
  await realm.configureAudioPage();
  const callback = callbacks.get("__opencutAudioCallback");
  const wire = await callback("compileBlock", { startSample: "0", frameCount: 4096 });
  assert.equal(typeof wire, "string");
  const received = JSON.parse(wire);
  assert.deepEqual(received, facts);
  assert(Object.is(received.voices[0].amplitudes[0], -0));
  assert.strictEqual(await callback("readVerifiedBytes", { transferId: "fixed", offset: "0", count: 1 }), metadata);
  await assert.rejects(callback("compileBlock", { startSample: "0", frameCount: 4097 }), { code: "INVALID_AUDIO_PLAN" });
  assert.throws(() => compiledFactsJson({ value: "x".repeat(PROTOCOL_LIMITS.replyBytes) }), { code: "AUDIO_BUDGET_EXCEEDED" });
});
test("stdio handles split UTF-8 and rejects unbounded packets or cross-execution replies", () => {
  const input = new PassThrough(), reader = new PacketReader(input), packets = [], errors = [];
  reader.on("packet", packet => packets.push(packet)); reader.on("failure", error => errors.push(error));
  const bytes = Buffer.from(JSON.stringify({ ...envelope, type: "reply", id: "1", ok: true, value: "音频" }) + "\n");
  for (const byte of bytes) input.write(new Uint8Array([byte]));
  assert.equal(packets[0].value, "音频");
  input.write(Buffer.alloc(1048577, 65));
  assert.equal(errors[0].code, "HOST_PACKET_TOO_LARGE");
  assert.throws(() => validateEnvelope({ ...envelope, executionId: "other" }, envelope.executionId), { code: "INVALID_HOST_PROTOCOL" });
  input.destroy();
});

test("correlated ordered UTF-8 fact chunks resolve once; duplicate and reordered replies reject", async () => {
  const sent = [], broker = new CallbackBroker({ send: packet => { sent.push(packet); } });
  const result = broker.request("compileBlock", { startSample: "0", frameCount: 1 });
  await Promise.resolve();
  const bytes = Buffer.from(JSON.stringify({ stable: "音频", frames: 1 }));
  broker.receive({ type: "reply_chunk", id: sent[0].id, sequence: 0, bytesHex: bytes.subarray(0, 9).toString("hex"), final: false });
  broker.receive({ type: "reply_chunk", id: sent[0].id, sequence: 1, bytesHex: bytes.subarray(9).toString("hex"), final: true });
  assert.deepEqual(await result, { stable: "音频", frames: 1 });
  assert.throws(() => broker.receive({ type: "reply", id: sent[0].id, ok: true, value: {} }), { code: "INVALID_HOST_PROTOCOL" });
  const next = broker.request("compileBlock", {}); await Promise.resolve();
  assert.throws(() => broker.receive({ type: "reply_chunk", id: sent[1].id, sequence: 2, bytesHex: "7b7d", final: true }), { code: "INVALID_HOST_PROTOCOL" });
  broker.close(); await assert.rejects(next, { code: "CANCELLED" });
});

test("RSS counts runner and dedicated browser descendants including reparented children, excluding other jobs", () => {
  const rows = "100 1 100 10\n200 100 200 20\n300 200 200 30\n400 1 200 40\n500 1 500 999\n";
  assert.equal(processTreeBytes(rows, 100, 200), 100n * 1024n);
  assert.throws(() => validateAdmission({ quotaProfile: "automatic", maxPcmBytes: 1024 }), { code: "INVALID_AUDIO_ADMISSION" });
  const begin = { ...envelope, type: "begin", plan: { identity: { projectId: "p", revision: "r", sequenceId: "s", profile: "classic-export-v31-web-audio", sampleRate: 44100 }, frameCount: "1", encodedSource: { mode: "verified-range", cacheBytes: 65536 }, resources: [{ assetId: "a", kind: "audio", contentHash: "a".repeat(64), size: 1 }], voices: [{ elementId: "e", assetId: "a" }] }, resources: [], admission: { maxPcmBytes: 1024, maxEncodedBytes: 1048576, maxRssBytes: "104857600", deadlineMs: 1000, quotaProfile: "monitored-terminate" }, runtime: { chromiumExecutable: "/private/host/chromium" } };
  assert.throws(() => validateBegin(begin), { code: "INVALID_HOST_PROTOCOL" });
  const valid = { ...begin, resources: [{ assetId: "a", handle: "fixed" }] };
  assert.equal(validateBegin(valid).type, "begin");
  assert.throws(() => validateBegin({ ...valid, type: "unknown" }), { code: "INVALID_HOST_PROTOCOL" });
});

test("PCM window callback preserves the exact fixed resource authority", async () => {
  const resource = { assetId: "pcm", kind: "audio", contentHash: "a".repeat(64), size: 28800044 };
  const primary = { trackId: 1, codec: "pcm-s16", sampleRate: 48000, channels: 1 };
  const callbacks = new Map(), calls = [];
  const realm = new BrowserRealm({ type: "begin", resources: [{ assetId: "pcm", handle: "owned" }], plan: { resources: [resource], voices: [] } },
    { request: async (op, args) => { calls.push({op,args}); return { mode: "whole" }; } }, {}, {}, new AbortController().signal);
  realm.page = { exposeFunction: async (name, callback) => callbacks.set(name, callback) };
  await realm.configureAudioPage();
  const callback = callbacks.get("__opencutAudioCallback");
  await assert.rejects(callback("admitPcmAudioDecode", {resource: {...resource, size: 1}, primary}), {code: "NOT_AUTHORIZED"});
  assert.equal(calls.length, 0);
  assert.deepEqual(await callback("admitPcmAudioDecode", {resource, primary}), {mode: "whole"});
  assert.deepEqual(calls, [{op: "admitPcmAudioDecode", args: {resource, primary}}]);
});

test("fixed registry loads exact browser dependency and denies network, host code and source paths", async () => {
  const modules = await moduleRegistry({});
  assert.match(modules.moduleSha256, /^[a-f0-9]{64}$/);
  assert.ok(moduleResponse(modules, ORIGIN + "/renderer/src/index.mjs", "GET"));
  const media = moduleResponse(modules, ORIGIN + "/dependencies/mediabunny/dist/bundles/mediabunny.mjs", "GET");
  assert.ok(media); assert.match(media.body.toString(), /\(disabled\):src\/node/);
  for (const address of ["https://example.org/audio", "file:///private/media.wav", ORIGIN + "/renderer/host/runner.mjs", ORIGIN + "/renderer/src/../../host/runner.mjs", ORIGIN + "/renderer/src/index.mjs?external=1"]) assert.equal(moduleResponse(modules, address, "GET"), undefined);
  assert.equal(moduleResponse(modules, ORIGIN + "/renderer/src/index.mjs", "POST"), undefined);
  const identityEntries = [...modules.registry];
  for (const filename of (await fs.readdir(new URL("../host/", import.meta.url))).filter(name => name.endsWith(".mjs"))) {
    const name = `/renderer/host/${filename}`;
    identityEntries.push([name, await fs.readFile(new URL(`../host/${filename}`, import.meta.url))]);
    assert.equal(modules.registry.has(name), false);
    assert.equal(moduleResponse(modules, ORIGIN + name, "GET"), undefined);
  }
  const digest = entries => {
    const hash = createHash("sha256");
    for (const [name, bytes] of entries.sort(([a], [b]) => a.localeCompare(b))) hash.update(name).update("\0").update(bytes).update("\0");
    return hash.digest("hex");
  };
  assert.equal(modules.moduleSha256, digest(identityEntries), "The execution identity includes actual host control/page source bytes");
  assert.notEqual(modules.moduleSha256, digest([...modules.registry]), "Served-only modules omit execution-affecting host code");
});

test("late browser operations share the original monotonic deadline and cannot restart its budget", async () => {
  let now = 500;
  const aborted = [], admission = { maxPcmBytes: 1024, maxEncodedBytes: 1048576, maxRssBytes: "104857600", deadlineMs: 15000, quotaProfile: "monitored-terminate" };
  const monitor = new QuotaMonitor(admission, error => aborted.push(error), () => now);
  assert.equal(monitor.remainingTimeout(), 10000);
  now = 8500; assert.equal(monitor.remainingTimeout(), 7000);
  now = 14500; assert.equal(monitor.remainingTimeout(), 1000);
  now = 15499.5;
  assert.throws(() => monitor.remainingTimeout(), { code: "AUDIO_DEADLINE_EXCEEDED" });
  assert.equal(aborted.length, 1);
  assert.equal(monitor.receipt().elapsedMs, 15000);
  // An expired start rejects before touching runtime paths or launching a child.
  const realm = new BrowserRealm({ runtime: {} }, null, null, monitor, new AbortController().signal);
  await assert.rejects(realm.start(), { code: "AUDIO_DEADLINE_EXCEEDED" });
  let evaluations = 0; realm.page = { evaluate() { evaluations++; } };
  await assert.rejects(realm.render(), { code: "AUDIO_DEADLINE_EXCEEDED" });
  assert.equal(evaluations, 0);
  assert.equal(aborted.length, 1);
  now = 0;
  const ownerControlled = new QuotaMonitor(admission, error => aborted.push(error), () => now, undefined, true);
  now = 16000;
  assert.equal(ownerControlled.remainingTimeout(15000), 15000);
  assert.equal(ownerControlled.receipt().elapsedMs, 16000);
  ownerControlled.stop();
  assert.throws(() => ownerControlled.remainingTimeout(), { code: "CANCELLED" });

});
