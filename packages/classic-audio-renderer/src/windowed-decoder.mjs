import { checkAbort, decodedCount, integer, natural, reject, validateBuffer } from "./validation.mjs";
import { cancellable } from "./runtime.mjs";
import { windowedPcmSource } from "./pcm-source.mjs";

export function validateWindowAdmission(admission, primary, targetRate) {
  const keys = ["mode", "maxFrames", "maxChannels", "workingFrames", "sourceSampleRate", "targetSampleRate", "maxNativeFrames", "windows"];
  if (!admission || !["whole", "windowed"].includes(admission.mode)
      || Object.keys(admission).length !== keys.length || Object.keys(admission).some(k => !keys.includes(k))
      || admission.sourceSampleRate !== primary.sampleRate || admission.targetSampleRate !== targetRate
      || admission.maxChannels !== primary.channels || !Array.isArray(admission.windows)
      || admission.windows.length > 256) reject("INVALID_AUDIO_PLAN", "Invalid decoder window admission");
  integer(admission.maxFrames, "logical decoder cap", 1);
  integer(admission.workingFrames, "decoder working cap", 1);
  if (admission.workingFrames > admission.maxFrames || (admission.mode === "whole" && admission.windows.length))
    reject("INVALID_AUDIO_PLAN", "Changed decoder mode or working bound");
  const maxNative = natural(admission.maxNativeFrames, "native decoder cap");
  if (maxNative > BigInt(Number.MAX_SAFE_INTEGER)) reject("INVALID_AUDIO_PLAN", "Unsafe native decoder count");
  if (maxNative !== BigInt(admission.maxFrames) * BigInt(primary.sampleRate) / BigInt(targetRate))
    reject("INVALID_AUDIO_PLAN", "Native and logical frame admissions differ");
  let previous = 0n, resident = 0n;
  let a = primary.sampleRate, b = targetRate;
  while (b) [a, b] = [b, a % b];
  const period = BigInt(primary.sampleRate / a);
  for (const window of admission.windows) {
    if (Object.keys(window).length !== 3 || Object.keys(window).some(k => !["sourceStartFrame", "sourceEndFrame", "targetStartFrame"].includes(k)))
      reject("INVALID_AUDIO_PLAN", "Unknown decoder window fields");
    const start = natural(window.sourceStartFrame, "window start"), end = natural(window.sourceEndFrame, "window end");
    const target = natural(window.targetStartFrame, "window target offset");
    const renderStart = start === 0n ? 0n : start + period;
    if (start < previous || end <= start || end > maxNative || renderStart * BigInt(targetRate) !== target * BigInt(primary.sampleRate))
      reject("INVALID_AUDIO_PLAN", "Decoder window order or resample phase changed");
    const count = end - start, resampled = (count * BigInt(targetRate) + BigInt(primary.sampleRate - 1)) / BigInt(primary.sampleRate);
    resident += count > resampled + 1n ? count : resampled + 1n; previous = end;
  }
  if (resident > BigInt(admission.workingFrames)) reject("AUDIO_BUDGET_EXCEEDED", "Windows exceed original working frame admission");
}

