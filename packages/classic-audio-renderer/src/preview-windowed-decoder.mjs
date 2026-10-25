import { decodeWindowedStream } from "./windowed-decoder.mjs";
import { pcmBytes, reject } from "./validation.mjs";

// PTS facts cover the entire observed stream exactly as before. The sparse
// segment map retains only buffers intersecting Rust's guarded source windows;
// missing requested segments remain errors in the existing preview mixer.
export async function decodeWindowedPreview(track, AudioBufferSink, primary, context, host, budget, admission, signal,
  { needsConcat, needsPts }) {
  const reserve = budget.reserve(pcmBytes(admission.workingFrames, admission.maxChannels) * 4);
  const ranges = admission.windows.map(window => [Number(window.sourceStartFrame), Number(window.sourceEndFrame)]);
  const segments = new Map(), facts = [], channels = Math.min(2, primary.channels);
  let residentFrames = 0, total = 0;
  try {
    const concatenated = await decodeWindowedStream(new AudioBufferSink(track).buffers(0), primary, context, host, admission, signal, {
      retainConcatenated: needsConcat,
      // Preserve preview's original operation order independently of export's
      // total*(target/native) decoder count at floating-point boundaries.
      resampledCount: total => Math.ceil(total * context.sampleRate / primary.sampleRate),
      observe(buffer, timestamp) {
        if (!Number.isFinite(timestamp) || (needsPts && facts.length >= 65536)) reject("AUDIO_BUDGET_EXCEEDED", "Preview PTS facts exceed original admission");
        total += buffer.length;
        if (!needsPts) return;
        const id = facts.length;
        facts.push({ segmentId: id, timestampSeconds: timestamp, frameCount: String(buffer.length) });
        const start = timestamp * primary.sampleRate, end = start + buffer.length;
        if (!needsPts || !ranges.some(([low, high]) => start < high && end > low)) return;
        residentFrames += buffer.length;
        if (residentFrames > admission.workingFrames) reject("AUDIO_BUDGET_EXCEEDED", "Retained PTS segments exceed fixed working admission");
        const copy = context.createBuffer(channels, buffer.length, primary.sampleRate);
        for (let channel = 0; channel < channels; channel++) copy.copyToChannel(buffer.getChannelData(channel), channel);
        segments.set(id, copy);
      },
    });
    reserve();
    const release = budget.reserve(pcmBytes(residentFrames, channels) + concatenated.residentBytes);
    return { primary, sampleRate: primary.sampleRate, channels, frameCount: String(total), segments, facts, residentFrames,
      windowedConcatenated: needsConcat ? concatenated : null, release };
  } finally { reserve(); }
}
