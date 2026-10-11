import { t } from "../shared/locale";
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { publicJobResult } from "../../lib/editor-sdk";
import type { OpenCutClient } from "../../lib/editor-sdk";

export async function uploadMedia(
  sdk: OpenCutClient,
  projectId: string,
  file: File,
  onProgress: (value: string) => void,
  signal: AbortSignal,
) {
  const capabilities = await sdk.hostCapabilities({ signal });
  if (!capabilities.uploads)
    throw new Error("This service does not support media uploads.");
  if (
    !file.size ||
    BigInt(file.size) > BigInt(capabilities.uploads.max_upload_bytes)
  )
    throw new Error(
      `File exceeds the service limit (${capabilities.uploads.max_upload_bytes} bytes).`,
    );
  const chunkSize = Math.min(
      Number(capabilities.uploads.max_chunk_bytes),
      4 * 1024 * 1024,
    ),
    hash = sha256.create();
  if (!Number.isSafeInteger(chunkSize) || chunkSize < 1)
    throw new Error("The service returned an invalid upload chunk limit.");
  const session = await sdk.call(
    "opencut_upload_stream_begin",
    {
      project_id: projectId,
      idempotency_key: crypto.randomUUID(),
      name: file.name,
      size_bytes: String(file.size),
    },
    { signal },
  );
  if (
    session.project_id !== projectId ||
    session.size_bytes !== String(file.size)
  )
    throw new Error("Upload response does not match this file.");
  let sealed = false;
  try {
    const offset = Number(session.next_offset);
    if (!Number.isSafeInteger(offset) || offset !== 0)
      throw new Error("The service returned an invalid upload offset.");
    const readChunk = async (offset: number) => {
      signal.throwIfAborted();
      const chunk = new Uint8Array(
        await file.slice(offset, offset + chunkSize).arrayBuffer(),
      );
      signal.throwIfAborted();
      hash.update(chunk);
      return chunk;
    };
    let chunk = await readChunk(0);
    for (let offset = 0; offset < file.size;) {
      const nextOffset = offset + chunk.length;
      // At most one read-ahead chunk; acknowledgements and sends stay ordered.
      const [accepted, nextChunk] = await Promise.all([
        sdk.uploadStreamChunk(session.upload_id, String(offset), chunk, {
          signal,
        }),
        nextOffset < file.size ? readChunk(nextOffset) : Promise.resolve(null),
      ]);
      if (accepted.next_offset !== String(nextOffset))
        throw new Error(
          `Upload offset is uncertain. Inspect ${session.upload_id} before retrying.`,
        );
      offset = nextOffset;
      onProgress(
        t("Importing {percent}%", {
          percent: Math.round((offset / file.size) * 100),
        }),
      );
      if (nextChunk) chunk = nextChunk;
    }
    const digest = bytesToHex(hash.digest());
    const accepted = await sdk.call(
      "opencut_upload_stream_complete",
      { upload_id: session.upload_id, content_hash: digest },
      { signal },
    );
    sealed = true;
    if (!accepted.job_id) throw new Error("Import was not accepted.");
    onProgress(t("Preparing media…"));
    const job = await sdk.waitForPublicJob(accepted.job_id, {
      signal,
      timeoutMs: 300000,
      pollIntervalMs: 300,
    });
    const result = publicJobResult(job, "media.import");
    const media = await sdk.call(
      "opencut_project_media_inspect",
      { project_id: projectId, media_id: result.media_id },
      { signal },
    );
    if (media.asset.version?.content_hash !== digest)
      throw new Error("Imported media does not match the file digest.");
    let warning: string | null = null;
    try {
      await sdk.call("opencut_upload_release", {
        upload_id: session.upload_id,
      });
    } catch {
      warning = `Imported. Staging cleanup is pending for ${session.upload_id}.`;
    }
    return { asset: media.asset, warning };
  } catch (error) {
    if (!sealed) {
      try {
        await sdk.call("opencut_upload_stream_abort", {
          upload_id: session.upload_id,
        });
        await sdk.call("opencut_upload_release", {
          upload_id: session.upload_id,
        });
      } catch {
        /* The service retains the discoverable upload when cleanup cannot complete. */
      }
    }
    throw error;
  }
}
