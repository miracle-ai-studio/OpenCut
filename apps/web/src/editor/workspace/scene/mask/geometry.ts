import type { BoxMask, MaskGeometryPlan, MaskHandle, Point } from "./types";

const rotate = (point: Point, angle: number): Point => ({
  x: point.x * Math.cos(angle) - point.y * Math.sin(angle),
  y: point.x * Math.sin(angle) + point.y * Math.cos(angle),
});

/** Inverse of the renderer's logical quad, including both authored flips. */
export function canvasToMaskLocal(
  point: Point,
  quad: MaskGeometryPlan["quad"],
): Point {
  const local = rotate(
    { x: point.x - quad.centerX, y: point.y - quad.centerY },
    -quad.rotationRadians,
  );
  return {
    x: local.x * (quad.flipX ? -1 : 1),
    y: local.y * (quad.flipY ? -1 : 1),
  };
}
export function maskLocalToCanvas(
  point: Point,
  quad: MaskGeometryPlan["quad"],
): Point {
  const world = rotate(
    { x: point.x * (quad.flipX ? -1 : 1), y: point.y * (quad.flipY ? -1 : 1) },
    quad.rotationRadians,
  );
  return { x: quad.centerX + world.x, y: quad.centerY + world.y };
}

/** Classic box-like.ts and param-update.ts rules, applied in the element's
 * local coordinate system so rotated/flipped elements retain the same meaning. */
export function dragMask(
  start: BoxMask,
  quad: MaskGeometryPlan["quad"],
  handle: MaskHandle,
  from: Point,
  to: Point,
): BoxMask {
  const next = structuredClone(start),
    g = next.shape.geometry;
  const a = canvasToMaskLocal(from, quad),
    b = canvasToMaskLocal(to, quad);
  const delta = { x: b.x - a.x, y: b.y - a.y },
    angle = (g.rotation_degrees * Math.PI) / 180;
  if (handle === "move") {
    g.center_x += delta.x / quad.width;
    g.center_y += delta.y / quad.height;
  } else if (handle === "rotate") {
    const cx = g.center_x * quad.width,
      cy = g.center_y * quad.height;
    const da = Math.atan2(b.y - cy, b.x - cx) - Math.atan2(a.y - cy, a.x - cx);
    g.rotation_degrees =
      (((g.rotation_degrees + (da * 180) / Math.PI) % 360) + 360) % 360;
  } else if (handle === "feather") {
    const projection = -Math.sin(angle) * delta.x + Math.cos(angle) * delta.y;
    next.feather.value = Math.max(
      0,
      Math.min(1000, Math.round(start.feather.value + projection / 0.11)),
    );
  } else {
    const local = rotate(delta, -angle);
    if (handle === "left" || handle === "right")
      g.width = Math.max(
        0.01,
        g.width + ((handle === "right" ? 1 : -1) * local.x * 2) / quad.width,
      );
    else if (handle === "top" || handle === "bottom")
      g.height = Math.max(
        0.01,
        g.height + ((handle === "bottom" ? 1 : -1) * local.y * 2) / quad.height,
      );
    else {
      const w = g.width * quad.width,
        h = g.height * quad.height;
      const scale =
        Math.hypot(
          w + (handle.endsWith("right") ? 1 : -1) * local.x,
          h + (handle.startsWith("bottom") ? 1 : -1) * local.y,
        ) / Math.hypot(w, h);
      g.width = Math.max(0.01, g.width * scale);
      g.height = Math.max(0.01, g.height * scale);
    }
  }
  return next;
}

export function maskOutline(
  mask: BoxMask,
  quad: MaskGeometryPlan["quad"],
  displayScale: number,
) {
  const g = mask.shape.geometry,
    cx = g.center_x * quad.width,
    cy = g.center_y * quad.height;
  const hw = (Math.max(0.01, g.width) * quad.width) / 2,
    hh = (Math.max(0.01, g.height) * quad.height) / 2;
  const angle = (g.rotation_degrees * Math.PI) / 180;
  const at = (x: number, y: number) => {
    const p = rotate({ x, y }, angle);
    return maskLocalToCanvas({ x: cx + p.x, y: cy + p.y }, quad);
  };
  const points =
    mask.shape.kind === "ellipse"
      ? Array.from({ length: 64 }, (_, i) =>
          at(
            hw * Math.cos((i * Math.PI) / 32),
            hh * Math.sin((i * Math.PI) / 32),
          ),
        )
      : [at(-hw, -hh), at(hw, -hh), at(hw, hh), at(-hw, hh)];
  const offset = 20 / displayScale;
  const handles: { id: MaskHandle; label: string; point: Point }[] = [
    { id: "rotate", label: "Rotate mask", point: at(0, -hh - offset) },
    {
      id: "feather",
      label: "Adjust mask feather",
      point: at(0, hh + offset + mask.feather.value * 0.11),
    },
    { id: "left", label: "Resize mask left", point: at(-hw, 0) },
    { id: "right", label: "Resize mask right", point: at(hw, 0) },
    { id: "top", label: "Resize mask top", point: at(0, -hh) },
    { id: "bottom", label: "Resize mask bottom", point: at(0, hh) },
    { id: "top-left", label: "Resize mask top left", point: at(-hw, -hh) },
    { id: "top-right", label: "Resize mask top right", point: at(hw, -hh) },
    { id: "bottom-left", label: "Resize mask bottom left", point: at(-hw, hh) },
    {
      id: "bottom-right",
      label: "Resize mask bottom right",
      point: at(hw, hh),
    },
  ];
  return { points, handles };
}
