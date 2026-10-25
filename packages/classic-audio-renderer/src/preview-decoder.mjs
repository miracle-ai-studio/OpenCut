import { capturePrimaryAudioTrack } from "./decoder.mjs";
import { guardIsoEditListsSource } from "./iso-edit-list.mjs";
import { checkAbort, decodedCount, integer, pcmBytes, reject, validateBuffer } from "./validation.mjs";
import { cancellable } from "./runtime.mjs";
import { validateWindowAdmission } from "./windowed-decoder.mjs";
import { decodeWindowedPreview } from "./preview-windowed-decoder.mjs";
import { rollingPreviewSource, validateRollingPreviewAdmission } from "./rolling-preview-source.mjs";

// Both uploaded audio and video use the same selected container track for
// realtime. Segment timestamps are observations; Rust alone maps their clock.
export async function decodePreviewResource(resource, bytes, context, host, budget, signal, modes) {
  let input, retained = false, prepared;
  try {
    await guardIsoEditListsSource(bytes);
    const { Input, ALL_FORMATS, AudioBufferSink } = await import("mediabunny");
    checkAbort(signal);
    input = new Input({ source: bytes.source, formats: ALL_FORMATS });
    const track = await cancellable(input.getPrimaryAudioTrack(), host, signal);
    if (!track) reject("KNOWN_ABSENT_AUDIO", "The fixed resource has no primary audio track");
    const primary = capturePrimaryAudioTrack(track);
    const allocation = await cancellable(host.admitPreviewDecode(resource, primary), host, signal);
    if ((!modes.needsPts && !modes.needsConcat) || !allocation || Object.keys(allocation).length !== 2 || Object.keys(allocation).some(k => !["stream", "preparation"].includes(k))
        || [allocation.stream, allocation.preparation].some(value => value !== null && (typeof value !== "object" || Array.isArray(value)))
        || !!allocation.stream !== modes.needsPts || !!allocation.preparation !== modes.needsConcat)
      reject("INVALID_AUDIO_PLAN", "Preview allocation does not cover its fixed consumers");
    if (allocation.preparation) {
      prepared = await decodePreparedPreviewSource(track, AudioBufferSink, primary, context, host, budget, allocation.preparation, signal);
    }
    if (allocation.stream) {
      validateRollingPreviewAdmission(allocation.stream, primary, context.sampleRate, modes);
      const stream = rollingPreviewSource(track, AudioBufferSink, primary, context, budget, allocation.stream, signal,
        async () => { input.dispose(); await bytes.close(); });
      retained = true;
      // Only the preparation pass knows a full concat extent. Ordinary PTS
      // windows remain transient even when they share that exact source.
      return prepared ? { ...prepared, rolling: true, sourceFrameCount: prepared.frameCount,
        ensureWindows: stream.ensureWindows, close: stream.close } : stream;
    }
    return { ...prepared, sourceFrameCount: prepared.frameCount };
  } catch (error) { prepared?.release(); throw error; }
  finally { if (!retained) { input?.dispose(); await bytes.close(); } }
}

async function decodePreparedPreviewSource(track, AudioBufferSink, primary, context, host, budget, admission, signal) {
  validateWindowAdmission(admission, primary, context.sampleRate);
  integer(admission.maxFrames, "prepared decode frames", 1);
  integer(admission.maxChannels, "prepared decode channels", 1, 64);
  if (primary.channels > admission.maxChannels) reject("AUDIO_BUDGET_EXCEEDED", "Primary channels exceed preparation admission");
  if (admission.mode === "windowed") return decodeWindowedPreview(track, AudioBufferSink, primary, context, host, budget, admission, signal,
    { needsConcat: true, needsPts: false });
  const reservation = budget.reserve(pcmBytes(admission.maxFrames, admission.maxChannels) * 4);
  const segments = new Map(); let total = 0;
  const channels = Math.min(2, primary.channels);
  try {
    for await (const { buffer, timestamp } of new AudioBufferSink(track).buffers(0)) {
      checkAbort(signal); validateBuffer(buffer, admission);
      if (buffer.sampleRate !== primary.sampleRate || buffer.numberOfChannels !== primary.channels || !Number.isFinite(timestamp))
        reject("INVALID_DECODED_PCM", "Primary preparation format or PTS changed");
      if (segments.size >= 65536) reject("AUDIO_BUDGET_EXCEEDED", "Prepared chunk metadata exceeds resident admission");
      total = decodedCount(total + buffer.length, "prepared decoded frames", admission.maxFrames);
      const copy = context.createBuffer(channels, buffer.length, primary.sampleRate);
      for (let channel = 0; channel < channels; channel++) copy.copyToChannel(buffer.getChannelData(channel), channel);
      segments.set(segments.size, copy);
    }
    if (!total) reject("EMPTY_DECODED_AUDIO", "The fixed primary track produced no PCM");
    reservation();
    const release = budget.reserve(pcmBytes(total, channels));
    return { primary, sampleRate: primary.sampleRate, channels, frameCount: String(total), segments, facts: [], release };
  } finally { reservation(); }
}

export async function concatenatePreviewSource(source, context, host, budget, signal) {
  if (source.windowedConcatenated) return { buffer: source.windowedConcatenated, release: () => {} };
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
