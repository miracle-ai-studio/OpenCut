import { AUDIO_EXECUTION_LIMITS, natural, integer, reject } from "./validation.mjs";

// Exact integer comparison precedes Number conversion. In particular a very
// large source tick cannot wrap or round down into this bounded decoded buffer.
export function sampleLinear(data, position) {
  const lower = natural(position.lower, "source lower");
  const numerator = natural(position.fractionNumerator, "source fraction numerator", 128);
  const denominator = natural(position.fractionDenominator, "source fraction denominator", 128);
  if (denominator === 0n || numerator >= denominator) reject("INVALID_AUDIO_PLAN", "Source fraction is not canonical");
  if (lower >= BigInt(data.length)) return 0;
  const index = Number(lower);
  const fraction = Number(numerator) / Number(denominator);
  return data[index] * (1 - fraction) + data[Math.min(data.length - 1, index + 1)] * fraction;
}

// One Float32 write per stored voice, matching Classic's accumulation order.
// A prepared-linear source already contains the separate preparation write.
export function mixCompiledBlock(output, sourceBuffers, voiceOrder, block, start, count) {
  integer(count, "compiled block frames", 1, AUDIO_EXECUTION_LIMITS.blockFrames);
  integer(start, "compiled block start", 0, output.length - count);
  if (block?.startSample !== String(start) || block.frameCount !== count || !Array.isArray(block.voices) || block.voices.length > AUDIO_EXECUTION_LIMITS.blockVoices) reject("INVALID_AUDIO_PLAN", "Compiled block does not match the requested interval");
  // Admit all actual voice ranges before writing any samples; number of voices
  // here is a block union bound, not a second timeline concurrency algorithm.
  let work = 0;
  for (const voice of block.voices) {
    const offset = integer(voice.blockOffset, "voice block offset", 0, count);
    work += integer(voice.frameCount, "voice frame count", 1, count - offset);
  }
  if (work > AUDIO_EXECUTION_LIMITS.voiceSamples) reject("AUDIO_BUDGET_EXCEEDED", "Compiled block exceeds the voice-sample budget");
  let lastOrder = -1;
  for (const voice of block.voices) {
    const order = voiceOrder.indexOf(voice.elementId);
    if (order <= lastOrder) reject("INVALID_AUDIO_PLAN", "Compiled voices must be unique and in fixed stored order");
    lastOrder = order;
    const source = sourceBuffers.get(voice.elementId);
    if (!source) reject("INVALID_AUDIO_PLAN", "Compiled voice references an unprepared source");
    const offset = integer(voice.blockOffset, "voice block offset", 0, count);
    const length = integer(voice.frameCount, "voice frame count", 1, count - offset);
    if (!Array.isArray(voice.positions) || voice.positions.length !== length || !Array.isArray(voice.amplitudes) || voice.amplitudes.length !== length) reject("INVALID_AUDIO_PLAN", "Compiled sample facts have the wrong length");
    for (let channel = 0; channel < output.numberOfChannels; channel++) {
      const destination = output.getChannelData(channel);
      const data = source.getChannelData(Math.min(channel, source.numberOfChannels - 1));
      for (let i = 0; i < length; i++) {
        const gain = voice.amplitudes[i];
        if (!Number.isFinite(gain) || gain < 0) reject("INVALID_AUDIO_PLAN", "Physical gain must be a finite nonnegative amplitude");
        const index = start + offset + i;
        const mixed = destination[index] + sampleLinear(data, voice.positions[i]) * gain;
        if (!Number.isFinite(mixed) || !Number.isFinite(Math.fround(mixed))) reject("INVALID_AUDIO_PLAN", "PCM accumulation overflowed");
        destination[index] = mixed;
      }
    }
  }
}
