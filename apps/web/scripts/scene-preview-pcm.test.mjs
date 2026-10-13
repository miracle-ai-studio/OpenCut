import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { densePcmWindows, measurePcmWindow, observePcmOrigin } from "./scene-preview-pcm.mjs";
import { installScenePreviewPumpWitness } from "./verify-v4-scene-preview.mjs";
function observer(windows) {
  const context = vm.createContext({performance: {timeOrigin: 0, now: () => 0}, Float32Array,
    addEventListener: (_, listener) => context.start = listener, fetch: async () => ({}),
    MessagePort: class { postMessage(data) { this.forwarded = data; } }});
  vm.runInContext(`(${installScenePreviewPumpWitness.toString()})(${JSON.stringify({pcmWindows: windows})})`, context);
  context.start({data: {channel: "opencut-audio-pump", version: 1, kind: "start", token: "owned"}});
  const port = new context.MessagePort();
  return {context, port, w: context.__scenePumpWitness,
    block(values, token = "owned") { const data = {kind: "block", token, pcm: Float32Array.from(values).buffer}; port.postMessage(data); assert.equal(port.forwarded, data); }};
}
const declaration = (id, start_sample, frames) => ({id, start_sample, frames});
test("terminal control cursor cannot replace the original stream origin or fabricate missing initial evidence", () => {
  const session = {state:"playing",actual_start_sample:"0",identity:{sample_rate:48000}};
  const observed = observePcmOrigin(false,session); assert.equal(observed,true);
  assert.equal(observePcmOrigin(observed,{...session,state:"ended",actual_start_sample:"14400000"}),true);
  assert.throws(()=>observePcmOrigin(false,{...session,state:"ended",actual_start_sample:"14400000"}),/original/);
  assert.equal(observePcmOrigin(observed,{...session,state:"stopped",actual_start_sample:"14400000"}),true);
  assert.throws(()=>observePcmOrigin(false,{...session,actual_start_sample:"14400000"}),/origin/);
  assert.throws(()=>observePcmOrigin(true,{...session,identity:{sample_rate:44100}}),/rate/);
});
test("authored windows cover20later source interiors and19gaps away from clip edges", () => {
  const windows = densePcmWindows(); assert.equal(windows.length, 39);
  for (const w of windows) {
    const start = 292 * w.placement / 19, time = w.start_sample / 48000;
    assert.equal(w.frames, 4096);
    assert.ok(w.kind === "source" ? time > start + 1 && time + w.frames / 48000 < start + 8 : time > start + 8 && time + w.frames / 48000 < 292 * (w.placement + 1) / 19);
  }
});
test("actual stereo bytes span blocks and two windows with exact absolute sample identity", () => {
  const o = observer([declaration("first", 2, 3), declaration("second", 6, 2)]);
  o.block([999,999], "other"); assert.equal(o.w.pcmCapture.transferred_frames, "0");
  o.block([0,10,1,11,2,12,3,13]); assert.equal(o.w.pcmCapture.partial_frames, 2);
  o.block([4,14,5,15,6,16,7,17]);
  assert.deepEqual(JSON.parse(JSON.stringify(o.w.selectedPcm)), [
    {...declaration("first",2,3), values:[2,12,3,13,4,14]}, {...declaration("second",6,2),values:[6,16,7,17]}]);
  assert.equal(o.w.pcmCapture.completed, 2); assert.equal(o.w.pcmCapture.transferred_frames, "8");
});
test("draining completed windows preserves an in-progress sample and future window coverage", () => {
  const o = observer([declaration("first",0,2),declaration("second",3,3)]);
  o.block([0,10,1,11,2,12,3,13]); o.w.selectedPcm.length = 0;
  o.block([4,14,5,15]); assert.deepEqual(Array.from(o.w.selectedPcm[0].values),[3,13,4,14,5,15]);
});
test("observation queues and declarations remain bounded; excess cannot establish passing coverage", () => {
  const o = observer(Array.from({length:9},(_,k)=>declaration(String(k),k,1)));
  o.block(new Array(18).fill(0)); assert.equal(o.w.selectedPcm.length,8); assert.equal(o.w.overflows,1);
  for (const windows of [new Array(40).fill(declaration("x",0,1)),[declaration("x",-1,1)],
    [declaration("x",0,4097)],[declaration("x",0,2),declaration("y",1,1)]]) assert.throws(()=>observer(windows));
  assert.throws(()=>observer([]).block([1]), /stereoFloat32/);
});
const tone = (scale = 1) => Array.from({length:8192},(_,i)=>Math.sin(Math.floor(i/2)*2*Math.PI*440/48000)*.125*3*10**(-18/20)*scale);
test("both independently expected440Hz channels and three-voice gain pass, missing/malformed/wrong-channel/gain fail", () => {
  const w = {...densePcmWindows()[0], values:tone()}; const result=measurePcmWindow(w);
  assert.equal(result.channels.length,2); assert.ok(Math.abs(result.channels[1].frequency_hz-440)<1);
  assert.ok(!("values" in result));
  for (const values of [tone(1/3),tone(2),tone().slice(2),tone().map((v,i)=>i%2?0:v),tone().map((v,i)=>i===0?NaN:v)])
    assert.throws(()=>measurePcmWindow({...w,values}));
});
test("gap must be exact zero in both channels; a tiny leaked tone or finite-length missing data fails", () => {
  const w = {...densePcmWindows()[1], values:new Array(8192).fill(0)};
  assert.equal(measurePcmWindow(w).channels[1].peak,0);
  assert.throws(()=>measurePcmWindow({...w,values:tone(.000001)}), /silence/);
  assert.throws(()=>measurePcmWindow({...w,values:[]}));
});
