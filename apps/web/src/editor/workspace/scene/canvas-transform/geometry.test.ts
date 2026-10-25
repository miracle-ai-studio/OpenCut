import { expect, it } from "vitest";
import {
  dragTransform,
  dragSelection,
  selectionOutline,
  groupRotationProblem,
  transformOutline,
  worldPoint,
} from "./geometry";
import type { Basis, Transform } from "./geometry";
const transform: Transform = {
  position_x_px: 17.123456789,
  position_y_px: -3.5,
  scale_x: -2,
  scale_y: 0.5,
  rotation_degrees: 90,
};
const media: Basis = {
  kind: "media",
  canvas: { width: 640, height: 360 },
  rect: { left: -80, top: -40, width: 160, height: 80 },
};
it("uses Classic media T R S and text T S R without commuting nonuniform scale", () => {
  const p = { x: 80, y: 40 },
    m = worldPoint(p, media, transform),
    text = worldPoint(p, { ...media, kind: "text" }, transform);
  expect(m.x).toBeCloseTo(320 + transform.position_x_px - 20);
  expect(m.y).toBeCloseTo(180 - 3.5 - 160);
  expect(text.x).toBeCloseTo(320 + transform.position_x_px + 80);
  expect(text.y).toBeCloseTo(180 - 3.5 + 40);
  expect(text).not.toEqual(m);
});
it("moves in canvas pixels preserving full precision, flips and rotation", () => {
  const next = dragTransform(
    transform,
    media,
    "move",
    { x: 10, y: 10 },
    { x: 22.25, y: -1 },
  );
  expect(next.position_x_px).toBeCloseTo(29.373456789, 12);
  expect({ ...next, position_x_px: transform.position_x_px }).toEqual({
    ...transform,
    position_y_px: -14.5,
  });
});
it("resizes a reflected media corner and returns to the exact original transform on zero movement", () => {
  const from = transformOutline(media, transform).corners[2];
  expect(
    dragTransform(transform, media, "bottom-right", from, from).scale_x,
  ).toBeCloseTo(-2);
  const next = dragTransform(transform, media, "bottom-right", from, {
    x: from.x,
    y: from.y - 80,
  });
  expect(next.scale_x).toBeCloseTo(-3);
  expect(next.scale_y).toBeCloseTo(0.5);
});
it("rotates text in its unscaled local plane instead of using a media rectangle angle", () => {
  const b = { ...media, kind: "text" as const },
    t = { ...transform, scale_x: 2, scale_y: 0.5, rotation_degrees: 0 },
    center = worldPoint({ x: 0, y: 0 }, b, t),
    from = worldPoint({ x: 40, y: 0 }, b, t),
    to = { x: center.x, y: center.y + 20 };
  expect(dragTransform(t, b, "rotate", from, to).rotation_degrees).toBeCloseTo(
    90,
  );
});
it("group movement and uniform resize preserve relative positions, reflections and member angles", () => {
  const second = {
      ...transform,
      position_x_px: 117.123456789,
      position_y_px: 26.5,
      scale_x: 1,
      rotation_degrees: 15,
    },
    members = [
      { basis: media, transform },
      { basis: media, transform: second },
    ],
    moved = dragSelection(members, "move", { x: 10, y: 20 }, { x: 25, y: 0 });
  expect(moved[1].position_x_px - moved[0].position_x_px).toBeCloseTo(100);
  expect(moved[0].position_x_px).toBeCloseTo(transform.position_x_px + 15);
  const { center, corners } = selectionOutline(members),
    from = corners[2],
    scaled = dragSelection(members, "bottom-right", from, {
      x: center.x + 2 * (from.x - center.x),
      y: center.y + 2 * (from.y - center.y),
    });
  expect(scaled[1].position_x_px - scaled[0].position_x_px).toBeCloseTo(200);
  expect(scaled[1].position_y_px - scaled[0].position_y_px).toBeCloseTo(60);
  expect(scaled[0].scale_x).toBe(-4);
  expect(scaled[0].rotation_degrees).toBe(transform.rotation_degrees);
});
it("group rotation applies one world rotation to every media corner and mirrored uniform text corner", () => {
  const members = [
      { basis: media, transform },
      {
        basis: { ...media, kind: "text" as const },
        transform: {
          ...transform,
          scale_x: -1,
          scale_y: 1,
          rotation_degrees: 30,
        },
      },
    ],
    { center } = selectionOutline(members),
    next = dragSelection(
      members,
      "rotate",
      { x: center.x + 100, y: center.y },
      { x: center.x, y: center.y + 100 },
    );
  members.forEach((member, index) => {
    const original = transformOutline(member.basis, member.transform).corners,
      rotated = transformOutline(member.basis, next[index]).corners;
    original.forEach((p, j) => {
      expect(rotated[j].x).toBeCloseTo(center.x - (p.y - center.y), 8);
      expect(rotated[j].y).toBeCloseTo(center.y + (p.x - center.x), 8);
    });
  });
});
it("does not invent shear for a group containing nonuniform Classic text", () => {
  const members = [
    { basis: media, transform },
    { basis: { ...media, kind: "text" as const }, transform },
  ];
  expect(groupRotationProblem(members)).toContain("Classic transform order");
  expect(() =>
    dragSelection(members, "rotate", { x: 1, y: 0 }, { x: 0, y: 1 }),
  ).toThrow("Classic transform order");
  expect(
    dragSelection(members, "move", { x: 1, y: 0 }, { x: 0, y: 1 }),
  ).toHaveLength(2);
});
