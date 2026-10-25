import type { Asset, Sequence } from "../types";
import type { Time } from "../../../model/types";
import {
  addTime,
  compareTime,
  halfTime,
  secondsForDisplay,
  subtractTime,
} from "../../../model/time";
import { stackTargetProblem } from "../scene-stack/model";
import {
  transitionLimit,
  transitionPairs,
  type TransitionTarget,
} from "./transition-target";
import type { TimelineScope } from "../timeline-edit/types";

export type ResourcePosition = TimelineScope & {
  trackId: string | null;
  at: Time;
};
/** This is local presentation intent, never a public DTO or transferable grant. */
export type DragResource =
  | { kind: "media"; assetId: string; name: string }
  | {
      kind: "catalog-audio";
      name: string;
      place(position: ResourcePosition): Promise<{ elementId: string } | void>;
    }
  | { kind: "effect"; name: string; intensity: number; duration: Time }
  | { kind: "transition"; name: string; duration: Time };
export type ResourceHit = {
  trackId: string | null;
  at: Time;
  elementId?: string;
  tolerance: number;
};
export type ResourceDrop = {
  kind: "media" | "effect-clip" | "effect-layer" | "transition";
  trackId: string | null;
  at: Time;
  duration: Time | null;
  name: string;
  elementId?: string;
  seam?: TransitionTarget;
  replace?: boolean;
  reason: string | null;
};
export function overlaps(
  track: Sequence["tracks"][number],
  at: Time,
  duration: Time,
) {
  const end = addTime(at, duration);
  return track.elements.some(
    (element) =>
      compareTime(
        at,
        addTime(element.timeline_range.start, element.timeline_range.duration),
      ) < 0 && compareTime(end, element.timeline_range.start) > 0,
  );
}
export function resourceDrop(
  resource: DragResource,
  sequence: Sequence,
  assets: Asset[],
  hit: ResourceHit,
): ResourceDrop {
  const track = sequence.tracks.find((item) => item.id === hit.trackId);
  const element = track?.elements.find((item) => item.id === hit.elementId);
  let result: ResourceDrop = {
    kind: "media",
    trackId: hit.trackId,
    at: hit.at,
    duration: null,
    name: track?.name ?? "New track",
    reason: null,
  };
  if (hit.trackId && !track)
    return { ...result, reason: "The drop target changed." };
  if (track?.locked) return { ...result, reason: "This track is locked." };
  if (track && !track.enabled)
    return {
      ...result,
      reason: "Enable this track before dropping resources.",
    };
  if (resource.kind === "transition") {
    const pair = transitionPairs(sequence)
      .filter((pair) => pair.track.id === hit.trackId)
      .map((pair) => ({
        pair,
        distance: Math.abs(
          secondsForDisplay(
            subtractTime(pair.incoming.timeline_range.start, hit.at),
          ),
        ),
      }))
      .filter(({ distance }) => distance <= hit.tolerance)
      .sort((a, b) => a.distance - b.distance)[0]?.pair;
    result = { ...result, kind: "transition", duration: resource.duration };
    if (!pair)
      return {
        ...result,
        reason: "Drop the transition on a cut between two adjacent videos.",
      };
    const limit = transitionLimit(pair, assets);
    return {
      ...result,
      at: subtractTime(
        pair.incoming.timeline_range.start,
        halfTime(resource.duration),
      ),
      seam: pair.target,
      name: `${pair.outgoing.name} → ${pair.incoming.name}`,
      replace: sequence.transitions.some(
        (item) =>
          item.outgoing_element_id === pair.outgoing.id &&
          item.incoming_element_id === pair.incoming.id,
      ),
      reason: limit.reason
        ? "This cut needs two enabled videos at normal speed with sufficient source handles."
        : limit.maximum && compareTime(resource.duration, limit.maximum) > 0
          ? "Transition duration exceeds the available source handles."
          : null,
    };
  }
  if (resource.kind === "effect") {
    if (element) {
      if (
        !element.visual ||
        stackTargetProblem(element) ||
        element.content.kind === "effect_layer"
      )
        return {
          ...result,
          reason: "Drop on an editable visual clip or an empty effect track.",
        };
      return {
        ...result,
        kind: "effect-clip",
        elementId: element.id,
        at: element.timeline_range.start,
        duration: element.timeline_range.duration,
        name: element.name,
      };
    }
    result = { ...result, kind: "effect-layer", duration: resource.duration };
    if (track && track.kind !== "effect")
      return {
        ...result,
        reason: "Drop on a visual clip, an effect track, or New track.",
      };
  } else {
    const asset =
      resource.kind === "media"
        ? assets.find((asset) => asset.id === resource.assetId)
        : null;
    if (resource.kind === "media" && !asset)
      return {
        ...result,
        reason: "The selected project media is unavailable.",
      };
    const kind = asset?.kind ?? "audio";
    result.duration =
      asset?.duration ??
      (kind === "image" || kind === "graphic"
        ? { ticks: "3", timebase: 1 }
        : null);
    if (
      asset &&
      (!["video", "audio", "image", "graphic"].includes(kind) ||
        !result.duration)
    )
      return {
        ...result,
        reason:
          "This media needs supported type and observed duration before placement.",
      };
    if (
      track &&
      !(kind === "audio"
        ? track.kind === "audio"
        : kind === "video"
          ? track.kind === "video"
          : ["video", "graphic"].includes(track.kind))
    )
      return {
        ...result,
        reason: "Choose a track compatible with this resource.",
      };
  }
  if (track && result.duration && overlaps(track, result.at, result.duration))
    return {
      ...result,
      reason: "This range overlaps existing clips. Choose an empty range.",
    };
  if (
    track &&
    !result.duration &&
    track.elements.some(
      (item) =>
        compareTime(hit.at, item.timeline_range.start) >= 0 &&
        compareTime(
          hit.at,
          addTime(item.timeline_range.start, item.timeline_range.duration),
        ) < 0,
    )
  )
    return {
      ...result,
      reason: "This range overlaps existing clips. Choose an empty range.",
    };
  return result;
}
