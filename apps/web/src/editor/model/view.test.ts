import { describe, expect, it } from "vitest";
import { restoredPlayhead } from "./view";
import type { Snapshot, Time } from "./types";
const fixture = (
  duration: Time,
  fps = { numerator: 30, denominator: 1 },
): Snapshot => ({
  format: "visual",
  revision: "r",
  curves: [],
  visual: [],
  project: {
    id: "p",
    name: "P",
    schema_version: 2,
    default_sequence_id: "main",
    assets: [],
    sequences: [
      {
        id: "main",
        name: "Main",
        width: 640,
        height: 360,
        fps,
        caption_cues: [],
        tracks: [
          {
            id: "v",
            name: "Video",
            kind: "video",
            enabled: true,
            muted: false,
            locked: false,
            params: {},
            clips: [
              {
                id: "c",
                asset_id: null,
                source_range: null,
                params: {},
                channels: [],
                timeline_range: {
                  start: { ticks: "0", timebase: 1 },
                  duration,
                },
              },
            ],
          },
        ],
      },
    ],
  },
});
describe("restored playhead", () => {
  it("clamps to the last frame before the exclusive end and preserves in-range time", () => {
    const project = fixture({ ticks: "7", timebase: 3 });
    expect(
      restoredPlayhead(project, "main", { ticks: "10", timebase: 1 }),
    ).toEqual({ ticks: "23", timebase: 10 });
    expect(
      restoredPlayhead(project, "main", { ticks: "3", timebase: 2 }),
    ).toEqual({ ticks: "3", timebase: 2 });
    expect(
      restoredPlayhead(project, "deleted", { ticks: "3", timebase: 2 }),
    ).toEqual({ ticks: "0", timebase: 1 });
  });
  it("uses exact large ticks and fractional-rate frames without a floating-point round trip", () => {
    expect(
      restoredPlayhead(
        fixture(
          { ticks: "9007199254740994", timebase: 1 },
          { numerator: 1, denominator: 1 },
        ),
        "main",
        { ticks: "9007199254740995", timebase: 1 },
      ),
    ).toEqual({ ticks: "9007199254740993", timebase: 1 });
    expect(
      restoredPlayhead(
        fixture(
          { ticks: "6", timebase: 1 },
          { numerator: 30000, denominator: 1001 },
        ),
        "main",
        { ticks: "10", timebase: 1 },
      ),
    ).toEqual({ ticks: "179179", timebase: 30000 });
  });
});
