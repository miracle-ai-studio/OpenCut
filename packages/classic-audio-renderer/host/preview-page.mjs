export async function preparePreviewPage({ plan, engineIdentity, admission }) {
  try {
    const { unpackPreviewBlock,readPreviewFactsText } = await import("/renderer/src/preview-packet.mjs");
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
        if(op==="readVerifiedBytes"){
          const value=envelope.value;
          if(!value||Object.keys(value).length!==3||value.offset!==args.offset||typeof value.eof!=="boolean"||typeof value.bytesBase64!=="string"||value.bytesBase64.length!==4*Math.ceil(args.count/3)||!/^[A-Za-z0-9+/]*={0,2}$/.test(value.bytesBase64))throw new Error("Invalid source byte transport");
          const binary=atob(value.bytesBase64),bytes=new Uint8Array(binary.length);
          for(let i=0;i<binary.length;i++)bytes[i]=binary.charCodeAt(i);
          if(bytes.length!==args.count)throw new Error("Invalid source byte count");
          return {offset:value.offset,eof:value.eof,bytes};
        }
        if (op !== "compilePreviewBlock" && op !== "bindAndCompilePreviewBlock") return envelope.value;
        try {
          return unpackPreviewBlock(JSON.parse(await readPreviewFactsText(envelope.value)));
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
      async resolveRangeSource(resource, budget, signal) {
        const { openVerifiedRangeSource } = await import("@opencut/classic-audio-renderer/verified-range-source");
        return openVerifiedRangeSource(resource, {
          open: resource => request("openVerifiedRange", { resource }),
          read: (transferId, offset, count) => request("readVerifiedBytes", { transferId, offset: String(offset), count }),
          release: transferId => request("releaseVerifiedBytes", { transferId }),
        // Reuse the existing aligned two-line range reader. Reserve its extra
        // two transport lines by shrinking the demuxer's cache, within the
        // unchanged native encoded-source budget; no whole-file prefetch.
        }, budget, Math.max(65536, plan.encodedSource.cacheBytes - Math.ceil(2 * 65536 / 3)), signal,
          { prefetch: false, readAhead: true });
      },
      compilePitchPreparation: args => request("compilePitchPreparation", args),
      releaseDecodedSource: resource => request("releaseDecodedSource", { resource }),
      compileSourcePositions: args => request("compileSourcePositions", args),
      bindPreviewSources: args => request("bindPreviewSources", args),
      bindAndCompilePreviewBlock: args => request("bindAndCompilePreviewBlock", args),
      observePreviewDecoded: args => request("observePreviewDecoded", args),
      previewSourceWindows: args => request("previewSourceWindows", args),
      compilePreviewBlock: args => request("compilePreviewBlock", args),
      // This realm produces PCM, it never plays it. Allocating source buffers
      // must not open another physical device/context alongside the UI clock.
      createAudioContext: sampleRate => {
        let closed=false;
        return {sampleRate,createBuffer(channels,length,rate){
          if(closed)throw new Error("Preview allocation realm has closed");
          return new AudioBuffer({numberOfChannels:channels,length,sampleRate:rate});
        },async close(){closed=true;}};
      },
      createOfflineContext: (channels, frames, sampleRate) => new OfflineAudioContext(channels, frames, sampleRate),
      abortRendering: () => globalThis.__opencutTerminatePreviewRealm(),
    };
    host.observeWindowedPreviewDecoded = args => request("observeWindowedPreviewDecoded", args);
    host.admitPreviewDecode = (resource, primary) => request("admitPreviewDecode", { resource, primary });
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
    if (!Number.isInteger(command.frameCount) || command.frameCount < 1 || command.frameCount > 4096 ||
        !Array.isArray(result.channels) || result.channels.length !== 2 ||
        result.channels.some(samples => !(samples instanceof Float32Array) || samples.length !== command.frameCount)) {
      const error = new Error("Preview PCM exceeds command credit"); error.code = "INVALID_OUTPUT_PCM"; throw error;
    }
    const channelsBase64 = [];
    for (let channel = 0; channel < 2; channel++) {
      const samples = result.channels[channel], bytes = new Uint8Array(samples.length * 4), view = new DataView(bytes.buffer);
      for (let n = 0; n < samples.length; n++) view.setFloat32(n * 4, samples[n], true);
      channelsBase64.push(btoa(String.fromCharCode(...bytes)));
    }
    // The command already admits at most two4096-frame Float32 channels.
    // Return both byte strings with the completion in this one evaluate call;
    // Node keeps the original private PCM packets and publication checks.
    return { ok: true, value: { receipt: result.receipt, frameCount: command.frameCount, channelsBase64 } };
  } catch (error) {
    return { ok: false, error: { code: typeof error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(error.code)
      ? error.code : "AUDIO_PREVIEW_FAILED" } };
  }
}
