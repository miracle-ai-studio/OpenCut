import {test,mock} from 'node:test';
import assert from 'node:assert/strict';
let executions=0,run;
mock.module('../src/index.mjs',{namedExports:{renderFrame:async(...args)=>{executions++;return run(...args);}}});
mock.module('../src/gpu.mjs',{namedExports:{clearGpuFrameCaches(){}}});
mock.module('../src/graphic-frame.mjs',{namedExports:{graphicFrameReceipt:()=>({})}});
mock.module('../src/scene-plan.mjs',{namedExports:{SCENE_FRAME_PROFILE:'scene',validateSceneFramePlan(){},sceneFontResources:()=>new Map(),composeSceneFrame:async()=>({bound:{},child:{execution:{}}})}});
const {renderClassicSceneFrame}=await import('../src/scene-frame.mjs');
const fixed={identity:{},execution:{identity:{},blends:[],execution:{execution:{execution:{media:{}}}}}};
const cache=()=>new Map([['last-font',{bytes:new Uint8Array(3)}]]);
test('unused preview fonts are charged on the first attempt and evicted before one exact-frame quota retry',async()=>{
 executions=0;const fonts=cache();
 run=async(_p,_h,_r,_s,_c,_d,compose,current)=>{
  if(executions===1){assert.equal(current.size,1);throw Object.assign(Error('quota'),{code:'VISUAL_BUDGET_EXCEEDED'});}
  assert.equal(current.size,0);await compose({});return {};
 };
 await renderClassicSceneFrame(fixed,{admission:{},captureSurface(){}},{},undefined,undefined,undefined,fonts);
 assert.equal(executions,2);assert.equal(fonts.size,0);
});
test('authority failures never retry and active/ordinary budgets retain original failure semantics',async()=>{
 for(const code of ['NOT_AUTHORIZED','PREVIEW_GENERATION_CONFLICT','VISUAL_GPU_FAILED']){
  executions=0;const fonts=cache();run=async()=>{throw Object.assign(Error(code),{code});};
  await assert.rejects(renderClassicSceneFrame(fixed,{admission:{},captureSurface(){}},{},undefined,undefined,undefined,fonts),{code});
  assert.equal(executions,1);assert.equal(fonts.size,1);
 }
 executions=0;run=async()=>{throw Object.assign(Error('quota'),{code:'VISUAL_BUDGET_EXCEEDED'});};
 await assert.rejects(renderClassicSceneFrame(fixed,{admission:{}},{},undefined,undefined,undefined,cache()),{code:'VISUAL_BUDGET_EXCEEDED'});assert.equal(executions,1);
});
