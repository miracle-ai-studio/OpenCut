import test from "node:test";
import assert from "node:assert/strict";
import { windowedPcmSource, copyPcmChannel } from "../src/pcm-source.mjs";
import { sampleLinear } from "../src/mixing.mjs";

test("sparse decoded PCM keeps absolute interpolation, adjacent blocks, EOF and rejects missing in-bounds taps", () => {
  const buffer = values => ({ sampleRate: 48000, numberOfChannels: 1, length: values.length,
    getChannelData: () => Float32Array.from(values) });
  const source = windowedPcmSource(48000, 100, 1, [
    { start: 10, buffer: buffer([.25, .5]) }, { start: 12, buffer: buffer([.75, 1]) },
    { start: 99, buffer: buffer([-.5]) },
  ]);
  const at = (lower, numerator = 0) => ({ lower: String(lower), fractionNumerator: String(numerator), fractionDenominator: "2" });
  const data = source.getChannelData(0);
  assert.equal(sampleLinear(data, at(11, 1)), .625);
  assert.equal(sampleLinear(data, at(99, 1)), -.5);
  assert.equal(sampleLinear(data, at("9007199254740993")), 0);
  assert.throws(() => sampleLinear(data, at(13, 1)), { code: "INVALID_AUDIO_PLAN" });
  const copied = new Float32Array(4);
  copyPcmChannel(source, 0, 10, 4, { copyToChannel: (bytes, channel, offset = 0) => { assert.equal(channel, 0); copied.set(bytes, offset); } });
  assert.deepEqual(copied, Float32Array.from([.25, .5, .75, 1]));
  assert.throws(() => copyPcmChannel(source, 0, 12, 4, { copyToChannel() {} }), { code: "INVALID_AUDIO_PLAN" });
  assert.equal(source.residentBytes, 20);
  assert.equal(source.length, 100);
});
