import type { V4Element } from "../types";
import type { SceneGeometry } from "../scene-stack/types";
import type { CanvasTextPlan } from "../text/canvas-types";
export type Transform = NonNullable<V4Element["visual"]>["transform"];
export type XY = { x: number; y: number };
export type Handle =
  | "move"
  | "rotate"
  | "left"
  | "right"
  | "top"
  | "bottom"
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right";
export type Basis = {
  kind: "media" | "text";
  canvas: { width: number; height: number };
  rect: { left: number; top: number; width: number; height: number };
};
const rotate = (p: XY, a: number): XY => ({
  x: p.x * Math.cos(a) - p.y * Math.sin(a),
  y: p.x * Math.sin(a) + p.y * Math.cos(a),
});
export function mediaBasis(
  plan: Pick<SceneGeometry, "quad">,
  transform: Transform,
  canvas: Basis["canvas"],
): Basis {
  if (!transform.scale_x || !transform.scale_y)
    throw Error("Zero-sized elements cannot be edited on canvas.");
  const width = plan.quad.width / Math.abs(transform.scale_x),
    height = plan.quad.height / Math.abs(transform.scale_y);
  return {
    kind: "media",
    canvas,
    rect: { left: -width / 2, top: -height / 2, width, height },
  };
}
export function textBasis(plan: CanvasTextPlan): Basis {
  const width = Math.max(plan.block.maxWidth, plan.layer.text.fontSizePx),
    align = plan.layer.text.align;
  return {
    kind: "text",
    canvas: { width: plan.width, height: plan.height },
    rect: {
      left: align === "left" ? 0 : align === "right" ? -width : -width / 2,
      top: -plan.block.height / 2,
      width,
      height: plan.block.height,
    },
  };
}
export function worldPoint(local: XY, basis: Basis, transform: Transform): XY {
  const a = (transform.rotation_degrees * Math.PI) / 180;
  const p =
    basis.kind === "text"
      ? rotate(local, a)
      : rotate(
          { x: local.x * transform.scale_x, y: local.y * transform.scale_y },
          a,
        );
  return {
    x:
      basis.canvas.width / 2 +
      transform.position_x_px +
      p.x * (basis.kind === "text" ? transform.scale_x : 1),
    y:
      basis.canvas.height / 2 +
      transform.position_y_px +
      p.y * (basis.kind === "text" ? transform.scale_y : 1),
  };
}
export function transformOutline(basis: Basis, transform: Transform) {
  const r = basis.rect,
    locals = [
      { x: r.left, y: r.top },
      { x: r.left + r.width, y: r.top },
      { x: r.left + r.width, y: r.top + r.height },
      { x: r.left, y: r.top + r.height },
    ],
    corners = locals.map((p) => worldPoint(p, basis, transform));
  return {
    corners,
    locals,
    center: worldPoint({ x: 0, y: 0 }, basis, transform),
  };
}
const nonzero = (n: number) =>
  Math.abs(n) < 0.0001 ? (n < 0 ? -0.0001 : 0.0001) : n;
/** Gesture input only. Application planning rechecks the complete fixed head.
 * Media uses T·R·S; text keeps Classic T·S·R, including nonuniform scales. */
export function dragTransform(
  original: Transform,
  basis: Basis,
  handle: Handle,
  from: XY,
  to: XY,
): Transform {
  if (from.x === to.x && from.y === to.y) return { ...original };
  const next = { ...original },
    center = {
      x: basis.canvas.width / 2 + original.position_x_px,
      y: basis.canvas.height / 2 + original.position_y_px,
    };
  if (handle === "move") {
    next.position_x_px += to.x - from.x;
    next.position_y_px += to.y - from.y;
    return next;
  }
  const a = (original.rotation_degrees * Math.PI) / 180;
  if (handle === "rotate") {
    const vector = (p: XY) => ({
        x: (p.x - center.x) / (basis.kind === "text" ? original.scale_x : 1),
        y: (p.y - center.y) / (basis.kind === "text" ? original.scale_y : 1),
      }),
      u = vector(from),
      v = vector(to);
    let delta = Math.atan2(v.y, v.x) - Math.atan2(u.y, u.x);
    if (delta > Math.PI) delta -= 2 * Math.PI;
    if (delta < -Math.PI) delta += 2 * Math.PI;
    next.rotation_degrees += (delta * 180) / Math.PI;
    return next;
  }
  const r = basis.rect,
    local = {
      x: handle.includes("left")
        ? r.left
        : handle.includes("right")
          ? r.left + r.width
          : r.left + r.width / 2,
      y: handle.includes("top")
        ? r.top
        : handle.includes("bottom")
          ? r.top + r.height
          : r.top + r.height / 2,
    };
  const origin = worldPoint(local, basis, original),
    target = {
      x: origin.x + to.x - from.x - center.x,
      y: origin.y + to.y - from.y - center.y,
    };
  const raw = basis.kind === "media" ? rotate(target, -a) : target,
    divisor = basis.kind === "media" ? local : rotate(local, a);
  if (handle.includes("left") || handle.includes("right")) {
    if (Math.abs(divisor.x) > 1e-8) next.scale_x = nonzero(raw.x / divisor.x);
  }
  if (handle.includes("top") || handle.includes("bottom")) {
    if (Math.abs(divisor.y) > 1e-8) next.scale_y = nonzero(raw.y / divisor.y);
  }
  return next;
}

