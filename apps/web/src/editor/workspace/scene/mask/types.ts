import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type { Element } from "../types";

export type MaskGeometryRequest = ToolInput<"opencut_mask_geometry">;
export type MaskGeometryPlan = ToolOutput<"opencut_mask_geometry">;
export type Mask = NonNullable<Element["visual"]>["masks"][number];
export type BoxMask = Mask & {
  shape: Extract<Mask["shape"], { kind: "rectangle" | "ellipse" }>;
  feather: Extract<Mask["feather"], { unit: "classic_pixels" }>;
};
export type BoxGeometry = BoxMask["shape"]["geometry"];
export type MaskKind = BoxMask["shape"]["kind"];
export type Point = { x: number; y: number };
export type MaskHandle =
  | "move"
  | "rotate"
  | "feather"
  | "left"
  | "right"
  | "top"
  | "bottom"
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right";

export function isBoxMask(mask: Mask): mask is BoxMask {
  return (
    mask.enabled &&
    (mask.shape.kind === "rectangle" || mask.shape.kind === "ellipse") &&
    mask.feather.unit === "classic_pixels" &&
    !mask.inverted &&
    mask.stroke.width_px === 0
  );
}

export function maskTargetProblem(
  element: Element | undefined,
  trackChannels = 0,
): string | null {
  if (
    !element?.visual ||
    !["video", "image", "graphic"].includes(element.content.kind)
  )
    return "Only video, image and graphic elements support mask editing.";
  if (
    (element.visual.profile !== "classic_canvas_v31"
      && !(element.visual.profile === "legacy_composition_v3"
        && ["video", "image"].includes(element.content.kind))) ||
    element.visual.effects.some((effect) => effect.enabled)
  )
    return "This clip’s current effects or layout prevent mask editing.";
  if (element.channels.length || trackChannels)
    return "Animated mask targets cannot be edited statically.";
  const enabled = element.visual.masks.filter((mask) => mask.enabled);
  if (enabled.length > 1 || enabled.some((mask) => !isBoxMask(mask)))
    return "Only one enabled rectangle or ellipse mask with pixel feather, no inversion and no stroke can be edited.";
  return null;
}
