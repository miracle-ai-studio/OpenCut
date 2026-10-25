import { addTime, compareTime, subtractTime, zero } from "../../../model/time";
import type { Time } from "../../../model/types";
import type { Asset, Sequence } from "../types";

export type TransitionTarget = {
  trackId: string;
  outgoingId: string;
  incomingId: string;
};
export function transitionPairs(sequence: Sequence) {
  return sequence.tracks.flatMap((track) => {
    const clips = track.elements
      .filter((element) => element.visual)
      .sort((a, b) =>
        compareTime(a.timeline_range.start, b.timeline_range.start),
      );
    return clips.slice(0, -1).flatMap((outgoing, index) => {
      const incoming = clips[index + 1];
      return compareTime(
        addTime(
          outgoing.timeline_range.start,
          outgoing.timeline_range.duration,
        ),
        incoming.timeline_range.start,
      ) === 0
        ? [
            {
              track,
              outgoing,
              incoming,
              target: {
                trackId: track.id,
                outgoingId: outgoing.id,
                incomingId: incoming.id,
              },
            },
          ]
        : [];
    });
  });
}
export type TransitionPair = ReturnType<typeof transitionPairs>[number];
export const sameTransitionTarget = (
  a: TransitionTarget | null,
  b: TransitionTarget | null,
) =>
  a?.trackId === b?.trackId &&
  a?.outgoingId === b?.outgoingId &&
  a?.incomingId === b?.incomingId;

/** Snapshot-derived UI guidance only. The application edit planner validates the
 * original request before publication, including source mapping and handles. */
export function transitionLimit(
  pair: TransitionPair,
  assets: Asset[],
): {
  maximum: Time | null;
  reason:
    | "locked"
    | "disabled"
    | "videoOnly"
    | "normalSpeed"
    | "unknownDuration"
    | "noHandles"
    | null;
} {
  if (pair.track.locked) return { maximum: null, reason: "locked" };
  if (!pair.track.enabled || !pair.outgoing.enabled || !pair.incoming.enabled)
    return { maximum: null, reason: "disabled" };
  const limits: Time[] = [];
  for (const element of [pair.outgoing, pair.incoming]) {
    if (element.content.kind !== "video")
      return { maximum: null, reason: "videoOnly" };
    const rate = element.content.retime.rate;
    if (rate.numerator !== rate.denominator)
      return { maximum: null, reason: "normalSpeed" };
    const span = element.source_span;
    const start =
      span.profile === "legacy_source_range"
        ? span.range.start
        : span.trim_start;
    const duration =
      span.profile === "legacy_source_range"
        ? span.range.duration
        : element.timeline_range.duration;
    if (compareTime(duration, element.timeline_range.duration) !== 0)
      return { maximum: null, reason: "normalSpeed" };
    limits.push(element.timeline_range.duration);
    if (element === pair.outgoing) {
      const assetId = element.content.asset_id;
      const extent = assets.find((asset) => asset.id === assetId)?.duration;
      if (!extent) return { maximum: null, reason: "unknownDuration" };
      limits.push(subtractTime(extent, addTime(start, duration)));
    } else limits.push(start);
  }
  const half = limits.reduce((a, b) => (compareTime(a, b) < 0 ? a : b));
  return compareTime(half, zero) <= 0
    ? { maximum: zero, reason: "noHandles" }
    : { maximum: addTime(half, half), reason: null };
}
