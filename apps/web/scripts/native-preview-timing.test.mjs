import test from "node:test";
import assert from "node:assert/strict";
import { installNativePreviewTiming, NativePreviewTiming } from "./native-preview-timing.mjs";
import { nextPreviewPage } from "../../../packages/classic-audio-renderer/host/preview-page.mjs";
import { summarizeNativePreviewTiming, correlateNativePreviewCredit } from "./summarize-native-preview-timing.mjs";
import { NativePreviewDispatch, summarizeNativePreviewDispatch } from "./native-preview-dispatch.mjs";
test("isolated observer preserves independent Float32 hex/receipt bytes and restores original callback/sampler",async()=>{
  const callback=async()=>({ok:true,value:"fixed-facts"});
  globalThis.__opencutPreviewCallback=callback;
  const channels=[new Float32Array([.25,-0,1,-.5]),new Float32Array([.5,0,-1,.125])];
  const producer={async next(){await globalThis.__opencutPreviewCallback("compilePreviewBlock",{});return {channels,receipt:{fixed:true}};}};
  globalThis.__opencutPreviewProducer=producer;
  const command={id:"1",startSample:"9007199254740993",frameCount:4};
  const before=await nextPreviewPage(command),original=producer.next;
  installNativePreviewTiming();
  try{
    assert.deepEqual(await nextPreviewPage(command),before);
    assert.equal(before.value.channelsHex[0],"0000803e000000800000803f000000bf");
    const row=globalThis.__opencutPreviewTiming.last;assert.equal(row.hex_calls,2);assert.equal(row.start_sample,command.startSample);
    assert(row.producer_ms>=row.callback_ms);assert(row.hex_ms>=0);assert(!JSON.stringify(row).includes(before.value.channelsHex[0]));
  }finally{globalThis.__restoreNativePreviewTiming();assert.equal(producer.next,original);assert.equal(globalThis.__opencutPreviewCallback,callback);delete globalThis.__opencutPreviewProducer;delete globalThis.__opencutPreviewCallback;}
});
test("refined page observer preserves PCM and records bounded long-task numbers without task names",async()=>{
  let notify,closed=false;const previous=globalThis.PerformanceObserver;
  globalThis.PerformanceObserver=class{static supportedEntryTypes=["longtask"];constructor(fn){notify=fn;}observe(){}disconnect(){closed=true;}};
  globalThis.__opencutPreviewCallback=async()=>({ok:true,value:"fixed"});
  globalThis.__opencutPreviewProducer={async next(){await globalThis.__opencutPreviewCallback("compilePreviewBlock",{});return {channels:[new Float32Array([.25]),new Float32Array([-.5])],receipt:{fixed:true}};}};
  const command={id:"1",startSample:"0",frameCount:1},before=await nextPreviewPage(command);
  installNativePreviewTiming(true);
  try{
    assert.deepEqual(await nextPreviewPage(command),before);
    notify({getEntries:()=>Array.from({length:40},(_,k)=>({startTime:k,duration:50+k,name:"must-not-be-recorded"}))});
    const meta=globalThis.__opencutPreviewTiming,row=meta.last;
    assert.equal(meta.long_tasks.length,32);assert.equal(meta.long_task_count,40);assert(!JSON.stringify(meta).includes("must-not-be-recorded"));
    assert(row.callback_enter_ms>=row.producer_enter_ms);assert(row.callback_exit_ms>=row.callback_enter_ms);assert(row.hex_exit_ms>=row.callback_exit_ms);
  }finally{globalThis.__restoreNativePreviewTiming();assert(closed);globalThis.PerformanceObserver=previous;delete globalThis.__opencutPreviewCallback;delete globalThis.__opencutPreviewProducer;}
});
test("dispatch keeps original biggest command identities and uncertainty intervals, rejects absent calibration and callbacks",()=>{
  const d=new NativePreviewDispatch(),rows=[];
  d.enableGc(true);
  for(const stage of ["prepare","terminal"])for(let k=0;k<4;k++)d.clock(stage,100+k,0+k,102+k);
  for(let k=1;k<=40;k++){
    const row={command_id:String(k),start_sample:String((k-1)*4096),frame_count:4096,total_ms:k};rows.push(row);
    d.complete(row,{evaluate_enter_ms:100,evaluate_exit_ms:170,node_callback_enter_ms:122,node_callback_exit_ms:142,producer_enter_ms:10,hex_exit_ms:50,callback_enter_ms:20,callback_exit_ms:45,node_callback_calls:1});
    d.lag(k,k+25+k,k);
    d.gcEntry(k,k,1,0);
  }
  d.gcEntry(120,100,4,0);
  d.page_tasks={long_tasks_supported:true,long_task_count:2,long_tasks:[{start_ms:12,duration_ms:50},{start_ms:60,duration_ms:55}]};
  const raw=d.snapshot(),s=summarizeNativePreviewDispatch(raw,rows),t=s.tails[0];
  assert.equal(raw.tails.length,32);assert.equal(raw.timer_lags.length,32);assert.equal(t.command_id,"40");assert.equal(s.maximum_clock_uncertainty_ms,2);
  assert.deepEqual(t.evaluate_ingress,{lower_ms:10,upper_ms:12});assert.deepEqual(t.evaluate_return,{lower_ms:18,upper_ms:20});
  assert.deepEqual(t.callback_ingress,{lower_ms:0,upper_ms:2});assert.deepEqual(t.callback_return,{lower_ms:3,upper_ms:5});
  assert.equal(t.overlapping_selected_page_long_tasks.length,2);
  assert.equal(s.node_gc.count,41);assert.equal(s.node_gc.samples.length,32);assert.equal(s.node_gc.maximum_duration_ms,100);
  assert.equal(t.overlapping_selected_node_gc.length,1);assert.equal(t.overlapping_selected_node_gc[0].kind,4);
  for(const mutate of [x=>x.clock_samples.pop(),x=>x.tails[0].node_callback_calls=0,x=>x.tails[0].start_sample="1",x=>x.tails[0].evaluate_exit_ms=1,x=>x.timer_lags[0].late_ms=NaN,x=>x.page_tasks=null,x=>x.clock_samples.forEach(c=>c.after_ms+=20),x=>x.node_gc.samples[0].kind=0,x=>x.node_gc.samples[0].duration_ms=NaN,x=>x.node_gc.count=0,x=>x.node_gc.maximum_duration_ms=101,x=>x.node_gc.supported=false]){
    const bad=structuredClone(raw);mutate(bad);assert.throws(()=>summarizeNativePreviewDispatch(bad,rows));
  }
});
test("timing artifact admits numeric command facts, caps history and flags incomplete observation",()=>{
  let now=0;const p=new NativePreviewTiming(()=>now);
  p.command({id:"1",startSample:"0",frameCount:4096,owner_id:"must-not-be-recorded"});now=25;p.finish("1");
  assert.equal(p.rows[0].total_ms,25);assert(!JSON.stringify(p.snapshot()).includes("owner"));
  for(let i=2;i<=6;i++)p.command({id:String(i),startSample:"0",frameCount:4096});
  assert.equal(p.snapshot().incomplete_commands,4);assert.equal(p.snapshot().overflow,true);
  assert.throws(()=>p.command({id:"01",startSample:"0",frameCount:4096}));
});
test("phase summary checks exact command/sample continuity and keeps overlapping elapsed phases separate",()=>{
  let now=0;const p=new NativePreviewTiming(()=>now);
  for(let k=0;k<3;k++){
    p.command({id:String(k+1),startSample:String(k*4096),frameCount:4096});
    Object.assign(p.row(String(k+1)),{evaluate_ms:70,broker_ms:12,page_callback_ms:20,page_producer_ms:50,page_hex_ms:4,observer_ms:1,pcm_write_ms:2,post_evaluate_to_first_pcm_ms:3,compiled_bytes:1000,hex_calls:2});
    now+=k===2?100:75;p.finish(String(k+1));
  }
  const snapshot=p.snapshot();snapshot.rows[0].owner_id="must-not-be-recorded";
  const s=summarizeNativePreviewTiming(snapshot);assert(!JSON.stringify(s).includes("must-not-be-recorded"));
  assert.equal(s.contiguous_supplied_frames,"12288");assert.equal(s.commands_over_own_audio_duration,1);
  assert.equal(s.phases.page_unpack_validate_mix_ms.mean_ms,30);assert.equal(s.phases.total_ms.max_ms,100);
  assert.equal(s.phases.evaluate_residual_ms.mean_ms,16);assert.equal(s.phases.callback_outside_broker_ms.mean_ms,8);
  for(const mutate of [x=>x.overflow=true,x=>x.incomplete_commands=1,x=>x.rows[1].start_sample="4097",x=>x.rows[1].command_id="3",x=>x.rows[0].hex_calls=1,x=>x.rows[0].evaluate_ms=NaN,x=>x.rows[0].page_callback_ms=51]){
    const bad=structuredClone(snapshot);mutate(bad);assert.throws(()=>summarizeNativePreviewTiming(bad));
  }
});
test("Worker refill identity joins original native sample facts and bounds prior-control overlap without leaking extras",()=>{
  let now=0;const p=new NativePreviewTiming(()=>now);
  for(let k=0;k<3;k++) {
    p.command({id:String(k+1),startSample:String(k*4096),frameCount:4096});
    Object.assign(p.row(String(k+1)),{compiled_bytes:1000,hex_calls:2,page_producer_ms:8,page_callback_ms:4,evaluate_ms:10,observer_ms:1});
    now+=12;p.finish(String(k+1));
  }
  const tail={completed_block:1,consumed_frames:"4096",supplied_block:3,supplied_start_sample:"8192",supplied_frames:4096,
    receipt_at_ms:10,report_began_ms:20,report_headers_ended_ms:23,report_ended_ms:25,supplied_at_ms:35,completion_to_report_ms:10,completion_to_supply_ms:25,
    preceding_control_count:1,preceding_controls:[{kind:"heartbeat",began_ms:8,headers_ended_ms:15,ended_ms:19,secret:"must-not-be-recorded"}],secret:"must-not-be-recorded"};
  const continuity={credit:{slowest_refills:[tail]},consumption:{windows:[{start_frame:"0",end_frame:"4096",missing_frames:0},{start_frame:"4096",end_frame:"16384",missing_frames:128}]}};
  const raw=p.snapshot(),r=correlateNativePreviewCredit(raw,continuity),s=r.samples[0];
  assert.equal(s.native_command.command_id,"3");assert.equal(s.native_command.total_ms,12);
  assert.equal(s.preceding_controls[0].overlap_ms,9);assert.equal(s.associated_sample_windows[1].missing_frames,128);
  assert(!JSON.stringify(r).includes("must-not-be-recorded"));
  for(const change of [t=>t.supplied_start_sample="8193",t=>t.supplied_block=4,t=>t.supplied_frames=4095,t=>t.completion_to_supply_ms=24,t=>t.preceding_controls[0].ended_ms=9,t=>t.receipt_at_ms=NaN,t=>t.report_headers_ended_ms=26,t=>t.preceding_controls[0].headers_ended_ms=20]) {
    const bad=structuredClone(continuity);change(bad.credit.slowest_refills[0]);assert.throws(()=>correlateNativePreviewCredit(raw,bad));
  }
  const duplicate=structuredClone(continuity);duplicate.credit.slowest_refills.push({...tail});assert.throws(()=>correlateNativePreviewCredit(raw,duplicate),/Duplicate/);
});
