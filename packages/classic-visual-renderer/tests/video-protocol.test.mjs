import {test,mock} from "node:test";
import assert from "node:assert/strict";
import {PassThrough} from "node:stream";
let starts=0,renders=0,stops=0;
const envelope=(p,id)=>{if(p.protocolVersion!==1||typeof p.executionId!=="string"||(id&&p.executionId!==id))throw Object.assign(new Error(),{code:"INVALID_HOST_PROTOCOL"});};
// Exercise actual reader/writer/broker framing, replacing only browser/quota.
const actual=await import("../../classic-audio-renderer/host/protocol.mjs");
mock.module("@opencut/classic-audio-renderer/internal-host-runtime",{namedExports:{
  ...actual,
  BrowserRealm:class{constructor(begin,broker,writer,monitor,signal){Object.assign(this,{begin,broker,writer,monitor,signal});}
    async start(){starts++;await this.writer.send({type:"started",browserPid:10});await this.writer.send({type:"ready",browserPid:10,identity:{}});}
    async render(){renders++;return {identity:this.currentFrame.plan.identity};}
    async stop(){stops++;}},
  QuotaMonitor:class{async start(){}stop(){}receipt(){return {elapsedMs:1};}},
  moduleRegistry:()=>{},playwrightRuntime:()=>{},PACKAGE_ROOT:"/tmp",runHost:()=>{},
}});
const {runVideoHost,validateVideoBegin}=await import("../host/video-runner.mjs");
const admission={deadlineMs:10000,maxEncodedBytes:1024,maxFramePixels:"64",maxDecodedPixels:"64",maxGpuBytes:"1024",maxRgbaBytes:"1024",maxOutputBytes:"1024",maxRssBytes:"1048576",quotaProfile:"monitored-terminate"};
const packet=value=>({...value,executionId:"video",protocolVersion:1});
const frame=index=>{const identity={projectId:"p",revision:"r",sequenceId:"s",time:{ticks:String(index),timebase:24},width:2,height:2,profile:"classic-v31-gpu-normal-text-frame-v2"};return packet({type:"frame",frameIndex:index,plan:{identity,media:{identity:{...identity,profile:"classic-v31-gpu-normal-frame"},resources:[],layers:[]},texts:[],order:[]},resources:[],fonts:[]});};
function harness(){const input=new PassThrough(),output=new PassThrough(),queue=[],waiters=[];output.on("data",chunk=>{for(const line of chunk.toString().trim().split("\n")){const value=JSON.parse(line);const next=waiters.shift();if(next)next(value);else queue.push(value);}});const work=runVideoHost(input,output);return {work,send:p=>input.write(JSON.stringify(p)+"\n"),next:()=>queue.length?Promise.resolve(queue.shift()):new Promise(resolve=>waiters.push(resolve))};}
test("video begin has a distinct bounded version and refuses frame-count/admission bypass",()=>{assert.equal(validateVideoBegin(packet({type:"beginTextVideoV1",frameCount:2,admission})).frameCount,2);for(const frameCount of [0,1801,1.5])assert.throws(()=>validateVideoBegin(packet({type:"beginTextVideoV1",frameCount,admission})));assert.throws(()=>validateVideoBegin(packet({type:"begin",frameCount:2,admission})));});
test("two acknowledged frames reuse one browser and terminate only after complete count",async()=>{starts=renders=stops=0;const h=harness();h.send(packet({type:"beginTextVideoV1",frameCount:2,admission}));assert.equal((await h.next()).type,"started");assert.equal((await h.next()).type,"ready");for(let i=0;i<2;i++){h.send(frame(i));const receipt=await h.next();assert.equal(receipt.type,"frameComplete");assert.equal(receipt.frameIndex,i);}h.send(packet({type:"endVideo"}));const complete=await h.next();assert.equal(complete.type,"complete");assert.equal(complete.frameCount,2);await h.work;assert.equal(starts,1);assert.equal(renders,2);assert.equal(stops,1);});
test("out-of-order frame and premature terminal cannot publish a completed video",async()=>{for(const invalid of [frame(1),packet({type:"endVideo"})]){const h=harness();h.send(packet({type:"beginTextVideoV1",frameCount:2,admission}));await h.next();await h.next();h.send(invalid);assert.equal((await h.next()).type,"failed");await h.work;}});
