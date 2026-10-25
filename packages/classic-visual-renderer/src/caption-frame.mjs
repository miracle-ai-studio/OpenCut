import { createCaptionHooks } from "./caption-hooks.mjs";
import { renderFrame } from "./index.mjs";
import { composeTextFrame, retainTextFrameFonts } from "./text-frame.mjs";
import { checkAbort } from "./validation.mjs";
import { CAPTION_FRAME_PROFILE, validateCaptionFramePlan } from "./caption-plan.mjs";
export { CAPTION_FRAME_PROFILE, validateCaptionFramePlan } from "./caption-plan.mjs";
/** Shared decode/font-resource/budget/GPU orchestration is unchanged. V3 adds
 * only caption raster preparation and a separately versioned bind message. */
export async function renderClassicCaptionFrame(fixedPlan, host, realm, signal, cache, decoders, fontCache) {
  const plan = structuredClone(fixedPlan); validateCaptionFramePlan(plan, host.admission);
  retainTextFrameFonts(fontCache, plan.execution.texts.map(t => t.font));
  const { hooks, receipts } = createCaptionHooks(plan, host);
  const receipt = await renderFrame(plan.execution.media, host, realm, signal, cache, decoders,
    state => composeTextFrame(plan.execution, host, state, signal, hooks), fontCache, undefined, plan.execution.texts.map(t=>t.font));
  checkAbort(signal);
  return { identity: plan.identity, engineIdentity: CAPTION_FRAME_PROFILE, execution: receipt, captions: receipts };
}
