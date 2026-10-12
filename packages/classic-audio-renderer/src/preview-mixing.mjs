import { AUDIO_EXECUTION_LIMITS, frameCount, integer, reject } from "./validation.mjs";

// Positions and amplitudes are executable Rust facts. No PTS, retime, gain
// curve, or absolute timeline arithmetic is performed by this sampler.
export function mixPreviewBlock(block, sources, frames) {
  integer(frames, "preview block frames", 1, AUDIO_EXECUTION_LIMITS.blockFrames);
  if (!Array.isArray(block.voices) || block.voices.length > AUDIO_EXECUTION_LIMITS.blockVoices) reject("INVALID_AUDIO_PLAN", "Invalid preview voices");
  const output = [new Float32Array(frames), new Float32Array(frames)];
  let work = 0, last = -1; const order = [...sources.keys()]; const used = new Set();
  for (const voice of block.voices) {
    if (used.has(voice.elementId)) reject("INVALID_AUDIO_PLAN", "Duplicate preview voice");
    used.add(voice.elementId);
    const source = sources.get(voice.elementId);
    const ordinal = order.indexOf(voice.elementId);
    if (ordinal <= last) reject("INVALID_AUDIO_PLAN", "Preview voice order changed");
    last = ordinal;
    const offset = integer(voice.blockOffset, "preview voice offset", 0, frames);
    const count = integer(voice.frameCount, "preview voice frames", 1, frames - offset);
    work += count;
    if (work > AUDIO_EXECUTION_LIMITS.voiceSamples || !source || !Array.isArray(voice.positions) || voice.positions.length !== count || !Array.isArray(voice.amplitudes) || voice.amplitudes.length !== count) reject("INVALID_AUDIO_PLAN", "Preview sample facts exceed their bounds");
    for (let i = 0; i < count; i++) {
      const gain = voice.amplitudes[i], point = voice.positions[i];
      if (!Number.isFinite(gain)) reject("INVALID_AUDIO_PLAN", "Non-finite preview gain");
      if (point === null) continue;
      integer(point.segmentId, "preview segment ID", 0, 0xffffffff);
      const buffer = source.get(point.segmentId);
      if (!buffer) reject("INVALID_AUDIO_PLAN", "Compiled preview segment is absent");
      const lower = frameCount(point.lower, "preview lower sample"), upper = frameCount(point.upper, "preview upper sample");
      if (lower >= buffer.length || upper < lower || upper > lower + 1 || upper >= buffer.length || !Number.isFinite(point.fraction) || point.fraction < 0 || point.fraction > 1) reject("INVALID_AUDIO_PLAN", "Compiled preview tap is outside its actual segment");
      for (let c = 0; c < 2; c++) {
        const data = buffer.getChannelData(Math.min(c, buffer.numberOfChannels - 1));
        const value = output[c][offset + i] + (data[lower] * (1 - point.fraction) + data[upper] * point.fraction) * gain;
        if (!Number.isFinite(value) || !Number.isFinite(Math.fround(value))) reject("INVALID_DECODED_PCM", "Preview mixing overflowed Float32");
        output[c][offset + i] = value;
      }
    }
  }
  return output;
}
