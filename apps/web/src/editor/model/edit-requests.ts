import type { Asset, Operation, Sequence, Time, Track } from "./types";
import { addTime, compareTime, fromSeconds, zero } from "./time";
export function placeAsset(
  asset: Asset,
  sequence: Sequence,
  at?: Time,
  trackId?: string,
): Operation[] {
  const kind =
    asset.kind === "audio"
      ? "audio"
      : asset.kind === "image"
        ? "graphic"
        : "video";
  if (!["audio", "image", "video"].includes(asset.kind))
    throw new Error("This asset cannot be placed as a media clip.");
  const existing =
    sequence.tracks.find((track) => track.id === trackId) ??
    sequence.tracks.find((track) => track.kind === kind && !track.locked);
  if (existing?.locked)
    throw new Error("Unlock the target track before adding clips.");
  if (
    existing &&
    existing.kind !== kind &&
    !(asset.kind === "image" && existing.kind === "video")
  )
    throw new Error("Choose a compatible track.");
  const track: Track = existing ?? {
    id: crypto.randomUUID(),
    name: asset.kind === "audio" ? "Audio" : "Video",
    kind,
    enabled: true,
    muted: false,
    locked: false,
    clips: [],
    params: {},
  };
  const end = track.clips.reduce((end, clip) => {
    const value = addTime(
      clip.timeline_range.start,
      clip.timeline_range.duration,
    );
    return compareTime(end, value) < 0 ? value : end;
  }, zero);
  const duration =
    asset.kind === "image"
      ? fromSeconds(5)
      : (asset.duration ?? fromSeconds(5));
  return [
    ...(existing
      ? []
      : [{ type: "track.create" as const, sequence_id: sequence.id, track }]),
    {
      type: "clip.insert",
      sequence_id: sequence.id,
      track_id: track.id,
      clip: {
        id: crypto.randomUUID(),
        asset_id: asset.id,
        timeline_range: { start: at ?? end, duration },
        source_range: asset.kind === "image" ? null : { start: zero, duration },
        params: {},
        channels: [],
      },
    },
  ];
}
