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
  a.add([],[{kind:"heartbeat",began:11,frames:"4096"},{kind:"report_consumed",began:12,frames:"4224"}],[{ordinal:2,at:11},{ordinal:3,at:45}]);
  const r=a.result();assert.equal(r.completion_to_report.maximum_ms,2);assert.equal(r.completion_to_next_supply.maximum_ms,35);
  assert.equal(r.report_to_next_supply.maximum_ms,33);assert.equal(r.unmatched_terminal_completions,0);
});
