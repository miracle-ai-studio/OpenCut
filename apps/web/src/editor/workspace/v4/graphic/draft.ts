import { converter, formatHex } from "culori";
import type { GraphicInitial, GraphicValues } from "./types";
import { secondsForDisplay } from "../../../model/time";
export type GraphicDraft = Record<
  keyof ReturnType<typeof graphicDraft>,
  string
>;
const rgb = converter("rgb");
const displayColor = (color: GraphicInitial["graphic"]["fill"]) =>
  formatHex(
    rgb({
      mode: color.encoding === "linear_srgb" ? "lrgb" : "rgb",
      r: color.red,
      g: color.green,
      b: color.blue,
    }),
  )!;
export function graphicDraft(initial: GraphicInitial) {
  const geometry = initial.graphic.geometry;
  return {
    name: initial.name,
    start: String(secondsForDisplay(initial.timeline_range.start)),
    duration: String(secondsForDisplay(initial.timeline_range.duration)),
    width: String(initial.graphic.source_size.width),
    height: String(initial.graphic.source_size.height),
    rounding:
      "corner_radius_percent" in geometry
        ? String(geometry.corner_radius_percent)
        : "",
    sides: "sides" in geometry ? String(geometry.sides) : "",
    points: "points" in geometry ? String(geometry.points) : "",
    depth: "depth_percent" in geometry ? String(geometry.depth_percent) : "",
    fill: displayColor(initial.graphic.fill),
    fillAlpha: String(initial.graphic.fill.alpha),
    stroke: displayColor(initial.graphic.stroke.color),
    strokeAlpha: String(initial.graphic.stroke.color.alpha),
    strokeWidth: String(initial.graphic.stroke.width_px),
    strokeAlign: initial.graphic.stroke.align,
    x: String(initial.transform.position_x_px),
    y: String(initial.transform.position_y_px),
    scaleX: String(initial.transform.scale_x),
    scaleY: String(initial.transform.scale_y),
    rotation: String(initial.transform.rotation_degrees),
    opacity: String(initial.opacity),
  };
}
const number = (value: string) => {
  if (!value.trim() || !Number.isFinite(Number(value)))
    throw new Error("Enter finite graphic parameter values.");
  return Number(value);
};
export function graphicDraftValues(
  initial: GraphicInitial,
  draft: GraphicDraft,
): GraphicValues {
  const original = graphicDraft(initial),
    graphic = structuredClone(initial.graphic),
    transform = structuredClone(initial.transform);
  const numeric = (
    key: keyof GraphicDraft,
    assign: (value: number) => void,
  ) => {
    if (draft[key] !== original[key]) assign(number(draft[key]));
  };
  numeric("width", (value) => {
    graphic.source_size.width = value;
  });
  numeric("height", (value) => {
    graphic.source_size.height = value;
  });
  const geometry = graphic.geometry;
  if ("corner_radius_percent" in geometry)
    numeric("rounding", (value) => {
      geometry.corner_radius_percent = value;
    });
  if ("sides" in geometry)
    numeric("sides", (value) => {
      geometry.sides = value;
    });
  if ("points" in geometry)
    numeric("points", (value) => {
      geometry.points = value;
    });
  if ("depth_percent" in geometry)
    numeric("depth", (value) => {
      geometry.depth_percent = value;
    });
  const color = (
    key: "fill" | "stroke",
    target: GraphicInitial["graphic"]["fill"],
  ) => {
    if (draft[key] === original[key]) return target;
    if (!/^#[0-9a-f]{6}$/i.test(draft[key]))
      throw new Error("Choose a valid graphic color.");
    return {
      encoding: "srgb" as const,
      red: parseInt(draft[key].slice(1, 3), 16) / 255,
      green: parseInt(draft[key].slice(3, 5), 16) / 255,
      blue: parseInt(draft[key].slice(5, 7), 16) / 255,
      alpha: target.alpha,
    };
  };
  graphic.fill = color("fill", graphic.fill);
  graphic.stroke.color = color("stroke", graphic.stroke.color);
  numeric("fillAlpha", (value) => {
    graphic.fill.alpha = value;
  });
  numeric("strokeAlpha", (value) => {
    graphic.stroke.color.alpha = value;
  });
  numeric("strokeWidth", (value) => {
    graphic.stroke.width_px = value;
  });
  if (!["inside", "center", "outside"].includes(draft.strokeAlign))
    throw new Error("Choose a valid stroke alignment.");
  graphic.stroke.align = draft.strokeAlign as typeof graphic.stroke.align;
  const keys = {
    x: "position_x_px",
    y: "position_y_px",
    scaleX: "scale_x",
    scaleY: "scale_y",
    rotation: "rotation_degrees",
  } as const;
  for (const key of Object.keys(keys) as (keyof typeof keys)[])
    numeric(key, (value) => {
      transform[keys[key]] = value;
    });
  return {
    name: draft.name,
    graphic,
    transform,
    opacity:
      draft.opacity === original.opacity
        ? initial.opacity
        : number(draft.opacity),
    start:
      draft.start === original.start
        ? { kind: "exact", time: structuredClone(initial.timeline_range.start) }
        : { kind: "decimal_seconds", value: draft.start },
    duration:
      draft.duration === original.duration
        ? {
            kind: "exact",
            time: structuredClone(initial.timeline_range.duration),
          }
        : { kind: "decimal_seconds", value: draft.duration },
  };
}
