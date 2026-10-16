// Private renderer protocol. All expensive digest work stays off the UI thread.
// Transfer the exact input buffer back; the caller uses it for FontFace loading.
self.onmessage = async ({ data }) => {
  try {
    if (!data || data.version !== 1 || data.kind !== "sha256" || !Number.isSafeInteger(data.id) || data.id < 1 || typeof data.identity !== "string" || !(data.buffer instanceof ArrayBuffer) || !Number.isSafeInteger(data.byteLength) || data.byteLength < 1 || data.buffer.byteLength !== data.byteLength || Object.keys(data).sort().join() !== "buffer,byteLength,id,identity,kind,version") throw Error("Invalid font hash request");
    const digest = await crypto.subtle.digest("SHA-256", data.buffer);
    const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, "0")).join("");
    self.postMessage({ version: 1, kind: "sha256-result", id: data.id, identity: data.identity, byteLength: data.byteLength, hash, buffer: data.buffer }, [data.buffer]);
  } catch {
    // Any failure invalidates the realm's outstanding requests. No local bytes
    // or font details are included in the failure response.
    self.postMessage({ version: 1, kind: "error" });
  }
};
