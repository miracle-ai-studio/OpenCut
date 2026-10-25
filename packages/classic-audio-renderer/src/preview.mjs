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
  const decoded = new Map(), sources = new Map(), formats = [], decoders = [], preparation = [], releases = [];
  let closed = false;
  try {
    for (const resource of plan.resources) {
      checkAbort(signal);
      if (typeof host.resolveRangeSource !== "function") reject("INVALID_AUDIO_HOST", "Preview requires a verified range source");
      const bytes = await host.resolveRangeSource(resource, budget, signal); checkAbort(signal);
      const voices = plan.voices.filter(voice => voice.assetId === resource.assetId);
      const value = await decodePreviewResource(resource, bytes, context, host, budget, signal,
        { needsConcat: voices.some(voice => voice.sourceMode === "prepared-concat"), needsPts: voices.some(voice => voice.sourceMode === "preserve-pts") });
      decoded.set(resource.assetId, value); releases.push(value.release);
      const observed = { resource, sampleRate: value.sampleRate, frameCount: value.frameCount, channels: value.channels };
      if (value.residentFrames !== undefined) {
        if (typeof host.observeWindowedPreviewDecoded !== "function") reject("INVALID_AUDIO_HOST", "Windowed preview requires residency observation");
        await host.observeWindowedPreviewDecoded({ decoded: observed, residentFrameCount: String(value.residentFrames) });
      } else await host.observePreviewDecoded(observed);
      checkAbort(signal);
      decoders.push({ assetId: resource.assetId, primary: value.primary, sampleRate: value.sampleRate, frameCount: value.sourceFrameCount, channels: value.channels });
      let joined;
      for (const voice of voices) {
        checkAbort(signal);
        if (sources.has(voice.elementId)) reject("INVALID_AUDIO_PLAN", "Preview voice changed its fixed resource");
        if (voice.sourceMode === "preserve-pts") {
          if (value.primary.channels > 2) reject("UNSUPPORTED_AUDIO_PREVIEW_CHANNEL_LAYOUT", "Ordinary realtime multichannel downmix is not implemented");
          sources.set(voice.elementId, value.rolling ? new Map() : value.segments);
          formats.push({ elementId: voice.elementId, sampleRate: value.sampleRate, frameCount: value.rolling ? "0" : value.frameCount, channels: value.channels, prepared: false, segments: value.rolling ? [] : value.facts });
        } else if (voice.sourceMode === "prepared-concat") {
          if (!joined) { joined = await concatenatePreviewSource(value, context, host, budget, signal); releases.push(joined.release); }
          const prepared = await prepareVoice(voice, joined.buffer, context, host, budget, signal); releases.push(prepared.release);
          preparation.push({ elementId: voice.elementId, facts: prepared.facts });
          if (prepared.facts.mode === "unprepared-linear") reject("INVALID_AUDIO_PLAN", "Prepared preview voice did not prepare its source");
          sources.set(voice.elementId, new Map([[0, prepared.buffer]]));
          formats.push({ elementId: voice.elementId, sampleRate: prepared.buffer.sampleRate, frameCount: String(prepared.buffer.length), channels: prepared.buffer.numberOfChannels, prepared: true, segments: [] });
        } else reject("INVALID_AUDIO_PLAN", "Unknown preview source mode");
      }
      if (joined) {
        // All prepared consumers of this exact asset now own independent
        // Float32/DSP outputs. The ordinary consumer keeps its separate rolling
        // decoder; neither it nor a later source needs this concat allocation.
        if (typeof host.releaseDecodedSource !== "function") reject("INVALID_AUDIO_HOST", "Preview preparation requires source retirement");
        const released = await host.releaseDecodedSource(Object.freeze({ ...resource })); checkAbort(signal);
        if (released?.released !== true) reject("INVALID_AUDIO_PLAN", "Preview preparation release was not acknowledged");
        joined.buffer = null; joined.release();
        value.segments.clear(); value.windowedConcatenated = null; value.release();
      }
    }
    // Resource-at-a-time preparation retires large inputs before decoding the
    // next resource. Restore authored voice order before binding or mixing.
    const order = new Map(plan.voices.map((voice, index) => [voice.elementId, index]));
    if (sources.size !== plan.voices.length || plan.voices.some(voice => !sources.has(voice.elementId))) reject("INVALID_AUDIO_PLAN", "Preview voice has no fixed resource");
    formats.sort((a, b) => order.get(a.elementId) - order.get(b.elementId));
    preparation.sort((a, b) => order.get(a.elementId) - order.get(b.elementId));
    const ordered = plan.voices.map(voice => [voice.elementId, sources.get(voice.elementId)]);
    sources.clear(); for (const [id, source] of ordered) sources.set(id, source);
    admitExpandedSourceFacts(formats);
    await host.bindPreviewSources({ sourceFormats: structuredClone(formats), window: null }); checkAbort(signal);
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
        const request = { startSample: command.startSample, frameCount: count };
        const windows = command.sourceWindows === undefined ? await host.previewSourceWindows(request) : structuredClone(command.sourceWindows); checkAbort(signal);
        if (!Array.isArray(windows) || windows.length > 32 || windows.some(w => !decoded.has(w.assetId)))
          reject("INVALID_AUDIO_PLAN", "Rust source windows changed fixed resources");
        const currentFormats = [];
        // Release the previous borrow before evicting its owner-held PCM. Keep
        // Map insertion order: the original mixer enforces fixed voice order.
        for (const voice of plan.voices) if (voice.sourceMode === "preserve-pts" && decoded.get(voice.assetId)?.rolling)
          sources.set(voice.elementId, new Map());
        const current = await ensurePreviewSourceWindows(decoded,windows,signal);
        for (const format of formats) {
          const voice = plan.voices.find(v => v.elementId === format.elementId), observation = current.get(voice.assetId);
          if (format.prepared || !observation) { currentFormats.push(format); continue; }
          sources.set(format.elementId, observation.segments);
          currentFormats.push({ ...format, frameCount: observation.frameCount, segments: observation.facts });
        }
        admitExpandedSourceFacts(currentFormats);
        let block;
        if (typeof host.bindAndCompilePreviewBlock === "function") {
          block = await host.bindAndCompilePreviewBlock({ sourceFormats: currentFormats, window: request });
        } else {
          await host.bindPreviewSources({ sourceFormats: currentFormats, window: request }); checkAbort(signal);
          block = await host.compilePreviewBlock(request);
        }
        checkAbort(signal);
        const currentDigest = await previewBindingDigest({ ...binding, sourceFormats: currentFormats, window: request }); checkAbort(signal);
        if (block.startSample !== command.startSample || block.frameCount !== count || Object.keys(plan.identity).some(k => block.identity?.[k] !== plan.identity[k])) reject("REVISION_CONFLICT", "Compiled preview block changed its fixed request");
        const release = budget.reserve(pcmBytes(count, 2));
        try { return { channels: mixPreviewBlock(block, sources, count), receipt: { ...completion, sourceBindingSha256: currentDigest, peakAccountedBytes: budget.peak, compilation: { startSample: block.startSample, frameCount: block.frameCount, receipts: block.receipts } } }; }
        finally { release(); }
      },
      async close() { if (!closed) {
        closed = true;
        await Promise.allSettled([...decoded.values()].map(source => source.close?.()));
        sources.clear(); decoded.clear(); for (const release of releases) release(); await context.close();
      } },
    };
  } catch (error) { await Promise.allSettled([...decoded.values()].map(source => source.close?.())); sources.clear(); decoded.clear(); for (const release of releases) release(); await context.close().catch(() => {}); throw error; }
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

// Independent decoder windows share the unchanged native aggregate budget.
// Drain all active operations before propagating failure; never race realm teardown.
export async function ensurePreviewSourceWindows(decoded,windows,signal){
 const entries=[...decoded].filter(([,source])=>source.rolling),current=new Map();
 for(let offset=0;offset<entries.length;offset+=4){
  checkAbort(signal);const group=entries.slice(offset,offset+4);
  const results=await Promise.allSettled(group.map(async([id,source])=>[id,await source.ensureWindows(windows.filter(window=>window.assetId===id))]));
  const failure=results.find(result=>result.status==="rejected");if(failure)throw failure.reason;
  checkAbort(signal);for(const result of results){const [id,value]=result.value;current.set(id,value);}
 }
 return current;
}
