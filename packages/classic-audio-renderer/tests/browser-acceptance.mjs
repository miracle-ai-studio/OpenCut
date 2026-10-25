import { renderClassicExport } from "../src/index.mjs";

// Central runner supplies actual realm termination and pinned browser identity.
// These tiny in-memory WAVs and hand-authored sample plans are independent
// waveform oracles. They do not replace Rust-to-host bridge integration tests.
function require(condition, message) { if (!condition) throw new Error(message); }
function wav(frames, valueAt, encoding = "pcm16") {
  const float = encoding === "ieee-float32", bytesPerSample = float ? 4 : 2;
  const bytes = new Uint8Array(44 + frames * bytesPerSample), view = new DataView(bytes.buffer);
  const text = (at, value) => { for (let i = 0; i < value.length; i++) bytes[at + i] = value.charCodeAt(i); };
  text(0, "RIFF"); view.setUint32(4, bytes.length - 8, true); text(8, "WAVE"); text(12, "fmt ");
  view.setUint32(16, 16, true); view.setUint16(20, float ? 3 : 1, true); view.setUint16(22, 1, true);
  view.setUint32(24, 44100, true); view.setUint32(28, 44100 * bytesPerSample, true); view.setUint16(32, bytesPerSample, true); view.setUint16(34, bytesPerSample * 8, true);
  text(36, "data"); view.setUint32(40, frames * bytesPerSample, true);
  for (let i = 0; i < frames; i++) {
    if (float) view.setFloat32(44 + i * 4, valueAt(i), true);
    else view.setInt16(44 + i * 2, Math.round(valueAt(i) * 32768), true);
  }
  return bytes;
}
const position = lower => ({ lower: String(lower), fractionNumerator: "0", fractionDenominator: "1" });
async function fixture(bytes, mode, gainAt, options, range) {
  const hash = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), v => v.toString(16).padStart(2, "0")).join("");
  const identity = { projectId: "independent-wav", sequenceId: "scene", revision: "fixture-v1", profile: "classic-export-v31-web-audio", sampleRate: 44100 };
  const plan = { identity, frameCount: "44100", resources: [{ assetId: "wav", kind: "audio", contentHash: hash, size: bytes.length }], voices: [{ elementId: "voice", assetId: "wav" }], ...(range ? { range } : {}) };
  const host = {
    engineIdentity: options.engineIdentity,
    abortRendering: options.abortRendering,
    resolveVerifiedBytes: async () => bytes,
    admitDecode: async () => ({ maxFrames: 100000, maxChannels: 1 }),
    releaseDecodedSource: async () => ({ released: true }),
    admitPcmAudioDecode: async () => ({ mode: "whole" }),
    createAudioContext: sampleRate => new AudioContext({ sampleRate }),
    createOfflineContext: (channels, frames, sampleRate) => new OfflineAudioContext(channels, frames, sampleRate),
    compilePitchPreparation: async input => {
      require(input.decodedSampleRate === 44100 && input.decodedFrameCount === "88200", "Actual WAV decoder format changed");
      const preparedFrames = mode === "unprepared-linear" ? 0 : mode === "prepared-linear" ? 44100 : 220500;
      return { mode, decodedSampleRate: 44100, targetSampleRate: 44100, numChannels: 1, tempo: { numerator: "2", denominator: "1" }, sourceSliceStartSample: "0", requestedSourceFrames: "88200", actualSourceFrames: "88200", resampledFrames: "88200", outputFrames: "44100", prefixFrames: "88200", totalChannelFrames: String(88200 + preparedFrames), decodedBytes: "352800", preparedBytes: String(preparedFrames * 4), totalBytes: String((88200 + preparedFrames) * 4) };
    },
    compileSourcePositions: async ({ localStartSample, frameCount }) => Array.from({ length: frameCount }, (_, i) => position((Number(localStartSample) + i) * 2)),
    compileBlock: async ({ startSample, frameCount, sourceFormats }) => {
      require(sourceFormats[0].prepared === (mode !== "unprepared-linear"), "Prepared source mode was lost");
      const start = Number(startSample), multiplier = mode === "unprepared-linear" ? 2 : 1;
      return { identity, startSample, frameCount, voices: [{ elementId: "voice", blockOffset: 0, frameCount, positions: Array.from({ length: frameCount }, (_, i) => position((start + i) * multiplier)), amplitudes: Array.from({ length: frameCount }, (_, i) => gainAt(start + i)) }] };
    },
  };
  return renderClassicExport(plan, host, { maxPcmBytes: 64 * 1024 * 1024, signal: options.signal });
}
function frequency(data) {
  // Count interpolated positive crossings in the central window; expected
  // frequency is the hand-authored sine, not another renderer's PCM output.
  const crossings = [];
  for (let i = 8193; i < 32768; i++) if (data[i - 1] <= 0 && data[i] > 0) crossings.push(i - 1 + (-data[i - 1]) / (data[i] - data[i - 1]));
  require(crossings.length > 100, "Pitch output contains insufficient nonzero signal");
  return (crossings.length - 1) * 44100 / (crossings.at(-1) - crossings[0]);
}
export async function runBrowserAudioAcceptance(options) {
  const tone = wav(88200, i => Math.sin(2 * Math.PI * i / 100) * 0.25);
  const direct = await fixture(tone, "unprepared-linear", () => 1, options);
  const pitch = await fixture(tone, "prepared-soundtouch", () => 1, options);
  const directHz = frequency(direct.buffer.getChannelData(0)), pitchHz = frequency(pitch.buffer.getChannelData(0));
  require(Math.abs(directHz - 882) < 1, `Linear rate 2 must double 441Hz: ${directHz}`);
  require(Math.abs(pitchHz - 441) < 3, `Actual SoundTouch rate 2 must preserve 441Hz: ${pitchHz}`);
  require(pitch.buffer.length === 44100 && !pitch.receipt.mastering.applied, "Pitch length or low-peak mastering gate changed");
  // Chromium153 decoded PCM16 +16384 as 16384/32767, so PCM16 cannot
  // establish the exact .5 amplitude oracle. IEEE Float stores .5 exactly;
  // the tone remains PCM16 because its independent oracle is frequency.
  const constant = wav(88200, () => 0.5, "ieee-float32");
  const low = await fixture(constant, "unprepared-linear", () => 0.5, options);
  const leftSample100 = low.buffer.getChannelData(0)[100], rightSample100 = low.buffer.getChannelData(1)[100];
  require(leftSample100 === 0.25 && rightSample100 === 0.25, `Low-peak mono PCM must remain unchanged in both channels: left[100]=${leftSample100}, right[100]=${rightSample100}, linearHz=${directHz}, pitchHz=${pitchHz}, decoder=${JSON.stringify(low.receipt.decoders)}, mastering=${JSON.stringify(low.receipt.mastering)}`);
  const limited = await fixture(constant, "unprepared-linear", i => i >= 44000 ? 4 : 0.5, options, { startSample: "0", frameCount: "100" });
  require(limited.receipt.mastering.applied && limited.receipt.mastering.inputPeak === 2 && limited.receipt.mastering.wholeSceneFrames === "44100", "A late scene peak must activate mastering before a quiet range is sliced");
  require(limited.buffer.length === 100, "Range must have its exact compiled sample count");
  for (let c = 0; c < 2; c++) for (const sample of limited.buffer.getChannelData(c)) require(Number.isFinite(sample) && Math.abs(sample) <= Math.fround(0.98), "Mastered output violates final clamp");
  return { linearFrequencyHz: directHz, preservedFrequencyHz: pitchHz, masteringWholeScenePeak: limited.receipt.mastering.inputPeak, constantOracleEncoding: "WAVE_FORMAT_IEEE_FLOAT/32bit", engineIdentity: options.engineIdentity };
}
