import assert from "node:assert/strict";
import test from "node:test";
import { unpackExportBlock } from "../src/preview-packet.mjs";
import { mixCompiledBlock, sampleLinear } from "../src/mixing.mjs";

function packet() {
  return {
    block: { identity: { projectId: "p", revision: "fixed", sequenceId: "s", profile: "classic-export-v31-web-audio", sampleRate: 48000, channels: 2 }, startSample: "0", frameCount: 4,
      voices: ["a", "b"].map((id, index) => ({ elementId: id, blockOffset: 0, frameCount: 4, positionRef: 0, amplitudeRef: index })),
      receipts: ["a", "b"].map(id => ({ trackId: "track", elementId: id, decision: "ready" })) },
    positionTables: [Array.from({ length: 4 }, (_, i) => [String(i), "0", "1"])],
    amplitudeTables: [[.5, .5, .5, .5], [2, 2, 2, 2]],
  };
}
test("one-block immutable point caching preserves each source bound and mutable/getter facts", () => {
  const cache = new WeakMap();
  const point = Object.freeze({ lower: "1", fractionNumerator: "1", fractionDenominator: "4" });
  assert.equal(sampleLinear(Float32Array.from([0, 1, 0]), point, cache), .75);
  assert.equal(sampleLinear(Float32Array.from([7]), point, cache), 0);
  const changing = { lower: "0", fractionNumerator: "1", fractionDenominator: "4" };
  assert.equal(sampleLinear(Float32Array.from([0, 1, 0]), changing, cache), .25);
  changing.lower = "1";
  assert.equal(sampleLinear(Float32Array.from([0, 1, 0]), changing, cache), .75);
  let lower = "0";
  const getter = Object.freeze({ get lower() { return lower; }, fractionNumerator: "1", fractionDenominator: "4" });
  assert.equal(sampleLinear(Float32Array.from([0, 1, 0]), getter, cache), .25);
  lower = "1";
  assert.equal(sampleLinear(Float32Array.from([0, 1, 0]), getter, cache), .75);
});
test("shared export tables preserve stereo PCM and the original Float32 accumulation order", () => {
  const block = unpackExportBlock(packet());
  assert.strictEqual(block.voices[0].positions, block.voices[1].positions);
  assert(Object.isFrozen(block.voices[0].positions));
  const source = values => ({ numberOfChannels: 1, getChannelData: () => Float32Array.from(values) });
  const channels = [new Float32Array(4), new Float32Array(4)];
  const output = { length: 4, numberOfChannels: 2, getChannelData: c => channels[c] };
  mixCompiledBlock(output, new Map([["a", source([.25, -.25, .5, -.5])], ["b", source([.5, -.5, .125, -.125])]]), ["a", "b"], block, 0, 4);
  for (const channel of channels) assert.deepEqual([...channel], [1.125, -1.125, .5, -.5]);
});
test("export fractions preserve full u64/u128 precision and signed-zero gain", () => {
  const p = packet(); p.block.frameCount = 1; p.block.startSample = "18446744073709551614";
  for (const voice of p.block.voices) voice.frameCount = 1;
  p.positionTables = [[["18446744073709551615", "340282366920938463463374607431768211454", "340282366920938463463374607431768211455"]]];
  p.amplitudeTables = [[-0], [Number.MIN_VALUE]];
  const block = unpackExportBlock(p);
  assert.equal(block.voices[0].positions[0].fractionDenominator, "340282366920938463463374607431768211455");
  assert(Object.is(block.voices[0].amplitudes[0], -0));
  assert.equal(block.voices[1].amplitudes[0], Number.MIN_VALUE);
});
test("sharing never legalizes excessive voice work, invalid fractions, reordered or unused tables", () => {
  for (const corrupt of [
    p => { p.block.voices[1].positionRef = 1; },
    p => { p.positionTables.push(p.positionTables[0]); },
    p => { p.block.voices.reverse(); },
    p => { p.positionTables[0][0][2] = "0"; },
    p => { p.positionTables[0][0][1] = "1"; },
    p => { p.positionTables[0][0][0] = "18446744073709551616"; },
    p => { p.amplitudeTables[0][0] = NaN; },
    p => { p.block.identity.profile = "classic-realtime-v31-web-audio"; },
  ]) { const p = packet(); corrupt(p); assert.throws(() => unpackExportBlock(p), { code: "AUDIO_PROTOCOL_ERROR" }); }
  const over = packet(); over.block.frameCount = 4096;
  over.block.voices = Array.from({ length: 32 }, (_, i) => ({ elementId: String(i), frameCount: 4096, blockOffset: 0, positionRef: 0, amplitudeRef: 0 }));
  over.block.receipts = over.block.voices.map(v => ({ elementId: v.elementId, trackId: "t", decision: "ready" }));
  assert.throws(() => unpackExportBlock(over), { code: "AUDIO_PROTOCOL_ERROR" });
});
