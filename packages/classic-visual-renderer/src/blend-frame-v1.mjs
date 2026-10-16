import { renderFrame } from "./index.mjs";
import { graphicFrameReceiptV1 } from "./graphic-frame-v1.mjs";
import {
  BLEND_FRAME_PROFILE_V1,
  composeBlendFrameV1,
  validateBlendFramePlanV1,
} from "./blend-plan-v1.mjs";
import { checkAbort } from "./validation.mjs";
import { clearGpuFrameCaches } from "./gpu.mjs";

export async function renderClassicBlendFrameV1(
  fixedPlan,
  host,
  realm,
  signal,
  cache,
  decoders,
  fontCache,
) {
  try {
  const plan = structuredClone(fixedPlan);
  validateBlendFramePlanV1(plan, host.admission);
  let captured;
  const actual = await renderFrame(
    plan.execution.execution.execution.media,
    host,
    realm,
    signal,
    cache,
    decoders,
    async (state) => {
      captured = await composeBlendFrameV1(plan, host, state, signal);
      return captured.bound;
    },
    fontCache,
    plan.execution,
  );
  checkAbort(signal);
  return {
    identity: plan.identity,
    engineIdentity: BLEND_FRAME_PROFILE_V1,
    execution: graphicFrameReceiptV1(
      plan.execution,
      captured.execution,
      actual,
    ),
    blends: captured.blends,
  };
  } catch (error) {
    clearGpuFrameCaches(realm);
    throw error;
  }
}
