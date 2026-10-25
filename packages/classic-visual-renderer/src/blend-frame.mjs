import { renderFrame } from "./index.mjs";
import { graphicFrameReceipt } from "./graphic-frame.mjs";
import {
  BLEND_FRAME_PROFILE,
  composeBlendFrame,
  validateBlendFramePlan,
} from "./blend-plan.mjs";
import { checkAbort } from "./validation.mjs";
import { clearGpuFrameCaches } from "./gpu.mjs";
import { retainTextFrameFonts } from "./text-frame.mjs";

export async function renderClassicBlendFrame(
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
  validateBlendFramePlan(plan, host.admission);
  retainTextFrameFonts(fontCache, plan.execution.execution.execution.texts.map(t => t.font));
  let captured;
  const actual = await renderFrame(
    plan.execution.execution.execution.media,
    host,
    realm,
    signal,
    cache,
    decoders,
    async (state) => {
      captured = await composeBlendFrame(plan, host, state, signal);
      return captured.bound;
    },
    fontCache,
    plan.execution,
  );
  checkAbort(signal);
  return {
    identity: plan.identity,
    engineIdentity: BLEND_FRAME_PROFILE,
    execution: graphicFrameReceipt(
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
