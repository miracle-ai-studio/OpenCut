import { expect, it, vi } from "vitest";
import {
  copySelection,
  parseClipboard,
  pasteInto,
  pasteSelection,
} from "./clipboard";
import { difference, inputTime, refKey } from "./types";
import type { Row, ScalarChannel } from "./types";
function row(id: string, start: string, local: string): Row {
  const channel: ScalarChannel = {
    id,
    kind: "scalar",
    target: { kind: "element", element_id: "old" },
    binding: { kind: "property", parameter: "position_x_px" },
    extrapolation: { before: "linear", after: "hold" },
    keys: [
      {
        id: `key-${id}`,
        time: { ticks: local, timebase: 3 },
        value: 25,
        left_handle: { dt: { ticks: "-1", timebase: 9 }, dv: -2 },
        right_handle: { dt: { ticks: "1", timebase: 9 }, dv: 45 },
        segment_to_next: { kind: "bezier" },
        tangent_mode: "aligned",
      },
    ],
  };
  return {
    channel,
    owner: { kind: "element", element_id: "old" },
    label: id,
    start: { ticks: start, timebase: 3 },
    duration: { ticks: "30", timebase: 1 },
  };
}
it("keeps exact times without Number coercion and explicit signed authoring quantization", () => {
  expect(
    difference(
      { ticks: "9007199254740993", timebase: 3 },
      { ticks: "9007199254740992", timebase: 3 },
    ),
  ).toEqual({ ticks: "1", timebase: 3 });
  expect(inputTime("-0.0000125")).toEqual({ ticks: "-2", timebase: 120000 });
});
it("cross-element copy uses the first selected scene time and retains all tangent metadata", () => {
  const rows = [row("a", "3", "1"), row("b", "4", "2")];
  const selection = new Set(
    rows.map((r) =>
      refKey({ channel_id: r.channel.id, key_id: r.channel.keys[0].id }),
    ),
  );
  const clip = copySelection(rows, selection);
  expect(clip.entries[0].channel.keys[0].time).toEqual({
    ticks: "0",
    timebase: 1,
  });
  expect(clip.entries[1].channel.keys[0].time).toEqual({
    ticks: "2",
    timebase: 3,
  });
  expect(clip.entries[0].channel.keys[0]).toMatchObject({
    left_handle: { dt: { ticks: "-1", timebase: 9 }, dv: -2 },
    right_handle: { dt: { ticks: "1", timebase: 9 }, dv: 45 },
    tangent_mode: "aligned",
  });
});
it("paste allocates fresh identities once and delegates time rebasing to Rust", () => {
  vi.stubGlobal("crypto", {
    randomUUID: vi
      .fn()
      .mockReturnValueOnce("new-key")
      .mockReturnValueOnce("new-channel"),
  });
  try {
    const source = row("a", "0", "1").channel,
      target = {
        ...source,
        id: "dest",
        target: { kind: "element" as const, element_id: "dest-element" },
      };
    const edit = pasteInto(source, target, { ticks: "7", timebase: 3 });
    expect(edit).toMatchObject({
      kind: "paste_scalar",
      channel_id: "dest",
      source_anchor: { ticks: "0", timebase: 1 },
      destination_anchor: { ticks: "7", timebase: 3 },
      keys: [{ id: "new-key", time: { ticks: "1", timebase: 3 } }],
    });
    if (edit.kind !== "paste_scalar") throw Error("scalar");
    expect(edit.keys[0].right_handle).toEqual(
      (source as ScalarChannel).keys[0].right_handle,
    );
    expect(source.keys[0].id).toBe("key-a");
  } finally {
    vi.unstubAllGlobals();
  }
});
it("rejects incompatible targets and malformed clipboard size rather than dropping channels", () => {
  const source = row("a", "0", "1").channel;
  const target = {
    ...source,
    binding: { kind: "property" as const, parameter: "opacity" as const },
  } as ScalarChannel;
  expect(() => pasteInto(source, target, { ticks: "0", timebase: 1 })).toThrow(
    "parameter types differ",
  );
  const effect = {
    ...source,
    target: { kind: "effect" as const, element_id: "e", effect_id: "fx" },
  };
  expect(() =>
    pasteSelection(
      { entries: [{ channel: effect }] },
      [],
      "e",
      { ticks: "0", timebase: 1 },
    ),
  ).toThrow("explicitly selected");
  expect(() => parseClipboard(" ".repeat(524289))).toThrow("512 KiB");
  expect(() =>
    parseClipboard(
      '{"opencut_keyframes_v1":{"schema_version":1,"entries":[]}}',
    ),
  ).toThrow();
});
