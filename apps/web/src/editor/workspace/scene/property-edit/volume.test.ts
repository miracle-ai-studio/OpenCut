import { expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import { audioGainAvailability } from "../audio-gain";
import { fadeChannel, simpleFades } from "../gain-keyframes/fades";
import {
  authoredVolume,
  convertGainUnit,
  parseVolume,
  volumeLevel,
} from "./volume";
import type { SessionState } from "../types";

function clip(unit: "linear_legacy" | "decibels", animated = true) {
  const head = initialSnapshot(),
    seq = head.project.sequences[0],
    element = seq.tracks[0].elements[0];
  element.timeline_range.duration = { ticks: "42", timebase: 1 };
  element.content = {
    kind: "audio",
    asset_id: "asset",
    audio: {
      gain: { unit, value: unit === "decibels" ? -6 : 1.6921544822993657 },
      muted: false,
      stream: null,
    },
    retime: {
      maintain_pitch: true,
      rate: { numerator: 1, denominator: 1 },
      mapping: { kind: "classic_rounded_ticks", timebase: 120000 },
    },
  };
  if (animated) {
    element.channels = [
      fadeChannel(element, simpleFades(element)!, "fadeIn", "1.52")!,
    ];
    element.channels = [
      fadeChannel(element, simpleFades(element)!, "fadeOut", "3")!,
    ];
  }
  const state = { head, historical: null, sequenceId: seq.id } as SessionState;
  return audioGainAvailability(state, element.id)!;
}
it("edits a legacy fade plateau without converting its unit, moving keys or mutating the original", () => {
  const target = clip("linear_legacy"),
    before = structuredClone(target.element);
  const { expected, channel } = authoredVolume(target, 2);
  expect(expected.content).toEqual(before.content);
  expect(channel!.keys.map((k) => k.value)).toEqual([0, 2, 2, 0]);
  expect(channel!.keys.map(({ value: _, ...rest }) => rest)).toEqual(
    simpleFades(before)!.channel!.keys.map(({ value: _, ...rest }) => rest),
  );
  expect(target.element).toEqual(before);
});
it("interprets percent as physical amplitude while retaining dB interpolation and both exact fade times", () => {
  const target = clip("decibels"),
    old = simpleFades(target.element)!;
  const { expected, channel } = authoredVolume(
    target,
    parseVolume("percent", "50"),
  );
  expect(channel!.keys.map((k) => k.value)).toEqual([
    -60,
    20 * Math.log10(0.5),
    20 * Math.log10(0.5),
    -60,
  ]);
  expect(simpleFades(expected)!.fadeIn).toEqual(old.fadeIn);
  expect(simpleFades(expected)!.fadeOut).toEqual(old.fadeOut);
  expect(expected.content).toEqual(target.element.content);
});
it("does not collapse an animated envelope to its floor or fit a custom curve", () => {
  const legacy = clip("linear_legacy"),
    db = clip("decibels");
  expect(() => authoredVolume(legacy, 0)).toThrow(/mute/);
  expect(() => authoredVolume(db, 0.001)).toThrow(/mute/);
  const custom = clip("linear_legacy");
  if (custom.element.channels[0].kind !== "scalar") throw Error();
  custom.element.channels[0].keys[1].value = .5;
  expect(volumeLevel(custom)).toBeNull();
  expect(() => authoredVolume(custom, 2)).toThrow(/keyframe/);
});
it("allows real zero legacy static gain without confusing silence with dB zero", () => {
  const target = clip("linear_legacy", false),
    { expected } = authoredVolume(target, 0);
  if (expected.content.kind !== "audio") throw Error();
  expect(expected.content.audio.gain).toEqual({
    unit: "linear_legacy",
    value: 0,
  });
  expect(convertGainUnit("0", "decibels", "linear_legacy")).toBe("1");
  expect(convertGainUnit("1", "linear_legacy", "decibels")).toBe("0");
  expect(() => convertGainUnit("0", "linear_legacy", "decibels")).toThrow(
    /represented/,
  );
});
it("preserves untouched raw precision and refuses unsupported finite ranges", () => {
  expect(
    convertGainUnit("1.0000000000000002", "linear_legacy", "linear_legacy"),
  ).toBe("1.0000000000000002");
  expect(() => authoredVolume(clip("linear_legacy"), 5)).toThrow(/400/);
  for (const text of ["", " ", "NaN", "Infinity", "-1", "1001"])
    expect(() => parseVolume("percent", text)).toThrow();
});
