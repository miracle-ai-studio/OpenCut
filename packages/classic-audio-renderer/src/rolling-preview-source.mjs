import { checkAbort, integer, pcmBytes, reject, validateBuffer } from "./validation.mjs";

export function validateRollingPreviewAdmission(admission, primary, targetRate, modes) {
  const fields = ["workingFrames", "maxChannels", "maxChunkFrames", "sourceSampleRate", "targetSampleRate"];
  if (!admission || Object.keys(admission).length !== fields.length || fields.some(key => !Object.hasOwn(admission, key))
      || !modes.needsPts || admission.sourceSampleRate !== primary.sampleRate
      || admission.targetSampleRate !== targetRate || admission.maxChannels !== primary.channels)
    reject("INVALID_AUDIO_PLAN", "Rolling admission differs from the fixed primary source");
  integer(admission.maxChannels, "rolling channels", 1, 2);
  integer(admission.workingFrames, "rolling resident frames", 1);
  integer(admission.maxChunkFrames, "rolling decoder chunk", 1, 65536);
  if (admission.workingFrames < admission.maxChunkFrames * 2)
    reject("INVALID_AUDIO_PLAN", "Rolling source omits its bounded read headroom");
}

// This owns one pinned iterator, as Classic's ordinary playback does. Rust
// supplies each source interval; this adapter observes PTS and evicts PCM only.
export function rollingPreviewSource(track, AudioBufferSink, primary, context, budget, admission, signal, closeInput) {
  const transient = budget.reserve(pcmBytes(admission.maxChunkFrames, primary.channels) * 4);
  const sink = new AudioBufferSink(track), pieces = new Map();
  let iterator, origin = 0, lastStart = 0, through = -Infinity, ended = false, closed = false, tail, previous = [];
  let resident = 0, evictedThrough = 0, lastDecodedEnd = -Infinity;
  const endOf = piece => piece.timestamp + piece.buffer.length / primary.sampleRate;
  const intersects = (piece, windows) => windows.some(w => piece.timestamp < w.endSeconds && endOf(piece) > w.startSeconds);
  const drop = key => { const piece = pieces.get(key); pieces.delete(key); resident -= piece.buffer.length; evictedThrough = Math.max(evictedThrough, endOf(piece)); piece.release(); };
  async function restart(start, windows) {
    await iterator?.return(); checkAbort(signal);
    for (const [key, piece] of pieces) if (!intersects(piece, windows)) drop(key);
    iterator = sink.buffers(start); origin = start; through = start; ended = false; tail = undefined; evictedThrough = start; lastDecodedEnd = -Infinity;
  }
  async function ensureWindows(windows) {
    if (closed) reject("AUDIO_SESSION_CLOSED", "Rolling source has closed");
    checkAbort(signal);
    if (!Array.isArray(windows) || windows.length > 32 || windows.some(w => !Number.isFinite(w.startSeconds)
        || !Number.isFinite(w.endSeconds) || w.startSeconds < 0 || w.endSeconds <= w.startSeconds))
      reject("INVALID_AUDIO_PLAN", "Invalid Rust decoder intervals");
    for (const [key, piece] of pieces) if (piece !== tail && !intersects(piece, windows)) drop(key);
    for (const window of windows) {
      checkAbort(signal);
      if (previous.some(w => w.startSeconds <= window.startSeconds && w.endSeconds >= window.endSeconds)) continue;
      const chunkSeconds = admission.maxChunkFrames / primary.sampleRate;
      if (!iterator || window.startSeconds < origin || window.startSeconds < lastStart || window.startSeconds < evictedThrough || window.startSeconds > through + chunkSeconds) await restart(window.startSeconds, windows);
      lastStart = window.startSeconds;
      while (!ended && through < window.endSeconds) {
        const item = await iterator.next(); checkAbort(signal);
        if (item.done) { ended = true; through = Infinity; break; }
        const { buffer, timestamp } = item.value;
        validateBuffer(buffer, { maxFrames: admission.maxChunkFrames, maxChannels: primary.channels });
        if (buffer.sampleRate !== primary.sampleRate || buffer.numberOfChannels !== primary.channels || !Number.isFinite(timestamp))
          reject("INVALID_DECODED_PCM", "Rolling decoder changed primary format or PTS");
        const key = `${timestamp}:${buffer.length}`, old = pieces.get(key);
        if (old) {
          for (let channel = 0; channel < primary.channels; channel++) {
            const before = old.buffer.getChannelData(channel), after = buffer.getChannelData(channel);
            for (let n = 0; n < before.length; n++) if (!Object.is(before[n], after[n]))
              reject("INVALID_DECODED_PCM", "Repeated decoder interval changed its PCM");
          }
          tail = old;
        } else {
          if (tail && !intersects(tail, windows)) {
            const oldKey = `${tail.timestamp}:${tail.buffer.length}`;
            if (pieces.has(oldKey)) drop(oldKey);
          }
          if (resident + buffer.length > admission.workingFrames || pieces.size >= 65536)
            reject("AUDIO_BUDGET_EXCEEDED", "Current decoder intervals exceed resident admission");
          const release = budget.reserve(pcmBytes(buffer.length, primary.channels));
          try {
            const copy = context.createBuffer(primary.channels, buffer.length, primary.sampleRate);
            for (let channel = 0; channel < primary.channels; channel++) copy.copyToChannel(buffer.getChannelData(channel), channel);
            tail = { timestamp, buffer: copy, release }; pieces.set(key, tail); resident += buffer.length;
          } catch (error) { release(); throw error; }
        }
        const next = timestamp + buffer.length / primary.sampleRate;
        if (!Number.isFinite(next) || next <= lastDecodedEnd) reject("INVALID_DECODED_PCM", "Rolling decoder did not advance its PTS interval");
        lastDecodedEnd = next; through = Math.max(through, next);
      }
    }
    previous = windows.map(w => ({ ...w }));
    const sorted = [...pieces.values()].sort((a, b) => a.timestamp - b.timestamp), segments = new Map(), facts = [];
    let total = 0;
    for (const piece of sorted) {
      const id = facts.length; segments.set(id, piece.buffer); total += piece.buffer.length;
      facts.push({ segmentId: id, timestampSeconds: piece.timestamp, frameCount: String(piece.buffer.length) });
    }
    return { segments, facts, frameCount: String(total) };
  }
  async function close() {
    if (closed) return; closed = true;
    try { await iterator?.return(); }
    finally { for (const key of [...pieces.keys()]) drop(key); transient(); await closeInput(); }
  }
  return { primary, sampleRate: primary.sampleRate, channels: primary.channels, frameCount: "0", sourceFrameCount: null,
    rolling: true, segments: new Map(), facts: [], release: () => {},
    async ensureWindows(windows) {
      try { return await ensureWindows(windows); }
      catch (error) { await close().catch(() => {}); throw error; }
    }, close,
  };
}
