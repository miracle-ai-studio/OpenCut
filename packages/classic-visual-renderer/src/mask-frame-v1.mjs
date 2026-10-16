import { renderFrame } from "./index.mjs";
import { graphicFrameReceiptV1 } from "./graphic-frame-v1.mjs";
import { BLEND_FRAME_PROFILE_V1 } from "./blend-plan-v1.mjs";
import { composeMaskFrameV1, MASK_FRAME_PROFILE_V1, validateMaskFramePlanV1 } from "./mask-plan-v1.mjs";
import { checkAbort } from "./validation.mjs";
import { clearGpuFrameCaches } from "./gpu.mjs";

export async function renderClassicMaskFrameV1(fixedPlan, host, realm, signal, cache, decoders, fontCache) {
  try {
    const plan = structuredClone(fixedPlan);
    validateMaskFramePlanV1(plan, host.admission);
    let captured;
    const graphic = plan.execution.execution;
    const actual = await renderFrame(graphic.execution.execution.media, host, realm, signal, cache, decoders, async state => {
      captured = await composeMaskFrameV1(plan, host, state, signal); return captured.bound;
    }, fontCache, graphic);
    checkAbort(signal);
    return { identity: plan.identity, engineIdentity: MASK_FRAME_PROFILE_V1,
      execution: { identity: plan.execution.identity, engineIdentity: BLEND_FRAME_PROFILE_V1,
        execution: graphicFrameReceiptV1(graphic, captured.execution.execution, actual), blends: captured.execution.blends },
      masks: captured.masks };
  } catch (error) { clearGpuFrameCaches(realm); throw error; }
}
