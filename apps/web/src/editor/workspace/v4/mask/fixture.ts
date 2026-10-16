import type { BoxMask, MaskGeometryPlan } from "./types";
export const boxMask = (): BoxMask => ({
  id: "mask",
  enabled: true,
  inverted: false,
  shape: {
    kind: "rectangle",
    geometry: {
      center_x: 0,
      center_y: 0,
      width: 0.6,
      height: 0.6,
      rotation_degrees: 0,
      scale: 2.123456789,
    },
  },
  feather: { unit: "classic_pixels", value: 8.125 },
  stroke: {
    width_px: 0,
    align: "outside",
    color: {
      encoding: "srgb",
      red: 0.123456789,
      green: 0.2,
      blue: 0.3,
      alpha: 0.4,
    },
  },
});
export const geometryPlan = (): MaskGeometryPlan => ({
  schema_version: 1,
  revision: "r0",
  element_id: "image",
  quad: {
    centerX: 320,
    centerY: 180,
    width: 320,
    height: 180,
    rotationRadians: 0,
    flipX: false,
    flipY: false,
    opacity: 1,
  },
  default_geometry: {
    center_x: 0,
    center_y: 0,
    width: 108 / 320,
    height: 0.6,
    rotation_degrees: 0,
    scale: 1,
  },
});
