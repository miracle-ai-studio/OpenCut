import { AUDIO_EXECUTION_LIMITS, AudioExecutionError, PcmBudget, checkAbort, frameCount, integer, pcmBytes, reject } from "./validation.mjs";
import { requireHost, cancellable, executionCancellation } from "./runtime.mjs";
import { decodeResource } from "./decoder.mjs";
import { prepareVoice } from "./preparation.mjs";
import { mixCompiledBlock } from "./mixing.mjs";
import { masterWholeScene } from "./mastering.mjs";
export { AudioExecutionError };
export { createClassicMasteringChain } from "./mastering.mjs";

const PROFILE = "classic-export-v31-web-audio";
function identityKey(identity) {
  if (!identity || identity.profile !== PROFILE || ["projectId", "sequenceId", "revision"].some(k => typeof identity[k] !== "string" || !identity[k])) reject("INVALID_AUDIO_PLAN", "A fixed Classic export identity is required");
  integer(identity.sampleRate, "target sample rate", 8000, 192000);
  return JSON.stringify([identity.projectId, identity.sequenceId, identity.revision, identity.profile, identity.sampleRate]);
}
async function authorizedBytes(resource, host, signal, maxBytes) {
  if (!resource || typeof resource.assetId !== "string" || !resource.assetId || !/^[a-f0-9]{64}$/.test(resource.contentHash) || !["audio", "video"].includes(resource.kind)) reject("INVALID_AUDIO_PLAN", "Invalid immutable CAS resource");
  integer(resource.size, "CAS encoded size", 1, maxBytes);
  const resolved = await cancellable(host.resolveVerifiedBytes(Object.freeze({ ...resource }), signal), host, signal);
  if (!(resolved instanceof Uint8Array) || resolved.length !== resource.size) reject("ASSET_RESOLUTION_FAILED", "Host bytes do not match the fixed CAS size");
  // Retain an owned copy so a mutable host buffer cannot change after hashing.
  const bytes = resolved.slice();
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  const actual = Array.from(new Uint8Array(hash), v => v.toString(16).padStart(2, "0")).join("");
  if (actual !== resource.contentHash) reject("ASSET_RESOLUTION_FAILED", "Host bytes do not match the fixed CAS hash");
  checkAbort(signal); return bytes;
}

/** Executes a fixed Rust plan. No project DTO, source URL, host path, keyframe or
 * authored timeline clock is accepted. The host owns policy and actual engine
 * termination; publication remains outside this in-process audio module. */
export async function renderClassicExport(plan, host, options = {}) {
  const { signal } = options;
  // A pre-cancelled call has started no realm work and needs no termination.
  requireHost(host); checkAbort(signal);
  const engineIdentity = host.engineIdentity;
  const cancellation = executionCancellation(host, signal);
  try {
    const result = await cancellable(executeClassicExport(plan, cancellation.host, options, engineIdentity), cancellation.host, signal);
    if (host.engineIdentity !== engineIdentity) reject("AUDIO_ENGINE_CHANGED", "Browser execution identity changed during audio rendering");
    checkAbort(signal);
    return result;
  } catch (error) {
    if (signal?.aborted) {
      await cancellation.terminate();
      checkAbort(signal);
    }
    throw error;
  } finally { cancellation.dispose(); }
}

