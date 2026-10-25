import { clearGraphicSourceCache } from "../src/graphic-source.mjs";
import {test,mock} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {validateGraphicFramePlan,GRAPHIC_FRAME_PROFILE} from '../src/graphic-plan.mjs';
const canvases=[],gpuCalls=[];
globalThis.Path2D=class {roundRect(){} ellipse(){} moveTo(){} lineTo(){} arcTo(){} closePath(){}};
globalThis.OffscreenCanvas=class {
  constructor(width,height){this.width=width;this.height=height;canvases.push(this);}
  getContext(){return {canvas:this,clearRect(){},fill(){},stroke(){},save(){},restore(){},clip(){},translate(){},drawImage(){},scale(){throw Error('quad applied twice');},rotate(){throw Error('quad applied twice');}};}
};
mock.module('../src/decoder.mjs',{namedExports:{VisualDecoderCache:class{},decodeImage:async()=>({width:2,height:1,canvas:new OffscreenCanvas(2,1)}),decodeVideo:async()=>{throw Error('unexpected video');}}});
mock.module('../src/gpu.mjs',{namedExports:{openGpuRealm:async()=>({}),clearGpuFrameCaches:clearGraphicSourceCache,closeGpuRealm:clearGraphicSourceCache,renderGpuFrame:async(bound,sources,_admission,emit)=>{
  gpuCalls.push({bound:structuredClone(bound),sourceIds:[...sources.keys()]});
  for(const layer of bound.layers)assert.ok(sources.get(layer.elementId).canvas.width>0);
  await emit(0,new Uint8Array(bound.identity.width*bound.identity.height*4));return {peakGpuBytes:'32'};
}}});
const {renderClassicGraphicFrame}=await import('../src/graphic-frame.mjs');
const bytes=Uint8Array.of(1,2,3,4),hash=createHash('sha256').update(bytes).digest('hex');
const admission={maxEncodedBytes:32,maxFramePixels:'32',maxDecodedPixels:'128',maxRgbaBytes:'4096',maxGpuBytes:'4096'};
function plan(){
 const fixed={projectId:'p',revision:'r',sequenceId:'s',time:{ticks:'1',timebase:7},width:8,height:4};
 const resource={assetId:'media',kind:'image',size:'4',contentHash:hash};
 const v2={identity:{...fixed,profile:'classic-v31-gpu-normal-text-frame-v2'},media:{identity:{...fixed,profile:'classic-v31-gpu-normal-frame'},resources:[resource],layers:[{elementId:'back',resource,decode:{kind:'image'}}]},texts:[],order:['back']};
 return {identity:{...fixed,profile:GRAPHIC_FRAME_PROFILE},execution:{identity:{...fixed,profile:'classic-v31-gpu-normal-caption-frame-v3'},execution:v2,captions:[]},graphics:[{elementId:'shape',graphic:{sourceWidth:2,sourceHeight:2,geometry:{kind:'star',points:5,depthPercent:45},fill:[1,0,0,.5],stroke:{color:[0,1,0,1],widthPx:0,align:'center'}},quad:{centerX:5,centerY:1,width:4,height:4,rotationRadians:1,opacity:1.5,flipX:true,flipY:false}}],order:['shape','back']};
}
function host(p,change=()=>{},signal){
 const calls=[],rgba=[];let media,child;
 return {calls,rgbaPackets:rgba,admission,async callback(op,args){
  calls.push(op);
  if(op==='resolveVerifiedBytes')return {transferId:'read',byteLength:'4'};
  if(op==='readVerifiedBytes')return {offset:'0',bytesHex:'01020304',eof:true};
  if(op==='releaseVerifiedBytes')return {};
  if(op==='admitDecode')return {maxDecodedBytes:'32'};
  if(op==='bindFrame'){media={identity:p.execution.execution.media.identity,clear:[0,0,0,0],background:[0,0,0,1],decoded:args.decoded,layers:[{elementId:'back',assetId:'media',quad:{centerX:4,centerY:2,width:8,height:4,rotationRadians:0,opacity:1,flipX:false,flipY:false}}]};return media;}
  if(op==='bindCaptionFrameV3'){child={...media,identity:p.execution.execution.identity,rasters:args.rasters};return child;}
  if(op==='bindGraphicFrameV1'){const bound={...child,identity:p.identity,captions:[],graphics:args.graphics,layers:p.order.map(id=>id==='back'?child.layers[0]:{elementId:id,assetId:`graphic:${id}`,quad:p.graphics.find(g=>g.elementId===id).quad})};change(bound,signal);return bound;}
  throw Error(op);
 },async rgba(offset,data){rgba.push({offset,count:data.length});}};
}
test('one GPU preserves complete order/raw quad; nested child receipt stays frozen with full actual accounting',async()=>{
 canvases.length=0;gpuCalls.length=0;const p=plan(),h=host(p);const receipt=await renderClassicGraphicFrame(p,h,{},null);
 assert.equal(gpuCalls.length,1);assert.deepEqual(gpuCalls[0].bound.layers.map(l=>l.elementId),p.order);assert.deepEqual(gpuCalls[0].bound.layers[0].quad,p.graphics[0].quad);
 assert.deepEqual(h.calls.filter(c=>c.startsWith('bind')),['bindFrame','bindCaptionFrameV3','bindGraphicFrameV1']);
 assert.equal(receipt.execution.execution.bound.identity.profile,'classic-v31-gpu-normal-text-frame-v2');assert.deepEqual(receipt.execution.execution.bound.layers.map(l=>l.elementId),['back']);assert.equal('graphics' in receipt.execution.execution.bound,false);
 assert.equal(receipt.execution.execution.rgbaBytes,'128');assert.equal(receipt.execution.execution.peakAccountedBytes,'236');assert.deepEqual(h.rgbaPackets,[{offset:0,count:128}]);
 assert.deepEqual(receipt.graphics,[{elementId:'shape',width:2,height:2,originX:0,originY:0}]);assert.ok(canvases.every(c=>c.width===0&&c.height===0));
});
test('bound tampering is rejected before GPU and releases all generated sources',async()=>{
 for(const change of [b=>b.identity.revision='other',b=>b.decoded=[],b=>b.rasters=[{}],b=>b.captions=[{}],b=>b.graphics[0].originX=1,b=>b.layers.reverse(),b=>b.layers[0].quad={...b.layers[0].quad,width:9},b=>b.layers[1].assetId='unauthorized']){
  canvases.length=0;gpuCalls.length=0;const p=plan();await assert.rejects(renderClassicGraphicFrame(p,host(p,change),{},null),e=>e.code==='INVALID_VISUAL_PLAN');assert.equal(gpuCalls.length,0);assert.ok(canvases.every(c=>c.width===0));
 }
});
test('complete identity/order/finite quad admissions reject bad input; null quad still generates full receipt',async()=>{
 for(const change of [p=>p.execution.identity.revision='other',p=>p.order=['back'],p=>p.graphics.push(p.graphics[0]),p=>p.order=['shape','shape'],p=>p.graphics[0].quad.opacity=Infinity]){const p=plan();change(p);assert.throws(()=>validateGraphicFramePlan(p,admission),e=>e.code==='INVALID_VISUAL_PLAN');}
 const p=plan();p.graphics[0].quad=null;const receipt=await renderClassicGraphicFrame(p,host(p),{},null);assert.equal(receipt.graphics.length,1);
});
test('aggregate media+graphic pixel budget and cancellation refuse before GPU; every allocation releases',async()=>{
 const p=plan();const limited={...host(p),admission:{...admission,maxDecodedPixels:'5'}};gpuCalls.length=0;canvases.length=0;
 await assert.rejects(renderClassicGraphicFrame(p,limited,{},null),e=>e.code==='VISUAL_BUDGET_EXCEEDED');assert.equal(canvases.length,1);assert.equal(gpuCalls.length,0);assert.ok(canvases.every(c=>c.width===0));
 const cancel=new AbortController(),reason=Error('cancelled');const h=host(p,()=>cancel.abort(reason));canvases.length=0;
 await assert.rejects(renderClassicGraphicFrame(p,h,{},cancel.signal),e=>e===reason);assert.ok(canvases.every(c=>c.width===0));
});

