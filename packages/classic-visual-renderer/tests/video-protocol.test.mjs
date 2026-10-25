import {test,mock} from "node:test";
import assert from "node:assert/strict";
import {PassThrough} from "node:stream";
let starts=0,renders=0,stops=0,monitoredAdmission,ownerControlsLifetime;
const envelope=(p,id)=>{if(typeof p.executionId!=="string"||(id&&p.executionId!==id))throw Object.assign(new Error(),{code:"INVALID_HOST_PROTOCOL"});};
// Exercise actual reader/writer/broker framing, replacing only browser/quota.
const actual=await import("../../classic-audio-renderer/host/protocol.mjs");
mock.module("@opencut/classic-audio-renderer/internal-host-runtime",{namedExports:{
  ...actual,
  BrowserRealm:class{constructor(begin,broker,writer,monitor,signal){Object.assign(this,{begin,broker,writer,monitor,signal});}
    async start(){starts++;await this.writer.send({type:"started",browserPid:10});await this.writer.send({type:"ready",browserPid:10,identity:{}});}
    async render(){renders++;assert.equal(this.currentFrame.admission.deadlineMs,this.begin.admission.deadlineMs);return {identity:this.currentFrame.plan.identity};}
    async stop(){stops++;}},
  QuotaMonitor:class{constructor(admission,_abort,_now,validate,owned){monitoredAdmission=validate(admission);ownerControlsLifetime=owned;}async start(){}stop(){}receipt(){return {elapsedMs:1};}},
  ORIGIN:"http://127.0.0.1:49199",moduleResponse:()=>{},moduleRegistry:()=>{},playwrightRuntime:()=>{},PACKAGE_ROOT:"/tmp",runHost:()=>{},
}});
const {runVideoHost,validateVideoBegin}=await import("../host/video-runner.mjs");
const admission={deadlineMs:10000,maxEncodedBytes:1024,maxFramePixels:"64",maxDecodedPixels:"64",maxGpuBytes:"1024",maxRgbaBytes:"1024",maxOutputBytes:"1024",maxRssBytes:"1048576",quotaProfile:"monitored-terminate"};
const packet=value=>({...(/^begin.*Video$/.test(value.type)?{executionLimit:{kind:"progress",milliseconds:admission.deadlineMs}}:{}),...value,executionId:"video"});
const frame=index=>{const identity={projectId:"p",revision:"r",sequenceId:"s",time:{ticks:String(index),timebase:24},width:2,height:2,profile:"classic-v31-gpu-normal-text-frame-v2"};return packet({type:"frame",frameIndex:index,plan:{identity,media:{identity:{...identity,profile:"classic-v31-gpu-normal-frame"},resources:[],layers:[]},texts:[],order:[]},resources:[],fonts:[]});};
function harness(){const input=new PassThrough(),output=new PassThrough(),queue=[],waiters=[];output.on("data",chunk=>{for(const line of chunk.toString().trim().split("\n")){const value=JSON.parse(line);const next=waiters.shift();if(next)next(value);else queue.push(value);}});const work=runVideoHost(input,output);return {work,send:p=>input.write(JSON.stringify(p)+"\n"),next:()=>queue.length?Promise.resolve(queue.shift()):new Promise(resolve=>waiters.push(resolve))};}
test("persistent video separates exact frame counters,per-frame admission and captured task time",()=>{
 const begin=validateVideoBegin(packet({type:"beginTextVideo",frameCount:1801,executionLimit:{kind:"progress",milliseconds:600001},admission}));
 assert.equal(begin.frameCount,1801);assert.equal(begin.executionLimit.milliseconds,600001);assert.equal(begin.admission.deadlineMs,10000);
 for(const frameCount of [0,Number.MAX_SAFE_INTEGER+1,1.5])assert.throws(()=>validateVideoBegin(packet({type:"beginTextVideo",frameCount,admission})));
 for(const milliseconds of [0,2147483648,NaN])assert.throws(()=>validateVideoBegin(packet({type:"beginTextVideo",frameCount:2,executionLimit:{kind:"progress",milliseconds},admission})));
 for(const executionLimit of [{kind:"unknown",milliseconds:1},{kind:"progress",milliseconds:1,extra:true}])assert.throws(()=>validateVideoBegin(packet({type:"beginTextVideo",frameCount:2,executionLimit,admission})));
 assert.throws(()=>validateVideoBegin(packet({type:"beginTextVideo",frameCount:2,executionDeadlineMs:1,admission})));
 assert.throws(()=>validateVideoBegin(packet({type:"begin",frameCount:2,admission})));
});
test("two acknowledged frames reuse one browser and delegate only progress lifetime to the native owner",async()=>{
 for(const kind of ["total","progress"]){
  starts=renders=stops=0;const h=harness();
  h.send(packet({type:"beginTextVideo",frameCount:2,executionLimit:{kind,milliseconds:600001},admission}));
  assert.equal((await h.next()).type,"started");assert.equal((await h.next()).type,"ready");
  assert.equal(monitoredAdmission.deadlineMs,600001);assert.equal(monitoredAdmission.maxRssBytes,admission.maxRssBytes);
  for(let i=0;i<2;i++){h.send(frame(i));const receipt=await h.next();assert.equal(receipt.type,"frameComplete");assert.equal(receipt.frameIndex,i);}
  h.send(packet({type:"endVideo"}));const complete=await h.next();assert.equal(complete.type,"complete");assert.equal(complete.frameCount,2);
  await h.work;assert.equal(starts,1);assert.equal(renders,2);assert.equal(stops,1);assert.equal(ownerControlsLifetime,kind==="progress");
 }
});
test("out-of-order frame and premature terminal cannot publish a completed video",async()=>{for(const invalid of [frame(1),packet({type:"endVideo"})]){const h=harness();h.send(packet({type:"beginTextVideo",frameCount:2,admission}));await h.next();await h.next();h.send(invalid);assert.equal((await h.next()).type,"failed");await h.work;}});
const captionFrame=index=>{const inner=frame(index);return {...inner,plan:{identity:{...inner.plan.identity,profile:"classic-v31-gpu-normal-caption-frame-v3"},execution:inner.plan,captions:[]}};};
const graphicFrame=index=>{const child=captionFrame(index);return {...child,plan:{identity:{...child.plan.identity,profile:"classic-v31-gpu-normal-graphic-frame-v1"},execution:child.plan,graphics:[{elementId:"shape",graphic:{geometry:{kind:"ellipse"},sourceWidth:2,sourceHeight:2,fill:[1,0,0,1],stroke:{widthPx:0,color:[0,0,0,1],align:"inside"}},quad:null}],order:["shape"]}};};
const blendFrame=index=>{const child=graphicFrame(index);return {...child,plan:{identity:{...child.plan.identity,profile:"classic-v31-gpu-blend-frame-v1"},execution:child.plan,blends:[{elementId:"shape",blendMode:"color-burn"}]}};};
test("caption video admits only V3 plans and retains ordered fixed identity",async()=>{
  const h=harness();h.send(packet({type:"beginCaptionVideo",frameCount:2,admission}));await h.next();await h.next();
  h.send(captionFrame(0));assert.equal((await h.next()).type,"frameComplete");
  const changed=captionFrame(1);changed.plan.identity.revision="other";h.send(changed);
  assert.equal((await h.next()).type,"failed");await h.work;
  for(const [type,plan] of [["beginTextVideo",captionFrame(0)],["beginCaptionVideo",frame(0)]]){
    const host=harness();host.send(packet({type,frameCount:1,admission}));await host.next();await host.next();host.send(plan);
    assert.equal((await host.next()).type,"failed");await host.work;
  }
});
test("graphic video reuses one browser for ordered complete shared graphic frames",async()=>{
  starts=renders=stops=0;const h=harness();h.send(packet({type:"beginGraphicVideo",frameCount:2,admission}));
  assert.equal((await h.next()).type,"started");assert.equal((await h.next()).type,"ready");
  for(let i=0;i<2;i++){h.send(graphicFrame(i));const receipt=await h.next();assert.equal(receipt.type,"frameComplete");assert.equal(receipt.frameIndex,i);assert.equal(receipt.receipt.identity.profile,"classic-v31-gpu-normal-graphic-frame-v1");}
  h.send(packet({type:"endVideo"}));assert.equal((await h.next()).type,"complete");await h.work;
  assert.equal(starts,1);assert.equal(renders,2);assert.equal(stops,1);
});
test("all video profiles reject another profile's frame plan",async()=>{
  const profiles=[["beginTextVideo",frame],["beginCaptionVideo",captionFrame],["beginGraphicVideo",graphicFrame],["beginBlendVideo",blendFrame]];
  for(const [type,accepted] of profiles)for(const [,other] of profiles){
    if(other===accepted)continue;
    const h=harness();h.send(packet({type,frameCount:1,admission}));await h.next();await h.next();h.send(other(0));
    assert.equal((await h.next()).type,"failed");await h.work;
  }
});
test("graphic video retains fixed revision, complete order and graphic raster admission",async()=>{
  for(const change of [p=>p.plan.identity.revision="other",p=>p.plan.order=[],p=>p.plan.graphics[0].graphic.sourceWidth=100]){
    const h=harness();h.send(packet({type:"beginGraphicVideo",frameCount:2,admission}));await h.next();await h.next();
    h.send(graphicFrame(0));assert.equal((await h.next()).type,"frameComplete");
    const next=graphicFrame(1);change(next);h.send(next);assert.equal((await h.next()).type,"failed");await h.work;
  }
});
test("graphic video cancellation cannot become a complete video",async()=>{
  const h=harness();h.send(packet({type:"beginGraphicVideo",frameCount:2,admission}));await h.next();await h.next();
  h.send(graphicFrame(0));assert.equal((await h.next()).type,"frameComplete");
  h.send(packet({type:"cancel"}));assert.equal((await h.next()).type,"cancelled");await h.work;
});
test("blend video receives two ordered wrapper frames in one realm and rejects missing layer modes",async()=>{
  starts=renders=stops=0;const h=harness();h.send(packet({type:"beginBlendVideo",frameCount:2,admission}));await h.next();await h.next();
  for(let i=0;i<2;i++){h.send(blendFrame(i));const receipt=await h.next();assert.equal(receipt.type,"frameComplete");assert.equal(receipt.receipt.identity.profile,"classic-v31-gpu-blend-frame-v1");}
  h.send(packet({type:"endVideo"}));assert.equal((await h.next()).type,"complete");await h.work;
  assert.equal(starts,1);assert.equal(renders,2);assert.equal(stops,1);
  const invalid=harness();invalid.send(packet({type:"beginBlendVideo",frameCount:1,admission}));await invalid.next();await invalid.next();
  const missing=blendFrame(0);missing.plan.blends=[];invalid.send(missing);assert.equal((await invalid.next()).type,"failed");await invalid.work;
});

