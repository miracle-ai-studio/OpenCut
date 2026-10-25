import {test,mock} from "node:test";
import assert from "node:assert/strict";
const opened=[],iterators=[];
class Input {
  constructor(){opened.push(this);this.disposed=false;}
  async getPrimaryVideoTrack(){return {displayWidth:960,displayHeight:540,canDecode:async()=>true};}
  dispose(){this.disposed=true;}
}
class CanvasSink {
  canvases(start){let i=Math.floor((start+1e-6)/.04);const iterator={returned:false,async next(){const timestamp=i++*.04;return timestamp<.12?{done:false,value:{timestamp,duration:.04,canvas:{width:960,height:540}}}:{done:true};},async return(){iterator.returned=true;return {done:true};}};iterators.push(iterator);return iterator;}
}
mock.module("mediabunny",{namedExports:{Input,CanvasSink,BlobSource:class{},ALL_FORMATS:[]}});
globalThis.createImageBitmap=async canvas=>({...canvas,close:mock.fn()});
const {PreviewSegmentDecoder}=await import("../src/preview-segment.mjs");
test("sequential cached pictures retain bounded decoders, exact intervals and restart only for backward seeks",async()=>{
 const decoder=new PreviewSegmentDecoder(),signal=new AbortController().signal;
 const first=await decoder.frame("0",new Blob(),960,540,0,signal);first.close();
 const next=await decoder.frame("0",new Blob(),960,540,.04,signal);next.close();assert.equal(iterators.length,1);
 await decoder.frame("0",new Blob(),960,540,0,signal);assert.equal(iterators[0].returned,true);assert.equal(iterators.length,2);
 await decoder.open("1",new Blob(),960,540);
 await assert.rejects(decoder.open("2",new Blob(),960,540),/budget/);
 await decoder.retain(new Set(["1"]));assert.equal(opened[0].disposed,true);
 await decoder.close();assert.equal(opened[1].disposed,true);
});
test("wrong raster sizes and missing CFR frames reject instead of presenting a different frame",async()=>{
 const decoder=new PreviewSegmentDecoder(),signal=new AbortController().signal;
 await assert.rejects(decoder.frame("bad",new Blob(),1920,1080,0,signal),/Invalid/);assert.equal(opened.at(-1).disposed,true);
 await assert.rejects(decoder.frame("0",new Blob(),960,540,.12,signal),/Missing frame/);
 await assert.rejects(decoder.frame("0",new Blob(),960,540,.02,signal),/CFR timestamp/);await decoder.close();
});
test("a cancelled bitmap transfer releases its new pixels and never publishes",async()=>{
 const decoder=new PreviewSegmentDecoder(),abort=new AbortController();let bitmap;
 const original=globalThis.createImageBitmap;
 globalThis.createImageBitmap=async()=>{bitmap={close:mock.fn()};abort.abort();return bitmap;};
 try{await assert.rejects(decoder.frame("0",new Blob(),960,540,0,abort.signal),{name:"AbortError"});assert.equal(bitmap.close.mock.callCount(),1);}
 finally{globalThis.createImageBitmap=original;await decoder.close();}
});
