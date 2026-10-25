import { renderFrame } from './index.mjs';
import { composeGraphicFrame, GRAPHIC_FRAME_PROFILE, validateGraphicFramePlan } from './graphic-plan.mjs';
import { checkAbort } from './validation.mjs';
import { clearGpuFrameCaches } from './gpu.mjs';
import { retainTextFrameFonts } from './text-frame.mjs';
export async function renderClassicGraphicFrame(fixedPlan,host,realm,signal,cache,decoders,fontCache) {
  try {
  const plan=structuredClone(fixedPlan);validateGraphicFramePlan(plan,host.admission);let captured;
  retainTextFrameFonts(fontCache, plan.execution.execution.texts.map(t => t.font));
  // Decode, font authorization, RGBA transport and GPU rendering run once.
  const actual=await renderFrame(plan.execution.execution.media,host,realm,signal,cache,decoders,async state=>{
    captured=await composeGraphicFrame(plan,host,state,signal);return captured.bound;
  },fontCache,plan);
  checkAbort(signal);
  return graphicFrameReceipt(plan,captured,actual);
  } catch(error) {
    clearGpuFrameCaches(realm);
    throw error;
  }
}
export function graphicFrameReceipt(plan,captured,actual) {
  const child=plan.execution;
  return {identity:plan.identity,engineIdentity:GRAPHIC_FRAME_PROFILE,
    execution:{identity:child.identity,engineIdentity:child.identity.profile,
      execution:{identity:child.execution.identity,engineIdentity:child.execution.identity.profile,bound:captured.childBound,rgbaBytes:actual.rgbaBytes,peakAccountedBytes:actual.peakAccountedBytes},
      captions:captured.captions},graphics:captured.graphics};
}
