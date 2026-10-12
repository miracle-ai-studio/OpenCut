export async function preparePreviewPage({ plan, engineIdentity, admission }) {
  try {
    const { unpackPreviewBlockV2 } = await import("/renderer/src/preview-packet.mjs");
    // evaluate/exposeFunction serialize their return values, but discard custom
    // Error properties. Only the explicit private envelope crosses this boundary.
    const request = async (op, args) => {
      const envelope = await globalThis.__opencutPreviewCallback(op, args);
      if (envelope?.ok === true) {
        if (op === "compileSourcePositions") {
          try {
            if (typeof envelope.value !== "string") throw new Error();
            const positions = JSON.parse(envelope.value);
            if (!Array.isArray(positions) || positions.length !== args.frameCount) throw new Error();
            return positions;
          } catch {
            const error = new Error("Invalid prepared source positions JSON"); error.code = "AUDIO_PROTOCOL_ERROR"; throw error;
          }
        }
        if (op !== "compilePreviewBlock") return envelope.value;
        try {
          if (typeof envelope.value !== "string") throw new Error();
          return unpackPreviewBlockV2(JSON.parse(envelope.value));
        } catch {
          const error = new Error("Invalid compiled preview JSON"); error.code = "AUDIO_PROTOCOL_ERROR"; throw error;
        }
      }
      const code = envelope?.ok === false && typeof envelope.error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(envelope.error.code)
        ? envelope.error.code : "AUDIO_PREVIEW_FAILED";
      const error = new Error("Preview host callback failed"); error.code = code; throw error;
    };
    const host = {
      engineIdentity,
      async resolveVerifiedBytes(resource) {
        const transfer = await request("resolveVerifiedBytes", { resource });
        if (typeof transfer.transferId !== "string" || transfer.byteLength !== String(resource.size) || resource.size > admission.maxEncodedBytes) throw new Error("Invalid authorized preview transfer");
        const bytes = new Uint8Array(resource.size);
        try {
          for (let offset = 0; offset < bytes.length;) {
            const count = Math.min(65536, bytes.length - offset);
            const chunk = await request("readVerifiedBytes", { transferId: transfer.transferId, offset: String(offset), count });
            if (chunk.offset !== String(offset) || typeof chunk.bytesHex !== "string" || chunk.bytesHex.length !== count * 2 || !/^(?:[0-9a-f]{2})+$/.test(chunk.bytesHex) || chunk.eof !== (offset + count === bytes.length)) throw new Error("Invalid authorized preview chunk");
            for (let n = 0; n < count; n++) bytes[offset + n] = Number.parseInt(chunk.bytesHex.slice(n * 2, n * 2 + 2), 16);
            offset += count;
          }
          return bytes;
        } finally { await request("releaseVerifiedBytes", { transferId: transfer.transferId }); }
      },
      admitDecode: (resource, primary) => request("admitDecode", { resource, primary }),
      compilePitchPreparation: args => request("compilePitchPreparation", args),
      compileSourcePositions: args => request("compileSourcePositions", args),
      bindPreviewSources: args => request("bindPreviewSources", args),
      observePreviewDecoded: args => request("observePreviewDecoded", args),
      compilePreviewBlock: args => request("compilePreviewBlock", args),
      createAudioContext: sampleRate => new AudioContext({ sampleRate }),
      createOfflineContext: (channels, frames, sampleRate) => new OfflineAudioContext(channels, frames, sampleRate),
      abortRendering: () => globalThis.__opencutTerminatePreviewRealm(),
    };
    const { prepareClassicPreview } = await import("/renderer/src/preview.mjs");
    globalThis.__opencutPreviewProducer = await prepareClassicPreview(plan, host, { maxPcmBytes: admission.maxPcmBytes, maxEncodedBytes: admission.maxEncodedBytes });
    return { ok: true, value: globalThis.__opencutPreviewProducer.receipt };
  } catch (error) {
    return { ok: false, error: { code: typeof error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(error.code)
      ? error.code : "AUDIO_PREVIEW_FAILED" } };
  }
}

export async function nextPreviewPage(command) {
  try {
    const result = await globalThis.__opencutPreviewProducer.next(command);
    for (let channel = 0; channel < 2; channel++) {
      const samples = result.channels[channel], bytes = new Uint8Array(samples.length * 4), view = new DataView(bytes.buffer);
      for (let n = 0; n < samples.length; n++) view.setFloat32(n * 4, samples[n], true);
      const bytesHex = Array.from(bytes, n => n.toString(16).padStart(2, "0")).join("");
      const envelope = await globalThis.__opencutPreviewPcm({ commandId: command.id, generation: command.generation, overlayEpoch: command.overlayEpoch, startSample: command.startSample, channel, frames: samples.length, bytesHex });
      if (envelope?.ok !== true) {
        const code = envelope?.ok === false && typeof envelope.error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(envelope.error.code)
          ? envelope.error.code : "AUDIO_PREVIEW_FAILED";
        const error = new Error("Preview PCM callback failed"); error.code = code; throw error;
      }
    }
    return { ok: true, value: result.receipt };
  } catch (error) {
    return { ok: false, error: { code: typeof error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(error.code)
      ? error.code : "AUDIO_PREVIEW_FAILED" } };
  }
}
