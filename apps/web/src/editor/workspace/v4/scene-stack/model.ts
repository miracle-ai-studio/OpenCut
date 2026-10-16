import type { V4Element, V4Sequence, V4Operation } from "../types";
import { requestedFontStyle, synthesisFont } from "../text/font-synthesis";
import type { V4TextFont } from "../text/types";
import type {
  Effect,
  Freeform,
  Mask,
  Point,
  SceneGeometry,
  ShapeKind,
  Track,
} from "./types";

/** Classic defaults come from masks/builtin/box-like.ts, cinematic-bars.ts,
 * split.ts and text.ts. Only application-authorized bounds size new box masks. */
export function createMask(
  kind: ShapeKind,
  geometry: SceneGeometry | null,
  font?: V4TextFont,
): Mask {
  const common = {
    id: crypto.randomUUID(),
    enabled: true,
    inverted: false,
    feather: { unit: "classic_pixels" as const, value: 0 },
    stroke: {
      color: { encoding: "srgb" as const, red: 1, green: 1, blue: 1, alpha: 1 },
      width_px: 0,
      align: "center" as const,
    },
  };
  if (kind === "split")
    return {
      ...common,
      shape: { kind, center_x: 0, center_y: 0, rotation_degrees: 0 },
    };
  if (kind === "text") {
    if (
      !font ||
      font.face_index !== 0 ||
      ![400, 700].includes(font.weight) ||
      (synthesisFont(font) && !font.synthesis_binding)
    )
      throw new Error(
        "Choose an imported static font before adding a text mask.",
      );
    return {
      ...common,
      shape: {
        kind,
        mask: {
          content: "Mask",
          font: { kind: "locked", font_id: font.id },
          font_size: { unit: "classic_height90", value: 15 },
          font_weight: font.weight === 700 ? "bold" : "normal",
          font_style: requestedFontStyle(font),
          decoration: "none",
          letter_spacing_px: 0,
          line_height_multiplier: 1.2,
          center_x: 0,
          center_y: 0,
          rotation_degrees: 0,
          scale: 1,
        },
      },
    };
  }
  if (kind === "freeform") {
    // A valid open path is editable before closing; the renderer ignores it.
    const point = (x: number): Point => ({
      id: crypto.randomUUID(),
      x,
      y: 0,
      in_x: x,
      in_y: 0,
      out_x: x,
      out_y: 0,
    });
    return {
      ...common,
      shape: {
        kind,
        path: {
          points: [point(-0.15), point(0.15)],
          closed: false,
          center_x: 0,
          center_y: 0,
          rotation_degrees: 0,
          scale: 1,
        },
      },
    };
  }
  if (!geometry)
    throw new Error("Read the target geometry before adding a sized mask.");
  const defaults = structuredClone(geometry.default_geometry);
  if (kind === "cinematic_bars") {
    defaults.width = Math.max(
      1,
      Math.hypot(geometry.quad.width, geometry.quad.height) /
        geometry.quad.width,
    );
    defaults.height = 0.6;
  }
  return { ...common, shape: { kind, geometry: defaults } };
}
export const createBlur = (): Effect => ({
  id: crypto.randomUUID(),
  enabled: true,
  effect: { kind: "classic_blur", intensity: 15 },
});
export function move<T>(items: readonly T[], from: number, delta: -1 | 1): T[] {
  const next = [...items],
    to = from + delta;
  if (from < 0 || to < 0 || from >= next.length || to >= next.length)
    return next;
  [next[from], next[to]] = [next[to], next[from]];
  return next;
}
export function removeMask(element: V4Element, id: string): V4Element {
  const next = structuredClone(element);
  if (next.visual)
    next.visual.masks = next.visual.masks.filter((mask) => mask.id !== id);
  next.channels = next.channels.filter(({ target }) =>
    !(target.kind === "mask" && target.element_id === element.id && target.mask_id === id));
  return next;
}
export function removeEffect(element: V4Element, id: string): V4Element {
  const next = structuredClone(element);
  if (next.visual)
    next.visual.effects = next.visual.effects.filter(
      (effect) => effect.id !== id,
    );
  next.channels = next.channels.filter(({ target }) =>
    !(target.kind === "effect" && target.element_id === element.id && target.effect_id === id));
  return next;
}
/** Classic deletes selected points and opens a closed path below three points.
 * Empty open drafts remain authored and can be continued with Append point. */
