import { renderFrame } from './index.mjs';
import { composeGraphicFrameV1, GRAPHIC_FRAME_PROFILE_V1, validateGraphicFramePlanV1 } from './graphic-plan-v1.mjs';
import { checkAbort } from './validation.mjs';
import { clearGpuFrameCaches } from './gpu.mjs';
export async function renderClassicGraphicFrameV1(fixedPlan,host,realm,signal,cache,decoders,fontCache) {
  try {
  const plan=structuredClone(fixedPlan);validateGraphicFramePlanV1(plan,host.admission);let captured;
  // Decode, font authorization, RGBA transport and GPU rendering run once.
  const actual=await renderFrame(plan.execution.execution.media,host,realm,signal,cache,decoders,async state=>{
    captured=await composeGraphicFrameV1(plan,host,state,signal);return captured.bound;
  },fontCache,plan);
  checkAbort(signal);
  return graphicFrameReceiptV1(plan,captured,actual);
  } catch(error) {
    clearGpuFrameCaches(realm);
    throw error;
  }
}
export function graphicFrameReceiptV1(plan,captured,actual) {
  const child=plan.execution;
  return {identity:plan.identity,engineIdentity:GRAPHIC_FRAME_PROFILE_V1,
    execution:{identity:child.identity,engineIdentity:child.identity.profile,
      execution:{identity:child.execution.identity,engineIdentity:child.execution.identity.profile,bound:captured.childBound,rgbaBytes:actual.rgbaBytes,peakAccountedBytes:actual.peakAccountedBytes},
      captions:captured.captions},graphics:captured.graphics};
}
