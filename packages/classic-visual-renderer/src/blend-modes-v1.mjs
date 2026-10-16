import { reject } from "./validation.mjs";

// Original Classic BlendMode::shader_code order. Unknown values never fall
// through to the vendor shader's default normal branch.
export const BLEND_MODES_V1 = Object.freeze([
  "normal",
  "darken",
  "multiply",
  "color-burn",
  "lighten",
  "screen",
  "plus-lighter",
  "color-dodge",
  "overlay",
  "soft-light",
  "hard-light",
  "difference",
  "exclusion",
  "hue",
  "saturation",
  "color",
  "luminosity",
]);
export function blendModeIndexV1(mode) {
  const index = BLEND_MODES_V1.indexOf(mode);
  if (index < 0)
    reject("INVALID_VISUAL_PLAN", "Unknown fixed Classic blend mode");
  return index;
}
