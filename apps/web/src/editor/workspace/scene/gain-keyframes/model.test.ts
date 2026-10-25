import { expect, it } from "vitest";
import {
  gainChannel,
  dragKeys,
  sampleTimes,
  boundedTimeDelta,
  axisBounds,
  curveEdits,
  authoringPath,
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

it("keeps short fades sampled inside a long clip without exceeding the request budget", () => {
  const keys = gainFixture().keys;
  keys[1].time = { ticks: "152", timebase: 100 };
  const times = sampleTimes({ ticks: "420", timebase: 10 }, keys);
  expect(
    times.filter(
      (time) =>
        Number(time.ticks) / time.timebase > 0 &&
        Number(time.ticks) / time.timebase < 1.52,
    ).length,
  ).toBeGreaterThanOrEqual(15);
  const dense = Array.from({ length: 880 }, (_, index) => ({
    ...keys[0],
    id: String(index),
    time: { ticks: String(index), timebase: 100 },
  }));
  expect(
    sampleTimes({ ticks: "10", timebase: 1 }, dense).length,
  ).toBeLessThanOrEqual(1024);
});

it("smooths adjacent spans including a plateau through the shared curve intent without changing point identities", () => {
  const channel = gainFixture();
  channel.keys.push({
    ...structuredClone(channel.keys[1]),
    id: "third",
    time: { ticks: "5", timebase: 1 },
  });
  const original = structuredClone(channel);
  const edits = curveEdits(channel, true);
  expect(edits).toHaveLength(2);
  expect(edits).toMatchObject([
    {
      kind: "curve",
      left_key_id: "first",
      right_key_id: "second",
      reference_span_value: 1,
      curve: { y1: 0, y2: 1 },
    },
    {
      kind: "curve",
      left_key_id: "second",
      right_key_id: "third",
      reference_span_value: 1,
      curve: { y1: 0, y2: 1 },
    },
  ]);
  expect(channel).toEqual(original);
  for (const [index, edit] of curveEdits(channel, false).entries()) {
    if (edit.kind !== "put_scalar") throw Error();
    expect(edit.key).toMatchObject({
      id: original.keys[index].id,
      time: original.keys[index].time,
      value: original.keys[index].value,
      segment_to_next: { kind: "linear" },
      left_handle: null,
      right_handle: null,
    });
  }
});

it("bounds point movement against both unchanged neighboring tangents", () => {
  const keys = gainFixture().keys;
  keys[0].right_handle = { dt: { ticks: "1", timebase: 1 }, dv: 0 };
  keys[1].left_handle = { dt: { ticks: "-1", timebase: 2 }, dv: 0 };
  expect(
    Number(
      boundedTimeDelta(keys, new Set(["first"]), { ticks: "5", timebase: 1 }, 9)
        .ticks,
    ) / 120000,
  ).toBeCloseTo(1);
  expect(
    Number(
      boundedTimeDelta(
        keys,
        new Set(["second"]),
        { ticks: "5", timebase: 1 },
        -9,
      ).ticks,
    ) / 120000,
  ).toBeCloseTo(-1);
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
  } as import("../types").Element;
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

it("keeps the authored linear extension visible throughout a pointer draft",()=>{
 const c=gainFixture(),keys=structuredClone(c.keys);
 const path=authoringPath(keys,c.extrapolation,0,5,n=>n,n=>n);
 expect(path).toMatch(/ L 5 17.5$/);
 expect(authoringPath(keys,{before:"hold",after:"hold"},0,5,n=>n,n=>n)).toMatch(/ L 5 -5$/);
 expect(keys).toEqual(c.keys);
});
