import { decodePreviewResource, concatenatePreviewSource } from "./preview-decoder.mjs";
import { mixPreviewBlock } from "./preview-mixing.mjs";
import { prepareVoice } from "./preparation.mjs";
import { AUDIO_EXECUTION_LIMITS, PREVIEW_FACTS_MAX_BYTES, PcmBudget, checkAbort, integer, natural, pcmBytes, reject } from "./validation.mjs";

function admitExpandedSourceFacts(formats) {
  const encoder = new TextEncoder();
  let bytes = 2 + Math.max(0, formats.length - 1);
  for (const format of formats) {
    // Measure one item at a time and stop before any repeated full collection
    // is cloned, hashed or marshalled through the browser callback.
    bytes += encoder.encode(JSON.stringify(format)).byteLength;
    if (bytes > PREVIEW_FACTS_MAX_BYTES) reject("AUDIO_BUDGET_EXCEEDED", "Expanded preview facts exceed the reply budget");
  }
}

// Long-lived unmastered producer. The consuming local player owns its actual
// realtime AudioContext/master chain. Export mastering never runs here.
export async function prepareClassicPreview(plan, host, { signal, maxPcmBytes, maxEncodedBytes }) {
  if (plan.identity?.profile !== "classic-realtime-v31-web-audio" || !Array.isArray(plan.resources) || !Array.isArray(plan.voices) || plan.resources.length > AUDIO_EXECUTION_LIMITS.resources || plan.voices.length > AUDIO_EXECUTION_LIMITS.voices) reject("INVALID_AUDIO_PLAN", "Invalid fixed preview profile");
  const context = host.createAudioContext(plan.identity.sampleRate), budget = new PcmBudget(maxPcmBytes);
  const decoded = new Map(), concatenated = new Map(), sources = new Map(), formats = [], decoders = [], preparation = [], releases = [];
  let encoded = 0, closed = false;
  try {
    for (const resource of plan.resources) {
      checkAbort(signal); encoded += integer(resource.size, "preview encoded bytes", 1, maxEncodedBytes);
      if (encoded > maxEncodedBytes) reject("AUDIO_BUDGET_EXCEEDED", "Encoded preview resources exceed admission");
      const bytes = await host.resolveVerifiedBytes(resource); checkAbort(signal);
      if (!(bytes instanceof Uint8Array) || bytes.length !== resource.size) reject("ASSET_HASH_MISMATCH", "Authorized preview byte length changed");
      const digest = await crypto.subtle.digest("SHA-256", bytes); checkAbort(signal);
      if (Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, "0")).join("") !== resource.contentHash) reject("ASSET_HASH_MISMATCH", "Authorized preview bytes changed");
      const value = await decodePreviewResource(resource, bytes, context, host, budget, signal);
      decoded.set(resource.assetId, value); releases.push(value.release);
      await host.observePreviewDecoded({ resource, sampleRate: value.sampleRate, frameCount: value.frameCount, channels: value.channels }); checkAbort(signal);
      decoders.push({ assetId: resource.assetId, primary: value.primary, sampleRate: value.sampleRate, frameCount: value.frameCount, channels: value.channels });
    }
    for (const voice of plan.voices) {
      checkAbort(signal);
      const value = decoded.get(voice.assetId);
      if (!value || sources.has(voice.elementId)) reject("INVALID_AUDIO_PLAN", "Preview voice changed its fixed resource");
      if (voice.sourceMode === "preserve-pts") {
        if (value.primary.channels > 2) reject("UNSUPPORTED_AUDIO_PREVIEW_CHANNEL_LAYOUT", "Ordinary realtime multichannel downmix is not implemented");
        sources.set(voice.elementId, value.segments);
        formats.push({ elementId: voice.elementId, sampleRate: value.sampleRate, frameCount: value.frameCount, channels: value.channels, prepared: false, segments: value.facts });
      } else if (voice.sourceMode === "prepared-concat") {
        let joined = concatenated.get(voice.assetId);
        if (!joined) { joined = await concatenatePreviewSource(value, context, host, budget, signal); concatenated.set(voice.assetId, joined); releases.push(joined.release); }
        const prepared = await prepareVoice(voice, joined.buffer, context, host, budget, signal); releases.push(prepared.release);
        preparation.push({ elementId: voice.elementId, facts: prepared.facts });
        if (prepared.facts.mode === "unprepared-linear") reject("INVALID_AUDIO_PLAN", "Prepared preview voice did not prepare its source");
        sources.set(voice.elementId, new Map([[0, prepared.buffer]]));
        formats.push({ elementId: voice.elementId, sampleRate: prepared.buffer.sampleRate, frameCount: String(prepared.buffer.length), channels: prepared.buffer.numberOfChannels, prepared: true, segments: [] });
      } else reject("INVALID_AUDIO_PLAN", "Unknown preview source mode");
    }
    admitExpandedSourceFacts(formats);
    await host.bindPreviewSources({ sourceFormats: structuredClone(formats) }); checkAbort(signal);
    const binding = { decoders, sourceFormats: structuredClone(formats), preparation };
    const sourceBindingSha256 = await previewBindingDigest(binding); checkAbort(signal);
    const completion = { identity: structuredClone(plan.identity), engineIdentity: host.engineIdentity, sourceBindingSha256 };
    const receipt = { ...completion, ...binding, peakAccountedBytes: budget.peak };
    return { receipt,
      async next(command) {
        if (closed) reject("AUDIO_SESSION_CLOSED", "Preview source realm has closed");
        checkAbort(signal);
        const start = natural(command.startSample, "preview start"), count = integer(command.frameCount, "preview count", 1, 4096);
        if (start + BigInt(count) > natural(plan.frameCount, "preview duration")) reject("INVALID_AUDIO_PLAN", "Preview block exceeds the fixed scene");
        const block = await host.compilePreviewBlock({ startSample: command.startSample, frameCount: count }); checkAbort(signal);
        if (block.startSample !== command.startSample || block.frameCount !== count || Object.keys(plan.identity).some(k => block.identity?.[k] !== plan.identity[k])) reject("REVISION_CONFLICT", "Compiled preview block changed its fixed request");
        const release = budget.reserve(pcmBytes(count, 2));
        try { return { channels: mixPreviewBlock(block, sources, count), receipt: { ...completion, peakAccountedBytes: budget.peak, compilation: { startSample: block.startSample, frameCount: block.frameCount, receipts: block.receipts } } }; }
        finally { release(); }
      },
      async close() { if (!closed) { closed = true; sources.clear(); decoded.clear(); concatenated.clear(); for (const release of releases) release(); await context.close(); } },
    };
  } catch (error) { for (const release of releases) release(); await context.close().catch(() => {}); throw error; }
}

export async function previewBindingDigest(binding) {
  const bytes = new TextEncoder().encode(JSON.stringify(canonical(binding)));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, "0")).join("");
}

function canonical(value) {
  if (typeof value === "number") {
    if (!Number.isFinite(value)) reject("INVALID_AUDIO_PLAN", "Non-finite preview binding fact");
    const bytes = new Uint8Array(8); new DataView(bytes.buffer).setFloat64(0, value === 0 ? 0 : value, false);
    return { $f64: Array.from(bytes, b => b.toString(16).padStart(2, "0")).join("") };
  }
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map(k => [k, canonical(value[k])]));
  return value;
}
