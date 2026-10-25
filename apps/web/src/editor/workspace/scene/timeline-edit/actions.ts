import { addTime, compareTime, zero } from "../../../model/time";
import type { Time } from "../../../model/types";
import type { Sequence } from "../types";
import type { TimelineCommand, TimelineTarget } from "./types";
import {
  normalizeTimelineSelection,
  timelineSelectionReason,
} from "./selection";

export function moveSelectionCommand(
  sequence: Sequence,
  selected: TimelineTarget[],
  delta: Time,
  trackOffset: number,
  ripple: boolean,
): TimelineCommand {
  const targets = normalizeTimelineSelection(sequence, selected);
  const reason = timelineSelectionReason(sequence, selected);
  if (reason || targets.length !== selected.length)
    throw new Error(reason ?? "The timeline target changed.");
  const moves = targets.map((target) => {
    const index = sequence.tracks.findIndex(
      (track) => track.id === target.track_id,
    );
    const source = sequence.tracks[index],
      destination = sequence.tracks[index + trackOffset];
    const element =
      source.elements.find((item) => item.id === target.element_id) ??
      sequence.captions.find(
        (c) => c.id === target.element_id && c.track_id === source.id,
      );
    if (!element) throw new Error("The timeline target changed.");
    if (!destination)
      throw new Error(
        "The selection needs a destination track for every element.",
      );
    if (destination.locked) throw new Error("The destination track is locked.");
    const compatible = !("content" in element)
      ? "text" in element && destination.kind === "caption"
      : (element.content.kind === "audio"
            ? destination.kind === "audio"
            : element.content.kind === "video"
              ? destination.kind === "video"
              : element.content.kind === "text"
                ? destination.kind === "text"
                : element.content.kind === "graphic"
                  ? destination.kind === "graphic"
                  : element.content.kind === "effect_layer"
                    ? destination.kind === "effect"
                    : destination.kind === "video" ||
                      destination.kind === "graphic");
    if (!compatible)
      throw new Error(
        "The destination track does not accept this element type.",
      );
    const start = addTime(element.timeline_range.start, delta);
    if (compareTime(start, zero) < 0)
      throw new Error("The selection cannot start before the scene.");
    return { target, target_track_id: destination.id, new_start: start };
  });
  return { kind: "move_selection", moves, ripple };
}

export function shortcutCommand(
  key: string,
  sequence: Sequence,
  selected: TimelineTarget[],
  at: Time,
  ripple: boolean,
  id: () => string = () => crypto.randomUUID(),
): TimelineCommand | null {
  const lower = key.toLowerCase();
  if (!["s", "q", "w", "delete", "backspace"].includes(lower)) return null;
  const reason = timelineSelectionReason(sequence, selected);
  if (reason) throw new Error(reason);
  const targets = normalizeTimelineSelection(sequence, selected);
  if (lower === "delete" || lower === "backspace")
    return { kind: "delete", targets, ripple };
  return {
    kind: "split",
    targets: targets.map((target) => ({ target, right_element_id: id() })),
    at,
    retain: lower === "q" ? "right" : lower === "w" ? "left" : "both",
    ripple,
  };
}

export function ignoresTimelineShortcut(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    !!target.closest(
      "input,textarea,select,button,a,[contenteditable=true],[role=slider],[role=dialog],[role=menu]",
    )
  );
}
