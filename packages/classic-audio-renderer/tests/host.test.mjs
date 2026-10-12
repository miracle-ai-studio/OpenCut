import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs/promises";
import { createHash } from "node:crypto";
import { PassThrough } from "node:stream";
import { PacketReader, CallbackBroker, validateEnvelope } from "../host/protocol.mjs";
import { validateBegin } from "../host/runner.mjs";
import { processTreeBytes, validateAdmission, QuotaMonitor } from "../host/limits.mjs";
import { BrowserRealm } from "../host/browser.mjs";
import { moduleRegistry, moduleResponse, ORIGIN } from "../host/modules.mjs";

const envelope = { protocolVersion: 1, executionId: "fixed-execution" };
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
  const begin = { ...envelope, type: "begin", plan: { identity: { projectId: "p", revision: "r", sequenceId: "s", profile: "classic-export-v31-web-audio", sampleRate: 44100 }, frameCount: "1", resources: [{ assetId: "a", kind: "audio", contentHash: "a".repeat(64), size: 1 }], voices: [{ elementId: "e", assetId: "a" }] }, resources: [], admission: { maxPcmBytes: 1024, maxEncodedBytes: 1024, maxRssBytes: "104857600", deadlineMs: 1000, quotaProfile: "monitored-terminate" }, runtime: { chromiumExecutable: "/private/host/chromium" } };
  assert.throws(() => validateBegin(begin), { code: "INVALID_HOST_PROTOCOL" });
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
  const aborted = [], admission = { maxPcmBytes: 1024, maxEncodedBytes: 1024, maxRssBytes: "104857600", deadlineMs: 15000, quotaProfile: "monitored-terminate" };
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
});
