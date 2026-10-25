export async function executeSourceArtPage(plan) {
  const safeCode = error => typeof error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(error.code) ? error.code : "SOURCE_ART_DECODE_FAILED";
  try {
    const request = async (op, args) => {
      const result = await globalThis.__opencutSourceArtCallback(op, args);
      if (result?.ok === true) return result.value;
      const error = new Error("Source-art host callback failed"); error.code = safeCode(result?.error); throw error;
    };
    let outputOffset = 0;
    const host = {
      createOfflineContext: (channels, frames, rate) => new OfflineAudioContext(channels, frames, rate),
      async openRangeSource(resource, admission, cacheBytes, signal) {
        const { openVerifiedRangeSource } = await import("@opencut/classic-audio-renderer/verified-range-source");
        const maximum = Math.min(admission.maxEncodedBytes, admission.maxPcmBytes);
        const budget = { reserve(bytes) {
          if (!Number.isSafeInteger(bytes) || bytes < 0 || bytes > maximum) {
            const error = new Error("Source-art range residency exceeds admission"); error.code = "SOURCE_ART_BUDGET_EXCEEDED"; throw error;
          }
          // One source/transfer only. Pixel admission below additionally charges
          // this exact reservation before any decoder surface is allocated.
          return () => {};
        } };
        return openVerifiedRangeSource(resource, {
          async open(fixed) {
            const transfer = await request("resolve", { resource: fixed });
            return { transferId: transfer.id, byteLength: String(transfer.size), contentHash: transfer.content_hash,
              maxChunkBytes: transfer.max_chunk_bytes };
          },
          async read(transferId, offset, count) {
            const chunk = await request("read", { transfer_id: transferId, offset: String(offset), count });
            return { offset: chunk.offset, bytesHex: chunk.bytes_hex, eof: chunk.eof };
          },
          release: transferId => request("release", { transfer_id: transferId }),
        }, budget, cacheBytes, signal, { prefetch: false });
      },
      async readResource(resource) {
        const transfer = await request("resolve", { resource });
        if (transfer.size !== resource.size) throw new Error("Source-art byte count changed");
        const bytes = new Uint8Array(resource.size);
        try {
          for (let offset = 0; offset < bytes.length;) {
            const count = Math.min(65536, bytes.length - offset);
            const chunk = await request("read", { transfer_id: transfer.id, offset: String(offset), count });
            if (chunk.offset !== String(offset) || typeof chunk.bytes_hex !== "string" || chunk.bytes_hex.length !== count * 2 || !/^(?:[0-9a-f]{2})+$/.test(chunk.bytes_hex) || chunk.eof !== (offset + count === bytes.length)) throw new Error("Invalid source-art byte chunk");
            for (let n = 0; n < count; n++) bytes[offset + n] = parseInt(chunk.bytes_hex.slice(n * 2, n * 2 + 2), 16);
            offset += count;
          }
          const hash = await crypto.subtle.digest("SHA-256", bytes);
          if (Array.from(new Uint8Array(hash), n => n.toString(16).padStart(2, "0")).join("") !== resource.contentHash) throw new Error("Source-art bytes changed hash");
          return bytes;
        } finally { await request("release", { transfer_id: transfer.id }); }
      },
      async emit(kind, bytes) {
        for (let offset = 0; offset < bytes.length; offset += 32768) {
          const part = bytes.subarray(offset, offset + 32768);
          const bytes_hex = Array.from(part, n => n.toString(16).padStart(2, "0")).join("");
          const result = await globalThis.__opencutSourceArtBytes({ kind, offset: String(outputOffset), bytes_hex });
          if (result?.ok !== true) { const error = new Error("Source-art output failed"); error.code = safeCode(result?.error); throw error; }
          outputOffset += part.length;
        }
      },
    };
    const { executeSourceArt } = await import("/renderer/src/art.mjs");
    return { ok: true, value: await executeSourceArt(plan, host) };
  } catch (error) { return { ok: false, error: { code: safeCode(error) } }; }
}
