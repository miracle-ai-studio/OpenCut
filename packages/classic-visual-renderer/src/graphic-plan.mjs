import { CAPTION_FRAME_PROFILE, validateCaptionFramePlan } from './caption-plan.mjs';
import { createCaptionHooks } from './caption-hooks.mjs';
import { composeTextFrame } from './text-frame.mjs';
import { graphicSourceLayout, rasterizeGraphicSource, resolveGraphicSource, isRetainedGraphicSource } from './graphic-source.mjs';
import { checkAbort, finite, natural, reject, sameJsonFacts } from './validation.mjs';
export const GRAPHIC_FRAME_PROFILE='classic-v31-gpu-normal-graphic-frame-v1';
const clone=value=>structuredClone(value);
function validateQuad(quad) {
  if(quad===null)return;
  if(!quad || typeof quad.flipX!=='boolean'||typeof quad.flipY!=='boolean')reject('INVALID_VISUAL_PLAN','Invalid graphic quad');
  for(const key of ['centerX','centerY','width','height','rotationRadians','opacity'])finite(quad[key],'graphic '+key);
  if(quad.width<=0||quad.height<=0)reject('INVALID_VISUAL_PLAN','Invalid graphic quad dimensions');
}
export function validateGraphicFramePlan(plan,admission) {
  if(!plan||plan.identity?.profile!==GRAPHIC_FRAME_PROFILE||!Array.isArray(plan.graphics)||!Array.isArray(plan.order)||plan.order.length>60)reject('INVALID_VISUAL_PLAN','Invalid fixed graphic frame');
  if(!sameJsonFacts({...plan.identity,profile:CAPTION_FRAME_PROFILE},plan.execution?.identity))reject('INVALID_VISUAL_PLAN','Graphic execution partition changed fixed identity');
  validateCaptionFramePlan(plan.execution,admission);
  const ids=new Set(plan.execution.execution.order),graphics=new Set();let pixels=0n;
  for(const layer of plan.graphics) {
    if(typeof layer.elementId!=='string'||!layer.elementId||layer.elementId.length>1024||ids.has(layer.elementId))reject('INVALID_VISUAL_PLAN','Duplicate or invalid graphic element');
    ids.add(layer.elementId);graphics.add(layer.elementId);validateQuad(layer.quad);
    const layout=graphicSourceLayout(layer.graphic,{maxPixels:admission.maxDecodedPixels,maxRasterBytes:admission.maxRgbaBytes});
    pixels+=BigInt(layout.width)*BigInt(layout.height);
  }
  if(pixels>natural(admission.maxDecodedPixels,'graphic aggregate pixels'))reject('VISUAL_BUDGET_EXCEEDED','Graphic sources exceed aggregate pixel admission');
  if(ids.size!==plan.order.length||new Set(plan.order).size!==ids.size||plan.order.some(id=>!ids.has(id))||!sameJsonFacts(plan.order.filter(id=>!graphics.has(id)),plan.execution.execution.order))reject('INVALID_VISUAL_PLAN','Graphic order must preserve every delegated layer and cover all elements exactly once');
}
export function verifyGraphicBound(plan,childBound,captions,graphics,bound) {
  if(!bound||!sameJsonFacts(bound.identity,plan.identity)||!sameJsonFacts(bound.decoded,childBound.decoded)||!sameJsonFacts(bound.rasters,childBound.rasters)||!sameJsonFacts(bound.captions,captions)||!sameJsonFacts(bound.graphics,graphics)||!sameJsonFacts(bound.clear,childBound.clear)||!sameJsonFacts(bound.background,childBound.background)||!Array.isArray(bound.layers)||bound.layers.length!==plan.order.length)reject('INVALID_VISUAL_PLAN','Graphic binding changed fixed identity or delegated observations');
  const children=new Map(childBound.layers.map(layer=>[layer.elementId,layer])),planned=new Map(plan.graphics.map(layer=>[layer.elementId,layer]));
  for(let n=0;n<bound.layers.length;n++) {
    const actual=bound.layers[n],id=plan.order[n];
    if(actual.elementId!==id)reject('INVALID_VISUAL_PLAN','Graphic binding changed complete layer order');
    if(children.has(id)) {if(!sameJsonFacts(actual,children.get(id)))reject('INVALID_VISUAL_PLAN','Graphic binding changed a delegated child layer');}
    else if(!sameJsonFacts(actual,{elementId:id,assetId:`graphic:${id}`,quad:planned.get(id).quad}))reject('INVALID_VISUAL_PLAN','Graphic binding changed its precomputed quad');
  }
  return bound;
}
export async function composeGraphicFrame(plan,host,state,signal,createCanvas=(w,h)=>new OffscreenCanvas(w,h),Path=globalThis.Path2D,sceneSynthesis) {
  const {hooks,receipts:captions}=createCaptionHooks(plan.execution,host);
  const childBound=await composeTextFrame(plan.execution.execution,host,state,signal,sceneSynthesis?sceneSynthesis.wrapTextHooks(hooks):hooks);
  checkAbort(signal);
  const {sources,budget,canvases,releases,retainedDecoded,preparedGraphics}=state;
  let pixels=BigInt(retainedDecoded/4);
  for(const source of new Set(sources.values()))if(!source.borrowed)pixels+=BigInt(source.width)*BigInt(source.height);
  const layouts=preparedGraphics?.layouts??plan.graphics.map(layer=>graphicSourceLayout(layer.graphic,{maxPixels:host.admission.maxDecodedPixels,maxRasterBytes:host.admission.maxRgbaBytes}));
  for(const layout of layouts)pixels+=BigInt(layout.width)*BigInt(layout.height);
  if(pixels>natural(host.admission.maxDecodedPixels,'graphic aggregate pixels'))reject('VISUAL_BUDGET_EXCEEDED','Media, text and graphics exceed aggregate pixel admission');
  // Reserve all graphics before the first generated source allocation. Pending
  // scratch is included conservatively until the frame releases its resources.
  if(!preparedGraphics)for(const layout of layouts)releases.push(budget.reserve(Math.max(layout.peakRgbaBytes,layout.width*layout.height*16)));
  const graphics=[];
  for(const layer of plan.graphics) {
    checkAbort(signal);
    const limits={maxPixels:host.admission.maxDecodedPixels,maxRasterBytes:host.admission.maxRgbaBytes};
    const source=preparedGraphics?resolveGraphicSource(preparedGraphics,layer,limits,createCanvas,signal,Path):rasterizeGraphicSource(layer.graphic,limits,createCanvas,signal,Path);
    if(!isRetainedGraphicSource(source))canvases.add(source.canvas);sources.set(layer.elementId,source);
    graphics.push({elementId:layer.elementId,width:source.width,height:source.height,originX:source.originX,originY:source.originY});
  }
  checkAbort(signal);const bound=clone(await host.callback('bindGraphicFrameV1',{graphics:clone(graphics)}));checkAbort(signal);
  verifyGraphicBound(plan,childBound,captions,graphics,bound);
  return {bound,childBound,captions,graphics};
}
