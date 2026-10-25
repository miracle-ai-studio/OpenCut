import {test,mock} from "node:test";
import assert from "node:assert/strict";
const native=[];
let finishRead;
mock.module("../host/browser.mjs",{namedExports:{browserHooks:{async configurePage(realm){
 await realm.page.exposeFunction("__opencutVisualCallback",async(op,args)=>{
  native.push({frame:realm.begin.frameIndex,op,args});
  if(op==="resolveVerifiedBytes")return {transferId:"transfer",contentHash:args.resource.contentHash};
  if(op==="readVerifiedBytes"&&native.filter(x=>x.op===op).length===1)return new Promise(resolve=>{finishRead=()=>resolve({bytesHex:"00"});});
  return {ok:true};
 });
 await realm.page.exposeFunction("__opencutVisualRgba",async()=>({}));
}}}});
const {videoBrowserHooks,configureVideoPageCallbacks}=await import("../host/video-browser.mjs");
test("in-flight source reads finish before frameComplete; later reads wait for and reauthorize under the next fixed frame",async()=>{
 const handlers=new Map(),resource={assetId:"video",kind:"video",contentHash:"a".repeat(64),size:"100"};
 const frame=index=>({frameIndex:index,plan:{media:{resources:[resource]}},type:"beginTextFrame"});
 const realm={currentFrame:frame(0),begin:{admission:{}},signal:new AbortController().signal,
  page:{exposeFunction:async(name,handler)=>handlers.set(name,handler)}};
 const invokeRgba=await configureVideoPageCallbacks(realm);
 const invoke=(op,args)=>handlers.get("__opencutVisualCallback")(op,args);
 await invoke("resolveVerifiedBytes",{resource});
 const first=invoke("readVerifiedBytes",{transferId:"transfer",offset:"0",count:1});
 while(!finishRead)await Promise.resolve();
 let settled=false;const closing=realm.parkVideoReads().then(()=>{settled=true;});
 const next=invoke("readVerifiedBytes",{transferId:"transfer",offset:"1",count:1});
 await Promise.resolve();assert.equal(settled,false);assert.equal(native.filter(x=>x.op==="readVerifiedBytes").length,1);
 finishRead();await first;await closing;realm.currentFrame=undefined;
 realm.currentFrame=frame(1);realm.resumeVideoReads();await next;
 assert.equal(native.at(-1).frame,1);
 await realm.parkVideoReads();realm.currentFrame=undefined;
 const retired=invoke("readVerifiedBytes",{transferId:"transfer",offset:"2",count:1});
 realm.currentFrame={...frame(2),plan:{media:{resources:[]}}};realm.resumeVideoReads();
 await assert.rejects(retired,{code:"RESOURCE_SOURCE_CLOSED"});assert.equal(native.filter(x=>x.op==="readVerifiedBytes").length,2);
 await invoke("releaseVerifiedBytes",{transferId:"transfer"});assert.equal(native.at(-1).frame,2);
});
