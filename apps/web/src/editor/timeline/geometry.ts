import type { Clip, Sequence, Time, VisualGraph } from "../model/types";
import {
  addTime,
  atFrame,
  compareTime,
  scaleTime,
  secondsForDisplay,
  subtractTime,
} from "../model/time";
export const PIXELS_PER_SECOND = 80;
export function duration(
  sequence: Sequence,
  shapes: VisualGraph["shapes"] = [],
) {
  return [
    ...sequence.tracks.flatMap((track) => track.clips),
    ...(sequence.caption_cues ?? []),
    ...shapes,
  ].reduce(
    (end, item) =>
      Math.max(
        end,
        secondsForDisplay(
          addTime(item.timeline_range.start, item.timeline_range.duration),
        ),
      ),
    0,
  );
}
export function snappedTime(
  seconds: number,
  sequence: Sequence,
  exclude: string | null,
  pixelsPerSecond: number,
  snap: boolean,
): Time {
  let result = seconds;
  if (snap) {
    let best = 8 / pixelsPerSecond;
    for (const track of sequence.tracks)
      for (const clip of track.clips) {
        if (clip.id === exclude) continue;
        for (const time of [
          clip.timeline_range.start,
          addTime(clip.timeline_range.start, clip.timeline_range.duration),
        ]) {
          const value = secondsForDisplay(time),
            difference = Math.abs(value - seconds);
          if (difference < best) {
            best = difference;
            result = value;
          }
        }
      }
  }
  return atFrame(Math.max(0, result), sequence.fps);
}
export function trimmedRange(clip: Clip, start: Time, end: Time) {
  if (compareTime(end, start) <= 0)
    throw new Error("A clip must contain at least one frame.");
  const range = { start, duration: subtractTime(end, start) };
  if (!clip.source_range) return { timeline_range: range, source_range: null };
  const delta = scaleTime(
    subtractTime(start, clip.timeline_range.start),
    clip.source_range.duration,
    clip.timeline_range.duration,
  );
  return {
    timeline_range: range,
    source_range: {
      start: addTime(clip.source_range.start, delta),
      duration: scaleTime(
        range.duration,
        clip.source_range.duration,
        clip.timeline_range.duration,
      ),
    },
  };
}
