export type RenderRevisionIdentity = {
  projectId: string;
  revision: string;
  sequenceId: string;
  generation: number;
};

export const isCurrentRenderRevision = (
  request: RenderRevisionIdentity,
  current: RenderRevisionIdentity,
) =>
  request.projectId === current.projectId &&
  request.revision === current.revision &&
  request.sequenceId === current.sequenceId &&
  request.generation === current.generation;

export type MediaAssetLock = {
  version: 1;
  revision: string;
  sequenceId: string;
  assets: Array<{ assetId: string; contentHash: string; size: number }>;
};

export const mediaAssetLock = (input: unknown): MediaAssetLock | null => {
  if (!input || typeof input !== "object") return null;
  const stored = input as {
      asset_lock?: unknown;
      sequence_id?: unknown;
      snapshot?: unknown;
    },
    lock = stored.asset_lock;
  if (!lock || typeof lock !== "object") return null;
  const candidate = lock as {
    version?: unknown;
    revision?: unknown;
    sequence_id?: unknown;
    assets?: unknown;
  };
  if (
    candidate.version !== 1 ||
    typeof candidate.revision !== "string" ||
    typeof candidate.sequence_id !== "string" ||
    !Array.isArray(candidate.assets)
  )
    return null;
  const snapshotRevision =
      stored.snapshot && typeof stored.snapshot === "object"
        ? (stored.snapshot as { revision?: unknown }).revision
        : undefined,
    assets: MediaAssetLock["assets"] = [],
    assetIds = new Set<string>();
  if (
    (typeof snapshotRevision === "string" &&
      snapshotRevision !== candidate.revision) ||
    (typeof stored.sequence_id === "string" &&
      stored.sequence_id !== candidate.sequence_id)
  )
    return null;
  for (const asset of candidate.assets) {
    if (!asset || typeof asset !== "object") return null;
    const item = asset as {
      asset_id?: unknown;
      content_hash?: unknown;
      size?: unknown;
    };
    if (
      typeof item.asset_id !== "string" ||
      typeof item.content_hash !== "string" ||
      !/^[0-9a-f]{64}$/.test(item.content_hash) ||
      assetIds.has(item.asset_id) ||
      typeof item.size !== "number" ||
      !Number.isSafeInteger(item.size) ||
      item.size < 0
    )
      return null;
    assetIds.add(item.asset_id);
    assets.push({
      assetId: item.asset_id,
      contentHash: item.content_hash,
      size: item.size,
    });
  }
  return {
    version: 1,
    revision: candidate.revision,
    sequenceId: candidate.sequence_id,
    assets,
  };
};
