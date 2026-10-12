import { checkAbort, pcmBytes, reject } from "./validation.mjs";
import { cancellable } from "./runtime.mjs";

export const CLASSIC_MASTERING = Object.freeze({ threshold: -1, knee: 0, ratio: 20, attack: 0.001, release: 0.12, headroom: 0.98 });
export function peakOf(buffer, signal) {
  let peak = 0;
  for (let c = 0; c < buffer.numberOfChannels; c++) {
    const data = buffer.getChannelData(c);
    for (let i = 0; i < data.length; i++) {
      if ((i & 4095) === 0) checkAbort(signal);
      if (!Number.isFinite(data[i])) reject("INVALID_DECODED_PCM", "Mastering input contains non-finite PCM");
      peak = Math.max(peak, Math.abs(data[i]));
    }
  }
  return peak;
}
export function createClassicMasteringChain(context, destination) {
  const input = context.createGain();
  const limiter = context.createDynamicsCompressor();
  const output = context.createGain();
  for (const name of ["threshold", "knee", "ratio", "attack", "release"]) limiter[name].value = CLASSIC_MASTERING[name];
  output.gain.value = CLASSIC_MASTERING.headroom;
  input.connect(limiter); limiter.connect(output); output.connect(destination);
  return { input, disconnect() { input.disconnect(); limiter.disconnect(); output.disconnect(); } };
}
export async function masterWholeScene(buffer, host, budget, signal) {
  const peak = peakOf(buffer, signal);
  if (peak <= CLASSIC_MASTERING.headroom) return { buffer, peak, applied: false, release: () => {} };
  const release = budget.reserve(pcmBytes(buffer.length, buffer.numberOfChannels));
  let source, chain;
  try {
    const context = host.createOfflineContext(buffer.numberOfChannels, buffer.length, buffer.sampleRate);
    source = context.createBufferSource(); source.buffer = buffer;
    chain = createClassicMasteringChain(context, context.destination);
    source.connect(chain.input); source.start(0);
    const rendered = await cancellable(context.startRendering(), host, signal);
    for (let c = 0; c < rendered.numberOfChannels; c++) {
      const data = rendered.getChannelData(c);
      for (let i = 0; i < data.length; i++) {
        if ((i & 4095) === 0) checkAbort(signal);
        if (!Number.isFinite(data[i])) reject("INVALID_DECODED_PCM", "WebAudio mastering returned non-finite PCM");
        data[i] = Math.max(-0.98, Math.min(0.98, data[i]));
      }
    }
    return { buffer: rendered, peak, applied: true, release };
  } catch (error) { release(); throw error; }
  finally { source?.disconnect(); chain?.disconnect(); }
}
