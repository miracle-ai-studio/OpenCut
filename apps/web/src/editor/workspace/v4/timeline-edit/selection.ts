import type { V4Sequence } from "../types";
import type { TimelineTarget } from "./types";

const same = (a: TimelineTarget, b: TimelineTarget) =>
  a.track_id === b.track_id && a.element_id === b.element_id;

/** Stable document order for presentation and explicit requests. This does not
 * compute movement, source clocks, overlap, frame alignment or ripple. */
export function normalizeTimelineSelection(
  sequence: V4Sequence,
  selected: readonly TimelineTarget[],
) {
  return sequence.tracks.flatMap((track) => [
    ...track.elements
      .filter((element) =>
        selected.some(
          (target) =>
            target.track_id === track.id && target.element_id === element.id,
        ),
      )
      .map((element) => ({ track_id: track.id, element_id: element.id })),
    ...sequence.captions
      .filter(
        (caption) =>
          caption.track_id === track.id &&
          selected.some(
            (target) =>
              target.track_id === track.id && target.element_id === caption.id,
          ),
      )
      .map((caption) => ({ track_id: track.id, element_id: caption.id })),
  ]);
}

export function selectTimelineTarget(
  sequence: V4Sequence,
  selected: readonly TimelineTarget[],
  target: TimelineTarget,
  mode: "replace" | "toggle" | "extend" = "replace",
) {
  const next =
    mode === "replace"
      ? [target]
      : mode === "toggle" && selected.some((item) => same(item, target))
        ? selected.filter((item) => !same(item, target))
        : [...selected, target];
  return normalizeTimelineSelection(sequence, next);
}

/** UI availability only; the planner remains the authority for locking,
 * profiles, source bounds, animation splitting and final aggregate validity. */
export function timelineSelectionReason(
  sequence: V4Sequence,
  selection: readonly TimelineTarget[],
) {
  if (!selection.length) return "Select elements or captions in the timeline.";
  if (
    normalizeTimelineSelection(sequence, selection).length !== selection.length
  )
    return "The timeline selection is no longer available.";
  if (
    selection.some(
      (target) =>
        sequence.tracks.find((track) => track.id === target.track_id)?.locked,
    )
  )
    return "Unlock the selected tracks before editing their elements.";
  return null;
}
