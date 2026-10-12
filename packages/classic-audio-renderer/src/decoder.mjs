import { checkAbort, decodedCount, integer, pcmBytes, reject, validateBuffer } from "./validation.mjs";
import { cancellable } from "./runtime.mjs";
import { guardIsoEditLists } from "./iso-edit-list.mjs";

export function capturePrimaryAudioTrack(track) {
  const primary = Object.freeze({
    trackId: integer(track.id, "primary audio track ID", 0, 0xffffffff),
    codec: track.codec,
    sampleRate: integer(track.sampleRate, "primary audio sample rate", 1, 384000),
    channels: integer(track.numberOfChannels, "primary audio channels", 1, 64),
  });
  if (typeof primary.codec !== "string" || !/^[a-z0-9][a-z0-9.-]{0,127}$/.test(primary.codec)) reject("UNSUPPORTED_AUDIO_CAPABILITY", "The primary audio track codec is unsupported");
  return primary;
}

async function decodeVideo(track, AudioBufferSink, primary, context, host, admission, signal) {
  let node;
  try {
    const chunks = [];
    let total = 0;
    for await (const { buffer } of new AudioBufferSink(track).buffers(0)) {
      checkAbort(signal); validateBuffer(buffer, admission);
      if (buffer.sampleRate !== primary.sampleRate || buffer.numberOfChannels !== primary.channels) reject("INVALID_DECODED_PCM", "Decoded chunks changed the admitted primary audio format");
      total = decodedCount(total + buffer.length, "video decoded frames", admission.maxFrames);
      chunks.push(buffer);
    }
    if (!chunks.length) reject("EMPTY_DECODED_AUDIO", "The authorized video decoded no audio");
    const sampleRate = chunks[0].sampleRate;
    const channels = Math.min(2, chunks[0].numberOfChannels);
    // Decoder resampling follows the Classic browser algorithm; this is not a
    // timeline placement or authored duration calculation. Timestamp gaps are
    // deliberately concatenated, just as Classic AudioBufferSink export does.
    const outputFrames = decodedCount(Math.ceil(total * (context.sampleRate / sampleRate)), "video resampled frames", admission.maxFrames);
    const offline = host.createOfflineContext(channels, outputFrames, context.sampleRate);
    const native = context.createBuffer(channels, total, sampleRate);
    for (let c = 0; c < channels; c++) {
      const combined = new Float32Array(total);
      let offset = 0;
      for (const chunk of chunks) {
        combined.set(chunk.getChannelData(Math.min(c, chunk.numberOfChannels - 1)), offset);
        offset += chunk.length;
      }
      native.copyToChannel(combined, c);
    }
    node = offline.createBufferSource(); node.buffer = native;
    node.connect(offline.destination); node.start(0);
    return await cancellable(offline.startRendering(), host, signal);
  } finally { node?.disconnect(); }
}

export async function decodeResource(resource, bytes, context, host, budget, signal) {
  let input, track, AudioBufferSink, primary = null, releaseUpper;
  try {
    checkAbort(signal);
    if (resource.kind === "video") {
      guardIsoEditLists(bytes);
      const media = await import("mediabunny");
      checkAbort(signal);
      input = new media.Input({ source: new media.BlobSource(new Blob([bytes])), formats: media.ALL_FORMATS });
      track = await cancellable(input.getPrimaryAudioTrack(), host, signal);
      if (!track) reject("KNOWN_ABSENT_AUDIO", "The authorized video has no primary audio track");
      primary = capturePrimaryAudioTrack(track);
      AudioBufferSink = media.AudioBufferSink;
    }
    // Metadata inspection uses the same Input/track that will decode. The host
    // admits its real native channel layout before any PCM buffer/sink exists.
    const admission = await cancellable(host.admitDecode(resource, primary), host, signal);
    integer(admission?.maxFrames, "decoder admission frames", 1);
    integer(admission?.maxChannels, "decoder admission channels", 1, 64);
    if (primary && primary.channels > admission.maxChannels) reject("AUDIO_BUDGET_EXCEEDED", "The selected primary channel layout exceeds decoder admission");
    // Browser codec allocation cannot be bounded by JavaScript alone. Host
    // admission must apply a memory/time quota to the isolated execution realm.
    // Reserve native chunks/copies plus output before invoking the codec.
    releaseUpper = budget.reserve(pcmBytes(admission.maxFrames, admission.maxChannels) * 4);
    checkAbort(signal);
    const buffer = resource.kind === "audio"
      ? await cancellable(context.decodeAudioData(bytes.slice().buffer), host, signal)
      : await cancellable(decodeVideo(track, AudioBufferSink, primary, context, host, admission, signal), host, signal);
    validateBuffer(buffer, admission);
    if (buffer.sampleRate !== context.sampleRate) reject("INVALID_DECODED_PCM", "The browser decoder did not produce the requested target sample rate");
    releaseUpper();
    const release = budget.reserve(pcmBytes(buffer.length, buffer.numberOfChannels));
    return { buffer, release, primary, profile: resource.kind === "audio" ? "web-audio-decode-at-target-rate" : "mediabunny-1.41.0-concatenate-offline-resample" };
  } finally { releaseUpper?.(); input?.dispose(); }
}
