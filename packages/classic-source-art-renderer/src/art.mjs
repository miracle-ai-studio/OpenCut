import { isVerifiedRangeSource } from "@opencut/classic-audio-renderer/verified-range-source";
import { requireFact, positive, checkAbort, SourceArtError } from "./validation.mjs";
import { consumeConcatenatedPcm } from "@opencut/classic-audio-renderer/streamed-pcm";
import { imageDimensions } from "./image-header.mjs";

export function thumbnailSize(width, height) {
  positive(width, 0xffffffff); positive(height, 0xffffffff);
  const aspect = width / height;
  let targetWidth = width, targetHeight = height;
  if (targetWidth > 1280) { targetWidth = 1280; targetHeight = Math.round(targetWidth / aspect); }
  if (targetHeight > 720) { targetHeight = 720; targetWidth = Math.round(targetHeight * aspect); }
  positive(targetWidth, 1280); positive(targetHeight, 720);
  return { width: targetWidth, height: targetHeight };
}
export function peakBuckets(buffer, maxPcmBytes, signal) {
  positive(buffer.sampleRate, 384000); positive(buffer.numberOfChannels, 64); requireFact(Number.isSafeInteger(buffer.length) && buffer.length >= 0);
  if (buffer.length * buffer.numberOfChannels * 4 > maxPcmBytes) throw new SourceArtError("SOURCE_ART_BUDGET_EXCEEDED", "Decoded source exceeds PCM budget");
  const peaks = new Float32Array(Math.max(1, Math.ceil(buffer.length / 128)));
  for (let c = 0; c < buffer.numberOfChannels; c++) {
    const data = buffer.getChannelData(c); requireFact(data.length === buffer.length);
    for (let start = 0; start < data.length; start += 128) {
      checkAbort(signal); let peak = peaks[start / 128];
      for (let n = start; n < Math.min(start + 128, data.length); n++) {
        requireFact(Number.isFinite(data[n]), "Non-finite decoded source sample");
        peak = Math.max(peak, Math.abs(data[n]));
      }
      peaks[start / 128] = peak;
    }
  }
  return peaks;
}
export function admitPixels(width, height, encodedBytes, admission) {
  return admitResidentPixels(width, height, encodedBytes * 3, admission);
}
function admitResidentPixels(width, height, encodedResidentBytes, admission) {
  const output = thumbnailSize(width, height);
  const bytes = width * height * 16 + output.width * output.height * 4 + encodedResidentBytes + admission.maxJpegBytes;
  if (!Number.isSafeInteger(bytes) || bytes > admission.maxPcmBytes) throw new SourceArtError("SOURCE_ART_BUDGET_EXCEEDED", "Decoded image exceeds source-art memory budget");
  return output;
}
async function jpeg(width, height, draw, maxBytes, signal) {
  const size = thumbnailSize(width, height), canvas = document.createElement("canvas");
  canvas.width = size.width; canvas.height = size.height;
  const context = canvas.getContext("2d"); requireFact(context, "Canvas 2D unavailable");
  draw(context, size.width, size.height); checkAbort(signal);
  const blob = await new Promise(resolve => canvas.toBlob(resolve, "image/jpeg", .8));
  checkAbort(signal); requireFact(blob?.type === "image/jpeg" && blob.size > 0 && blob.size <= maxBytes, "JPEG exceeds output budget");
  return { ...size, bytes: new Uint8Array(await blob.arrayBuffer()) };
}
export async function executeSourceArt(plan, host, signal) {
  const ranged = ["audio", "video"].includes(plan.resource.kind);
  const bytes = ranged ? null : await host.readResource(plan.resource); checkAbort(signal);
  if (plan.query.kind === "waveform") return executeWaveform(plan, host, signal);
  requireFact(["thumbnail", "source_frame"].includes(plan.query.kind));
  const requestedSeconds = plan.query.kind === "source_frame" ? plan.query.seconds : 1;
  requireFact(Number.isFinite(requestedSeconds) && requestedSeconds >= 0);
  if (plan.resource.kind === "image") {
    const dimensions = imageDimensions(bytes); admitPixels(dimensions.width, dimensions.height, bytes.length, plan.admission);
    const url = URL.createObjectURL(new Blob([bytes]));
    try {
      const image = new Image();
      await new Promise((resolve, reject) => { image.onload = resolve; image.onerror = () => reject(new SourceArtError("SOURCE_ART_DECODE_FAILED", "Image decoding failed")); image.src = url; });
      checkAbort(signal); requireFact(image.naturalWidth === dimensions.width && image.naturalHeight === dimensions.height, "Actual image dimensions differ from header");
      const result = await jpeg(image.naturalWidth, image.naturalHeight, (ctx, w, h) => ctx.drawImage(image, 0, 0, w, h), plan.admission.maxJpegBytes, signal);
      await host.emit("thumbnail", result.bytes);
      return { kind: "thumbnail_ready", width: result.width, height: result.height, output_bytes: result.bytes.length,
        observation: { kind: "static_image", source_width: image.naturalWidth, source_height: image.naturalHeight } };
    } finally { URL.revokeObjectURL(url); }
  }
  requireFact(plan.resource.kind === "video");
  const { Input, ALL_FORMATS, VideoSampleSink } = await import("mediabunny");
  const range = await host.openRangeSource(plan.resource, plan.admission, plan.rangeCacheBytes, signal);
  let input;
  try {
    checkAbort(signal);
    requireFact(isVerifiedRangeSource(range) && ["assetId","contentHash","size","kind"].every(key=>range.resource[key]===plan.resource[key]),
      "Source-art video requires its fixed verified bounded source");
    input = new Input({ source: range.source, formats: ALL_FORMATS });
    const track = await input.getPrimaryVideoTrack(); checkAbort(signal);
    if (!track || !await track.canDecode()) throw new SourceArtError("SOURCE_ART_DECODE_FAILED", "Primary video cannot be decoded");
    requireFact(Number.isSafeInteger(track.id) && track.id >= 0 && track.id <= 0xffffffff); positive(track.displayWidth, 0xffffffff); positive(track.displayHeight, 0xffffffff);
    const observation = { kind: "video", track_id: track.id, source_width: track.displayWidth, source_height: track.displayHeight,
      requested_seconds: requestedSeconds, actual_timestamp_seconds: null, frame_duration_seconds: null };
    // Preserve the original surface/JPEG admission; replace full-file copies
    // with actual bounded cache/workers/transport residency only.
    admitResidentPixels(track.displayWidth, track.displayHeight, range.residentBytes, plan.admission);
    const frame = await new VideoSampleSink(track).getSample(requestedSeconds); checkAbort(signal);
    if (!frame) return { kind: "thumbnail_no_frame", output_bytes: 0, observation, reason: "no_sample_at_one_second" };
    try {
      requireFact(Number.isFinite(frame.timestamp) && Number.isFinite(frame.duration) && frame.duration > 0, "Invalid observed video sample");
      observation.actual_timestamp_seconds = frame.timestamp; observation.frame_duration_seconds = frame.duration;
      const result = await jpeg(track.displayWidth, track.displayHeight, (ctx, w, h) => frame.draw(ctx, 0, 0, w, h), plan.admission.maxJpegBytes, signal);
      await host.emit("thumbnail", result.bytes);
      return { kind: "thumbnail_ready", width: result.width, height: result.height, output_bytes: result.bytes.length, observation };
    } finally { frame.close(); }
  } finally { try { input?.dispose(); } finally { if (isVerifiedRangeSource(range)) await range.close(); } }
}