test("canvas encoding is selected only for a bounded, typed scene video",()=>{
 const encoding={fpsNumerator:25,fpsDenominator:1,bitrate:1608000,maxOutputBytes:8*1024*1024};
 const begin=packet({type:"beginSceneVideo",frameCount:50,admission,canvasEncoding:encoding});
 assert.deepEqual(validateVideoBegin(begin).canvasEncoding,encoding);
 for(const alter of [p=>p.type="beginTextVideo",p=>p.canvasEncoding.extra=true,p=>p.canvasEncoding.maxOutputBytes++,p=>p.canvasEncoding.fpsNumerator=0,p=>p.canvasEncoding.bitrate=NaN]){
  const bad=structuredClone(begin);alter(bad);assert.throws(()=>validateVideoBegin(bad));
 }
});

test("cancellation while the canvas encoder finalizes cannot publish completion",async()=>{
  const {videoBrowserHooks}=await import("../host/video-browser.mjs");
  const original=videoBrowserHooks.finishCanvas;let entered;
  const finalizing=new Promise(resolve=>entered=resolve);
  videoBrowserHooks.finishCanvas=async realm=>{entered();return new Promise((_resolve,reject)=>realm.signal.addEventListener("abort",()=>reject(realm.signal.reason),{once:true}));};
  try {
    const h=harness(),child=blendFrame(0),order=child.plan.execution.order;
    const scene={...child,fonts:[],plan:{identity:{...child.plan.identity,profile:"classic-v31-gpu-scene-frame-v1"},execution:child.plan,
      stacks:order.map(elementId=>({elementId,masks:[],effects:[]})),maskFonts:[],fontSynthesis:[],steps:order.map(element_id=>({kind:"layer",element_id})),
      background:{kind:"color",color:{encoding:"srgb",red:0,green:0,blue:0,alpha:1}}}};
    h.send(packet({type:"beginSceneVideo",frameCount:1,admission,canvasEncoding:{fpsNumerator:25,fpsDenominator:1,bitrate:1000000,maxOutputBytes:1024}}));
    await h.next();await h.next();h.send(scene);assert.equal((await h.next()).type,"frameComplete");
    h.send(packet({type:"endVideo"}));await finalizing;h.send(packet({type:"cancel"}));
    assert.equal((await h.next()).type,"cancelled");await h.work;
  } finally {videoBrowserHooks.finishCanvas=original;}
});