test('warm graphic frames retain only source pixels while renewing every binding and accounting the full active set',async()=>{
 canvases.length=0;gpuCalls.length=0;const p=plan(),realm={device:{}},h=host(p);
 const first=await renderClassicGraphicFrame(p,h,realm,null);
 const source=canvases.find(c=>c.width===2&&c.height===2);
 assert.ok(source);const count=canvases.length;
 // Time and quad changes must bind afresh, without changing source pixels.
 const next=structuredClone(p);for(const identity of [next.identity,next.execution.identity,next.execution.execution.identity,next.execution.execution.media.identity])identity.time.ticks='2';next.graphics[0].quad.opacity=.25;
 const nextHost=host(next),second=await renderClassicGraphicFrame(next,nextHost,realm,null);
 assert.equal(canvases.length,count+1);assert.equal(source.width,2);
 assert.equal(first.execution.execution.peakAccountedBytes,second.execution.execution.peakAccountedBytes);
 assert.deepEqual(nextHost.calls.filter(c=>c.startsWith('bind')),['bindFrame','bindCaptionFrameV3','bindGraphicFrameV1']);
 assert.equal(gpuCalls.at(-1).bound.layers[0].quad.opacity,.25);
 clearGraphicSourceCache(realm);assert.ok(canvases.every(c=>c.width===0));
});

