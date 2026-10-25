/** Runs inside the isolated browser; every domain fact comes from Rust. */
export async function executeAudioPage({ plan, engineIdentity, maximumEncodedBytes }) {
  try {
  const controller = new AbortController();
  const request = async (op, args) => {
    const value = await globalThis.__opencutAudioCallback(op, args);
    if (op !== "compileBlock" && op !== "compileSourcePositions") return value;
    try {
      if (typeof value !== "string") throw new Error();
      const facts = JSON.parse(value);
      if (op === "compileBlock") {
        const { unpackExportBlock } = await import("/renderer/src/preview-packet.mjs");
        return unpackExportBlock(facts);
      }
      return facts;
    } catch {
      const error = new Error("Invalid compiled audio facts JSON"); error.code = "AUDIO_PROTOCOL_ERROR"; throw error;
    }
  };
  const host = {
    engineIdentity,
    async resolveVerifiedBytes(resource) {
      const transfer = await request("resolveVerifiedBytes", { resource });
      if (typeof transfer.transferId !== "string" || !/^(0|[1-9][0-9]*)$/.test(transfer.byteLength) || BigInt(transfer.byteLength) !== BigInt(resource.size) || resource.size > maximumEncodedBytes) throw new Error("Invalid authorized byte transfer");
      const bytes = new Uint8Array(resource.size);
      try {
        for (let offset = 0; offset < bytes.length;) {
          const count = Math.min(65536, bytes.length - offset);
          const reply = await request("readVerifiedBytes", { transferId: transfer.transferId, offset: String(offset), count });
          if (reply.offset !== String(offset) || typeof reply.bytesHex !== "string" || reply.bytesHex.length !== count * 2 || !/^(?:[0-9a-f]{2})*$/.test(reply.bytesHex) || reply.eof !== (offset + count === bytes.length)) throw new Error("Invalid authorized byte chunk");
          for (let i = 0; i < count; i++) bytes[offset + i] = Number.parseInt(reply.bytesHex.slice(i * 2, i * 2 + 2), 16);
          offset += count;
        }
        return bytes;
      } finally { await request("releaseVerifiedBytes", { transferId: transfer.transferId }); }
    },
    async resolveRangeSource(resource, budget, signal) {
      const { openVerifiedRangeSource } = await import("@opencut/classic-audio-renderer/verified-range-source");
      return openVerifiedRangeSource(resource, {
        open: resource => request("openVerifiedRange", { resource }),
        read: (transferId, offset, count) => request("readVerifiedBytes", { transferId, offset: String(offset), count }),
        release: transferId => request("releaseVerifiedBytes", { transferId }),
      }, budget, plan.encodedSource.cacheBytes, signal);
    },
    admitDecode: (resource, primary) => request("admitDecode", { resource, primary }),
    releaseDecodedSource: resource => request("releaseDecodedSource", { resource }),
    compilePitchPreparation: args => request("compilePitchPreparation", args),
    compileSourcePositions: args => request("compileSourcePositions", args),
    compileBlock: args => request("compileBlock", args),
    createAudioContext: sampleRate => new AudioContext({ sampleRate }),
    createOfflineContext: (channels, frames, sampleRate) => new OfflineAudioContext(channels, frames, sampleRate),
    abortRendering: () => globalThis.__opencutTerminateAudioRealm(),
  };
  {
    host.admitContainerDecode = (resource, primary) => request("admitContainerDecode", { resource, primary });
  }
  host.admitPcmAudioDecode = (resource, primary) => request("admitPcmAudioDecode", { resource, primary });
  const { renderClassicExport } = await import("/renderer/src/index.mjs");
  const result = await renderClassicExport(plan, host, { maxPcmBytes: globalThis.__opencutAudioAdmission.maxPcmBytes, maxEncodedBytes: maximumEncodedBytes, signal: controller.signal });
  if (result.buffer.numberOfChannels !== 2) throw new Error("Compiled stereo format changed");
  for (let channel = 0; channel < 2; channel++) {
    const samples = result.buffer.getChannelData(channel);
    for (let offset = 0; offset < samples.length; offset += 4096) {
      const count = Math.min(4096, samples.length - offset);
      const bytes = new Uint8Array(count * 4), view = new DataView(bytes.buffer);
      for (let i = 0; i < count; i++) {
        if (!Number.isFinite(samples[offset + i])) throw new Error("Non-finite output PCM");
        view.setFloat32(i * 4, samples[offset + i], true);
      }
      const bytesHex = Array.from(bytes, value => value.toString(16).padStart(2, "0")).join("");
      await globalThis.__opencutAudioPcm({ channel, offsetFrames: String(offset), frames: count, bytesHex });
    }
  }
  return { ok: true, receipt: result.receipt };
  } catch (error) {
    // Preserve typed execution codes across Playwright's value serialization.
    return { ok: false, error: { code: typeof error?.code === "string" ? error.code.slice(0, 128) : "AUDIO_BROWSER_EXECUTION_FAILED", message: "Browser audio execution failed" } };
  }
}