export type TransformMember = { basis: Basis; transform: Transform };
/** A group is view geometry only. All proposed member transforms still pass the
 * fixed-head property planner before either temporary rendering or a commit. */
export function selectionOutline(members: readonly TransformMember[]) {
  if (!members.length)
    throw Error("Select visual elements before transforming them.");
  if (members.length === 1)
    return transformOutline(members[0].basis, members[0].transform);
  const points = members.flatMap(
      ({ basis, transform }) => transformOutline(basis, transform).corners,
    ),
    left = Math.min(...points.map((p) => p.x)),
    right = Math.max(...points.map((p) => p.x)),
    top = Math.min(...points.map((p) => p.y)),
    bottom = Math.max(...points.map((p) => p.y));
  return {
    corners: [
      { x: left, y: top },
      { x: right, y: top },
      { x: right, y: bottom },
      { x: left, y: bottom },
    ],
    center: { x: (left + right) / 2, y: (top + bottom) / 2 },
  };
}
export function groupRotationProblem(
  members: readonly TransformMember[],
): string | null {
  return members.length > 1 &&
    members.some(
      ({ basis, transform }) =>
        basis.kind === "text" &&
        Math.abs(Math.abs(transform.scale_x) - Math.abs(transform.scale_y)) >
          1e-8,
    )
    ? "Rotate nonuniformly scaled text separately; its Classic transform order cannot express a group rotation."
    : null;
}
export function dragSelection(
  members: readonly TransformMember[],
  handle: Handle,
  from: XY,
  to: XY,
): Transform[] {
  if (members.length === 1)
    return [
      dragTransform(members[0].transform, members[0].basis, handle, from, to),
    ];
  if (from.x === to.x && from.y === to.y)
    return members.map((m) => ({ ...m.transform }));
  if (handle === "move")
    return members.map(({ transform }) => ({
      ...transform,
      position_x_px: transform.position_x_px + to.x - from.x,
      position_y_px: transform.position_y_px + to.y - from.y,
    }));
  const { center } = selectionOutline(members);
  let angle = 0,
    scale = 1;
  if (handle === "rotate") {
    const problem = groupRotationProblem(members);
    if (problem) throw Error(problem);
    angle =
      Math.atan2(to.y - center.y, to.x - center.x) -
      Math.atan2(from.y - center.y, from.x - center.x);
    if (angle > Math.PI) angle -= 2 * Math.PI;
    if (angle < -Math.PI) angle += 2 * Math.PI;
  } else {
    const x = from.x - center.x,
      y = from.y - center.y,
      length = x * x + y * y;
    if (length < 1e-8) return members.map((m) => ({ ...m.transform }));
    // One uniform scale preserves member angles and the Classic text/media
    // matrix orders without inventing an unrepresented shear.
    scale = Math.max(
      0.0001,
      ((to.x - center.x) * x + (to.y - center.y) * y) / length,
    );
  }
  return members.map(({ basis, transform }) => {
    const origin = {
        x: basis.canvas.width / 2 + transform.position_x_px,
        y: basis.canvas.height / 2 + transform.position_y_px,
      },
      offset = rotate(
        { x: (origin.x - center.x) * scale, y: (origin.y - center.y) * scale },
        angle,
      ),
      direction =
        basis.kind === "text"
          ? Math.sign(transform.scale_x * transform.scale_y)
          : 1;
    return {
      ...transform,
      position_x_px: center.x + offset.x - basis.canvas.width / 2,
      position_y_px: center.y + offset.y - basis.canvas.height / 2,
      scale_x: transform.scale_x * scale,
      scale_y: transform.scale_y * scale,
      rotation_degrees:
        transform.rotation_degrees + (direction * angle * 180) / Math.PI,
    };
  });
}
