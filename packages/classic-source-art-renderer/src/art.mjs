import { requireFact, positive, checkAbort, SourceArtError } from "./validation.mjs";
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
  const output = thumbnailSize(width, height);
  const bytes = width * height * 16 + output.width * output.height * 4 + encodedBytes * 3 + admission.maxJpegBytes;
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
  const bytes = await host.readResource(plan.resource); checkAbort(signal);
  if (plan.query.kind === "waveform") {
    requireFact([44100, 48000].includes(plan.query.sampleRate));
    const { Input, BlobSource, ALL_FORMATS } = await import("mediabunny");
    const probe = new Input({ source: new BlobSource(new Blob([bytes])), formats: ALL_FORMATS });
    try {
      const track = await probe.getPrimaryAudioTrack(); checkAbort(signal);
      if (!track) throw new SourceArtError("NO_AUDIO_TRACK", "Source has no decodable audio track");
      const duration = await track.computeDuration(); checkAbort(signal);
      positive(track.numberOfChannels, 64);
      const expected = Math.ceil(duration * plan.query.sampleRate) * track.numberOfChannels * 4;
      if (!Number.isFinite(duration) || duration < 0 || !Number.isSafeInteger(expected) || expected > plan.admission.maxPcmBytes) throw new SourceArtError("SOURCE_ART_BUDGET_EXCEEDED", "Audio decode exceeds source-art memory budget");
    } finally { probe.dispose(); }
    const context = new AudioContext({ sampleRate: plan.query.sampleRate });
    try {
      requireFact(context.sampleRate === plan.query.sampleRate, "AudioContext rate changed");
      const buffer = await context.decodeAudioData(bytes.buffer); checkAbort(signal);
      requireFact(buffer.sampleRate === plan.query.sampleRate, "Decoded rate changed");
      const peaks = peakBuckets(buffer, plan.admission.maxPcmBytes, signal);
      const packed = new Uint8Array(peaks.length * 4), view = new DataView(packed.buffer);
      for (let i = 0; i < peaks.length; i++) view.setFloat32(i * 4, peaks[i], true);
      await host.emit("waveform", packed); checkAbort(signal);
      return { kind: "waveform", sample_rate: buffer.sampleRate, channels: buffer.numberOfChannels,
        total_samples: String(buffer.length), bucket_size: 128, output_bytes: packed.length };
    } finally { await context.close(); }
  }
  requireFact(plan.query.kind === "thumbnail");
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
  const { Input, BlobSource, ALL_FORMATS, VideoSampleSink } = await import("mediabunny");
  const input = new Input({ source: new BlobSource(new Blob([bytes])), formats: ALL_FORMATS });
  try {
    const track = await input.getPrimaryVideoTrack(); checkAbort(signal);
    if (!track || !await track.canDecode()) throw new SourceArtError("SOURCE_ART_DECODE_FAILED", "Primary video cannot be decoded");
    requireFact(Number.isSafeInteger(track.id) && track.id >= 0 && track.id <= 0xffffffff); positive(track.displayWidth, 0xffffffff); positive(track.displayHeight, 0xffffffff);
    const observation = { kind: "video", track_id: track.id, source_width: track.displayWidth, source_height: track.displayHeight,
      requested_seconds: 1, actual_timestamp_seconds: null, frame_duration_seconds: null };
    admitPixels(track.displayWidth, track.displayHeight, bytes.length, plan.admission);
    const frame = await new VideoSampleSink(track).getSample(1); checkAbort(signal);
    if (!frame) return { kind: "thumbnail_no_frame", output_bytes: 0, observation, reason: "no_sample_at_one_second" };
    try {
      requireFact(Number.isFinite(frame.timestamp) && Number.isFinite(frame.duration) && frame.duration > 0, "Invalid observed video sample");
      observation.actual_timestamp_seconds = frame.timestamp; observation.frame_duration_seconds = frame.duration;
      const result = await jpeg(track.displayWidth, track.displayHeight, (ctx, w, h) => frame.draw(ctx, 0, 0, w, h), plan.admission.maxJpegBytes, signal);
      await host.emit("thumbnail", result.bytes);
      return { kind: "thumbnail_ready", width: result.width, height: result.height, output_bytes: result.bytes.length, observation };
    } finally { frame.close(); }
  } finally { input.dispose(); }
}
