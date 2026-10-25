import { renderFrame } from "./index.mjs";
import { graphicFrameReceipt } from "./graphic-frame.mjs";
import { BLEND_FRAME_PROFILE } from "./blend-plan.mjs";
import { composeMaskFrame, MASK_FRAME_PROFILE, validateMaskFramePlan } from "./mask-plan.mjs";
import { checkAbort } from "./validation.mjs";
import { clearGpuFrameCaches } from "./gpu.mjs";
import { retainTextFrameFonts } from "./text-frame.mjs";

export async function renderClassicMaskFrame(fixedPlan, host, realm, signal, cache, decoders, fontCache) {
  try {
    const plan = structuredClone(fixedPlan);
    validateMaskFramePlan(plan, host.admission);
    let captured;
    const graphic = plan.execution.execution;
    retainTextFrameFonts(fontCache, graphic.execution.execution.texts.map(t => t.font));
    const actual = await renderFrame(graphic.execution.execution.media, host, realm, signal, cache, decoders, async state => {
      captured = await composeMaskFrame(plan, host, state, signal); return captured.bound;
    }, fontCache, graphic);
    checkAbort(signal);
    return { identity: plan.identity, engineIdentity: MASK_FRAME_PROFILE,
      execution: { identity: plan.execution.identity, engineIdentity: BLEND_FRAME_PROFILE,
        execution: graphicFrameReceipt(graphic, captured.execution.execution, actual), blends: captured.execution.blends },
      masks: captured.masks };
  } catch (error) { clearGpuFrameCaches(realm); throw error; }
}
