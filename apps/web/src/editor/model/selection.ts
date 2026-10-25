import type { Selection, Sequence, Snapshot } from "./types";
import { compareTime } from "./time";

export type ElementSelection = NonNullable<Selection>;
export type ClipSelection = Extract<ElementSelection, { kind: "clip" }>;
export const sameSelection = (a: ElementSelection, b: ElementSelection) =>
  a.kind === b.kind &&
  a.id === b.id &&
  (a.kind !== "clip" || (b.kind === "clip" && a.trackId === b.trackId));

export function validSelections(
  view: Snapshot | null,
  sequenceId: string | null,
  selections: ElementSelection[],
) {
  const sequence = view?.project.sequences.find((s) => s.id === sequenceId);
  return selections.filter(
    (item, index) =>
      selections.findIndex((other) => sameSelection(item, other)) === index &&
      (item.kind === "clip"
        ? sequence?.tracks.some(
            (track) =>
              track.id === item.trackId &&
              track.clips.some((clip) => clip.id === item.id),
          )
        : item.kind === "caption"
          ? sequence?.caption_cues?.some((cue) => cue.id === item.id)
          : view?.visual
              .find((graph) => graph.sequence_id === sequenceId)
              ?.shapes.some((shape) => shape.id === item.id)),
  );
}

export function clipSelections(sequence: Sequence): ClipSelection[] {
  return sequence.tracks.flatMap((track) =>
    [...track.clips]
      .sort(
        (a, b) =>
          compareTime(a.timeline_range.start, b.timeline_range.start) ||
          a.id.localeCompare(b.id),
      )
      .map((clip) => ({
        kind: "clip" as const,
        trackId: track.id,
        id: clip.id,
      })),
  );
}

