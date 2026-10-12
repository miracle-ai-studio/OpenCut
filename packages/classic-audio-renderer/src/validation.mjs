// Private execution bounds. Rust owns concurrent admission and block partitioning.
export const AUDIO_EXECUTION_LIMITS = Object.freeze({ resources: 32, voices: 256, blockVoices: 32, blockFrames: 4096, voiceSamples: 65536 });
// Expanded private preview facts share the native reply budget. This browser
// limit runs before cloning or sending repeated source observations to Node.
export const PREVIEW_FACTS_MAX_BYTES = 16 * 1024 * 1024;

export class AudioExecutionError extends Error {
  constructor(code, message) { super(message); this.name = "AudioExecutionError"; this.code = code; }
}
export function reject(code, message) { throw new AudioExecutionError(code, message); }
export function integer(value, name, min = 0, max = Number.MAX_SAFE_INTEGER) {
  if (!Number.isSafeInteger(value) || value < min || value > max) reject("INVALID_AUDIO_PLAN", `${name} is outside its integer bounds`);
  return value;
}
export function natural(value, name, bits = 64) {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)$/.test(value)) reject("INVALID_AUDIO_PLAN", `${name} must be an unsigned decimal string`);
  const result = BigInt(value);
  if (result >= (1n << BigInt(bits))) reject("INVALID_AUDIO_PLAN", `${name} exceeds u${bits}`);
  return result;
}
export function frameCount(value, name, min = 0) {
  const count = natural(value, name);
  if (count > BigInt(Number.MAX_SAFE_INTEGER)) reject("AUDIO_BUDGET_EXCEEDED", `${name} cannot be represented by a bounded browser buffer`);
  return integer(Number(count), name, min);
}
export function checkAbort(signal) {
  if (signal?.aborted) reject("CANCELLED", "Audio execution was cancelled");
}
export class PcmBudget {
  constructor(maxBytes) { this.maximum = integer(maxBytes, "maxPcmBytes", 4); this.used = 0; this.peak = 0; }
  reserve(bytes) {
    integer(bytes, "PCM reservation");
    if (bytes > this.maximum - this.used) reject("AUDIO_BUDGET_EXCEEDED", "PCM working buffers exceed the admitted budget");
    this.used += bytes; this.peak = Math.max(this.peak, this.used);
    let released = false;
    return () => { if (!released) { released = true; this.used -= bytes; } };
  }
}
export function pcmBytes(frames, channels) { return integer(frames * channels * 4, "PCM bytes"); }
export function decodedCount(value, name, maximum) {
  if (!Number.isSafeInteger(value) || value < 1) reject("INVALID_DECODED_PCM", `${name} is not a positive decoded count`);
  integer(maximum, "decoder admission count", 1);
  if (value > maximum) reject("AUDIO_BUDGET_EXCEEDED", `${name} exceeds the admitted decoder allocation`);
  return value;
}
export function validateBuffer(buffer, admission) {
  decodedCount(buffer.length, "decoded length", admission.maxFrames);
  decodedCount(buffer.numberOfChannels, "decoded channels", admission.maxChannels);
  integer(buffer.sampleRate, "decoded sample rate", 1, 384000);
  for (let c = 0; c < buffer.numberOfChannels; c++) {
    const data = buffer.getChannelData(c);
    if (!(data instanceof Float32Array) || data.length !== buffer.length) reject("INVALID_DECODED_PCM", "Decoder returned inconsistent PCM channels");
    for (const value of data) if (!Number.isFinite(value)) reject("INVALID_DECODED_PCM", "Decoder returned non-finite PCM");
  }
}
