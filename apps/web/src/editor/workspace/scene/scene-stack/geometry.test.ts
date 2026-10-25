import { expect, it } from "vitest";
import { createMask } from "./model";
import { dragSceneMask, sceneMaskOverlay } from "./geometry";
import { maskLocalToCanvas } from "../mask/geometry";
import { geometry } from "./fixture";
it("moves a mask in element-local coordinates under rotation and reflection without altering stroke or scale", () => {
  const q = { ...geometry.quad, rotationRadians: Math.PI / 2, flipX: true },
    mask = createMask("heart", geometry);
  mask.stroke.width_px = 3;
  mask.inverted = true;
  const from = maskLocalToCanvas({ x: 0, y: 0 }, q),
    to = maskLocalToCanvas({ x: 64, y: -36 }, q),
    next = dragSceneMask(mask, q, "move", from, to);
  if (next.shape.kind !== "heart") throw Error();
  expect(next.shape.geometry.center_x).toBeCloseTo(0.1);
  expect(next.shape.geometry.center_y).toBeCloseTo(-0.1);
  expect(next.stroke).toEqual(mask.stroke);
  expect(next.inverted).toBe(true);
  expect(mask.shape.kind === "heart" && mask.shape.geometry.center_x).toBe(0);
});
it("drags absolute path tangents in the inverse sampled quad and path transform", () => {
  const mask = createMask("freeform", null);
  if (mask.shape.kind !== "freeform") throw Error();
  mask.shape.path.scale = 2;
  mask.shape.path.rotation_degrees = 90;
  const point = mask.shape.path.points[0],
    to = maskLocalToCanvas({ x: -72, y: 128 }, geometry.quad),
    next = dragSceneMask(
      mask,
      geometry.quad,
      { pointId: point.id, kind: "out" },
      { x: 0, y: 0 },
      to,
    );
  if (next.shape.kind !== "freeform") throw Error();
  expect(next.shape.path.points[0].out_x).toBeCloseTo(0.1);
  expect(next.shape.path.points[0].out_y).toBeCloseTo(0.1);
  expect(next.shape.path.points[0].x).toBe(point.x);
});
it("keeps an open path open in its authoring outline and exposes independent anchor/tangent handles", () => {
  const mask = createMask("freeform", null),
    outline = sceneMaskOverlay(mask, geometry.quad)!;
  expect(outline.path).not.toMatch(/Z$/);
  expect(
    outline.handles.filter((h) => typeof h.handle === "object"),
  ).toHaveLength(6);
});
