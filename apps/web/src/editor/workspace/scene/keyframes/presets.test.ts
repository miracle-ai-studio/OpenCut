import { expect, it } from "vitest";
import { BUILTIN_PRESETS, parsePresets } from "./presets";
it("preserves Classic Pop overshoot without widening the old normalized curve type", () => {
  expect(BUILTIN_PRESETS.find((p) => p.id === "pop")?.curve).toEqual({
    x1: 0.175,
    y1: 0.885,
    x2: 0.32,
    y2: 1.275,
  });
});
it("validates only bounded preference values, not project state or arbitrary blobs", () => {
  const item = {
    id: "test",
    name: "Snap",
    curve: { x1: 0, y1: -0.5, x2: 1, y2: 1.5 },
  };
  expect(parsePresets(JSON.stringify([item]))).toEqual([item]);
  for (const bad of [
    [{ ...item, project: { id: "secret" } }],
    [{ ...item, curve: { ...item.curve, y2: 1.6 } }],
    [item, item],
    Array.from({ length: 33 }, (_, i) => ({ ...item, id: `p${i}` })),
    [{ ...item, name: "x".repeat(65) }],
  ])
    expect(() => parsePresets(JSON.stringify(bad))).toThrow();
  expect(() => parsePresets(" ".repeat(32769))).toThrow();
});
