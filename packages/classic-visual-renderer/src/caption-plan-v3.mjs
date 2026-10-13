import { validateTextFramePlan } from "./text-frame.mjs";
import { CAPTION_WRAP_PROFILE_V3 } from "./caption-layout-v3.mjs";
import { reject, sameJsonFacts } from "./validation.mjs";
export const CAPTION_FRAME_PROFILE_V3 = "classic-v31-gpu-normal-caption-frame-v3";
export function validateCaptionFramePlanV3(plan, admission) {
  if (!plan || plan.identity?.profile !== CAPTION_FRAME_PROFILE_V3 || !Array.isArray(plan.captions)) reject("INVALID_VISUAL_PLAN", "Invalid V3 caption frame plan");
  const identity = { ...plan.identity, profile: "classic-v31-gpu-normal-text-frame-v2" };
  if (!sameJsonFacts(identity, plan.execution?.identity)) reject("INVALID_VISUAL_PLAN", "V3 execution partition changed fixed identity");
  validateTextFramePlan(plan.execution, admission);
  const ids = new Set();
  for (const caption of plan.captions) {
    if (ids.has(caption.elementId) || !plan.execution.texts.some(layer => layer.elementId === caption.elementId) || caption.wrapProfile !== CAPTION_WRAP_PROFILE_V3 || !Number.isFinite(caption.maxWidthPx) || caption.maxWidthPx <= 0 || caption.maxWidthPx > identity.width || caption.bottomMarginPx !== identity.height * .05)
      reject("INVALID_VISUAL_PLAN", "V3 caption directives changed identity or fixed placement");
    ids.add(caption.elementId);
  }
}
