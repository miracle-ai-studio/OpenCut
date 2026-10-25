import { sceneFontResources,resolveSceneTextMaskFonts,prepareSceneTextMasks,createSceneSynthesis } from './scene-text.mjs';
export {sceneFontResources} from './scene-text.mjs';
import { validateBlendFramePlan, composeBlendFrame, BLEND_FRAME_PROFILE } from './blend-plan.mjs';
import { verifyMaskBound } from './mask-plan.mjs';
import { exactMaskKeys } from './mask-source.mjs';
import { validateSceneMask, prepareSceneSources, admitSceneMaskSources } from './scene-source.mjs';
import { sceneEffectPasses } from './scene-effects.mjs';
import { checkAbort, finite, integer, natural, reject, sameJsonFacts } from './validation.mjs';
export const SCENE_FRAME_PROFILE='classic-v31-gpu-scene-frame-v1';
export function validateSceneFramePlan(plan,admission){
  if(!exactMaskKeys(plan,['identity','execution','stacks','maskFonts','fontSynthesis','steps','background'])||plan.identity?.profile!==SCENE_FRAME_PROFILE||!sameJsonFacts({...plan.identity,profile:BLEND_FRAME_PROFILE},plan.execution?.identity))reject('INVALID_VISUAL_PLAN','Scene wrapper changed fixed identity');
  validateBlendFramePlan(plan.execution,admission);
  const ids=plan.execution.execution.order;
  if(!Array.isArray(plan.stacks)||!Array.isArray(plan.steps)||plan.steps.length>120||!sameJsonFacts(plan.stacks.map(s=>s.elementId),ids))reject('INVALID_VISUAL_PLAN','Scene stacks changed complete layer order');
  for(const stack of plan.stacks){if(!exactMaskKeys(stack,['elementId','masks','effects'])||!Array.isArray(stack.masks)||stack.masks.length>64)reject('INVALID_VISUAL_PLAN','Invalid scene stack');for(const m of stack.masks)validateSceneMask(m);sceneEffectPasses(stack.effects,plan.identity.width,plan.identity.height);}
  const sceneFonts=sceneFontResources(plan);const media=plan.execution.execution.execution.execution.media;let encoded=media.resources.filter(r=>r.kind!=='video').reduce((n,r)=>n+natural(r.size,'scene media encoded bytes'),0n);for(const font of sceneFonts.values())encoded+=natural(font.size,'scene encoded font bytes');
  if(sceneFonts.size+media.resources.length>32||encoded>BigInt(integer(admission.maxEncodedBytes,'scene encoded admission',1)))reject('VISUAL_BUDGET_EXCEEDED','Scene mask fonts and media exceed aggregate admission');
  const seen=new Set(),layers=[];
  for(const step of plan.steps){if(!step||typeof step.element_id!=='string'||seen.has(step.element_id))reject('INVALID_VISUAL_PLAN','Duplicate scene step');seen.add(step.element_id);if(step.kind==='layer'){if(!exactMaskKeys(step,['kind','element_id']))reject('INVALID_VISUAL_PLAN','Invalid layer step');layers.push(step.element_id);}else if(step.kind==='effect'){if(!exactMaskKeys(step,['kind','element_id','effects'])||ids.includes(step.element_id))reject('INVALID_VISUAL_PLAN','Invalid scene effect step');sceneEffectPasses(step.effects,plan.identity.width,plan.identity.height);}else reject('INVALID_VISUAL_PLAN','Unknown scene step');}
  if(!sameJsonFacts(layers,ids))reject('INVALID_VISUAL_PLAN','Scene steps omit or reorder delegated layers');
  const bg=plan.background;
  if(bg?.kind==='color'){if(!exactMaskKeys(bg,['kind','color'])||!exactMaskKeys(bg.color,['encoding','red','green','blue','alpha'])||bg.color.encoding!=='srgb')reject('INVALID_VISUAL_PLAN','Invalid scene color');for(const k of ['red','green','blue','alpha'])if(finite(bg.color[k],k)<0||bg.color[k]>1)reject('INVALID_VISUAL_PLAN','Out of range background');}
  else if(bg?.kind==='main_track_blur'){const mediaIds=new Set(plan.execution.execution.execution.execution.media.layers.map(l=>l.elementId));if(!exactMaskKeys(bg,['kind','intensity','source_element_ids'])||finite(bg.intensity,'background blur')<0||!Array.isArray(bg.source_element_ids)||new Set(bg.source_element_ids).size!==bg.source_element_ids.length||bg.source_element_ids.some(id=>!mediaIds.has(id)))reject('INVALID_VISUAL_PLAN','Invalid main-track backdrop');}
  else reject('INVALID_VISUAL_PLAN','Unknown scene background');
}
export async function composeSceneFrame(plan,host,state,signal,createCanvas,Path){
  const synthesis=createSceneSynthesis(plan,signal);
  try {
  const child=await composeBlendFrame(plan.execution,host,state,signal,createCanvas,Path,synthesis);checkAbort(signal);
  const maskFontBytes=await resolveSceneTextMaskFonts(plan,state,signal);
  const wrapper=structuredClone(await host.callback('bindSceneFrameV1',{}));checkAbort(signal);
  if(!exactMaskKeys(wrapper,['identity','execution','layers','steps','background'])||!sameJsonFacts(wrapper.identity,plan.identity)||!sameJsonFacts(wrapper.execution,child.wrapper)||!sameJsonFacts(wrapper.steps,plan.steps)||!sameJsonFacts(wrapper.background,plan.background)||!Array.isArray(wrapper.layers)||wrapper.layers.length!==plan.stacks.length)reject('INVALID_VISUAL_PLAN','Scene binding changed fixed facts');
  for(let i=0;i<wrapper.layers.length;i++)if(!exactMaskKeys(wrapper.layers[i],['stack','quad'])||!sameJsonFacts(wrapper.layers[i].stack,plan.stacks[i]))reject('INVALID_VISUAL_PLAN','Scene binding changed raw stack');
  // Reuse the strict f64/f32 linked-quad proof without weakening the frozen
  // single-mask profile. The synthetic map is private and never rendered.
  const masks=plan.stacks.map(s=>({elementId:s.elementId,mask:s.masks.find(m=>m.enabled)??null}));
  verifyMaskBound({...plan,masks},child,{identity:plan.identity,execution:child.wrapper,masks:wrapper.layers.map((l,i)=>({...masks[i],quad:masks[i].mask?l.quad:null}))});
  const background=plan.background.kind==='color'?['red','green','blue','alpha'].map(k=>plan.background.color[k]):null;
  const bound={...structuredClone(child.bound),identity:structuredClone(plan.identity),background,scene:{layers:wrapper.layers,steps:wrapper.steps,background:wrapper.background}};
  admitSceneMaskSources(bound,state,host.admission);
  await prepareSceneTextMasks(plan,bound,state,host.admission,signal,createCanvas,maskFontBytes,synthesis);
  prepareSceneSources(bound,state,host.admission,signal,createCanvas,Path);
  return {bound,child,wrapper};
  } finally {synthesis.close();}
}
