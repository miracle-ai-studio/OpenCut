import { describe, expect, it } from "vitest";
import { placeAsset } from "./edit-requests";
import {
  addTime,
  atFrame,
  compareTime,
  fromSeconds,
  scaleTime,
  timeFromDecimalSeconds,
} from "./time";
import { duration, trimmedRange, snappedTime } from "../timeline/geometry";
import type { Asset, Clip, Sequence } from "./types";
const clip: Clip = {
  id: "clip",
  asset_id: "asset",
  params: {},
  channels: [],
  timeline_range: { start: fromSeconds(2), duration: fromSeconds(4) },
  source_range: { start: fromSeconds(1), duration: fromSeconds(8) },
};
const sequence: Sequence = {
  id: "main",
  name: "Main",
  width: 1920,
  height: 1080,
  fps: { numerator: 24000, denominator: 1001 },
  caption_cues: [],
  tracks: [
    {
      id: "track",
      name: "Video",
      kind: "video",
      locked: false,
      muted: false,
      enabled: true,
      params: {},
      clips: [clip],
    },
  ],
};
describe("exact editor time", () => {
  it("preserves large tick values and rational frame rates", () => {
    expect(
      addTime(
        { ticks: "9007199254740993", timebase: 1 },
        { ticks: "1", timebase: 1 },
      ).ticks,
    ).toBe("9007199254740994");
    expect(atFrame(1, sequence.fps)).toEqual({ ticks: "1001", timebase: 1000 });
    expect(timeFromDecimalSeconds("12.125")).toEqual({
      ticks: "97",
      timebase: 8,
    });
  });
  it("scales source trims at the original playback speed", () => {
    const result = trimmedRange(clip, fromSeconds(3), fromSeconds(5));
    expect(result.timeline_range).toEqual({
      start: fromSeconds(3),
      duration: fromSeconds(2),
    });
    expect(result.source_range).toEqual({
      start: fromSeconds(3),
      duration: fromSeconds(4),
    });
    expect(() => trimmedRange(clip, fromSeconds(4), fromSeconds(4))).toThrow();
  });
  it("rejects unrepresentable rational time", () => {
    expect(() =>
      scaleTime(fromSeconds(1), fromSeconds(1), fromSeconds(0)),
    ).toThrow();
  });
  it("only snaps within the pointer tolerance and excludes the dragged clip", () => {
    expect(
      compareTime(
        snappedTime(2.02, sequence, null, 80, true),
        atFrame(2, sequence.fps),
      ),
    ).toBe(0);
    expect(
      compareTime(
        snappedTime(2.2, sequence, null, 80, true),
        atFrame(2.2, sequence.fps),
      ),
    ).toBe(0);
  });
  it("includes captions and shapes in the visible timeline length", () => {
    const caption = {
      id: "cue",
      text: "test",
      timeline_range: { start: fromSeconds(20), duration: fromSeconds(2) },
      style: {
        font_family: "Arial",
        font_size: 24,
        color: "#ffffff",
        text_align: "center" as const,
      },
    };
    expect(duration({ ...sequence, caption_cues: [caption] })).toBe(22);
  });
  it("appends media exactly and rejects incompatible or locked tracks", () => {
    const asset = {
      id: "new",
      name: "new",
      kind: "video",
      duration: fromSeconds(3),
    } as Asset;
    expect(placeAsset(asset, sequence).at(-1)).toMatchObject({
      clip: { timeline_range: { start: fromSeconds(6) } },
    });
    expect(() =>
      placeAsset({ ...asset, kind: "audio" }, sequence, undefined, "track"),
    ).toThrow("compatible");
    expect(() =>
      placeAsset(
        asset,
        { ...sequence, tracks: [{ ...sequence.tracks[0], locked: true }] },
        undefined,
        "track",
      ),
    ).toThrow("Unlock");
  });
});
