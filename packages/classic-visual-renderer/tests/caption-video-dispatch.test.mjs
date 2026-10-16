import {test} from "node:test";
import assert from "node:assert/strict";
import {executeVideoFrame} from "../host/video-browser.mjs";
test("persistent caption video chooses shared V3 renderer and retains caches",async()=>{
  const calls=[];
  globalThis.__opencutVisualModule={VisualDecoderCache:class{},renderClassicTextFrameV2:async(...args)=>{calls.push(["v2",args]);return {old:true};},renderClassicCaptionFrameV3:async(...args)=>{calls.push(["v3",args]);return {captions:[]};}};
  try{
    assert.deepEqual(await executeVideoFrame({plan:{},admission:{},profile:"beginCaptionFrameV3"}),{ok:true,receipt:{captions:[]}});
    assert.deepEqual(await executeVideoFrame({plan:{},admission:{}}),{ok:true,receipt:{old:true}});
    assert.equal(calls[0][0],"v3");assert.equal(calls[1][0],"v2");
    assert.equal(calls[0][1][4],calls[1][1][4]);assert.equal(calls[0][1][5],calls[1][1][5]);assert.equal(calls[0][1][6],calls[1][1][6]);
  }finally{for(const key of ["__opencutVisualModule","__opencutVideoBytes","__opencutVideoFonts","__opencutVideoDecoders"])delete globalThis[key];}
});
