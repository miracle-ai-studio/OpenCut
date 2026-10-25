import { expect, it } from "vitest";
import {
  canvasToMaskLocal,
  dragMask,
  maskLocalToCanvas,
  maskOutline,
} from "./geometry";
import { boxMask, geometryPlan } from "./fixture";

it("maps the letterboxed canvas-independent logical quad through rotation and both flips", () => {
  const quad = {
    ...geometryPlan().quad,
    flipX: true,
    flipY: true,
    rotationRadians: Math.PI / 2,
  };
  const canvas = maskLocalToCanvas({ x: 20, y: -10 }, quad);
  expect(canvas.x).toBeCloseTo(310);
  expect(canvas.y).toBeCloseTo(160);
  const local = canvasToMaskLocal(canvas, quad);
  expect(local.x).toBeCloseTo(20);
  expect(local.y).toBeCloseTo(-10);
  const mask = boxMask(),
    moved = dragMask(
      mask,
      quad,
      "move",
      { x: 320, y: 180 },
      { x: 320, y: 148 },
    );
  expect(moved.shape.geometry.center_x).toBeCloseTo(0.1);
  expect(moved.shape.geometry.center_y).toBeCloseTo(0);
  expect(moved.shape.geometry.scale).toBe(mask.shape.geometry.scale);
  expect(mask.shape.geometry.center_x).toBe(0);
});

it("resizes in the mask's rotated axes, preserves scale and uses Classic corner and feather rules", () => {
  const mask = boxMask(),
    quad = geometryPlan().quad;
  mask.shape.geometry.rotation_degrees = 90;
  const resized = dragMask(
    mask,
    quad,
    "right",
    { x: 320, y: 180 },
    { x: 320, y: 196 },
  );
  expect(resized.shape.geometry.width).toBeCloseTo(0.7);
  expect(resized.shape.geometry.height).toBe(0.6);
  expect(resized.shape.geometry.scale).toBe(mask.shape.geometry.scale);
  const corner = dragMask(
    mask,
    quad,
    "bottom-right",
    { x: 320, y: 180 },
    { x: 309.2, y: 199.2 },
  );
  expect(corner.shape.geometry.width).toBeCloseTo(0.66);
  expect(corner.shape.geometry.height).toBeCloseTo(0.66);
  const feather = dragMask(
    mask,
    quad,
    "feather",
    { x: 320, y: 180 },
    { x: 309, y: 180 },
  );
  expect(feather.feather.value).toBe(108);
  expect(
    dragMask(mask, quad, "feather", { x: 0, y: 0 }, { x: -10000, y: 0 }).feather
      .value,
  ).toBe(1000);
  expect(
    dragMask(mask, quad, "feather", { x: 0, y: 0 }, { x: 10000, y: 0 }).feather
      .value,
  ).toBe(0);
});

it("rotates relative to the grabbed radial vector without a first-move jump and bounds outline handles", () => {
  const mask = boxMask(),
    quad = geometryPlan().quad;
  expect(
    dragMask(mask, quad, "rotate", { x: 320, y: 100 }, { x: 320, y: 100 }),
  ).toEqual(mask);
  expect(
    dragMask(mask, quad, "rotate", { x: 320, y: 100 }, { x: 400, y: 180 }).shape
      .geometry.rotation_degrees,
  ).toBe(90);
  const rectangle = maskOutline(mask, quad, 0.5);
  expect(rectangle.points).toHaveLength(4);
  expect(
    rectangle.handles.find((handle) => handle.id === "rotate")!.point.y,
  ).toBe(86);
  mask.shape.kind = "ellipse";
  expect(maskOutline(mask, quad, 0.5).points).toHaveLength(64);
});
