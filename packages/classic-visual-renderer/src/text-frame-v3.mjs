import { createCaptionHooksV3 } from "./caption-hooks-v3.mjs";
import { renderFrame } from "./index.mjs";
import { composeTextFrame } from "./text-frame.mjs";
import { checkAbort } from "./validation.mjs";
import { CAPTION_FRAME_PROFILE_V3, validateCaptionFramePlanV3 } from "./caption-plan-v3.mjs";
export { CAPTION_FRAME_PROFILE_V3, validateCaptionFramePlanV3 } from "./caption-plan-v3.mjs";
/** Shared decode/font-resource/budget/GPU orchestration is unchanged. V3 adds
 * only caption raster preparation and a separately versioned bind message. */
export async function renderClassicCaptionFrameV3(fixedPlan, host, realm, signal, cache, decoders, fontCache) {
  const plan = structuredClone(fixedPlan); validateCaptionFramePlanV3(plan, host.admission);
  const { hooks, receipts } = createCaptionHooksV3(plan, host);
  const receipt = await renderFrame(plan.execution.media, host, realm, signal, cache, decoders,
    state => composeTextFrame(plan.execution, host, state, signal, hooks), fontCache);
  checkAbort(signal);
  return { identity: plan.identity, engineIdentity: CAPTION_FRAME_PROFILE_V3, execution: receipt, captions: receipts };
}
