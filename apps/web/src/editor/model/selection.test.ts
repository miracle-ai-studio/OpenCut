import { expect, it } from "vitest";
import { moveClips, type ClipSelection } from "./selection";
import type { Sequence } from "./types";

const time = (ticks: string, timebase: number) => ({ ticks, timebase });
function scene(): Sequence {
  return {
    id: "main",
    name: "Main",
    width: 640,
    height: 360,
    fps: { numerator: 30000, denominator: 1001 },
    tracks: [0, 1, 2, 3].map((index) => ({
      id: `track-${index}`,
      name: `Track ${index}`,
      kind: "video",
      enabled: true,
      muted: false,
      locked: false,
      params: {},
      clips:
        index < 2
          ? [
              {
                id: `clip-${index}`,
                asset_id: null,
                params: {},
                channels: [],
                source_range: null,
                timeline_range: {
                  start: time(index ? "1001" : "0", index ? 15000 : 1),
                  duration: time("1001", 30000),
                },
              },
            ]
          : [],
    })),
  };
}
const selected: ClipSelection[] = [
  { kind: "clip", trackId: "track-0", id: "clip-0" },
  { kind: "clip", trackId: "track-1", id: "clip-1" },
];
it("preserves exact mixed-base time and track distances in a single move batch", () => {
  expect(
    moveClips(scene(), selected, selected[0], "track-1", time("1001", 30000)),
  ).toMatchObject([
    {
      clip_id: "clip-0",
      destination_track_id: "track-1",
      start: time("1001", 30000),
    },
    {
      clip_id: "clip-1",
      destination_track_id: "track-2",
      start: time("1001", 10000),
    },
  ]);
});
it("clamps the earliest member at zero instead of independently clamping every clip", () => {
  expect(
    moveClips(scene(), selected, selected[1], "track-1", time("0", 1)),
  ).toEqual([]);
});
it("rejects the entire draft for a locked source or destination, incompatible type and out-of-range group", () => {
  const s = scene();
  s.tracks[2].locked = true;
  expect(() =>
    moveClips(s, selected, selected[0], "track-1", time("1", 1)),
  ).toThrow("Unlock");
  s.tracks[2].locked = false;
  s.tracks[1].locked = true;
  expect(() =>
    moveClips(s, selected, selected[0], "track-0", time("1", 1)),
  ).toThrow("Unlock");
  s.tracks[1].locked = false;
  s.tracks[2].kind = "audio";
  expect(() =>
    moveClips(s, selected, selected[0], "track-1", time("1", 1)),
  ).toThrow("compatible");
  expect(() =>
    moveClips(s, selected, selected[0], "track-3", time("1", 1)),
  ).toThrow("compatible");
});
it("rejects an empty or over-limit group before any operation is sent", () => {
  expect(() =>
    moveClips(scene(), [], selected[0], "track-0", time("1", 1)),
  ).toThrow("128");
  expect(() =>
    moveClips(
      scene(),
      Array(129).fill(selected[0]),
      selected[0],
      "track-0",
      time("1", 1),
    ),
  ).toThrow("128");
});
