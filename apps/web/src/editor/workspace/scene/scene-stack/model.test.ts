import { describe, expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import {
  createMask,
  createBlur,
  move,
  moveAnchor,
  moveEffectTrack,
  newEffectTrack,
  removeMask,
  removeEffect,
  removePathPoints,
  splitPathSegment,
  validateStack,
} from "./model";
import type { Point } from "./types";
import { geometry, font } from "./fixture";
describe("Classic stack authoring intents", () => {
  it("takes sized defaults only from authoritative geometry and pins the exact imported font face", () => {
    expect(() => createMask("ellipse", null)).toThrow(/geometry/);
    const heart = createMask("heart", geometry);
    expect(heart.shape).toEqual({
      kind: "heart",
      geometry: geometry.default_geometry,
    });
    const bars = createMask("cinematic_bars", geometry);
    expect(
      bars.shape.kind === "cinematic_bars" && bars.shape.geometry.width,
    ).toBe(Math.hypot(640, 360) / 640);
    expect(() => createMask("text", null)).toThrow(/font/);
    const text = createMask("text", null, font);
    expect(text.shape.kind === "text" && text.shape.mask).toMatchObject({
      font: { kind: "locked", font_id: "font" },
      font_weight: "bold",
      font_style: "italic",
    });
  });
  it("reorders full mask identities without changing channels, untouched fields or the source", () => {
    const source = initialSnapshot().project.sequences[0].tracks[0].elements[0],
      element = structuredClone(source),
      a = createMask("heart", geometry),
      b = createMask("split", geometry);
    element.visual!.masks = [a, b];
    element.channels = [
      {
        id: "mask-motion",
        kind: "scalar",
        target: { kind: "mask", element_id: element.id, mask_id: a.id },
        binding: { kind: "property", parameter: "mask_center_x" },
        keys: [],
        extrapolation: { before: "hold", after: "hold" },
      },
    ];
    const before = structuredClone(element);
    element.visual!.masks = move(element.visual!.masks, 0, 1);
    expect(element.visual!.masks).toEqual([b, a]);
    expect(element.channels).toEqual(before.channels);
    expect({ ...element, visual: source.visual, channels: [] }).toEqual(source);
    const removed = removeMask(element, a.id);
    expect(removed.visual!.masks).toEqual([b]);
    expect(removed.channels).toEqual([]);
    expect(element.visual!.masks).toHaveLength(2);
  });
  it("reorders an effect layer through sequence_put retaining bookmarks, captions, transitions and every track field", () => {
    const scene = initialSnapshot().project.sequences[0],
      effect = newEffectTrack(
        { ticks: "1001", timebase: 30000 },
        { ticks: "5", timebase: 1 },
      );
    scene.tracks.push(effect);
    const before = structuredClone(scene);
    const op = moveEffectTrack(scene, effect.id, -1);
    expect(op.type).toBe("sequence_put");
    if (op.type !== "sequence_put") throw Error();
    expect(op.sequence).toEqual({
      ...before,
      tracks: [effect, before.tracks[0]],
    });
    expect(scene).toEqual(before);
    expect(effect.elements[0].timeline_range.start).toEqual({
      ticks: "1001",
      timebase: 30000,
    });
  });
  it("keeps open paths explicit and rejects short closed paths or unresolved text fonts", () => {
    const element =
        initialSnapshot().project.sequences[0].tracks[0].elements[0],
      mask = createMask("freeform", null);
    element.visual!.masks = [mask];
    expect(validateStack(element)).toBeNull();
    if (mask.shape.kind !== "freeform") throw Error();
    mask.shape.path.closed = true;
    expect(validateStack(element)).toMatch(/three/);
  });
});
it("removes only the deleted owner's exact channel targets and preserves all surviving keys, tangents and IDs", () => {
  const element = initialSnapshot().project.sequences[0].tracks[0].elements[0],
    mask = createMask("heart", geometry), otherMask = createMask("split", null),
    effect = createBlur(), otherEffect = createBlur();
  element.visual!.masks = [mask, otherMask];
  element.visual!.effects = [effect, otherEffect];
  type Channel = typeof element.channels[number];
  const channel = (id: string, target: Channel["target"]): Channel => ({
    kind: "scalar", id, target,
    binding: { kind: "property", parameter: target.kind === "mask" ? "mask_center_x" : target.kind === "effect" ? "classic_blur_intensity" : "opacity" },
    extrapolation: { before: "hold", after: "linear" },
    keys: [{ id: `${id}-key`, time: { ticks: "1", timebase: 3 }, value: 0.5,
      left_handle: { dt: { ticks: "-1", timebase: 9 }, dv: -0.2 },
      right_handle: { dt: { ticks: "1", timebase: 9 }, dv: 0.4 },
      segment_to_next: { kind: "bezier" }, tangent_mode: "broken" }],
  });
  element.channels = [
    channel("removed-mask", { kind: "mask", element_id: element.id, mask_id: mask.id }),
    channel("other-mask", { kind: "mask", element_id: element.id, mask_id: otherMask.id }),
    channel("removed-effect", { kind: "effect", element_id: element.id, effect_id: effect.id }),
    channel("other-effect", { kind: "effect", element_id: element.id, effect_id: otherEffect.id }),
    channel("element", { kind: "element", element_id: element.id }),
  ];
  const before = structuredClone(element), maskRemoved = removeMask(element, mask.id),
    bothRemoved = removeEffect(maskRemoved, effect.id);
  expect(maskRemoved.channels).toEqual(before.channels.slice(1));
  expect(bothRemoved.channels).toEqual(before.channels.filter(c => ["other-mask", "other-effect", "element"].includes(c.id)));
  expect(bothRemoved.visual).toEqual({ ...before.visual, masks: [otherMask], effects: [otherEffect] });
  expect({ ...bothRemoved, channels: before.channels, visual: before.visual }).toEqual(before);
  expect(element).toEqual(before);
  // Even malformed foreign-owner targets must not be silently rewritten by a
  // local delete; full-project validation remains the authority to reject them.
  const foreign = channel("foreign", { kind: "mask", element_id: "other-element", mask_id: mask.id });
  element.channels.push(foreign);
  expect(removeMask(element, mask.id).channels).toContainEqual(foreign);
});

it("deletes path points by stable ID, opens triangles at two points, and preserves the empty draft and surviving handles", () => {
  const mask = createMask("freeform", null);
  if (mask.shape.kind !== "freeform") throw Error();
  const path = mask.shape.path;
  path.points.push({ id: "third", x: 0, y: 0.5, in_x: -0.2, in_y: 0.6, out_x: 0.3, out_y: 0.4 });
  path.closed = true;
  const before = structuredClone(path), remaining = removePathPoints(path, [path.points[0].id]);
  expect(remaining).toEqual({ ...before, closed: false, points: before.points.slice(1) });
  const empty = removePathPoints(remaining, remaining.points.map(p => p.id));
  expect(empty).toEqual({ ...before, closed: false, points: [] });
  expect(removePathPoints(empty, ["unknown"])).toEqual(empty);
  expect(path).toEqual(before);
});
const cubic = (a: Point, b: Point, t: number) => {
  const u = 1 - t;
  return {
    x:
      u * u * u * a.x +
      3 * u * u * t * a.out_x +
      3 * u * t * t * b.in_x +
      t * t * t * b.x,
    y:
      u * u * u * a.y +
      3 * u * u * t * a.out_y +
      3 * u * t * t * b.in_y +
      t * t * t * b.y,
  };
};
it("inserting into a cubic preserves its independently evaluated curve and absolute tangents", () => {
  const a: Point = {
      id: "a",
      x: -0.4,
      y: -0.1,
      in_x: -0.4,
      in_y: -0.1,
      out_x: -0.2,
      out_y: 0.7,
    },
    b: Point = {
      id: "b",
      x: 0.3,
      y: 0.2,
      in_x: 0.1,
      in_y: -0.5,
      out_x: 0.3,
      out_y: 0.2,
    },
    points = splitPathSegment([a, b], 0, false);
  expect(points).toHaveLength(3);
  for (const time of [0, 0.1, 0.25, 0.49, 0.5, 0.7, 1]) {
    const expected = cubic(a, b, time),
      actual =
        time <= 0.5
          ? cubic(points[0], points[1], time * 2)
          : cubic(points[1], points[2], (time - 0.5) * 2);
    expect(actual.x).toBeCloseTo(expected.x, 12);
    expect(actual.y).toBeCloseTo(expected.y, 12);
  }
  expect(a.out_x).toBe(-0.2);
  const moved = moveAnchor(a, 0.1, 0.3);
  expect(moved.in_x - moved.x).toBeCloseTo(a.in_x - a.x);
  expect(moved.out_y - moved.y).toBeCloseTo(a.out_y - a.y);
});

it("retains zero and one point open Classic drafts while rejecting duplicate IDs and non-finite points", () => {
  const element = initialSnapshot().project.sequences[0].tracks[0].elements[0];
  const mask = createMask("freeform", null);
  if (mask.shape.kind !== "freeform") throw Error();
  element.visual!.masks = [mask];
  const point = structuredClone(mask.shape.path.points[0]);
  mask.shape.path.points = [];
  expect(validateStack(element)).toBeNull();
  mask.shape.path.points = [point];
  expect(validateStack(element)).toBeNull();
  mask.shape.path.closed = true;
  expect(validateStack(element)).toMatch(/three/);
  mask.shape.path.closed = false;
  mask.shape.path.points = [point, structuredClone(point)];
  expect(validateStack(element)).toMatch(/unique/);
  mask.shape.path.points = [{ ...point, x: NaN }];
  expect(validateStack(element)).toMatch(/finite/);
});
