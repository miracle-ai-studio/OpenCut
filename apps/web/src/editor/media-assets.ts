import type { Asset, Operation } from "../lib/editor-client";

export type AssetVersionView = {
  id: string;
  contentHash: string;
  size: number;
};
type AssetReplaceOperation = Extract<Operation, { type: "asset.replace" }>;

export const assetVersionView = (asset: Asset): AssetVersionView | null => {
  const version = asset.version;
  return version
    ? {
        id: version.id,
        contentHash: version.content_hash,
        size: version.size,
      }
    : null;
};

export const assetReplaceOperation = (
  existing: Asset,
  uploaded: Asset,
): AssetReplaceOperation => {
  if (existing.kind !== uploaded.kind)
    throw new Error(
      `Cannot replace ${existing.kind} media with ${uploaded.kind} media`,
    );
  return {
    type: "asset.replace" as const,
    asset: { ...uploaded, id: existing.id, version: undefined },
  };
};

export const trackKindForAsset = (
  kind: Asset["kind"],
): "video" | "audio" | "graphic" => {
  if (kind === "video" || kind === "audio") return kind;
  if (kind === "image") return "graphic";
  throw new Error(
    `Importing ${kind} assets onto the timeline is not supported`,
  );
};
