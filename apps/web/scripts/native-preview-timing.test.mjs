import test from "node:test";
import assert from "node:assert/strict";
import { installNativePreviewTiming, NativePreviewTiming } from "./native-preview-timing.mjs";
import { nextPreviewPage } from "../../../packages/classic-audio-renderer/host/preview-page.mjs";
import { summarizeNativePreviewTiming } from "./summarize-native-preview-timing.mjs";
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
