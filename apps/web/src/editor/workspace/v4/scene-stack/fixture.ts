import type { SceneGeometry, Font } from "./types";
export const geometry: SceneGeometry = {
  schema_version: 1,
  revision: "r0",
  element_id: "image",
  quad: {
    centerX: 320,
    centerY: 180,
    width: 640,
    height: 360,
    rotationRadians: 0,
    opacity: 1,
    flipX: false,
    flipY: false,
  },
  default_geometry: {
    center_x: 0,
    center_y: 0,
    width: 216 / 640,
    height: 0.6,
    rotation_degrees: 0,
    scale: 1,
  },
};
export const font: Font = {
  id: "font",
  family: "Lato",
  weight: 700,
  style: "italic",
  face_index: 0,
  content: { id: "font-version", content_hash: "b".repeat(64), size: 12 },
  origin: { kind: "legacy" },
};
