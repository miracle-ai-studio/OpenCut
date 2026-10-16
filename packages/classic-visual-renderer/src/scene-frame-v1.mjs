import { renderFrame } from './index.mjs';
import { graphicFrameReceiptV1 } from './graphic-frame-v1.mjs';
import { BLEND_FRAME_PROFILE_V1 } from './blend-plan-v1.mjs';
import { SCENE_FRAME_PROFILE_V1, validateSceneFramePlanV1, composeSceneFrameV1 } from './scene-plan-v1.mjs';
import { checkAbort } from './validation.mjs';
import { clearGpuFrameCaches } from './gpu.mjs';
export async function renderClassicSceneFrameV1(fixed,host,realm,signal,cache,decoders,fontCache){
  try {const plan=structuredClone(fixed);validateSceneFramePlanV1(plan,host.admission);let captured;const graphic=plan.execution.execution;
    const actual=await renderFrame(graphic.execution.execution.media,host,realm,signal,cache,decoders,async state=>{captured=await composeSceneFrameV1(plan,host,state,signal);return captured.bound;},fontCache,graphic);
    checkAbort(signal);return {identity:plan.identity,engineIdentity:SCENE_FRAME_PROFILE_V1,execution:{identity:plan.execution.identity,engineIdentity:BLEND_FRAME_PROFILE_V1,execution:graphicFrameReceiptV1(graphic,captured.child.execution,actual),blends:plan.execution.blends},stacks:plan.stacks,maskFonts:plan.maskFonts,fontSynthesis:plan.fontSynthesis,steps:plan.steps,background:plan.background};
  }catch(error){clearGpuFrameCaches(realm);throw error;}
}