export async function decodeVideoWindows(track, AudioBufferSink, primary, context, host, admission, signal) {
  return decodeWindowedStream(new AudioBufferSink(track).buffers(0), primary, context, host, admission, signal);
}
// Shared codec observation pass; preview may retain PTS-indexed native segments
// alongside the exact concatenated source. Neither callback authors a clock.
export async function decodeWindowedStream(buffers, primary, context, host, admission, signal,
  { observe, retainConcatenated = true, resampledCount } = {}) {
  const windows = admission.windows.map(w => ({ start: Number(w.sourceStartFrame), end: Number(w.sourceEndFrame),
    target: Number(w.targetStartFrame), chunks: [], length: 0 }));
  let total = 0;
  const chunkAdmission = { maxFrames: admission.workingFrames, maxChannels: admission.maxChannels };
  // Decode from zero once, so discarded packet timestamps and the concatenated
  // frame count remain exactly the Classic algorithm. Only residency is sparse.
  for await (const { buffer, timestamp } of buffers) {
    checkAbort(signal); validateBuffer(buffer, chunkAdmission);
    if (buffer.sampleRate !== primary.sampleRate || buffer.numberOfChannels !== primary.channels)
      reject("INVALID_DECODED_PCM", "Windowed primary audio format changed");
    const end = decodedCount(total + buffer.length, "streamed native frames", Number(admission.maxNativeFrames));
    observe?.(buffer, timestamp);
    for (const window of retainConcatenated ? windows : []) {
      const from = Math.max(total, window.start), to = Math.min(end, window.end);
      if (to <= from) continue;
      if (from - window.start !== window.length) reject("INVALID_DECODED_PCM", "Windowed concatenation has a gap");
      window.chunks.push({ start: from, channels: Array.from({ length: Math.min(2, primary.channels) }, (_, c) =>
        new Float32Array(buffer.getChannelData(c).subarray(from - total, to - total))) });
      window.length += to - from;
    }
    total = end;
  }
  if (!total) reject("EMPTY_DECODED_AUDIO", "The authorized source decoded no audio");
  // Preserve Classic's floating-point decoder length calculation as well as
  // its PCM. Authored source positions remain exact facts supplied by Rust.
  const logicalFrames = decodedCount(resampledCount ? resampledCount(total) : Math.ceil(total * (context.sampleRate / primary.sampleRate)),
    "logical resampled frames", admission.maxFrames);
  const channels = Math.min(2, primary.channels), retained = [];
  if (!retainConcatenated) return windowedPcmSource(context.sampleRate, logicalFrames, channels, []);
  // Chromium carries an accumulated double read index between render quanta.
  // Keep that absolute decoder phase, including its floating-point rounding;
  // resetting to an exact rational offset after minutes changes source PCM.
  // This is decoder-format work, never authored source/time mapping.
  const ratio = primary.sampleRate / context.sampleRate;
  let phase = 0, cursor = 0;
  const advance = goal => {
    for (; cursor < goal; cursor++) {
      if ((cursor & 16383) === 0) checkAbort(signal);
      phase += ratio;
    }
    return phase;
  };
  for (const window of windows) {
    checkAbort(signal); if (!window.length || window.target >= logicalFrames) continue;
    const end = window.start + window.length === total ? logicalFrames
      : Math.min(logicalFrames, Math.ceil((window.start + window.length) * (context.sampleRate / primary.sampleRate)));
    for (let start = window.target; start < end;) {
      checkAbort(signal); const frames = Math.min(4096, end - start);
      const sourceStart = advance(start), sourceEnd = advance(start + frames);
      if (sourceStart >= total) {
        retained.push({ start, buffer: context.createBuffer(channels, frames, context.sampleRate) });
        start += frames; continue;
      }
      const from = Math.max(window.start, Math.floor(sourceStart) - 32);
      const to = Math.min(window.start + window.length, Math.ceil(sourceEnd) + 32);
      if (from > sourceStart || to <= from) reject("INVALID_AUDIO_PLAN", "Window omits required absolute decoder phase");
      const native = context.createBuffer(channels, to - from, primary.sampleRate);
      for (const chunk of window.chunks) {
        const begin = Math.max(from, chunk.start), finish = Math.min(to, chunk.start + chunk.channels[0].length);
        if (finish <= begin) continue;
        for (let c = 0; c < channels; c++) native.copyToChannel(chunk.channels[c].subarray(begin - chunk.start, finish - chunk.start), c, begin - from);
      }
      const buffer = await resamplePcmBlock(native, sourceStart - from, frames, context.sampleRate, host, signal);
      retained.push({ start, buffer });
      start += frames;
    }
    window.chunks.length = 0;
  }
  return windowedPcmSource(context.sampleRate, logicalFrames, channels, retained);
}

// The same original Web Audio sampler serves sparse preparation and streamed
// source summaries. Channel count belongs to the consumer; source peaks must
// preserve all channels rather than preview's explicit first-two mapping.
export async function resamplePcmBlock(native, sourceOffset, frames, targetRate, host, signal) {
  const channels = native.numberOfChannels, ratio = native.sampleRate / targetRate;
  const admission = { workingFrames: frames };
  const offline = host.createOfflineContext(channels, frames, targetRate);
  const node = offline.createBufferSource(); node.buffer = native;
  if (ratio !== 1) {
    // Effective playback remains exactly1. Chromium otherwise rounds a
    // start offset to a whole sample when base rate1/detune0 are selected.
    // See AudioBufferSourceHandler::ClampGrainParameters in Chromium.
    node.playbackRate.value = .5; node.detune.value = 1200;
  }
  node.connect(offline.destination); node.start(0, sourceOffset / native.sampleRate);
  try {
    const buffer = await cancellable(offline.startRendering(), host, signal);
    validateBuffer(buffer, { maxFrames: admission.workingFrames, maxChannels: channels });
    if (buffer.sampleRate !== targetRate || buffer.length !== frames || buffer.numberOfChannels !== channels)
      reject("INVALID_DECODED_PCM", "Window resampler changed the admitted format");
    return buffer;
  } finally { node.disconnect(); }
}

