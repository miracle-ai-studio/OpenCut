import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { publicJobResult } from "../../../../lib/editor-sdk";
import type { ArtifactReference, PublicJob } from "../../../../lib/editor-sdk";
import type { EditorApi } from "../../api";
import type { V4FrameRequest, V4FrameResult } from "../types";
const KIND = "render.project_frame.v4";
const MAX_PNG_BYTES = 32 * 1024 * 1024;

export function sameTarget(job: PublicJob, request: V4FrameRequest) {
  return (
    job.kind === KIND &&
    job.target?.project_id === request.project_id &&
    job.target.revision === request.revision &&
    job.target.sequence_id === request.sequence_id
  );
}

export function verifiedResult(
  job: PublicJob,
  request: V4FrameRequest,
  width: number,
  height: number,
) {
  if (!sameTarget(job, request))
    throw new Error("Scene frame returned an unexpected task identity.");
  const result: V4FrameResult = publicJobResult(job, KIND);
  const identity = result?.identity;
  if (
    result?.schema_version !== 1 ||
    !identity ||
    identity.project_id !== request.project_id ||
    identity.revision !== request.revision ||
    identity.sequence_id !== request.sequence_id ||
    identity.time?.ticks !== request.time.ticks ||
    identity.time.timebase !== request.time.timebase ||
    identity.profile !== "classic-v31-gpu-normal-frame" ||
    !Number.isSafeInteger(identity.width) ||
    !Number.isSafeInteger(identity.height) ||
    identity.width <= 0 ||
    identity.height <= 0 ||
    identity.width !== width ||
    identity.height !== height ||
    identity.width * identity.height > 32 * 1024 * 1024
  )
    throw new Error(
      "Scene frame returned an unexpected render identity or exceeds the display limit.",
    );
  const artifacts = job.artifacts.filter((a) => a.mime_type === "image/png");
  const artifact = artifacts[0];
  if (
    artifacts.length !== 1 ||
    !artifact ||
    artifact.job_id !== job.job_id ||
    result.artifact?.mime_type !== "image/png" ||
    artifact.content_hash !== result.artifact.content_hash ||
    artifact.size_bytes !== result.artifact.size_bytes ||
    !/^[a-f0-9]{64}$/.test(artifact.content_hash)
  )
    throw new Error("Scene frame returned an unexpected PNG artifact.");
  return { result, artifact };
}

/** A bounded authenticated read, with independent envelope/hash/PNG dimensions
 * checks. These are publication checks, not a renderer pixel oracle. */
export async function readPng(
  api: EditorApi,
  artifact: ArtifactReference,
  result: V4FrameResult,
  signal: AbortSignal,
) {
  if (!/^\d{1,10}$/.test(artifact.size_bytes))
    throw new Error("Invalid scene frame size.");
  const size = BigInt(artifact.size_bytes);
  if (size < 24n) throw new Error("Invalid scene frame size.");
  if (size > BigInt(MAX_PNG_BYTES))
    throw new Error(
      "This scene frame exceeds the editor’s 32 MB display limit. Download the PNG from Tasks.",
    );
  const response = await api.sdk.downloadArtifact(artifact, { signal });
  if (
    response.headers.get("content-type")?.split(";")[0].trim() !==
      "image/png" ||
    !response.body
  )
    throw new Error("Scene frame has no PNG content.");
  const reader = response.body.getReader(),
    chunks: Uint8Array[] = [];
  let total = 0;
  try {
    for (;;) {
      signal.throwIfAborted();
      const { value, done } = await reader.read();
      if (done) break;
      total += value.byteLength;
      if (total > MAX_PNG_BYTES || BigInt(total) > size)
        throw new Error("Scene frame exceeds its published size.");
      chunks.push(value);
    }
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
  signal.throwIfAborted();
  if (BigInt(total) !== size)
    throw new Error("Scene frame differs from its published size.");
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  if (bytesToHex(sha256(bytes)) !== artifact.content_hash)
    throw new Error("Scene frame differs from its published hash.");
  const header = new DataView(bytes.buffer);
  if (
    [137, 80, 78, 71, 13, 10, 26, 10].some(
      (value, index) => bytes[index] !== value,
    ) ||
    header.getUint32(8) !== 13 ||
    header.getUint32(12) !== 0x49484452 ||
    header.getUint32(16) !== result.identity.width ||
    header.getUint32(20) !== result.identity.height
  )
    throw new Error("Scene frame PNG dimensions differ from its receipt.");
  return new Blob([bytes], { type: "image/png" });
}
