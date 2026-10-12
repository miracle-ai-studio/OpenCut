import { capturePrimaryAudioTrack } from "./decoder.mjs";
import { guardIsoEditLists } from "./iso-edit-list.mjs";
import { checkAbort, decodedCount, integer, pcmBytes, reject, validateBuffer } from "./validation.mjs";
import { cancellable } from "./runtime.mjs";

// Both uploaded audio and video use the same selected container track for
// realtime. Segment timestamps are observations; Rust alone maps their clock.
export async function decodePreviewResource(resource, bytes, context, host, budget, signal) {
  guardIsoEditLists(bytes);
  const { Input, BlobSource, ALL_FORMATS, AudioBufferSink } = await import("mediabunny");
  checkAbort(signal);
  const input = new Input({ source: new BlobSource(new Blob([bytes])), formats: ALL_FORMATS });
  let reservation;
  try {
    const track = await cancellable(input.getPrimaryAudioTrack(), host, signal);
    if (!track) reject("KNOWN_ABSENT_AUDIO", "The fixed resource has no primary audio track");
    const primary = capturePrimaryAudioTrack(track);
    const admission = await cancellable(host.admitDecode(resource, primary), host, signal);
    integer(admission.maxFrames, "preview decode frames", 1);
    integer(admission.maxChannels, "preview decode channels", 1, 64);
    if (primary.channels > admission.maxChannels) reject("AUDIO_BUDGET_EXCEEDED", "Primary channels exceed preview admission");
    reservation = budget.reserve(pcmBytes(admission.maxFrames, admission.maxChannels) * 4);
    const segments = new Map(), facts = []; let total = 0;
    const channels = Math.min(2, primary.channels);
    for await (const { buffer, timestamp } of new AudioBufferSink(track).buffers(0)) {
      checkAbort(signal); validateBuffer(buffer, admission);
      if (buffer.sampleRate !== primary.sampleRate || buffer.numberOfChannels !== primary.channels || !Number.isFinite(timestamp)) reject("INVALID_DECODED_PCM", "Primary preview format or PTS changed");
      if (segments.size >= 65536) reject("AUDIO_BUDGET_EXCEEDED", "Preview segment count exceeds admission");
      total = decodedCount(total + buffer.length, "preview decoded frames", admission.maxFrames);
      const id = segments.size, copy = context.createBuffer(channels, buffer.length, primary.sampleRate);
      for (let c = 0; c < channels; c++) copy.copyToChannel(buffer.getChannelData(c), c);
      segments.set(id, copy);
      facts.push({ segmentId: id, timestampSeconds: timestamp, frameCount: String(buffer.length) });
    }
    if (!total) reject("EMPTY_DECODED_AUDIO", "The fixed primary track produced no PCM");
    reservation();
    const release = budget.reserve(pcmBytes(total, channels));
    return { primary, sampleRate: primary.sampleRate, channels, frameCount: String(total), segments, facts, release };
  } finally { reservation?.(); input.dispose(); }
}

export async function concatenatePreviewSource(source, context, host, budget, signal) {
  const total = Number(source.frameCount);
  // This is the same decoder resample as Classic's prepared branch, not a
  // timeline conversion. Authored source/placement arithmetic stays in Rust.
  const outputFrames = Math.ceil(total * context.sampleRate / source.sampleRate);
  const release = budget.reserve(pcmBytes(total + outputFrames, source.channels));
  let node;
  try {
    const offline = host.createOfflineContext(source.channels, outputFrames, context.sampleRate);
    const native = offline.createBuffer(source.channels, total, source.sampleRate);
    for (let c = 0; c < source.channels; c++) {
      let offset = 0;
      for (const buffer of source.segments.values()) { native.copyToChannel(buffer.getChannelData(c), c, offset); offset += buffer.length; }
    }
    node = offline.createBufferSource(); node.buffer = native; node.connect(offline.destination); node.start(0);
    const buffer = await cancellable(offline.startRendering(), host, signal);
    return { buffer, release };
  } catch (error) { release(); throw error; }
  finally { node?.disconnect(); }
}