// Sequential concatenation observes actual EOF and retains only the next
// resampling block and guards. Packet PTS never becomes an authored source clock.
export async function consumeConcatenatedPcm(buffers, primary, targetRate, context, host, signal,
  { maxChunkFrames, maxResidentFrames, consume }) {
  integer(maxChunkFrames, "streamed decoder chunk", 1, 65536);
  integer(maxResidentFrames, "streamed PCM residency", maxChunkFrames);
  integer(primary.channels, "streamed decoder channels", 1, 64);
  integer(primary.sampleRate, "streamed decoder rate", 1, 384000);
  integer(targetRate, "streamed target rate", 1, 384000);
  const chunks = [], ratio = primary.sampleRate / targetRate;
  // Size the source-summary block from its existing resident share. A whole
  // decoder chunk may overlap either guard; native and output copies
  // are charged too. Keep128-sample alignment for the source peak consumer.
  const blockFrames = Math.min(65536,
    Math.floor((maxResidentFrames - maxChunkFrames * 2 - 128) / (2 * ratio + 2) / 128) * 128);
  if (blockFrames < 128) reject("AUDIO_BUDGET_EXCEEDED", "No guarded streamed PCM block fits resident admission");
  let total = 0, target = 0, phase = 0, resident = 0;
  const nextPhase = frames => { let next = phase; for (let n = 0; n < frames; n++) next += ratio; return next; };
  async function render(frames, next, final) {
    checkAbort(signal);
    const from = Math.max(0, Math.floor(phase) - 32), to = Math.min(total, Math.ceil(next) + 32);
    let buffer;
    if (phase >= total) buffer = context.createBuffer(primary.channels, frames, targetRate);
    else {
      if (to <= from || resident + (to - from) + frames * 2 > maxResidentFrames)
        reject("AUDIO_BUDGET_EXCEEDED", "Streamed resampling block exceeds resident admission");
      const native = context.createBuffer(primary.channels, to - from, primary.sampleRate);
      let covered = 0;
      for (const chunk of chunks) {
        const begin = Math.max(from, chunk.start), end = Math.min(to, chunk.start + chunk.buffer.length);
        if (end <= begin) continue;
        if (begin !== from + covered) reject("INVALID_DECODED_PCM", "Streamed PCM lost its guarded source samples");
        for (let c = 0; c < primary.channels; c++)
          native.copyToChannel(chunk.buffer.getChannelData(c).subarray(begin - chunk.start, end - chunk.start), c, begin - from);
        covered += end - begin;
      }
      if (covered !== to - from || (!final && to < Math.ceil(next) + 32))
        reject("INVALID_DECODED_PCM", "Streamed resampling guard is incomplete");
      if (ratio === 1) {
        // Same-rate concatenation is an exact PCM copy. No offline render or
        // resampling is needed for the common source-waveform path.
        buffer = context.createBuffer(primary.channels, frames, targetRate);
        for (let c = 0; c < primary.channels; c++)
          buffer.copyToChannel(native.getChannelData(c).subarray(phase - from, phase - from + frames), c);
      } else buffer = await resamplePcmBlock(native, phase - from, frames, targetRate, host, signal);
    }
    await consume(buffer, target); checkAbort(signal);
    target += frames; phase = next;
    while (chunks.length && chunks[0].start + chunks[0].buffer.length <= Math.max(0, Math.floor(phase) - 32))
      resident -= chunks.shift().buffer.length;
  }
  try {
    for await (const { buffer } of buffers) {
      checkAbort(signal);
      validateBuffer(buffer, { maxFrames: maxChunkFrames, maxChannels: primary.channels });
      if (buffer.sampleRate !== primary.sampleRate || buffer.numberOfChannels !== primary.channels)
        reject("INVALID_DECODED_PCM", "Streamed primary audio format changed");
      if (!Number.isSafeInteger(total + buffer.length) || resident + buffer.length > maxResidentFrames)
        reject("AUDIO_BUDGET_EXCEEDED", "Streamed PCM exceeds exact addressing or resident admission");
      chunks.push({ start: total, buffer }); total += buffer.length; resident += buffer.length;
      let next = nextPhase(blockFrames);
      while (Math.ceil(next) + 32 <= total) {
        await render(blockFrames, next, false); next = nextPhase(blockFrames);
      }
    }
    const logicalFrames = Math.ceil(total * (targetRate / primary.sampleRate));
    if (!Number.isSafeInteger(logicalFrames)) reject("AUDIO_BUDGET_EXCEEDED", "Streamed decoded extent exceeds exact addressing");
    while (target < logicalFrames) {
      const frames = Math.min(blockFrames, logicalFrames - target);
      await render(frames, nextPhase(frames), true);
    }
    return { nativeFrames: total, frameCount: logicalFrames, channels: primary.channels, sampleRate: targetRate };
  } finally { chunks.length = 0; }
}
