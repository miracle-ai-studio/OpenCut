import type {
  Selection,
  Sequence,
  Snapshot,
  Operation,
  Time,
  VisualOperation,
} from "./types";
import { addTime, compareTime, subtractTime, zero } from "./time";

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

/** Resolve visual lock targets before removing their base clips. */
export function deleteClips(
  view: Snapshot,
  sequenceId: string,
  selections: ClipSelection[],
): VisualOperation[] {
  const ids = new Set(selections.map((item) => item.id));
  return [
    ...deleteTargetNodes(view, sequenceId, ids),
    ...selections.map((item) => ({
      type: "base.apply" as const,
      operation: {
        type: "clip.delete" as const,
        sequence_id: sequenceId,
        track_id: item.trackId,
        clip_id: item.id,
      },
    })),
  ];
}

/** Explicit removal intent; Rust validates the complete atomic batch and locks. */
export function deleteTrack(
  view: Snapshot,
  sequenceId: string,
  trackId: string,
): VisualOperation[] {
  const track = view.project.sequences
    .find((sequence) => sequence.id === sequenceId)
    ?.tracks.find((track) => track.id === trackId);
  if (!track) throw new Error("This track no longer exists.");
  if (track.locked) throw new Error("Unlock this track before deleting it.");
  const shapes =
    view.visual
      .find((graph) => graph.sequence_id === sequenceId)
      ?.shapes.filter((shape) => shape.track_id === trackId) ?? [];
  const ids = new Set([
    ...track.clips.map((clip) => clip.id),
    ...shapes.map((shape) => shape.id),
  ]);
  const operations: VisualOperation[] = [
    ...deleteTargetNodes(view, sequenceId, ids),
    ...shapes.map((shape) => ({
      type: "shape.delete" as const,
      sequence_id: sequenceId,
      id: shape.id,
    })),
    ...track.clips.map((clip) => ({
      type: "base.apply" as const,
      operation: {
        type: "clip.delete" as const,
        sequence_id: sequenceId,
        track_id: trackId,
        clip_id: clip.id,
      },
    })),
    {
      type: "base.apply",
      operation: {
        type: "track.delete",
        sequence_id: sequenceId,
        track_id: trackId,
      },
    },
  ];
  if (operations.length > 128)
    throw new Error(
      "This track exceeds the atomic edit limit. Remove some elements first.",
    );
  return operations;
}

function deleteTargetNodes(
  view: Snapshot,
  sequenceId: string,
  ids: Set<string>,
): VisualOperation[] {
  const graph = view.visual.find((graph) => graph.sequence_id === sequenceId);
  return [
    ...(graph?.effects
      .filter((effect) => ids.has(effect.target_id))
      .map((effect) => ({
        type: "effect.delete" as const,
        sequence_id: sequenceId,
        id: effect.id,
      })) ?? []),
    ...(graph?.masks
      .filter((mask) => ids.has(mask.target_id))
      .map((mask) => ({
        type: "mask.delete" as const,
        sequence_id: sequenceId,
        id: mask.id,
      })) ?? []),
    ...(graph?.transitions
      .filter(
        (item) =>
          ids.has(item.outgoing_clip_id) || ids.has(item.incoming_clip_id),
      )
      .map((item) => ({
        type: "transition.delete" as const,
        sequence_id: sequenceId,
        id: item.id,
      })) ?? []),
    ...view.curves
      .filter(
        (curve) => curve.sequence_id === sequenceId && ids.has(curve.clip_id),
      )
      .map((curve) => ({
        type: "keyframe.curve_delete" as const,
        sequence_id: sequenceId,
        id: curve.id,
      })),
  ];
}

/** UI intent only. Authorization, fixed revision and atomic application stay in Rust. */
export function moveClips(
  sequence: Sequence,
  selections: ClipSelection[],
  anchor: ClipSelection,
  destinationTrackId: string,
  start: Time,
): Operation[] {
  if (!selections.length || selections.length > 128)
    throw new Error("Select between 1 and 128 clips.");
  const sourceIndex = sequence.tracks.findIndex(
    (track) => track.id === anchor.trackId,
  );
  const targetIndex = sequence.tracks.findIndex(
    (track) => track.id === destinationTrackId,
  );
  const anchorClip = sequence.tracks[sourceIndex]?.clips.find(
    (clip) => clip.id === anchor.id,
  );
  if (!anchorClip || targetIndex < 0)
    throw new Error("Choose a compatible track.");
  const entries = selections.map((item) => {
    const index = sequence.tracks.findIndex(
      (track) => track.id === item.trackId,
    );
    const track = sequence.tracks[index];
    const clip = track?.clips.find((clip) => clip.id === item.id);
    const destination = sequence.tracks[index + targetIndex - sourceIndex];
    if (!clip || !destination || track.kind !== destination.kind)
      throw new Error("Choose a compatible track.");
    if (track.locked || destination.locked)
      throw new Error(
        "Unlock the selected and target tracks before moving clips.",
      );
    return { track, clip, destination };
  });
  let delta = subtractTime(start, anchorClip.timeline_range.start);
  const earliest = entries.reduce(
    (time, entry) =>
      compareTime(entry.clip.timeline_range.start, time) < 0
        ? entry.clip.timeline_range.start
        : time,
    anchorClip.timeline_range.start,
  );
  if (compareTime(addTime(earliest, delta), zero) < 0)
    delta = subtractTime(zero, earliest);
  return entries.flatMap(({ track, clip, destination }) => {
    const time = addTime(clip.timeline_range.start, delta);
    return track.id === destination.id &&
      compareTime(time, clip.timeline_range.start) === 0
      ? []
      : [
          {
            type: "clip.move" as const,
            sequence_id: sequence.id,
            track_id: track.id,
            clip_id: clip.id,
            destination_track_id: destination.id,
            start: time,
          },
        ];
  });
}
