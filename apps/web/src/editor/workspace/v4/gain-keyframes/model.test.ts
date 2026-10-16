import { expect, it } from "vitest";
import {
  gainChannel,
  dragKeys,
  sampleTimes,
  boundedTimeDelta,
  axisBounds,
} from "./model";
import { gainFixture } from "./fixture";
it("builds bounded exact sampling intentions without evaluating curves in the client", () => {
  const times = sampleTimes({ ticks: "5", timebase: 1 }, gainFixture().keys, {
    ticks: "1001",
    timebase: 30000,
  });
  expect(times.length).toBeGreaterThanOrEqual(129);
  expect(times.length).toBeLessThan(1024);
  expect(times).toContainEqual({ ticks: "1001", timebase: 30000 });
  expect(times.at(-1)).toEqual({ ticks: "600000", timebase: 120000 });
});
it("offsets all selected key values preserving every original tangent, segment and extrapolation owner", () => {
  const channel = gainFixture(),
    before = structuredClone(channel),
    edits = dragKeys(
      channel,
      new Set(["first", "second"]),
      { ticks: "0", timebase: 1 },
      2.5,
    );
  for (let i = 0; i < edits.length; i++) {
    const edit = edits[i];
    if (edit.kind !== "put_scalar") throw Error();
    expect(edit.key).toEqual({
      ...before.keys[i],
      value: before.keys[i].value + 2.5,
    });
  }
  expect(channel).toEqual(before);
});
it("does not rewrite an untouched rational timestamp or unselected key", () => {
  const channel = gainFixture(),
    edits = dragKeys(
      channel,
      new Set(["first"]),
      { ticks: "0", timebase: 120000 },
      0,
    );
  expect(edits).toEqual([
    { kind: "put_scalar", channel_id: "gain", key: channel.keys[0] },
  ]);
  const moved = dragKeys(
    channel,
    new Set(["second"]),
    { ticks: "1001", timebase: 30000 },
    0,
  )[0];
  if (moved.kind !== "put_scalar") throw Error();
  expect(moved.key.time).toEqual({ ticks: "61001", timebase: 30000 });
  expect(moved.key.left_handle).toEqual(channel.keys[1].left_handle);
});
it("constrains pointer time intents to the element extent and retains overshoot in the displayed axis", () => {
  const channel = gainFixture();
  expect(
    boundedTimeDelta(
      channel.keys,
      new Set(["first", "second"]),
      { ticks: "5", timebase: 1 },
      -100,
    ).ticks,
  ).toBe("0");
  expect(axisBounds(channel, [{ value: 24 }])).toEqual({
    min: -60,
    max: 24,
    unit: "dB",
  });
});

it("only exposes one gain channel matching the element's current gain unit", () => {
  const audio = {
    id: "audio",
    content: { kind: "audio", audio: { gain: { unit: "decibels", value: 0 } } },
    channels: [gainFixture()],
  } as import("../types").V4Element;
  expect(gainChannel(audio)?.id).toBe("gain");
  const other = gainFixture();
  other.id = "legacy";
  other.binding = { kind: "property", parameter: "gain_linear_legacy" };
  audio.channels.push(other);
  expect(gainChannel(audio)?.id).toBe("gain");
  if (audio.content.kind !== "audio") throw Error();
  audio.content.audio.gain = { unit: "linear_legacy", value: 1 };
  expect(gainChannel(audio)?.id).toBe("legacy");
  audio.channels.push({ ...structuredClone(other), id: "duplicate" });
  expect(gainChannel(audio)).toBeNull();
});
