import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { V4Document, V4Element, V4Sequence } from "../types";

export type SceneGeometry = ToolOutput<"opencut_scene_v1_geometry">;
export type SceneGeometryRequest = ToolInput<"opencut_scene_v1_geometry">;
export type SceneEditRequest = ToolInput<"opencut_edit_v4_apply">;
export type Mask = NonNullable<V4Element["visual"]>["masks"][number];
export type Effect = NonNullable<V4Element["visual"]>["effects"][number];
export type Font = V4Document["fonts"][number];
export type Track = V4Sequence["tracks"][number];
export type Shape = Mask["shape"];
export type ShapeKind = Exclude<
  Shape["kind"],
  "legacy_rectangle" | "legacy_ellipse"
>;
export type Freeform = Extract<Shape, { kind: "freeform" }>["path"];
export type Point = Freeform["points"][number];
export const MASK_KINDS: readonly ShapeKind[] = [
  "rectangle",
  "ellipse",
  "split",
  "cinematic_bars",
  "heart",
  "diamond",
  "star",
  "freeform",
  "text",
];
export const MASK_LABELS: Record<ShapeKind, string> = {
  rectangle: "Rectangle",
  ellipse: "Ellipse",
  split: "Split",
  cinematic_bars: "Cinematic bars",
  heart: "Heart",
  diamond: "Diamond",
  star: "Star",
  freeform: "Freeform",
  text: "Text",
};
