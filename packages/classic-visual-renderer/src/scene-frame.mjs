import { renderFrame } from './index.mjs';
import { graphicFrameReceipt } from './graphic-frame.mjs';
import { BLEND_FRAME_PROFILE } from './blend-plan.mjs';
import { SCENE_FRAME_PROFILE, validateSceneFramePlan, composeSceneFrame, sceneFontResources } from './scene-plan.mjs';
import { retainTextFrameFonts } from './text-frame.mjs';
import { checkAbort } from './validation.mjs';
import { clearGpuFrameCaches } from './gpu.mjs';
export async function renderClassicSceneFrame(fixed,host,realm,signal,cache,decoders,fontCache){
  try {const plan=structuredClone(fixed);validateSceneFramePlan(plan,host.admission);let captured;const graphic=plan.execution.execution;
    const fonts=[...sceneFontResources(plan).values()],dormant=typeof host.captureSurface==='function'&&fonts.length===0;
    // Keep only the last active set across a preview's text-free gap. The
    // shared frame budget still charges all retained font bytes every frame;
    // future use obtains fresh native authorization and immutable-face proof.
    retainTextFrameFonts(fontCache, fonts,dormant);
    const execute=()=>renderFrame(graphic.execution.execution.media,host,realm,signal,cache,decoders,async state=>{captured=await composeSceneFrame(plan,host,state,signal);return captured.bound;},fontCache,graphic,fonts);
    let actual;
    try {actual=await execute();}
    catch(error){
      // An unused cache may not make a formerly admissible frame fail. Evict
      // it and retry this exact frame once with the original fresh bindings.
      if(!dormant||!fontCache?.size||error?.code!=='VISUAL_BUDGET_EXCEEDED'||signal?.aborted)throw error;
      retainTextFrameFonts(fontCache,[]);actual=await execute();
    }
    checkAbort(signal);return {identity:plan.identity,engineIdentity:SCENE_FRAME_PROFILE,execution:{identity:plan.execution.identity,engineIdentity:BLEND_FRAME_PROFILE,execution:graphicFrameReceipt(graphic,captured.child.execution,actual),blends:plan.execution.blends},stacks:plan.stacks,maskFonts:plan.maskFonts,fontSynthesis:plan.fontSynthesis,steps:plan.steps,background:plan.background};
  }catch(error){clearGpuFrameCaches(realm);throw error;}
}
