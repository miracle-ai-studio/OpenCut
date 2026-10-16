import { canvasToMaskLocal, maskLocalToCanvas } from "../mask/geometry";
import { moveAnchor } from "./model";
import type { Mask, SceneGeometry } from "./types";
export type XY = { x: number; y: number };
export type Handle =
  | "move"
  | "rotate"
  | "width"
  | "height"
  | "scale"
  | "feather"
  | { pointId: string; kind: "anchor" | "in" | "out" };
const rotate = (p: XY, a: number): XY => ({
  x: p.x * Math.cos(a) - p.y * Math.sin(a),
  y: p.x * Math.sin(a) + p.y * Math.cos(a),
});
const geometry = (mask: Mask) => {
  const s = mask.shape;
  return "geometry" in s
    ? s.geometry
    : s.kind === "split"
      ? s
      : s.kind === "freeform"
        ? s.path
        : s.kind === "text"
          ? s.mask
          : null;
};
export function dragSceneMask(
  mask: Mask,
  quad: SceneGeometry["quad"],
  handle: Handle,
  from: XY,
  to: XY,
): Mask {
  const next = structuredClone(mask),
    g = geometry(next);
  if (!g || mask.feather.unit !== "classic_pixels") return next;
  const a = canvasToMaskLocal(from, quad),
    b = canvasToMaskLocal(to, quad),
    angle = (g.rotation_degrees * Math.PI) / 180,
    c = { x: g.center_x * quad.width, y: g.center_y * quad.height };
  if (typeof handle === "object") {
    if (next.shape.kind !== "freeform" || next.shape.path.scale <= 0)
      return next;
    const path = next.shape.path,
      p = rotate({ x: b.x - c.x, y: b.y - c.y }, -angle),
      x = p.x / quad.width / path.scale,
      y = p.y / quad.height / path.scale,
      index = path.points.findIndex((p) => p.id === handle.pointId),
      point = path.points[index];
    if (!point) return next;
    path.points[index] =
      handle.kind === "anchor"
        ? moveAnchor(point, x, y)
        : handle.kind === "in"
          ? { ...point, in_x: x, in_y: y }
          : { ...point, out_x: x, out_y: y };
  } else if (handle === "move") {
    g.center_x += (b.x - a.x) / quad.width;
    g.center_y += (b.y - a.y) / quad.height;
  } else if (handle === "rotate") {
    let delta =
      Math.atan2(b.y - c.y, b.x - c.x) - Math.atan2(a.y - c.y, a.x - c.x);
    if (delta > Math.PI) delta -= Math.PI * 2;
    if (delta < -Math.PI) delta += Math.PI * 2;
    g.rotation_degrees += (delta * 180) / Math.PI;
  } else if (handle === "scale" && "scale" in g) {
    const initial = Math.hypot(a.x - c.x, a.y - c.y);
    if (initial > 0)
      g.scale = Math.max(
        0.01,
        (g.scale * Math.hypot(b.x - c.x, b.y - c.y)) / initial,
      );
  } else if (handle === "feather" && next.feather.unit === "classic_pixels") {
    const delta = rotate({ x: b.x - a.x, y: b.y - a.y }, -angle);
    next.feather.value = Math.max(0, next.feather.value + delta.y / 0.11);
  } else if ("width" in g && "height" in g) {
    const delta = rotate({ x: b.x - a.x, y: b.y - a.y }, -angle);
    if (handle === "width")
      g.width = Math.max(0.01, g.width + (2 * delta.x) / quad.width);
    if (handle === "height")
      g.height = Math.max(0.01, g.height + (2 * delta.y) / quad.height);
  }
  return next;
}
export function sceneMaskOverlay(
  mask: Mask,
  quad: SceneGeometry["quad"],
): {
  path: string;
  lines: [XY, XY][];
  handles: { handle: Handle; point: XY; label: string }[];
} | null {
  const s = mask.shape,
    g = geometry(mask);
  if (
    !g ||
    mask.feather.unit !== "classic_pixels" ||
    s.kind.startsWith("legacy_")
  )
    return null;
  const a = (g.rotation_degrees * Math.PI) / 180,
    c = { x: g.center_x * quad.width, y: g.center_y * quad.height };
  const world = (p: XY) => {
    const v = rotate(p, a);
    return maskLocalToCanvas({ x: v.x + c.x, y: v.y + c.y }, quad);
  };
  const point = (x: number, y: number) => {
    const p = world({ x, y });
    return `${p.x},${p.y}`;
  };
  const handles: { handle: Handle; point: XY; label: string }[] = [
      { handle: "move", point: world({ x: 0, y: 0 }), label: "Move mask" },
    ],
    lines: [XY, XY][] = [];
  let path = "",
    half = Math.min(quad.width, quad.height) * 0.3;
  if ("geometry" in s) {
    const w = (s.geometry.width * quad.width) / 2,
      h = (s.geometry.height * quad.height) / 2;
    half = h;
    if (s.kind === "ellipse") {
      // Cubic ellipse is only an authoring outline; the renderer owns real pixels.
      const k = 0.5522847498307936;
      path = `M ${point(w, 0)} C ${point(w, h * k)} ${point(w * k, h)} ${point(0, h)} C ${point(-w * k, h)} ${point(-w, h * k)} ${point(-w, 0)} C ${point(-w, -h * k)} ${point(-w * k, -h)} ${point(0, -h)} C ${point(w * k, -h)} ${point(w, -h * k)} ${point(w, 0)} Z`;
    } else if (s.kind === "heart")
      path = `M ${point(0, -h * 0.475)} C ${point(w, -h * 1.225)} ${point(w, -h * 0.125)} ${point(0, h * 0.725)} C ${point(-w, -h * 0.125)} ${point(-w, -h * 1.225)} ${point(0, -h * 0.475)} Z`;
    else if (s.kind === "diamond")
      path = `M ${point(0, -h)} L ${point(w, 0)} L ${point(0, h)} L ${point(-w, 0)} Z`;
    else if (s.kind === "star") {
      path =
        Array.from({ length: 10 }, (_, i) => {
          const r = i % 2 ? 0.45 : 1,
            angle = -Math.PI / 2 + (i * Math.PI) / 5;
          return `${i ? "L" : "M"} ${point(Math.cos(angle) * w * r, Math.sin(angle) * h * r)}`;
        }).join(" ") + " Z";
    } else
      path = `M ${point(-w, -h)} L ${point(w, -h)} L ${point(w, h)} L ${point(-w, h)} Z`;
    handles.push(
      {
        handle: "width",
        point: world({ x: w, y: 0 }),
        label: "Resize mask width",
      },
      {
        handle: "height",
        point: world({ x: 0, y: h }),
        label: "Resize mask height",
      },
    );
  } else if (s.kind === "split") {
    const span = Math.hypot(quad.width, quad.height);
    path = `M ${point(-span, 0)} L ${point(span, 0)}`;
    half = Math.min(quad.width, quad.height) * 0.25;
  } else if (s.kind === "freeform") {
    const p = s.path,
      loc = (p: XY) => ({
        x: p.x * quad.width * s.path.scale,
        y: p.y * quad.height * s.path.scale,
      }),
      w = (p: XY) => world(loc(p));
    if (p.points.length) {
      const p0 = p.points[0],
        xy = (p: XY) => {
          const v = w(p);
          return `${v.x},${v.y}`;
        };
      path = `M ${xy(p0)}`;
      const segment = (p: typeof p0, q: typeof p0) =>
        ` C ${xy({ x: p.out_x, y: p.out_y })} ${xy({ x: q.in_x, y: q.in_y })} ${xy(q)}`;
      for (let i = 1; i < p.points.length; i++)
        path += segment(p.points[i - 1], p.points[i]);
      if (p.closed) path += segment(p.points.at(-1)!, p0) + " Z";
    }
    for (const point of p.points) {
      const anchor = w(point),
        left = w({ x: point.in_x, y: point.in_y }),
        right = w({ x: point.out_x, y: point.out_y });
      lines.push([anchor, left], [anchor, right]);
      handles.push(
        {
          handle: { pointId: point.id, kind: "in" },
          point: left,
          label: "Move incoming tangent",
        },
        {
          handle: { pointId: point.id, kind: "out" },
          point: right,
          label: "Move outgoing tangent",
        },
        {
          handle: { pointId: point.id, kind: "anchor" },
          point: anchor,
          label: "Move path point",
        },
      );
    }
  }
  if ((s.kind === "text" || s.kind === "freeform") && "scale" in g)
    handles.push({
      handle: "scale",
      point: world({ x: half, y: half }),
      label: "Scale mask",
    });
  handles.push(
    {
      handle: "rotate",
      point: world({ x: 0, y: -half - 24 }),
      label: "Rotate mask",
    },
    {
      handle: "feather",
      point: world({ x: 0, y: half + 24 + mask.feather.value * 0.11 }),
      label: "Adjust feather",
    },
  );
  return { path, lines, handles };
}
