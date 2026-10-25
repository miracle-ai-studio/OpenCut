import { expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import type { Element } from "../types";
import { fadeChannel, simpleFades } from "./fades";
import { seconds } from "../keyframes/types";

function clip(unit: "decibels" | "linear_legacy" = "linear_legacy"): Element {
  const element = initialSnapshot().project.sequences[0].tracks[0].elements[0];
  element.timeline_range.duration = { ticks: "1050", timebase: 25 };
  element.content = {
    kind: "audio",
    asset_id: "asset",
    audio: {
      gain: { unit, value: unit === "decibels" ? -6 : 1.6921544822993657 },
      stream: null,
      muted: false,
    },
    retime: {
      maintain_pitch: false,
      rate: { numerator: 1, denominator: 1 },
      mapping: { kind: "classic_rounded_ticks", timebase: 120000 },
    },
  };
  return element;
}
function envelope(unit: "decibels" | "linear_legacy" = "linear_legacy") {
  const e = clip(unit);
  const m = simpleFades(e)!;
  const a = fadeChannel(e, m, "fadeIn", "1.52")!;
  e.channels = [a];
  e.channels = [fadeChannel(e, simpleFades(e)!, "fadeOut", "3")!];
  return e;
}
it("reads the original legacy envelope in clip-local seconds without changing its unit or level", () => {
  const e = envelope(),
    before = structuredClone(e),
    m = simpleFades(e)!;
  expect(seconds(m.fadeIn)).toBe(1.52);
  expect(seconds(m.fadeOut)).toBe(3);
  expect(m.level).toBe(1.6921544822993657);
  expect(m.parameter).toBe("gain_linear_legacy");
  expect(e).toEqual(before);
});
it.each(["decibels", "linear_legacy"] as const)(
  "zero fades and touching fades retain the existing %s gain semantics",
  (unit) => {
    const e = envelope(unit),
      m = simpleFades(e)!;
    e.channels = [fadeChannel(e, m, "fadeIn", "39")!];
    expect(simpleFades(e)!.channel!.keys).toHaveLength(3);
    e.channels = [fadeChannel(e, simpleFades(e)!, "fadeIn", "0")!];
    e.channels = [fadeChannel(e, simpleFades(e)!, "fadeOut", "0")!];
    const result = simpleFades(e)!;
    expect(result.channel!.keys).toHaveLength(2);
    expect(result.channel!.keys.every((k) => k.value === m.level)).toBe(true);
  },
);
it("does not quantize the untouched exact fade when only the other field changes", () => {
  const e = envelope();
  const c = simpleFades(e)!.channel!;
  c.keys[1].time = { ticks: "1", timebase: 3 };
  const next = fadeChannel(e, simpleFades(e)!, "fadeOut", "2")!;
  expect(next.keys[1].time).toEqual({ ticks: "1", timebase: 3 });
  expect(next.keys[0].id).toBe(c.keys[0].id);
  expect(next.keys.at(-1)!.id).toBe(c.keys.at(-1)!.id);
});
it.each(["-1", "-0.000001", "", "NaN", "Infinity", "40"])(
  "rejects invalid/overlapping input %s without editing source",
  (value) => {
    const e = envelope(),
      before = structuredClone(e);
    expect(() => fadeChannel(e, simpleFades(e)!, "fadeIn", value)).toThrow();
    expect(e).toEqual(before);
  },
);
it("treats an unchanged value as zero work, including no new IDs", () => {
  const e = envelope();
  expect(
    fadeChannel(e, simpleFades(e)!, "fadeOut", "3", () => {
      throw Error("new id");
    }),
  ).toBeNull();
});
it.each([
  "bezier",
  "handle",
  "extra-level",
  "trimmed",
  "duplicate-channel",
  "wrong-unit",
  "extrapolation",
])("preserves custom %s curves by refusing simplification", (kind) => {
  const e = envelope(),
    c = simpleFades(e)!.channel!;
  if (kind === "bezier") c.keys[0].segment_to_next = { kind: "bezier" };
  if (kind === "handle")
    c.keys[1].left_handle = { dt: { ticks: "-1", timebase: 4 }, dv: -0.1 };
  if (kind === "extra-level") c.keys[2].value *= 0.9;
  if (kind === "trimmed")
    e.timeline_range.duration = { ticks: "40", timebase: 1 };
  if (kind === "duplicate-channel")
    e.channels.push({ ...structuredClone(c), id: "other" });
  if (kind === "wrong-unit")
    c.binding = { kind: "property", parameter: "gain_decibels" };
  if (kind === "extrapolation") c.extrapolation.after = "linear";
  expect(simpleFades(e)).toBeNull();
});