export function removePathPoints(path: Freeform, pointIds: readonly string[]): Freeform {
  const next = structuredClone(path), removed = new Set(pointIds);
  next.points = next.points.filter(point => !removed.has(point.id));
  next.closed = next.closed && next.points.length >= 3;
  return next;
}
export function stackTargetProblem(
  element: V4Element | undefined,
): string | null {
  if (!element || element.content.kind === "audio")
    return "Select a visual element to edit its stack.";
  if (element.visual && element.visual.profile !== "classic_canvas_v31")
    return "This stack uses a legacy visual profile.";
  return null;
}
export const acceptsMasks = (element: V4Element) =>
  ["video", "image", "graphic"].includes(element.content.kind);
export function validateMask(mask: Mask): string | null {
  if (
    mask.shape.kind.startsWith("legacy_") ||
    mask.feather.unit !== "classic_pixels"
  )
    return null; // Preserve, do not reinterpret legacy values.
  if (
    !Number.isFinite(mask.feather.value) ||
    mask.feather.value < 0 ||
    !Number.isFinite(mask.stroke.width_px) ||
    mask.stroke.width_px < 0
  )
    return "Feather and stroke must be finite and nonnegative.";
  const color = mask.stroke.color;
  if (
    ![color.red, color.green, color.blue, color.alpha].every(
      (value) => Number.isFinite(value) && value >= 0 && value <= 1,
    )
  )
    return "Color channels must be between zero and one.";
  const shape = mask.shape;
  if (
    "geometry" in shape &&
    (!Object.values(shape.geometry).every(Number.isFinite) ||
      shape.geometry.width <= 0 ||
      shape.geometry.height <= 0 ||
      shape.geometry.scale < 0)
  )
    return "Enter positive mask dimensions and a nonnegative scale.";
  if (
    shape.kind === "split" &&
    ![shape.center_x, shape.center_y, shape.rotation_degrees].every(
      Number.isFinite,
    )
  )
    return "Enter finite mask coordinates.";
  if (shape.kind === "freeform") {
    const p = shape.path;
    if (
      (p.closed && p.points.length < 3) ||
      p.points.length > 4096 ||
      new Set(p.points.map((point) => point.id)).size !== p.points.length
    )
      return "A closed path needs three points; point IDs must be unique and the limit is 4096.";
    if (
      ![p.center_x, p.center_y, p.rotation_degrees, p.scale].every(
        Number.isFinite,
      ) ||
      p.scale < 0 ||
      p.points.some(
        ({ id, ...point }) =>
          !id || !Object.values(point).every(Number.isFinite),
      )
    )
      return "Enter finite path coordinates and a nonnegative scale.";
  }
  if (shape.kind === "text") {
    const p = shape.mask;
    if (p.font.kind !== "locked")
      return "Choose an imported static font before saving a text mask.";
    if (
      ![
        p.font_size.value,
        p.line_height_multiplier,
        p.scale,
        p.letter_spacing_px,
        p.center_x,
        p.center_y,
        p.rotation_degrees,
      ].every(Number.isFinite) ||
      p.font_size.value <= 0 ||
      p.line_height_multiplier <= 0 ||
      p.scale < 0
    )
      return "Enter valid text mask size and spacing.";
  }
  return null;
}
export function validateStack(element: V4Element): string | null {
  for (const mask of element.visual?.masks ?? []) {
    const issue = validateMask(mask);
    if (issue) return issue;
  }
  const effects = [
    ...(element.visual?.effects ?? []),
    ...(element.content.kind === "effect_layer"
      ? [element.content.effect]
      : []),
  ];
  if (
    effects.some(
      (e) =>
        e.effect.kind === "classic_blur" &&
        (!Number.isFinite(e.effect.intensity) || e.effect.intensity < 0),
    )
  )
    return "Blur intensity must be finite and nonnegative.";
  return null;
}
export function newEffectTrack(
  start: V4Element["timeline_range"]["start"],
  duration: V4Element["timeline_range"]["duration"],
): Track {
  return {
    id: crypto.randomUUID(),
    name: "Blur effect layer",
    kind: "effect",
    enabled: true,
    locked: false,
    muted: false,
    opacity: 1,
    gain: { unit: "decibels", value: 0 },
    channels: [],
    elements: [
      {
        id: crypto.randomUUID(),
        name: "Blur",
        enabled: true,
        timeline_range: {
          start: structuredClone(start),
          duration: structuredClone(duration),
        },
        source_span: {
          profile: "classic_trimmed",
          trim_start: { ticks: "0", timebase: 1 },
          trim_end: { ticks: "0", timebase: 1 },
          source_duration: null,
        },
        content: { kind: "effect_layer", effect: createBlur() },
        channels: [],
        visual: {
          profile: "classic_canvas_v31",
          transform: {
            position_x_px: 0,
            position_y_px: 0,
            scale_x: 1,
            scale_y: 1,
            rotation_degrees: 0,
          },
          opacity: 1,
          blend_mode: "normal",
          effects: [],
          masks: [],
        },
      },
    ],
  };
}
/** No TrackPut ordering field exists. Preserve every scene field and reorder
 * only the selected effect track using the original public SequencePut. */
