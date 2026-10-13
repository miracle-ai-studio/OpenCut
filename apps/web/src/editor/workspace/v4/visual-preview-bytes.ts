/** Read only the size admitted by the fixed visual plan. The shared renderer
 * verifies SHA-256 before decoding or selecting a font face. */
export async function readPreviewBytes(
  response: Response,
  size: string,
  signal: AbortSignal,
): Promise<Uint8Array> {
  signal.throwIfAborted();
  if (!/^[1-9][0-9]*$/.test(size) || !Number.isSafeInteger(Number(size)))
    throw new Error("Invalid admitted visual resource size");
  if (!response.ok) throw new Error("Visual resource download failed");
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Visual resource stream is unavailable");
  const bytes = new Uint8Array(Number(size));
  let offset = 0;
  const abort = () => {
    void reader.cancel().catch(() => {});
  };
  signal.addEventListener("abort", abort, { once: true });
  try {
    for (;;) {
      const { value, done } = await reader.read();
      signal.throwIfAborted();
      if (done) break;
      if (offset + value.length > bytes.length)
        throw new Error("Visual resource exceeded its admitted size");
      bytes.set(value, offset);
      offset += value.length;
    }
    if (offset !== bytes.length) throw new Error("Visual resource ended early");
    return bytes;
  } finally {
    signal.removeEventListener("abort", abort);
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
