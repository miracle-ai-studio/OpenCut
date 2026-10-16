import {
  composeGraphicFrameV1,
  GRAPHIC_FRAME_PROFILE_V1,
  validateGraphicFramePlanV1,
} from "./graphic-plan-v1.mjs";
import { blendModeIndexV1 } from "./blend-modes-v1.mjs";
import { checkAbort, reject, sameJsonFacts } from "./validation.mjs";

export const BLEND_FRAME_PROFILE_V1 = "classic-v31-gpu-blend-frame-v1";
const clone = (value) => structuredClone(value);
function exactKeys(value, keys) {
  return (
    value &&
    !Array.isArray(value) &&
    sameJsonFacts(Object.keys(value).sort(), [...keys].sort())
  );
}
export function validateBlendFramePlanV1(plan, admission) {
  if (
    !exactKeys(plan, ["identity", "execution", "blends"]) ||
    plan.identity?.profile !== BLEND_FRAME_PROFILE_V1 ||
    !sameJsonFacts(
      { ...plan.identity, profile: GRAPHIC_FRAME_PROFILE_V1 },
      plan.execution?.identity,
    )
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Blend wrapper changed fixed graphic identity",
    );
  validateGraphicFramePlanV1(plan.execution, admission);
  if (
    !Array.isArray(plan.blends) ||
    plan.blends.length !== plan.execution.order.length
  )
    reject("INVALID_VISUAL_PLAN", "Blend map must cover every fixed layer");
  for (const [index, blend] of plan.blends.entries()) {
    if (
      !exactKeys(blend, ["elementId", "blendMode"]) ||
      blend.elementId !== plan.execution.order[index]
    )
      reject("INVALID_VISUAL_PLAN", "Blend map changed complete layer order");
    blendModeIndexV1(blend.blendMode);
  }
}

export function verifyBlendBoundV1(plan, childBound, bound) {
  if (
    !sameJsonFacts(bound, {
      identity: plan.identity,
      execution: childBound,
      blends: plan.blends,
    })
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Blend binding changed fixed identity, source binding or modes",
    );
  if (
    !Array.isArray(childBound.layers) ||
    !sameJsonFacts(
      childBound.layers.map((layer) => layer.elementId),
      plan.blends.map((blend) => blend.elementId),
    )
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Blend binding changed complete child layer order",
    );
  return bound;
}

// All resource reads, raster admission and source observations stay in the
// existing graphic composition. Only after that succeeds may the final host
// binding authorize modes. The original bound DTO stays byte-for-byte intact.
export async function composeBlendFrameV1(
  plan,
  host,
  state,
  signal,
  createCanvas,
  Path,
  sceneSynthesis,
) {
  const execution = await composeGraphicFrameV1(
    plan.execution,
    host,
    state,
    signal,
    createCanvas,
    Path,
    sceneSynthesis,
  );
  checkAbort(signal);
  const wrapper = clone(await host.callback("bindBlendFrameV1", {}));
  checkAbort(signal);
  verifyBlendBoundV1(plan, execution.bound, wrapper);
  const bound = {
    ...clone(execution.bound),
    identity: clone(plan.identity),
    layers: execution.bound.layers.map((layer, index) => ({
      ...clone(layer),
      blendMode: plan.blends[index].blendMode,
    })),
  };
  return { bound, execution, blends: clone(plan.blends), wrapper };
}