export function moveEffectTrack(
  sequence: V4Sequence,
  id: string,
  delta: -1 | 1,
): V4Operation {
  const index = sequence.tracks.findIndex((t) => t.id === id),
    track = sequence.tracks[index];
  if (!track || track.kind !== "effect" || track.locked)
    throw new Error("Unlock the effect track before reordering it.");
  const next = structuredClone(sequence);
  next.tracks = move(next.tracks, index, delta);
  return { type: "sequence_put", sequence: next };
}
/** De Casteljau subdivision retains the exact cubic when inserting a point. */
export function splitPathSegment(
  points: readonly Point[],
  index: number,
  closed: boolean,
): Point[] {
  if (
    index < 0 ||
    index >= points.length ||
    (!closed && index === points.length - 1) ||
    points.length >= 4096
  )
    return structuredClone([...points]);
  const next = structuredClone([...points]),
    i = (index + 1) % next.length,
    p = next[index],
    q = next[i];
  const mid = (a: { x: number; y: number }, b: { x: number; y: number }) => ({
    x: (a.x + b.x) / 2,
    y: (a.y + b.y) / 2,
  });
  const a = mid(p, { x: p.out_x, y: p.out_y }),
    b = mid({ x: p.out_x, y: p.out_y }, { x: q.in_x, y: q.in_y }),
    c = mid({ x: q.in_x, y: q.in_y }, q),
    d = mid(a, b),
    e = mid(b, c),
    anchor = mid(d, e);
  p.out_x = a.x;
  p.out_y = a.y;
  q.in_x = c.x;
  q.in_y = c.y;
  next.splice(index + 1, 0, {
    id: crypto.randomUUID(),
    ...anchor,
    in_x: d.x,
    in_y: d.y,
    out_x: e.x,
    out_y: e.y,
  });
  return next;
}
export function moveAnchor(point: Point, x: number, y: number): Point {
  const dx = x - point.x,
    dy = y - point.y;
  return {
    ...point,
    x,
    y,
    in_x: point.in_x + dx,
    in_y: point.in_y + dy,
    out_x: point.out_x + dx,
    out_y: point.out_y + dy,
  };
}
