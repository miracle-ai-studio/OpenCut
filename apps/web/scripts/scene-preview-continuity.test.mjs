import test from "node:test";
import assert from "node:assert/strict";
import { PreviewContinuity, PreviewCreditObservations } from "./scene-preview-continuity.mjs";
const point=(n,frames,copied=128)=>({renderFrame:n*128,quantumFrames:128,frames:String(frames),copied,completedBlocks:0,
  contextFrame:(n+1)*128,at:n*128/48,stalled:copied===0});
test("drained windows cover every quantum once, including joins, initial copy and the final cursor",()=>{
  const a=new PreviewContinuity(48000,"25600",10);
  for(let i=0;i<200;i++)a.add([point(i,(i+1)*128)]);
  const r=a.result();assert.equal(r.copied_frames,25600);assert.equal(r.rendered_frames,25600);
  assert.equal(r.missing_frames,0);assert.equal(r.final_cursor,"25600");assert.ok(r.windows.length>1);assert.ok(r.max_raw_points<=5);
});
test("a silent quantum on a drain boundary cannot disappear in carry accounting",()=>{
  const a=new PreviewContinuity(48000,"1024",10);
  let cursor=0;
  for(let i=0;i<9;i++){const copied=i===4?0:128;cursor+=copied;a.add([point(i,cursor,copied)]);}
  const r=a.result();assert.equal(r.missing_frames,128);assert.equal(r.copied_frames,1024);assert.equal(r.rendered_frames,1152);
  assert.ok(r.windows.some(w=>w.render_quantum_deficit_ratio>0));
});
test("missing quantum or dishonest cursor at a window join fails instead of beginning another healthy window",()=>{
  for(const bad of [{renderFrame:7*128},{frames:"999"}]){
    const a=new PreviewContinuity(48000,"4096",10);for(let i=0;i<5;i++)a.add([point(i,(i+1)*128)]);
    a.add([{...point(5,768),...bad}]);
    assert.throws(()=>{for(let i=6;i<12;i++)a.add([point(i,(i+1)*128)]);},/Missing|inconsistent/);
  }
  assert.throws(()=>new PreviewContinuity(48000,"4096").add([point(0,256)]),/initial/);
  const unfinished=new PreviewContinuity(48000,"4096");unfinished.add([point(0,128)]);assert.throws(()=>unfinished.result(),/final/);
});
test("credit uses Worker entry, not later page arrival, and joins adjacent observation drains",()=>{
  const a=new PreviewCreditObservations();
  a.add([{at:10,frames:"4096",completedBlocks:1}],[],[]);
  a.add([],[{kind:"heartbeat",began:8,ended:11},{kind:"report_consumed",began:12,ended:20,frames:"4224"}],[{ordinal:2,frames:4096,start_sample:"4096",at:11},{ordinal:3,frames:4096,start_sample:"8192",at:45}]);
  const r=a.result();assert.equal(r.completion_to_report.maximum_ms,2);assert.equal(r.completion_to_next_supply.maximum_ms,35);
  assert.equal(r.report_to_next_supply.maximum_ms,33);assert.equal(r.unmatched_terminal_completions,0);
  assert.equal(r.slowest_refills[0].supplied_start_sample,"8192");assert.deepEqual(r.slowest_refills[0].preceding_controls,[{kind:"heartbeat",began_ms:8,headers_ended_ms:11,ended_ms:11}]);
});
test("Worker control observation waits for the original response body, preserving its text",async()=>{
  const {installScenePreviewPumpWitness}=await import("./verify-v4-scene-preview.mjs");
  const original={fetch:globalThis.fetch,MessagePort:globalThis.MessagePort,addEventListener:globalThis.addEventListener};
  let controller;
  const response=new Response(new ReadableStream({start(c){controller=c;}}));
  globalThis.fetch=async()=>response;globalThis.MessagePort=class{postMessage(){}};globalThis.addEventListener=()=>{};
  try {
    installScenePreviewPumpWitness();
    const value=await fetch("/v1/tools/opencut_audio_preview_v4_control",{body:JSON.stringify({command:{kind:"heartbeat"}})});
    assert.equal(value,response);assert.equal(globalThis.__scenePumpWitness.controls.length,0);
    const reading=value.text();assert.equal(globalThis.__scenePumpWitness.controls.length,0);
    controller.enqueue(new TextEncoder().encode('{"fixed":true}'));controller.close();
    assert.equal(await reading,'{"fixed":true}');
    const row=globalThis.__scenePumpWitness.controls[0];assert.equal(row.kind,"heartbeat");assert(row.ended>=row.headers_ended&&row.headers_ended>=row.began);
  } finally {Object.assign(globalThis,original);delete globalThis.__scenePumpWitness;}
});
test("Worker-to-page conversion includes response headers and body when time origins differ",async()=>{
  const {trackScenePreviewPumps}=await import("./verify-v4-scene-preview.mjs");
  let born,calls=0;
  const worker={url:()=>"http://localhost/pump-worker-synthetic.js",on(){},async evaluate(){if(++calls===1)return;return {token:"synthetic",startObserved:true,timeOrigin:6000,blocks:3,pcm:[],overflows:0,
    selectedPcm:[{id:"later",start_sample:42,frames:1,values:[.1,.1]}],pcmCapture:{completed:1},
    supplies:[{ordinal:3,frames:4096,start_sample:"8192",at:45}],controls:[{kind:"report_consumed",frames:"4224",began:12,headers_ended:13,ended:20}],progresses:[{at:10,frames:"4096",completedBlocks:1}]};}};
  const page={on(_name,fn){born=fn;},async evaluate(){return {progress:{token:"synthetic"},timeOrigin:1000,controls:[]};}};
  const collect=trackScenePreviewPumps(page,{rolling:true});born(worker);
  const w=await collect();assert.equal(w.controls[0].headers_ended,5013);assert.equal(w.controls[0].ended,5020);
  assert.deepEqual(w.selectedPcm,[{id:"later",start_sample:42,frames:1,values:[.1,.1]}]);
  assert.equal(w.pcmCapture.completed,1);
  const account=new PreviewCreditObservations();account.add(w.workerProgresses,w.controls,w.supplies);
  assert.equal(account.result().completion_to_report.maximum_ms,2);
});
test("refill detail keeps only greatest32 numeric samples and distinguishes prior report from heartbeat",()=>{
  const a=new PreviewCreditObservations();
  for(let k=1;k<=40;k++) {
    const at=k*1000;
    a.add([{at,frames:String(k*4096),completedBlocks:k}],
      [{kind:"report_consumed",frames:String(k*4096-128),began:at-3,ended:at+1},{kind:"report_consumed",frames:String(k*4096),began:at+2,ended:at+3}],
      [{ordinal:k+2,frames:4096,start_sample:String((k+1)*4096),at:at+k+3}]);
  }
  const r=a.result();assert.equal(r.completion_to_report.count,40);assert.equal(r.slowest_refills.length,32);
  assert.equal(r.slowest_refills[0].completed_block,40);assert.equal(r.slowest_refills.at(-1).completed_block,9);
  assert.equal(r.slowest_refills[0].preceding_controls[0].kind,"report_consumed");
  for(const bad of [{ordinal:1,frames:4097,start_sample:"0",at:1},{ordinal:1,frames:4096,start_sample:"01",at:1}])assert.throws(()=>new PreviewCreditObservations().add([],[],[bad]),/Invalid supply/);
  assert.throws(()=>new PreviewCreditObservations().add([],[{kind:"heartbeat",began:2,ended:1}],[]),/Invalid control/);
});