async function executeClassicExport(plan, host, { signal, maxPcmBytes, maxEncodedBytes = 32 * 1024 * 1024 }, engineIdentity) {
  // Capture immutable JSON execution facts before awaiting any host callback.
  plan = structuredClone(plan);
  const key = identityKey(plan.identity);
  const frames = frameCount(plan.frameCount, "scene frame count", 1);
  const range = plan.range ? { start: frameCount(plan.range.startSample, "range start"), count: frameCount(plan.range.frameCount, "range frame count", 1) } : undefined;
  if (range && (range.start > frames || range.count > frames - range.start)) reject("INVALID_AUDIO_PLAN", "Range is outside the complete scene");
  if (plan.outputFrameCount !== undefined && frameCount(plan.outputFrameCount, "selected output frame count", 1) !== (range?.count ?? frames)) reject("INVALID_AUDIO_PLAN", "Selected output length differs from the fixed range");
  const budget = new PcmBudget(maxPcmBytes);
  integer(maxEncodedBytes, "encoded input budget", 1);
  if (!Array.isArray(plan.resources) || !Array.isArray(plan.voices) || plan.resources.length > AUDIO_EXECUTION_LIMITS.resources || plan.voices.length > AUDIO_EXECUTION_LIMITS.voices) reject("INVALID_AUDIO_PLAN", "Bounded resource and voice lists are required");
  const resources = new Map(plan.resources.map(r => [r.assetId, r]));
  const order = plan.voices.map(v => v.elementId);
  if (resources.size !== plan.resources.length || new Set(order).size !== order.length || plan.voices.some(v => typeof v.elementId !== "string" || !v.elementId || !resources.has(v.assetId))) reject("INVALID_AUDIO_PLAN", "Resource and voice identities must be unique and bound");
  const releases = [];
  const decoded = new Map(), sources = new Map(), preparation = [], decoders = [];
  const context = host.createAudioContext(plan.identity.sampleRate);
  let failure;
  try {
    if (context.sampleRate !== plan.identity.sampleRate) reject("UNSUPPORTED_AUDIO_CAPABILITY", "Browser does not support the exact requested sample rate");
    releases.push(budget.reserve(pcmBytes(frames, 2)));
    const output = context.createBuffer(2, frames, context.sampleRate);
    for (const voice of plan.voices) {
      checkAbort(signal);
      if (!decoded.has(voice.assetId)) {
        const resource = resources.get(voice.assetId);
        const releaseEncoded = budget.reserve(integer(resource.size * 3, "encoded working bytes", 1, maxEncodedBytes * 3));
        try {
          const bytes = await authorizedBytes(resource, host, signal, maxEncodedBytes);
          const result = await decodeResource(resource, bytes, context, host, budget, signal);
          decoded.set(voice.assetId, result.buffer); releases.push(result.release);
          decoders.push({ assetId: voice.assetId, profile: result.profile, sampleRate: result.buffer.sampleRate, frameCount: String(result.buffer.length), channels: result.buffer.numberOfChannels, primary: result.primary });
        } finally { releaseEncoded(); }
      }
      const prepared = await prepareVoice(voice, decoded.get(voice.assetId), context, host, budget, signal);
      sources.set(voice.elementId, prepared.buffer); releases.push(prepared.release);
      preparation.push({ elementId: voice.elementId, facts: prepared.facts });
    }
    const sourceFormats = plan.voices.map(v => ({ elementId: v.elementId, sampleRate: sources.get(v.elementId).sampleRate, frameCount: String(sources.get(v.elementId).length), channels: sources.get(v.elementId).numberOfChannels, prepared: preparation.find(p => p.elementId === v.elementId).facts.mode !== "unprepared-linear" }));
    for (let start = 0; start < frames;) {
      checkAbort(signal);
      const requestedCount = Math.min(AUDIO_EXECUTION_LIMITS.blockFrames, frames - start);
      const block = await cancellable(host.compileBlock({ startSample: String(start), frameCount: requestedCount, sourceFormats: structuredClone(sourceFormats) }), host, signal);
      if (identityKey(block?.identity) !== key) reject("REVISION_CONFLICT", "Compiled block changed the fixed audio identity");
      // Rust may shorten the requested interval to admit its union/work budget.
      // Advance only after this exact contiguous block has been validated/mixed.
      const count = integer(block.frameCount, "compiled block frames", 1, requestedCount);
      mixCompiledBlock(output, sources, order, block, start, count);
      start += count;
    }
    const mastered = await masterWholeScene(output, host, budget, signal);
    releases.push(mastered.release);
    let result = mastered.buffer;
    if (range) {
      const { start, count } = range;
      releases.push(budget.reserve(pcmBytes(count, 2)));
      result = context.createBuffer(2, count, context.sampleRate);
      for (let c = 0; c < 2; c++) result.copyToChannel(mastered.buffer.getChannelData(c).subarray(start, start + count), c);
    }
    checkAbort(signal);
    return { buffer: result, receipt: { identity: plan.identity, engineIdentity, decoders, preparation, mastering: { profile: "classic-export-whole-scene-web-audio-v31", wholeSceneFrames: plan.frameCount, inputPeak: mastered.peak, applied: mastered.applied }, range: plan.range ?? null, peakAccountedBytes: budget.peak } };
  } catch (error) { failure = error; throw error; }
  finally {
    sources.clear(); decoded.clear();
    for (const release of releases) release();
    try {
      if (context.state !== "closed") await cancellable(context.close(), host, signal?.aborted ? undefined : signal);
    } catch (error) { if (!failure) throw error; }
  }
}
