import { PitchShifter } from "../vendor/soundtouchjs-0.3.0/soundtouch.js";
import { checkAbort, frameCount, integer, natural, pcmBytes, reject } from "./validation.mjs";
import { cancellable } from "./runtime.mjs";
import { sampleLinear } from "./mixing.mjs";

export async function prepareVoice(voice, decoded, context, host, budget, signal) {
  const supplied = await cancellable(host.compilePitchPreparation({ elementId: voice.elementId, decodedSampleRate: decoded.sampleRate, decodedFrameCount: String(decoded.length), decodedChannels: decoded.numberOfChannels }), host, signal);
  const facts = freezeFacts(structuredClone(supplied));
  if (!facts || !["unprepared-linear", "prepared-linear", "prepared-soundtouch"].includes(facts.mode)) reject("INVALID_AUDIO_PLAN", "Unknown pitch preparation mode");
  if (facts.decodedSampleRate !== decoded.sampleRate || facts.targetSampleRate !== context.sampleRate || facts.numChannels !== Math.min(2, decoded.numberOfChannels)) reject("INVALID_AUDIO_PLAN", "Pitch facts do not bind the actual decoded format");
  const outputFrames = frameCount(facts.outputFrames, "pitch output frames", 1);
  // Rust facts include actual decoder memory plus all preparation buffers.
  // Adapter allocations remain separately admitted, including full-scene PCM.
  if (facts.mode === "unprepared-linear") return { buffer: decoded, facts, release: () => {} };
  const channels = integer(facts.numChannels, "pitch channels", 1, 2);
  const release = budget.reserve(pcmBytes(outputFrames, channels));
  try {
    if (facts.mode === "prepared-linear") {
      const buffer = context.createBuffer(channels, outputFrames, context.sampleRate);
      for (let start = 0; start < outputFrames; start += 4096) {
        checkAbort(signal);
        const count = Math.min(4096, outputFrames - start);
        const positions = await cancellable(host.compileSourcePositions({ elementId: voice.elementId, localStartSample: String(start), frameCount: count, sourceSampleRate: decoded.sampleRate }), host, signal);
        if (!Array.isArray(positions) || positions.length !== count) reject("INVALID_AUDIO_PLAN", "Prepared-linear positions have the wrong length");
        for (let c = 0; c < channels; c++) {
          const input = decoded.getChannelData(c), output = buffer.getChannelData(c);
          for (let i = 0; i < count; i++) output[start + i] = sampleLinear(input, positions[i]);
        }
      }
      return { buffer, facts, release };
    }
    const exactSliceStart = natural(facts.sourceSliceStartSample, "pitch source slice");
    // A very large valid trim beyond EOF is a one-frame zero source. Never
    // convert that absolute source position through an imprecise Number.
    const sliceStart = exactSliceStart >= BigInt(decoded.length) ? decoded.length : Number(exactSliceStart);
    const actual = frameCount(facts.actualSourceFrames, "pitch actual source frames", 1);
    const resampled = frameCount(facts.resampledFrames, "pitch resampled frames", 1);
    const numerator = natural(facts.tempo?.numerator, "tempo numerator");
    const denominator = natural(facts.tempo?.denominator, "tempo denominator");
    const tempo = Number(numerator) / Number(denominator);
    if (tempo < 0.01 || tempo > 5 || Math.abs(tempo - 1) <= 1e-6) reject("INVALID_AUDIO_PLAN", "SoundTouch facts do not select the Classic pitch branch");
    if (actual !== Math.max(1, Math.min(frameCount(facts.requestedSourceFrames, "requested source frames", 1), Math.max(0, decoded.length - sliceStart)))) reject("INVALID_AUDIO_PLAN", "Pitch facts disagree with actual decoded availability");
    const releaseWorking = budget.reserve(pcmBytes(actual + resampled, channels));
    let node, shifter;
    try {
      const resampleContext = host.createOfflineContext(channels, resampled, context.sampleRate);
      const native = resampleContext.createBuffer(channels, actual, decoded.sampleRate);
      for (let c = 0; c < channels; c++) native.copyToChannel(decoded.getChannelData(c).subarray(sliceStart, sliceStart + actual), c);
      node = resampleContext.createBufferSource(); node.buffer = native;
      node.connect(resampleContext.destination); node.start(0);
      const resampledBuffer = await cancellable(resampleContext.startRendering(), host, signal);
      node.disconnect(); node = undefined;
      const stretchContext = host.createOfflineContext(channels, outputFrames, context.sampleRate);
      // Preserve the exact library's 4096 ScriptProcessor blocks, 44100 internal
      // Stretch default, filter history and trailing input behavior.
      shifter = new PitchShifter(stretchContext, resampledBuffer, 4096);
      shifter.tempo = tempo; shifter.pitch = 1; shifter.connect(stretchContext.destination);
      const buffer = await cancellable(stretchContext.startRendering(), host, signal);
      return { buffer, facts, release };
    } finally { node?.disconnect(); shifter?.disconnect(); releaseWorking(); }
  } catch (error) { release(); throw error; }
}

function freezeFacts(value) {
  if (value && typeof value === "object") {
    for (const child of Object.values(value)) freezeFacts(child);
    Object.freeze(value);
  }
  return value;
}
