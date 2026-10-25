import { AUDIO_EXECUTION_LIMITS, frameCount, integer, reject } from "./validation.mjs";

// Positions and amplitudes are executable Rust facts. No PTS, retime, gain
// curve, or absolute timeline arithmetic is performed by this sampler.
export function mixPreviewBlock(block, sources, frames) {
  integer(frames, "preview block frames", 1, AUDIO_EXECUTION_LIMITS.blockFrames);
  if (!Array.isArray(block.voices) || block.voices.length > AUDIO_EXECUTION_LIMITS.blockVoices) reject("INVALID_AUDIO_PLAN", "Invalid preview voices");
  const output = [new Float32Array(frames), new Float32Array(frames)];
  // Tables may be shared by several Rust-compiled voices. Normalize each exact
  // table once per block, and borrow each decoded channel once, rather than
  // crossing WebAudio/BigInt validation for every sample of every voice.
  const positions = new WeakMap(), gains = new WeakSet(), channels = new WeakMap();
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
    let taps = positions.get(voice.positions);
    if (!taps) {
      taps = voice.positions.map(point => {
        if (point === null) return null;
        integer(point.segmentId, "preview segment ID", 0, 0xffffffff);
        const lower = frameCount(point.lower, "preview lower sample"), upper = frameCount(point.upper, "preview upper sample");
        if (upper < lower || upper > lower + 1 || !Number.isFinite(point.fraction) || point.fraction < 0 || point.fraction > 1)
          reject("INVALID_AUDIO_PLAN", "Compiled preview tap is outside its actual segment");
        return { segmentId: point.segmentId, lower, upper, fraction: point.fraction };
      });
      positions.set(voice.positions, taps);
    }
    if (!gains.has(voice.amplitudes)) {
      if (voice.amplitudes.some(value => !Number.isFinite(value))) reject("INVALID_AUDIO_PLAN", "Non-finite preview gain");
      gains.add(voice.amplitudes);
    }
    for (let i = 0; i < count; i++) {
      const gain = voice.amplitudes[i], point = taps[i];
      if (point === null) continue;
      const buffer = source.get(point.segmentId);
      if (!buffer) reject("INVALID_AUDIO_PLAN", "Compiled preview segment is absent");
      const { lower, upper } = point;
      if (lower >= buffer.length || upper >= buffer.length) reject("INVALID_AUDIO_PLAN", "Compiled preview tap is outside its actual segment");
      let data = channels.get(buffer);
      if (!data) {
        const left = buffer.getChannelData(0);
        data = [left, buffer.numberOfChannels === 1 ? left : buffer.getChannelData(1)];
        channels.set(buffer, data);
      }
      for (let c = 0; c < 2; c++) {
        const value = output[c][offset + i] + (data[c][lower] * (1 - point.fraction) + data[c][upper] * point.fraction) * gain;
        if (!Number.isFinite(value) || !Number.isFinite(Math.fround(value))) reject("INVALID_DECODED_PCM", "Preview mixing overflowed Float32");
        output[c][offset + i] = value;
      }
    }
  }
  return output;
}
