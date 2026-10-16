import test from 'node:test';
import assert from 'node:assert/strict';
import {validateGraphicBeginV1,validateCaptionBeginV3} from '../host/protocol.mjs';
import {browserHooks} from '../host/browser.mjs';
import {executeGraphicVisualPageV1} from '../host/page.mjs';
function begin(){
 const fixed={projectId:'p',revision:'r',sequenceId:'s',time:{ticks:'1',timebase:7},width:8,height:4};
 const execution={identity:{...fixed,profile:'classic-v31-gpu-normal-text-frame-v2'},media:{identity:{...fixed,profile:'classic-v31-gpu-normal-frame'},resources:[],layers:[]},texts:[],order:[]};
 return {protocolVersion:1,type:'beginGraphicFrameV1',executionId:'graphic',plan:{identity:{...fixed,profile:'classic-v31-gpu-normal-graphic-frame-v1'},execution:{identity:{...fixed,profile:'classic-v31-gpu-normal-caption-frame-v3'},execution,captions:[]},graphics:[{elementId:'shape',graphic:{geometry:{kind:'ellipse'},sourceWidth:2,sourceHeight:2,fill:[0,0,0,0],stroke:{widthPx:1,color:[1,0,0,1],align:'outside'}},quad:null}],order:['shape']},resources:[],fonts:[],admission:{maxEncodedBytes:8,maxFramePixels:'32',maxDecodedPixels:'1024',maxGpuBytes:'16384',maxRgbaBytes:'16384',maxOutputBytes:'1048576',maxRssBytes:'536870912',deadlineMs:1000,quotaProfile:'monitored-terminate'},runtime:{chromiumExecutable:'/host/browser'}};
}
async function configured(packet,request=async()=>({})){
 const exposed=new Map(),calls=[];
 await browserHooks.configurePage({begin:packet,signal:new AbortController().signal,page:{exposeFunction:async(name,fn)=>exposed.set(name,fn)},broker:{request:(op,args)=>{calls.push({op,args});return request(op,args);}},writer:{send:async()=>{}}});
 return {callback:exposed.get('__opencutVisualCallback'),calls};
}
const observations={graphics:[{elementId:'shape',width:22,height:22,originX:10,originY:10}]};
test('graphic begin has independent identity, complete order and expanded allocation admission',()=>{
 assert.equal(validateGraphicBeginV1(begin()).plan.graphics[0].quad,null);
 assert.throws(()=>validateCaptionBeginV3(begin()));
 for(const change of [p=>p.plan.order=[],p=>p.plan.execution.identity.revision='other',p=>p.fonts=[{fontId:'invented',handle:'private'}],p=>p.admission.maxDecodedPixels='483']){const p=begin();change(p);assert.throws(()=>validateGraphicBeginV1(p));}
});
test('graphic callback waits for completed child and permits only one fixed observation set, including concurrent requests',async()=>{
 let complete;const gate=await configured(validateGraphicBeginV1(begin()),op=>op==='bindCaptionFrameV3'?new Promise(resolve=>complete=resolve):{});
 assert.throws(()=>gate.callback('bindGraphicFrameV1',observations));
 const pending=gate.callback('bindCaptionFrameV3',{rasters:[],captions:[]});
 assert.throws(()=>gate.callback('bindCaptionFrameV3',{rasters:[],captions:[]}));
 assert.throws(()=>gate.callback('bindGraphicFrameV1',observations));
 complete({});await pending;
 assert.throws(()=>gate.callback('bindGraphicFrameV1',{graphics:[{...observations.graphics[0],originX:0}]}));
 await gate.callback('bindGraphicFrameV1',observations);
 assert.throws(()=>gate.callback('bindGraphicFrameV1',observations));
 assert.equal(gate.calls.length,2);
 assert.throws(()=>gate.callback('resolveVerifiedBytes',{resource:{assetId:'invented'}}),e=>e.code==='NOT_AUTHORIZED');
});
test('old caption runner cannot bind graphics; graphic runner selects distinct serialized entry',async()=>{
 const p=begin(),old={...p,type:'beginCaptionFrameV3',plan:p.plan.execution};
 const gate=await configured(validateCaptionBeginV3(old));assert.throws(()=>gate.callback('bindGraphicFrameV1',observations),e=>e.code==='INVALID_HOST_CALLBACK');
 let selected;await browserHooks.render({begin:p,page:{evaluate:async fn=>{selected=fn;return {ok:true,receipt:{}};}}});assert.equal(selected,executeGraphicVisualPageV1);
});