test('warm graphic admission rejects before media allocation and clears retained sources; a later retry remains valid',async()=>{
 canvases.length=0;const p=plan(),realm={device:{}};
 await renderClassicGraphicFrame(p,host(p),realm,null);const source=canvases.find(c=>c.width>0),count=canvases.length;
 const limited=host(p);limited.admission={...admission,maxRgbaBytes:'191'};
 await assert.rejects(renderClassicGraphicFrame(p,limited,realm,null),e=>e.code==='VISUAL_BUDGET_EXCEEDED');
 assert.equal(limited.calls.length,0);assert.equal(canvases.length,count);assert.equal(source.width,0);
 await renderClassicGraphicFrame(p,host(p),realm,null);assert.ok(canvases.some(c=>c.width>0));
 clearGraphicSourceCache(realm);
});

test('binding rejection and cancellation discard warmed graphics before any further GPU output',async()=>{
 for(const cancel of [false,true]){
  canvases.length=0;gpuCalls.length=0;const p=plan(),realm={device:{}},controller=new AbortController(),reason=Error('cancel warm');
  await renderClassicGraphicFrame(p,host(p),realm,null);
  const failing=host(p,b=>{if(cancel)controller.abort(reason);else b.graphics[0].originX=1;});
  await assert.rejects(renderClassicGraphicFrame(p,failing,realm,controller.signal),e=>cancel?e===reason:e.code==='INVALID_VISUAL_PLAN');
  assert.equal(gpuCalls.length,1);assert.ok(canvases.every(c=>c.width===0));
 }
});

test('warm caches retire on wrapper and base-frame early validation errors before any callback, then retry',async()=>{
 const {renderClassicFrame}=await import('../src/index.mjs');
 for(const fault of ['wrapper','clone','width','frame-pixels','rgba-limit']){
  canvases.length=0;gpuCalls.length=0;let destroys=0;
  const p=plan(),realm={device:{destroy(){destroys++;}}};
  await renderClassicGraphicFrame(p,host(p),realm,null);
  const warm=canvases.find(c=>c.width>0),h=host(p),bad=structuredClone(p);
  let call;
  if(fault==='wrapper'){bad.order=[];call=()=>renderClassicGraphicFrame(bad,h,realm,null);}
  else if(fault==='clone'){bad.uncloneable=()=>{};call=()=>renderClassicGraphicFrame(bad,h,realm,null);}
  else {
   if(fault==='width')bad.execution.execution.media.identity.width=0;
   if(fault==='frame-pixels')h.admission={...admission,maxFramePixels:'31'};
   if(fault==='rgba-limit')h.admission={...admission,maxRgbaBytes:'invalid'};
   call=()=>renderClassicFrame(bad.execution.execution.media,h,realm,null);
  }
  await assert.rejects(call,e=>fault==='clone'?e.name==='DataCloneError':e.code==='INVALID_VISUAL_PLAN');
  assert.equal(warm.width,0,fault);assert.equal(h.calls.length,0);assert.equal(gpuCalls.length,1);assert.equal(destroys,0);
  await renderClassicGraphicFrame(p,host(p),realm,null);
  assert.equal(gpuCalls.length,2);assert.ok(canvases.some(c=>c.width>0));clearGraphicSourceCache(realm);
 }
});