async function executeWaveform(plan, host, signal) {
  requireFact([44100, 48000].includes(plan.query.sampleRate));
  const { Input, AudioBufferSink, ALL_FORMATS } = await import("mediabunny");
  const range = await host.openRangeSource(plan.resource, plan.admission, plan.rangeCacheBytes, signal);
  let input;
  try {
    checkAbort(signal);
    requireFact(isVerifiedRangeSource(range) && ["assetId", "contentHash", "size", "kind"].every(key => range.resource[key] === plan.resource[key]),
      "Source waveform requires its fixed verified bounded source");
    const workingBytes = Math.floor((plan.admission.maxPcmBytes - range.residentBytes) / 2);
    requireFact(workingBytes >= 65536 && workingBytes === plan.waveformWorkingBytes && plan.maxWaveformBytes === Math.floor(workingBytes / 12) * 4);
    input = new Input({ source: range.source, formats: ALL_FORMATS });
    const track = await input.getPrimaryAudioTrack(); checkAbort(signal);
    if (!track || !await track.canDecode()) throw new SourceArtError("NO_AUDIO_TRACK", "Source has no decodable primary audio track");
    const primary = { sampleRate: positive(track.sampleRate, 384000), channels: positive(track.numberOfChannels, 64) };
    // The pinned decoder adapts its queue between8 and40 packets. Reserve64
    // maximum chunks for that queue, current source copies and resampling guards.
    const maxChunkFrames = Math.min(65536, Math.floor(workingBytes / (64 * primary.channels * 4)));
    if (!maxChunkFrames) throw new SourceArtError("SOURCE_ART_BUDGET_EXCEEDED", "No waveform decoder chunk fits resident admission");
    const context = host.createOfflineContext(primary.channels, 1, plan.query.sampleRate);
    const packed = new Uint8Array(16384), view = new DataView(packed.buffer);
    let pending = 0, outputBytes = 0, total = 0;
    async function flush() {
      if (!pending) return;
      await host.emit("waveform", packed.subarray(0, pending)); checkAbort(signal);
      outputBytes += pending; pending = 0;
    }
    async function consume(buffer, offset) {
      requireFact(offset === total && offset % 128 === 0, "Waveform blocks changed the source bucket clock");
      const peaks = peakBuckets(buffer, workingBytes, signal);
      if (outputBytes + pending + peaks.byteLength > plan.maxWaveformBytes)
        throw new SourceArtError("SOURCE_ART_BUDGET_EXCEEDED", "Waveform summary exceeds retained memory admission");
      for (const peak of peaks) {
        if (pending === packed.length) await flush();
        view.setFloat32(pending, peak, true); pending += 4;
      }
      total += buffer.length;
    }
    const observed = await consumeConcatenatedPcm(new AudioBufferSink(track).buffers(0), primary, plan.query.sampleRate,
      context, host, signal, { maxChunkFrames, maxResidentFrames: maxChunkFrames * 16, consume });
    if (!total) { view.setFloat32(0, 0, true); pending = 4; }
    await flush();
    requireFact(total === observed.frameCount && outputBytes === Math.max(1, Math.ceil(total / 128)) * 4,
      "Waveform summary differs from actual decoded EOF");
    return { kind: "waveform", sample_rate: observed.sampleRate, channels: observed.channels,
      total_samples: String(total), bucket_size: 128, output_bytes: outputBytes };
  } catch (error) {
    if (error?.code === "AUDIO_BUDGET_EXCEEDED")
      throw new SourceArtError("SOURCE_ART_BUDGET_EXCEEDED", "Streamed waveform PCM exceeds resident admission");
    throw error;
  } finally { try { input?.dispose(); } finally { if (isVerifiedRangeSource(range)) await range.close(); } }
}
