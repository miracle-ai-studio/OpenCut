import { AUDIO_EXECUTION_LIMITS, integer, natural, reject } from "../src/validation.mjs";
import { PROTOCOL_LIMITS } from "./protocol.mjs";

const invalid = () => reject("INVALID_HOST_CALLBACK", "Preview source binding differs from the fixed plan");
function fields(value, names) {
  if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).length !== names.length || names.some(name => !Object.hasOwn(value, name))) invalid();
}
function fixedVoices(plan) {
  if (!Array.isArray(plan?.voices) || plan.voices.length > AUDIO_EXECUTION_LIMITS.voices || !Array.isArray(plan.resources) || plan.resources.length > AUDIO_EXECUTION_LIMITS.resources) invalid();
  const assets = new Set(plan.resources.map(r => r.assetId)), ids = new Set();
  if (assets.size !== plan.resources.length) invalid();
  for (const voice of plan.voices) {
    if (typeof voice.elementId !== "string" || !voice.elementId || ids.has(voice.elementId) || !assets.has(voice.assetId) || !["preserve-pts", "prepared-concat"].includes(voice.sourceMode)) invalid();
    ids.add(voice.elementId);
  }
  return plan.voices;
}
function segments(value) {
  if (!Array.isArray(value) || !value.length || value.length > 65536) invalid();
  const ids = new Set();
  return value.map(segment => {
    fields(segment, ["segmentId", "timestampSeconds", "frameCount"]);
    integer(segment.segmentId, "preview segment ID", 0, 0xffffffff);
    if (ids.has(segment.segmentId) || !Number.isFinite(segment.timestampSeconds) || natural(segment.frameCount, "preview segment frames") === 0n) invalid();
    ids.add(segment.segmentId);
    return { segmentId: segment.segmentId, timestampSeconds: segment.timestampSeconds, frameCount: segment.frameCount };
  });
}
function sameSegments(left, right) {
  return left.length === right.length && left.every((s, i) => s.segmentId === right[i].segmentId && s.timestampSeconds === right[i].timestampSeconds && s.frameCount === right[i].frameCount);
}

// Lossless Node-only transport projection. Source clocks and all PTS validity
// remain authoritative in Rust; this performs no timeline or gain arithmetic.
export function packPreviewBinding(plan, formats) {
  const voices = fixedVoices(plan);
  if (!Array.isArray(formats) || formats.length !== voices.length) invalid();
  const ordinary = new Map(), ordinaryFormats = new Map();
  let expandedBytes = 2; // Full sourceFormats array brackets, before copies/hash.
  const sourceFormats = formats.map((format, i) => {
    fields(format, ["elementId", "sampleRate", "frameCount", "channels", "prepared", "segments"]);
    const formatBytes = Buffer.byteLength(JSON.stringify(format)) + (i ? 1 : 0);
    if (formatBytes > PROTOCOL_LIMITS.replyBytes - expandedBytes) reject("AUDIO_BUDGET_EXCEEDED", "Expanded preview source facts exceed 16 MiB");
    expandedBytes += formatBytes;
    const voice = voices[i];
    if (format.elementId !== voice.elementId || typeof format.prepared !== "boolean" || format.prepared !== (voice.sourceMode === "prepared-concat")) invalid();
    integer(format.sampleRate, "preview source rate", 1, 384000);
    integer(format.channels, "preview source channels", 1, 2);
    if (natural(format.frameCount, "preview source frames") === 0n) invalid();
    let sourceRef = null;
    if (format.prepared) {
      if (!Array.isArray(format.segments) || format.segments.length) invalid();
    } else {
      sourceRef = voice.assetId;
      const facts = segments(format.segments), prior = ordinary.get(sourceRef);
      if (prior) {
        const old = ordinaryFormats.get(sourceRef);
        if (!sameSegments(prior.segments, facts) || old.sampleRate !== format.sampleRate || old.frameCount !== format.frameCount || old.channels !== format.channels) invalid();
      } else {
        ordinary.set(sourceRef, { assetId: sourceRef, segments: facts });
        ordinaryFormats.set(sourceRef, format);
      }
    }
    return { elementId: format.elementId, sampleRate: format.sampleRate, frameCount: format.frameCount, channels: format.channels, prepared: format.prepared, sourceRef };
  });
  return { schemaVersion: 1, ordinarySources: [...ordinary.values()], sourceFormats };
}

// Reverse projection is used to verify the transport is lossless. Native uses
// its independently typed equivalent before existing application validation.
export function expandPreviewBinding(plan, binding) {
  const voices = fixedVoices(plan);
  fields(binding, ["schemaVersion", "ordinarySources", "sourceFormats"]);
  if (binding.schemaVersion !== 1 || !Array.isArray(binding.ordinarySources) || binding.ordinarySources.length > AUDIO_EXECUTION_LIMITS.resources || !Array.isArray(binding.sourceFormats) || binding.sourceFormats.length !== voices.length) invalid();
  const table = new Map();
  for (const source of binding.ordinarySources) {
    fields(source, ["assetId", "segments"]);
    if (typeof source.assetId !== "string" || !source.assetId || table.has(source.assetId)) invalid();
    table.set(source.assetId, segments(source.segments));
  }
  const formats = binding.sourceFormats.map((format, i) => {
    fields(format, ["elementId", "sampleRate", "frameCount", "channels", "prepared", "sourceRef"]);
    const voice = voices[i];
    if (format.prepared ? format.sourceRef !== null : format.sourceRef !== voice.assetId || !table.has(format.sourceRef)) invalid();
    return { elementId: format.elementId, sampleRate: format.sampleRate, frameCount: format.frameCount, channels: format.channels, prepared: format.prepared, segments: format.prepared ? [] : table.get(format.sourceRef) };
  });
  const packed = packPreviewBinding(plan, formats);
  if (packed.ordinarySources.length !== binding.ordinarySources.length || packed.ordinarySources.some((source, i) => source.assetId !== binding.ordinarySources[i].assetId)) invalid();
  return formats;
}

export function compactPreviewReceipt(plan, receipt, acceptedBinding) {
  if (!receipt || Object.hasOwn(receipt, "sourceBinding")) invalid();
  const sourceBinding = packPreviewBinding(plan, receipt.sourceFormats);
  // Both factory outputs have fixed field order; compare their transport facts,
  // not the spelling of the original browser or Rust object keys.
  if (!acceptedBinding || JSON.stringify(sourceBinding) !== JSON.stringify(packPreviewBinding(plan, expandPreviewBinding(plan, acceptedBinding)))) invalid();
  const result = { ...receipt, sourceBinding };
  delete result.sourceFormats;
  return result;
}
