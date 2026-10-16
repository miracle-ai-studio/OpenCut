type SavePicker = (options: {
  suggestedName: string;
}) => Promise<{ createWritable(): Promise<WritableStream<Uint8Array>> }>;
export function downloadBlob(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob),
    anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export async function saveResponse(
  name: string,
  fetchResponse: () => Promise<Response>,
) {
  const picker = (window as Window & { showSaveFilePicker?: SavePicker })
    .showSaveFilePicker;
  // Obtain the handle during the click gesture, before awaiting the authenticated request.
  const handle = picker
    ? await picker.call(window, { suggestedName: name })
    : null;
  const response = await fetchResponse();
  if (!response.body) throw new Error("The download has no content.");
  if (handle) {
    await response.body.pipeTo(await handle.createWritable());
    return;
  }
  const limit = 256 * 1024 * 1024;
  if (Number(response.headers.get("content-length")) > limit) {
    await response.body.cancel();
    throw new Error(
      "For downloads above 256 MB, use Chrome or Edge to save directly to disk.",
    );
  }
  const reader = response.body.getReader(),
    chunks: BlobPart[] = [];
  let size = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > limit)
        throw new Error(
          "Use Chrome or Edge to save this large file directly to disk.",
        );
      chunks.push(new Uint8Array(value));
    }
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
  downloadBlob(
    new Blob(chunks, {
      type: response.headers.get("content-type") ?? "application/octet-stream",
    }),
    name,
  );
}
export function extension(mime: string) {
  return (
    (
      {
        "image/png": "png",
        "video/mp4": "mp4",
        "video/webm": "webm",
        "video/x-matroska": "mkv",
        "audio/wav": "wav",
        "application/json": "json",
      } as Record<string, string>
    )[mime] ?? "bin"
  );
}
