import type { ClassicSnapshot } from "./decode";
import { getProjectId, isRecord } from "./legacy/utils";
import { fileDigest } from "./identity";
export const bundleMagic = new TextEncoder().encode(
  "OPENCUT-CLASSIC-RECOVERY\n",
);
const maxHeader = 4 * 1024 * 1024,
  originalMaxFile = 16 * 1024 ** 3;
export type RecoveryMedia = { id: string; file: File; sha256: string };
type Header = {
  format: "opencut.classic-recovery";
  snapshot: ClassicSnapshot;
  media: Array<{
    id: string;
    name: string;
    type: string;
    size: number;
    sha256: string;
  }>;
};
function checkSnapshot(value: unknown): asserts value is ClassicSnapshot {
  if (
    !isRecord(value) ||
    typeof value.origin !== "string" ||
    typeof value.sourceId !== "string" ||
    !isRecord(value.raw) ||
    getProjectId({ project: value.raw }) !== value.sourceId ||
    !isRecord(value.context) ||
    !isRecord(value.context.legacyTracksBySceneId) ||
    !isRecord(value.context.mediaTypesById) ||
    !Array.isArray(value.media) ||
    value.media.length > 256
  )
    throw new Error("Invalid Classic recovery snapshot");
  if (
    new URL(value.origin).origin !== value.origin ||
    !/^https?:/.test(value.origin)
  )
    throw new Error("Invalid Classic source origin");
  const ids = new Set<string>();
  for (const row of value.media) {
    if (
      !isRecord(row) ||
      typeof row.id !== "string" ||
      !row.id ||
      ids.has(row.id) ||
      !["video", "audio", "image"].includes(String(row.type))
    )
      throw new Error("Invalid Classic media manifest");
    ids.add(row.id);
  }
}
export async function createRecoveryBundle(
  snapshot: ClassicSnapshot,
  media: RecoveryMedia[],
  signal: AbortSignal,
): Promise<Blob> {
  signal.throwIfAborted();
  checkSnapshot(snapshot);
  if (
    media.length !== snapshot.media.length ||
    new Set(media.map((m) => m.id)).size !== media.length
  )
    throw new Error("Recovery bundle must include every original media file");
  const entries: Header["media"] = [];
  let mediaBytes = 0;
  for (const item of media) {
    if (
      !snapshot.media.some((row) => row.id === item.id) ||
      !item.file.size ||
      !Number.isSafeInteger(item.file.size) ||
      item.file.size > Number.MAX_SAFE_INTEGER - mediaBytes ||
      !/^[a-f0-9]{64}$/.test(item.sha256) ||
      (await fileDigest(item.file, signal)) !== item.sha256
    )
      throw new Error("Classic recovery file changed or exceeds its limit");
    mediaBytes += item.file.size;
    entries.push({
      id: item.id,
      name: item.file.name,
      type: item.file.type,
      size: item.file.size,
      sha256: item.sha256,
    });
  }
  const header: Header = {
    format: "opencut.classic-recovery",
    snapshot,
    media: entries,
  };
  const bytes = new TextEncoder().encode(
    JSON.stringify(header, (_key, value) => {
      if (typeof value === "number" && !Number.isFinite(value))
        throw new Error("Nonfinite Classic source value");
      return value;
    }),
  );
  if (bytes.length > maxHeader)
    throw new Error("Classic recovery header exceeds 4 MiB");
  if (mediaBytes > Number.MAX_SAFE_INTEGER - bundleMagic.length - 4 - bytes.length)
    throw new Error("Classic recovery size exceeds exact file addressing");
  const size = new Uint8Array(4);
  new DataView(size.buffer).setUint32(0, bytes.length, true);
  return new Blob([bundleMagic, size, bytes, ...media.map((m) => m.file)], {
    type: "application/x-opencut-classic-recovery",
  });
}
export async function readRecoveryBundle(
  file: File,
  signal: AbortSignal,
  progress: (name: string, index: number, count: number) => void,
) {
  signal.throwIfAborted();
  if (!Number.isSafeInteger(file.size))
    throw new Error("Classic recovery size exceeds exact file addressing");
  const prefixLength = bundleMagic.length + 4;
  const prefix = new Uint8Array(
    await file.slice(0, prefixLength).arrayBuffer(),
  );
  if (
    prefix.length !== prefixLength ||
    !bundleMagic.every((b, i) => prefix[i] === b)
  )
    throw new Error(
      "Choose a Classic recovery bundle containing media, not a standalone project JSON",
    );
  const size = new DataView(prefix.buffer).getUint32(bundleMagic.length, true);
  if (!size || size > maxHeader || prefixLength + size > file.size)
    throw new Error("Invalid Classic recovery header size");
  const raw: unknown = JSON.parse(
    await file.slice(prefixLength, prefixLength + size).text(),
  );
  if (
    !isRecord(raw) ||
    (raw.format !== "opencut.classic-recovery" && raw.format !== "opencut.classic-recovery.v1") ||
    Object.keys(raw).some(
      (key) => !["format", "snapshot", "media"].includes(key),
    )
  )
    throw new Error("Invalid Classic recovery bundle format");
  const historical = raw.format === "opencut.classic-recovery.v1";
  checkSnapshot(raw.snapshot);
  if (
    !Array.isArray(raw.media) ||
    raw.media.length !== raw.snapshot.media.length
  )
    throw new Error("Incomplete Classic recovery media");
  const media: RecoveryMedia[] = [],
    seen = new Set<string>();
  let offset = prefixLength + size;
  for (const [index, row] of raw.media.entries()) {
    if (
      !isRecord(row) ||
      typeof row.id !== "string" ||
      seen.has(row.id) ||
      !raw.snapshot.media.some((m) => m.id === row.id) ||
      typeof row.name !== "string" ||
      row.name.length > 1024 ||
      typeof row.type !== "string" ||
      typeof row.size !== "number" ||
      !Number.isSafeInteger(row.size) ||
      row.size <= 0 ||
      (historical && row.size > originalMaxFile) ||
      typeof row.sha256 !== "string" ||
      !/^[a-f0-9]{64}$/.test(row.sha256) ||
      row.size > file.size - offset ||
      Object.keys(row).some(
        (key) => !["id", "name", "type", "size", "sha256"].includes(key),
      )
    )
      throw new Error("Invalid Classic recovery media entry");
    seen.add(row.id);
    const original = raw.snapshot.media.find((m) => m.id === row.id)!;
    if (
      (typeof original.size === "number" && original.size !== row.size) ||
      (typeof original.name === "string" && original.name !== row.name)
    )
      throw new Error("Classic recovery metadata differs from its media bytes");
    progress(row.name, index, raw.media.length);
    const bytes = new File([file.slice(offset, offset + row.size)], row.name, {
      type: row.type,
    });
    if ((await fileDigest(bytes, signal)) !== row.sha256)
      throw new Error(`Classic recovery checksum mismatch: ${row.name}`);
    media.push({ id: row.id, file: bytes, sha256: row.sha256 });
    offset += row.size;
  }
  if (offset !== file.size)
    throw new Error("Classic recovery bundle has unexpected trailing bytes");
  signal.throwIfAborted();
  return { snapshot: raw.snapshot, media };
}
