import { BLEND_FRAME_PROFILE, composeBlendFrame, validateBlendFramePlan } from "./blend-plan.mjs";
import { exactMaskKeys, prepareMaskFrameSources, validateMask, validateMaskQuad } from "./mask-source.mjs";
import { checkAbort, reject, sameJsonFacts } from "./validation.mjs";

export const MASK_FRAME_PROFILE = "classic-v31-gpu-mask-frame-v1";
function f32Spacing(value) {
  const magnitude = Math.abs(Math.fround(value));
  return magnitude < 2 ** -126 ? 2 ** -149 : 2 ** (Math.floor(Math.log2(magnitude)) - 23);
}
function linkedF32(actual, observed, ulps = 0) {
  const projected = Math.fround(actual), expected = Math.fround(observed);
  // These are wire-precision checks, never pixel tolerances. Absolute changes
  // of one or more are rejected even at huge off-canvas coordinates where f32
  // spacing alone would allow them. Both sides are rounded because Rust emits
  // f32 as its shortest decimal, not as an expanded f64 representation.
  if (!Number.isFinite(projected) || !Number.isFinite(expected) || Math.abs(actual - observed) >= 1) return false;
  return Math.abs(projected - expected) <= ulps * Math.max(f32Spacing(projected), f32Spacing(expected));
}
export function validateMaskFramePlan(plan, admission) {
  if (!exactMaskKeys(plan, ["identity", "execution", "masks"]) || plan.identity?.profile !== MASK_FRAME_PROFILE ||
      !sameJsonFacts({ ...plan.identity, profile: BLEND_FRAME_PROFILE }, plan.execution?.identity))
    reject("INVALID_VISUAL_PLAN", "Mask wrapper changed fixed blend identity");
  validateBlendFramePlan(plan.execution, admission);
  const graphic = plan.execution.execution;
  if (!Array.isArray(plan.masks) || plan.masks.length !== graphic.order.length)
    reject("INVALID_VISUAL_PLAN", "Mask map must cover every fixed layer");
  const visualIds = new Set([...graphic.graphics.map(layer => layer.elementId), ...graphic.execution.execution.media.layers.map(layer => layer.elementId)]);
  for (const [index, entry] of plan.masks.entries()) {
    if (!exactMaskKeys(entry, ["elementId", "mask"]) || entry.elementId !== graphic.order[index])
      reject("INVALID_VISUAL_PLAN", "Mask map changed complete layer order");
    validateMask(entry.mask);
    if (entry.mask !== null && !visualIds.has(entry.elementId)) reject("INVALID_VISUAL_PLAN", "Classic text and captions cannot carry masks");
  }
}
export function verifyMaskBound(plan, child, wrapper) {
  if (!exactMaskKeys(wrapper, ["identity", "execution", "masks"]) || !sameJsonFacts(wrapper.identity, plan.identity) ||
      !sameJsonFacts(wrapper.execution, child.wrapper) || !Array.isArray(wrapper.masks) || wrapper.masks.length !== plan.masks.length)
    reject("INVALID_VISUAL_PLAN", "Mask binding changed identity, child binding or mask inventory");
  for (const [index, entry] of wrapper.masks.entries()) {
    const layer = child.bound.layers[index], planned = plan.masks[index];
    if (!exactMaskKeys(entry, ["elementId", "mask", "quad"]) || !sameJsonFacts({ elementId: entry.elementId, mask: entry.mask }, planned) || entry.elementId !== layer.elementId)
      reject("INVALID_VISUAL_PLAN", "Mask binding changed ordered mask facts");
    validateMaskQuad(entry.quad);
    const old = layer.quad, actual = entry.quad;
    const graphic = plan.execution.execution.graphics.find(value => value.elementId === entry.elementId);
    if (entry.mask === null || old === null) {
      if (actual !== null) reject("INVALID_VISUAL_PLAN", "Inactive mask cannot allocate a logical quad");
      continue;
    }
    if (actual === null || actual.flipX !== old.flipX || actual.flipY !== old.flipY ||
        !["centerX", "centerY", "opacity"].every(key => linkedF32(actual[key], old[key])))
      reject("INVALID_VISUAL_PLAN", "Mask binding changed the logical source transform");
    // The new Canvas quad retains f64 transform geometry. The frozen GPU path
    // rounds degrees to f32, multiplies by its f32 PI/180 constant, then rounds
    // the product. Three ULPs bound that chain relative to the f64 radians.
    if (!linkedF32(actual.rotationRadians, old.rotationRadians, 3))
      reject("INVALID_VISUAL_PLAN", "Mask rotation differs from the bound GPU transform");
    let width = actual.width, height = actual.height, ulps = 0;
    if (graphic) {
      const receipt = child.execution.graphics.find(value => value.elementId === entry.elementId);
      // Reproject logical dimensions into the old padded GPU space. Native
      // computes (receipt * contain) * scale, while Canvas uses (source *
      // contain) * scale; changing multiplication order can cross a rounding
      // boundary. Two f32 ULPs conservatively cover the f64 re-association.
      width = width * receipt.width / graphic.graphic.sourceWidth;
      height = height * receipt.height / graphic.graphic.sourceHeight;
      ulps = 2;
    }
    if (!linkedF32(width, old.width, ulps) || !linkedF32(height, old.height, ulps))
      reject("INVALID_VISUAL_PLAN", "Mask binding changed the logical source dimensions");
  }
  return wrapper;
}
export async function composeMaskFrame(plan, host, state, signal, createCanvas, Path) {
  const execution = await composeBlendFrame(plan.execution, host, state, signal, createCanvas, Path);
  checkAbort(signal);
  const wrapper = structuredClone(await host.callback("bindMaskFrameV1", {}));
  checkAbort(signal); verifyMaskBound(plan, execution, wrapper);
  const bound = { ...structuredClone(execution.bound), identity: structuredClone(plan.identity), masks: structuredClone(wrapper.masks) };
  prepareMaskFrameSources(bound, state, host.admission, signal, createCanvas, Path);
  return { bound, execution, masks: structuredClone(plan.masks), wrapper };
}
