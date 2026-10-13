import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {correlateHotWindow,retainAcousticWindow,installAcousticDeliveryGate} from "./scene-preview-acoustic-diagnosis.mjs";

function authored() {
  const quanta=[];let consumed=0;
  for(let i=0;i<330;i++){
    const copied=i>=288&&i<318?0:i===287?64:128;
    consumed+=copied;
    quanta.push({token:"owned",copied,consumedFrames:String(consumed),renderFrame:1024+i*128,quantumFrames:128,completedBlocks:Math.floor(consumed/4096)});
  }
  const start=1024+36000,values=Array.from({length:8192},(_,k)=>{
    const frame=start+Math.floor(k/2),q=quanta.find(q=>frame>=q.renderFrame&&frame<q.renderFrame+128);
    return frame>=q.renderFrame+q.copied?0:k%2?.48:.96;
  });
  return {token:"owned",quantum_overflows:0,quanta,
    window:{first_nonzero_frame:1024+288,start_context_frame:start+288,values,frames:4096}};
}
test("authored DC gap maps real copied prefixes and frozen cursor through measured output delay",()=>{
  const result=correlateHotWindow(authored());
  assert.equal(result.measured_first_output_delay_frames,288);
  assert.equal(result.missing_frames,3296);assert.equal(result.post_zero_frames,3296);
  assert.equal(result.zero_mask_mismatches,0);
  assert.equal(result.consumed_end-result.consumed_start,800);
});
test("missing, duplicate, foreign and inconsistent quantum receipts fail instead of implying healthy output",()=>{
  for(const edit of [r=>r.quanta.splice(9,1),r=>r.quanta.splice(9,0,r.quanta[9]),
    r=>r.quanta[9].token="other",r=>r.quanta[9].consumedFrames="1",
    r=>r.quanta.splice(290),r=>r.quantum_overflows++,r=>r.window.values[0]=NaN]){
    const record=authored();edit(record);assert.throws(()=>correlateHotWindow(record));
  }
  const wrong=authored();wrong.window.values[0]=0;wrong.window.values[1]=0;
  assert.equal(correlateHotWindow(wrong).zero_mask_mismatches,1);
});
test("failed raw stereo bytes and actual receipts are bounded, unique and never overwrite another failure",async()=>{
  const dir=await fs.mkdtemp(path.join(os.tmpdir(),"opencut-acoustic-test-"));
  try{
    const post=authored(),pre={start_sample:36000,values:Array.from({length:8192},(_,k)=>k%2?1:2)};
    const input={pre,post,session:{generation:"1"},correlation:correlateHotWindow(post),gate:null,failure:"retained"};
    const first=await retainAcousticWindow(dir,input),second=await retainAcousticWindow(dir,input);
    assert.notEqual(first.directory,second.directory);assert.ok(first.bytes<=320*1024);
    assert.deepEqual(await fs.readFile(path.join(first.directory,"pre.f32")),Buffer.from(Float32Array.from(pre.values).buffer));
    assert.deepEqual(await fs.readFile(path.join(first.directory,"post.f32")),Buffer.from(Float32Array.from(post.window.values).buffer));
    assert.equal(JSON.parse(await fs.readFile(path.join(first.directory,"window.json"))).quanta.length,330);
    await assert.rejects(retainAcousticWindow(dir,{...input,pre:{values:[]}}));
  }finally{await fs.rm(dir,{recursive:true,force:true});}
});
function gateEnv(blocks=0){
  const sent=[],listeners=[],timers=new Map();let next=0;
  class Port{listeners=[];postMessage(...args){sent.push({port:this,args});}addEventListener(_,fn){this.listeners.push(fn);}}
  const witness={blocks,token:"owned"};
  const context=vm.createContext({MessagePort:Port,__scenePumpWitness:witness,
    addEventListener:(_,fn)=>listeners.push(fn),setTimeout:fn=>{timers.set(++next,fn);return next;},clearTimeout:id=>timers.delete(id)});
  const install=()=>vm.runInContext(`(${installAcousticDeliveryGate.toString()})()`,context);
  const port=new Port();
  const send=()=>{const data={kind:"block",token:"owned",pcm:new ArrayBuffer(32768)},transfer=[data.pcm];port.postMessage(data,transfer);return {data,transfer};};
  const receipt=(copied=0,token="owned")=>port.listeners.forEach(fn=>fn({data:{kind:"progress",token,copied}}));
  return {install,witness,port,send,receipt,sent,listeners,timers};
}
test("gate preserves original buffers/transfer lists/order and releases only after30 actual zero-copy receipts",()=>{
  const e=gateEnv();e.install();for(let i=0;i<9;i++)e.send();
  const ten=e.send(),eleven=e.send();assert.equal(e.sent.length,9);assert.equal(e.witness.deliveryGate.max_held,2);
  for(let i=0;i<50;i++){e.receipt(128);e.receipt(0,"other");}assert.equal(e.sent.length,9);
  for(let i=0;i<29;i++)e.receipt();assert.equal(e.sent.length,9);e.receipt();
  assert.equal(e.sent.length,11);assert.equal(e.sent[9].args[0],ten.data);assert.equal(e.sent[9].args[1],ten.transfer);
  assert.equal(e.sent[10].args[0],eleven.data);assert.equal(e.sent[10].args[1],eleven.transfer);
  assert.equal(e.timers.size,0);assert.equal(e.witness.deliveryGate.released,true);
});
test("gate refuses extra credits/late install and distinguishes watchdog release from controlled receipt release",()=>{
  assert.throws(()=>gateEnv(10).install(),/fence/);
  const e=gateEnv();e.install();for(let i=0;i<11;i++)e.send();assert.throws(()=>e.send(),/two credits/);
  e.timers.values().next().value();assert.equal(e.witness.deliveryGate.timed_out,true);assert.equal(e.sent.length,11);
});
test("original stop or abort cancels held transfers without late publication",()=>{
  for(const mode of ["stop","abort"]){
    const e=gateEnv();e.install();for(let i=0;i<11;i++)e.send();
    if(mode==="stop")e.port.postMessage({kind:"stop"});
    else e.listeners[0]({data:{kind:"abort",token:"owned"}});
    for(let i=0;i<35;i++)e.receipt();
    assert.equal(e.witness.deliveryGate.cancelled,true);assert.equal(e.witness.deliveryGate.held,0);
    assert.equal(e.sent.filter(x=>x.args[0].kind==="block").length,9);assert.equal(e.timers.size,0);
  }
});
