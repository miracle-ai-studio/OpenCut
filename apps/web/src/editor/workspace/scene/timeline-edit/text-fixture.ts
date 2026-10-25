import { initialSnapshot } from "../fixture";
import { textInitial } from "../text/fixture";

/** Authored text fixture, not a replica of timeline planning rules. */
export function textTimelineSequence() {
  const sequence = initialSnapshot().project.sequences[0];
  const track = structuredClone(sequence.tracks[0]);
  track.id = "titles";
  track.name = "Titles";
  track.kind = "text";
  const element = track.elements[0];
  element.id = "title";
  element.name = "Independent title";
  element.content = { kind: "text", text: textInitial().text };
  element.source_span = { profile: "classic_trimmed", trim_start: { ticks: "0", timebase: 1 }, trim_end: { ticks: "0", timebase: 1 }, source_duration: null };
  track.elements = [element];
  sequence.tracks.push(track);
  const second = structuredClone(track);
  second.id = "titles-above";
  second.name = "Titles above";
  second.elements = [];
  sequence.tracks.push(second);
  return sequence;
}
