import {test,mock} from "node:test";
import assert from "node:assert/strict";
const outputs=[];
class Target {constructor(writable){this.writer=writable.getWriter();}}
class Output {constructor(config){this.config=config;this.at=0;outputs.push(this);}addVideoTrack(source,metadata){this.source=source;source.output=this;this.metadata=metadata;}async start(){}async finalize(){await this.config.target.writer.write({type:"write",position:0,data:new Uint8Array([1,2])});}async cancel(){this.cancelled=true;}}
class Source {constructor(canvas,config){this.canvas=canvas;this.config=config;this.samples=[];}async add(timestamp,duration){this.samples.push({timestamp,duration});await this.output.config.target.writer.write({type:"write",position:this.output.at,data:new Uint8Array([10,20,30,40])});this.output.at+=4;}close(){this.closed=true;}}
mock.module("mediabunny",{namedExports:{Output,CanvasSource:Source,Mp4OutputFormat:class{},StreamTarget:Target}});
const {createCanvasVideo}=await import("../src/canvas-video.mjs");
const frame={projectId:"p",revision:"r",sequenceId:"s",width:2,height:2,profile:"scene"};
const encoding={fpsNumerator:30000,fpsDenominator:1001,bitrate:1608000,maxOutputBytes:64};
const canvas=t=>{const previous=globalThis.OffscreenCanvas;globalThis.OffscreenCanvas=class{constructor(width,height){this.width=width;this.height=height;}};t.after(()=>{if(previous)globalThis.OffscreenCanvas=previous;else delete globalThis.OffscreenCanvas;});};
test("canvas encoder preserves complete CFR samples, Classic bitrate, backpressure and bounded seekable MP4 writes",async t=>{
 canvas(t);const video=await createCanvasVideo(frame,encoding);const output=outputs.at(-1);
 assert.equal(output.source.config.latencyMode,"quality");assert.equal(output.source.config.hardwareAcceleration,"prefer-hardware");assert.equal(output.source.config.bitrate,1608000);
 await video.add(frame,0);await video.add(frame,1);assert.equal(video.retainedBytes,8);
 assert.deepEqual(output.source.samples,[{timestamp:0,duration:1001/30000},{timestamp:1001/30000,duration:1001/30000}]);
 const blob=await video.finish(2);assert.equal(blob.type,"video/mp4");assert.equal(blob.size,8);assert.deepEqual([...new Uint8Array(await blob.arrayBuffer())],[1,2,30,40,10,20,30,40]);assert.equal(output.source.closed,true);
 await assert.rejects(video.add(frame,2),{code:"INVALID_HOST_PROTOCOL"});
});
test("canvas encoder rejects a replaced revision, out-of-order frame and incomplete final count",async t=>{
 canvas(t);const video=await createCanvasVideo(frame,encoding);
 await assert.rejects(video.add(frame,1),{code:"INVALID_HOST_PROTOCOL"});await assert.rejects(video.add({...frame,revision:"other"},0),{code:"INVALID_HOST_PROTOCOL"});
 await video.add(frame,0);await assert.rejects(video.finish(2),{code:"INVALID_HOST_PROTOCOL"});await video.finish(1);
});
test("encoded output quota and cancellation fail without accepting another frame",async t=>{
 canvas(t);const tiny=await createCanvasVideo(frame,{...encoding,maxOutputBytes:2});await assert.rejects(tiny.add(frame,0),{code:"VIDEO_BUDGET_EXCEEDED"});await tiny.cancel();
 const video=await createCanvasVideo(frame,encoding);await video.cancel();assert.equal(outputs.at(-1).cancelled,true);assert.equal(video.canvas.width,0);await assert.rejects(video.add(frame,0),{code:"INVALID_HOST_PROTOCOL"});
});

test("encoded storage grows in charged chunks and rejects an allocation before consuming it",async t=>{
 canvas(t);const video=await createCanvasVideo(frame,{...encoding,maxOutputBytes:131072});
 let used=0,peak=0;
 const charge=size=>{used+=size;peak=Math.max(peak,used);return ()=>used-=size;};
 const retire=video.account(charge);assert.equal(used,65536+64);
 await video.add(frame,0);assert.equal(used,65536*2+64);assert.equal(video.retainedBytes,4);
 retire();assert.equal(used,0);assert.equal(peak,131136);
 const retireNext=video.account(charge);assert.equal(used,131136);retireNext();
 await video.cancel();
 const denied=await createCanvasVideo(frame,encoding);const close=denied.account(size=>{if(size===64)throw Object.assign(Error("budget"),{code:"VISUAL_BUDGET_EXCEEDED"});return ()=>{};});
 await assert.rejects(denied.add(frame,0),{code:"VISUAL_BUDGET_EXCEEDED"});assert.equal(denied.retainedBytes,0);close();await denied.cancel();
});
