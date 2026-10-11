import type { Asset } from "./types";
import { compareTime, zero } from "./time";

export type MediaSort = "name" | "type" | "duration" | "size";
export function sortMedia(assets: Asset[], key: MediaSort, ascending: boolean) {
  return [...assets].sort((a, b) => {
    const value =
      key === "duration"
        ? compareTime(a.duration ?? zero, b.duration ?? zero)
        : key === "size"
          ? BigInt(a.version?.size ?? 0) < BigInt(b.version?.size ?? 0)
            ? -1
            : BigInt(a.version?.size ?? 0) > BigInt(b.version?.size ?? 0)
              ? 1
              : 0
          : key === "type"
            ? a.kind.localeCompare(b.kind)
            : a.name.localeCompare(b.name);
    return (ascending ? 1 : -1) * value || a.id.localeCompare(b.id);
  });
}
