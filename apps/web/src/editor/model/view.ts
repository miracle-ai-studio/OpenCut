import type { Snapshot, Time } from "./types";
import { addTime, compareTime, zero } from "./time";

export function restoredPlayhead(
  snapshot: Snapshot,
  sequenceId: string,
  requested: Time,
): Time {
  const scene = snapshot.project.sequences.find((s) => s.id === sequenceId);
  if (!scene) return zero;
  try {
    let end = zero;
    for (const item of [
      ...scene.tracks.flatMap((track) => track.clips),
      ...(scene.caption_cues ?? []),
      ...(snapshot.visual.find((graph) => graph.sequence_id === sequenceId)
        ?.shapes ?? []),
    ]) {
      const time = addTime(
        item.timeline_range.start,
        item.timeline_range.duration,
      );
      if (compareTime(time, end) > 0) end = time;
    }
    if (compareTime(end, zero) <= 0 || compareTime(requested, zero) < 0)
      return zero;
    const numerator = BigInt(end.ticks) * BigInt(scene.fps.numerator);
    const denominator = BigInt(end.timebase) * BigInt(scene.fps.denominator);
    const frame = (numerator - 1n) / denominator;
    const last = addTime(
      {
        ticks: String(frame * BigInt(scene.fps.denominator)),
        timebase: scene.fps.numerator,
      },
      zero,
    );
    return compareTime(requested, last) <= 0 ? requested : last;
  } catch {
    return zero;
  }
}
