import {
  addTime,
  secondsForDisplay,
  subtractTime,
  timeFromDecimalSeconds,
} from "../../../model/time";
import type { Time } from "../../../model/types";
import type { Sequence } from "../types";
import type { TimelineTarget } from "./types";

export const TRACK_HEIGHT = 80;
export const TRACK_LABEL_WIDTH = 136;
export const RULER_HEIGHT = 24;
export const MARKER_HEIGHT = 22;
const PIXELS_PER_SECOND = 80;
export type TimelineRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};
export const pixelsPerSecond = (zoom: number) => PIXELS_PER_SECOND * zoom;
export const MIN_TIMELINE_ZOOM = 0.1;
export const MAX_TIMELINE_ZOOM = 8;
export const clampTimelineZoom = (zoom: number) =>
  Math.max(MIN_TIMELINE_ZOOM, Math.min(MAX_TIMELINE_ZOOM, zoom));
/** A uniform slider step changes the time scale by a uniform ratio. */
export const timelineZoomAt = (position: number) =>
  MIN_TIMELINE_ZOOM * (MAX_TIMELINE_ZOOM / MIN_TIMELINE_ZOOM) ** Math.max(0, Math.min(1, position));
export const timelineZoomPosition = (zoom: number) =>
  Math.log(clampTimelineZoom(zoom) / MIN_TIMELINE_ZOOM) / Math.log(MAX_TIMELINE_ZOOM / MIN_TIMELINE_ZOOM);

export function timelineZoomAnchor(scrollLeft: number, width: number, zoom: number, playhead: Time) {
  const available = Math.max(0, width - TRACK_LABEL_WIDTH);
  const x = secondsForDisplay(playhead) * pixelsPerSecond(zoom) - scrollLeft;
  return x >= 0 && x <= available ? x : available / 2;
}

/** Pointer coordinates are new scene-time intentions, never a source clock or
 * frame alignment. Existing exact endpoints are returned untouched when hit. */
export function pointerTime(
  seconds: number,
  edges: readonly Time[] = [],
  tolerance = 0,
): Time {
  if (!Number.isFinite(seconds)) throw new Error("Time must be finite");
  const positive = Math.max(0, seconds);
  let closest: Time | undefined,
    distance = tolerance;
  for (const edge of edges) {
    const delta = Math.abs(secondsForDisplay(edge) - positive);
    if (delta <= distance) {
      closest = edge;
      distance = delta;
    }
  }
  return closest ?? timeFromDecimalSeconds(positive.toFixed(6));
}

export function signedPointerDelta(seconds: number): Time {
  if (!Number.isFinite(seconds)) throw new Error("Time must be finite");
  const result = timeFromDecimalSeconds(Math.abs(seconds).toFixed(6));
  return seconds < 0
    ? { ...result, ticks: String(-BigInt(result.ticks)) }
    : result;
}

export function elementRects(
  sequence: Sequence,
  zoom: number,
  trackHeight = TRACK_HEIGHT,
): (TimelineRect & { target: TimelineTarget })[] {
  const scale = pixelsPerSecond(zoom);
  return sequence.tracks.flatMap((track, index) =>
    track.elements.map((element) => ({
      target: { track_id: track.id, element_id: element.id },
      x: secondsForDisplay(element.timeline_range.start) * scale,
      y: index * trackHeight,
      width: secondsForDisplay(element.timeline_range.duration) * scale,
      height: trackHeight,
    })),
  );
}

export function intersectingMedia(
  sequence: Sequence,
  zoom: number,
  rect: TimelineRect,
  trackHeight = TRACK_HEIGHT,
): TimelineTarget[] {
  const scale = pixelsPerSecond(zoom);
  const captions = sequence.captions.map((c) => ({
    target: { track_id: c.track_id, element_id: c.id },
    x: secondsForDisplay(c.timeline_range.start) * scale,
    y: sequence.tracks.findIndex((t) => t.id === c.track_id) * trackHeight,
    width: secondsForDisplay(c.timeline_range.duration) * scale,
    height: trackHeight,
  }));
  return [...elementRects(sequence, zoom, trackHeight), ...captions]
    .filter((item) => {
      const element =
        sequence.tracks
          .find((track) => track.id === item.target.track_id)!
          .elements.find((e) => e.id === item.target.element_id) ??
        sequence.captions.find((c) => c.id === item.target.element_id);
      return (
        !!element &&
        item.x < rect.x + rect.width &&
        item.x + item.width > rect.x &&
        item.y < rect.y + rect.height &&
        item.y + item.height > rect.y
      );
    })
    .map((item) => item.target);
}

export function sequenceEnd(sequence: Sequence): number {
  return Math.max(
    1,
    ...sequence.tracks.flatMap((track) =>
      track.elements.map((element) =>
        secondsForDisplay(
          addTime(
            element.timeline_range.start,
            element.timeline_range.duration,
          ),
        ),
      ),
    ),
    ...sequence.captions.map((caption) =>
      secondsForDisplay(
        addTime(caption.timeline_range.start, caption.timeline_range.duration),
      ),
    ),
    ...sequence.bookmarks.map((bookmark) =>
      secondsForDisplay(
        addTime(
          bookmark.time,
          bookmark.duration ?? { ticks: "0", timebase: 1 },
        ),
      ),
    ),
  );
}

export function sequenceEdges(
  sequence: Sequence,
  excluded: readonly TimelineTarget[] = [],
): Time[] {
  const ids = new Set(excluded.map((target) => target.element_id));
  return [
    ...sequence.tracks.flatMap((track) =>
      track.elements
        .filter((element) => !ids.has(element.id))
        .flatMap((element) => [
          element.timeline_range.start,
          addTime(
            element.timeline_range.start,
            element.timeline_range.duration,
          ),
        ]),
    ),
    ...sequence.captions
      .filter((c) => !ids.has(c.id))
      .flatMap((c) => [
        c.timeline_range.start,
        addTime(c.timeline_range.start, c.timeline_range.duration),
      ]),
  ];
}

export function anchoredScroll(
  scrollLeft: number,
  anchorX: number,
  oldZoom: number,
  newZoom: number,
): number {
  return Math.max(0, ((scrollLeft + anchorX) * newZoom) / oldZoom - anchorX);
}

export function moveDeltaAtEdge(
  sequence: Sequence,
  primary: TimelineTarget,
  rawDelta: Time,
  zoom: number,
  selected: TimelineTarget[],
): Time {
  const element =
    sequence.tracks
      .find((track) => track.id === primary.track_id)
      ?.elements.find((item) => item.id === primary.element_id) ??
    sequence.captions.find(
      (c) => c.id === primary.element_id && c.track_id === primary.track_id,
    );
  if (!element) throw new Error("The timeline target changed.");
  const start = addTime(element.timeline_range.start, rawDelta);
  let snapped: Time | undefined,
    distance = 8 / pixelsPerSecond(zoom);
  for (const edge of sequenceEdges(sequence, selected)) {
    const gap = Math.abs(secondsForDisplay(edge) - secondsForDisplay(start));
    if (gap <= distance) {
      snapped = edge;
      distance = gap;
    }
  }
  // No display-roundtrip of the imported rational start, even on a vertical move.
  return snapped
    ? subtractTime(snapped, element.timeline_range.start)
    : rawDelta;
}
