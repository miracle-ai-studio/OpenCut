import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import {assertTaggedStereo, measureTaggedStereo, chromiumLowMasterGain, chromiumHotMasterLeft, acousticCaptureMatches,
  masteringWitnessProcessor, installMasteringWitness} from "./scene-preview-acoustics.mjs";
function authored({db = 0, factor = 1, phase = .31, swap = false, referenceFactor = 1, mono = false} = {}) {
  return Array.from({length: 8192}, (_, k) => {
    const i = Math.floor(k / 2), c = swap ? 1 - k % 2 : k % 2;
    const t = i / 48000;
    return factor * (.125 * 10 ** (db / 20) * Math.sin(2 * Math.PI * 440 * t + phase) +
      (mono || c === 0 ? .1 * Math.sin(2 * Math.PI * 880 * t + .79) : .05 * Math.cos(2 * Math.PI * 660 * t - 1.1)) * referenceFactor);
  });
}
test("distinct authored voices/stereo fit unknown phases independently at both observation points", () => {
  for (const db of [0, -12]) for (const postMaster of [false, true]) {
    const result = assertTaggedStereo(authored({db, factor: postMaster ? chromiumLowMasterGain : 1}), {db, postMaster});
    assert.equal(result.length, 2);
    assert.ok(result.every(c => c.residual_rms < 1e-12));
    assert.ok(!("values" in result[0]));
  }
});
test("missing target, changed reference, swapped/downmixed channels and stale gain fail", () => {
  for (const options of [{db: -60}, {referenceFactor: 10 ** (-12 / 20)}, {swap: true}, {mono: true}, {db: -6}])
    assert.throws(() => assertTaggedStereo(authored(options), {db: -12}));
});
test("pre-master data/bare headroom cannot masquerade as post-master or hide distortion", () => {
  for (const values of [authored(), authored({factor: .98}), authored().map(v => Math.max(-.05, Math.min(.05, v)))])
    assert.throws(() => assertTaggedStereo(values, {postMaster: true}));
});
test("independent linked high DC static curve and makeup/headroom reject bypass or per-channel limiter", () => {
  const window = (l, r) => Array.from({length:8192}, (_, k) => k % 2 ? r : l);
  assertTaggedStereo(window(2, 1), {hot: true});
  assertTaggedStereo(window(chromiumHotMasterLeft, chromiumHotMasterLeft / 2), {hot:true,postMaster:true});
  for (const values of [window(2,1), window(.98,.98), window(chromiumHotMasterLeft, chromiumLowMasterGain * 10 ** (-.95 / 20))])
    assert.throws(() => assertTaggedStereo(values, {hot:true,postMaster:true}));
});
test("missing/infinite/malformed acoustic windows cannot pass", () => {
  for (const values of [[], authored().slice(2), authored().map((v,k)=>k===4?NaN:v)])
    assert.throws(()=>measureTaggedStereo(values));
});
test("hot seek cannot read a ready old window; actual startup token needs matching session and sample interior", () => {
  const s={session_id:"session",generation:"3",overlay_epoch:"2",state:"playing",identity:{sample_rate:48000},actual_start_sample:"300000"};
  const p={token:"gesture-key:epoch",consumedFrames:"4096"};
  const r={session:s,window:{token:p.token}};
  assert.equal(acousticCaptureMatches(s,p,r,{db:0,hot:true}),true);
  assert.equal(acousticCaptureMatches({...s,actual_start_sample:"133632"},p,r,{db:0,hot:true}),false);
  assert.equal(acousticCaptureMatches({...s,actual_start_sample:"0"},p,r,{db:0,hot:false}),true);
  for (const [session,progress,record] of [
    [{...s,generation:"4"},p,r],[s,{...p,token:"other"},r],[s,{...p,consumedFrames:"0"},r],
    [{...s,temporary_gain:{decibels_tenths:-120}},p,r],[{...s,actual_start_sample:"383999"},p,r]])
    assert.equal(acousticCaptureMatches(session,progress,record,{db:0,hot:true}),false);
});
function processor() {
  const messages = [];
  const context = vm.createContext({Float32Array,currentFrame:0,
    AudioWorkletProcessor: class {constructor(){this.port={postMessage:(data, transfer)=>messages.push({data,transfer})};}},
    registerProcessor: (_, Type) => {context.Type=Type;}});
  vm.runInContext(`(${masteringWitnessProcessor.toString()})()`, context);
  const node = new context.Type();
  const output = [new Float32Array(128), new Float32Array(128)];
  const block = (l,r) => { output.forEach(c=>c.fill(9)); const result=node.process([[new Float32Array(128).fill(l),new Float32Array(128).fill(r)]],[output]);
    assert.ok(output.every(c=>c.every(v=>v===0)));context.currentFrame+=128;return result;};
  return {node,context,messages,block};
}
test("sink leaves exact zero output; ignores unarmed/pre-signal silence and transfers one bounded actual window", () => {
  const p=processor();p.block(3,4);p.node.port.onmessage({data:{kind:"arm",token:"actual"}});p.block(0,0);
  const first=p.context.currentFrame;
  while(p.block(.1,.2));
  assert.equal(p.messages.length,1);const {data,transfer}=p.messages[0];
  assert.equal(data.token,"actual");assert.equal(data.first_nonzero_frame,first);
  assert.equal(data.start_context_frame,first+36000);assert.equal(data.frames,4096);
  assert.equal(data.values.byteLength,32768);assert.equal(transfer[0],data.values);
  const values=new Float32Array(data.values);assert.ok(values.every((v,k)=>v===Math.fround(k%2?.2:.1)));
  assert.equal(p.block(8,9),false);assert.equal(p.messages.length,1);
});
test("sink rejects a second owner/token", () => {
  const p=processor();p.node.port.onmessage({data:{kind:"arm",token:"first"}});
  assert.throws(()=>p.node.port.onmessage({data:{kind:"arm",token:"second"}}),/ownership/);
});
test("graph tee preserves original connections/results, waits for observation module and uses actual reset token", async () => {
  const calls=[], modules=[];
  class AudioNode {constructor(context){this.context=context;}connect(other){calls.push([this,other]);return other;}}
  class GainNode extends AudioNode {constructor(c){super(c);this.gain={value:.98};}}
  class AudioWorklet {async addModule(url){modules.push(url);}}
  class Native extends AudioNode {constructor(c,name,options){super(c);this.name=name;this.options=options;this.sent=[];this.port={postMessage:(...args)=>this.sent.push(args)};}}
  class Worker {constructor(){this.sent=[];}postMessage(...args){this.sent.push(args);}}
  const context={sampleRate:48000,destination:{},audioWorklet:new AudioWorklet()};
  const env=vm.createContext({window:{AudioWorkletNode:Native,Worker},AudioNode,GainNode,AudioWorklet,Blob,structuredClone,
    URL:{createObjectURL:()=>"blob:observer",revokeObjectURL:()=>{}}});
  vm.runInContext(`(${installMasteringWitness.toString()})({processorSource:"actual observer"})`,env);
  await context.audioWorklet.addModule("product.js");assert.deepEqual(modules,["product.js","blob:observer"]);
  const gain=new GainNode(context);const result=gain.connect(context.destination);
  assert.equal(result,context.destination);assert.equal(calls[0][1],context.destination);assert.equal(calls.length,3);
  const product=new env.window.AudioWorkletNode(context,"opencut-v4-audio-preview");
  product.port.postMessage({kind:"reset",token:"owned"});
  const observer=calls[1][1];assert.equal(observer.options.numberOfInputs,1);
  assert.equal(observer.sent[0][0].token,"owned");assert.equal(product.sent[0][0].token,"owned");
  const record=env.window.__sceneMasteringWitness.records[0];assert.equal(record.token,"owned");
  const worker=new env.window.Worker("actual",{name:"opencut-audio-pump"});
  const message={channel:"opencut-audio-pump",version:1,kind:"start",token:"owned",
    session:{session_id:"different-from-gesture-token",generation:"1",overlay_epoch:"0",identity:{revision:"fixed"}}};
  const transfer=[{}];worker.postMessage(message,transfer);
  assert.equal(worker.sent[0][0],message);assert.equal(worker.sent[0][1],transfer);
  assert.equal(record.session.session_id,"different-from-gesture-token");
  assert.equal(record.session.identity.revision,"fixed");
  assert.throws(()=>worker.postMessage({...message,token:"other"}),/ownership/);
  observer.port.onmessage({data:{kind:"window",token:"owned",frames:4096,values:new Float32Array(8192).buffer}});
  assert.equal(record.window.values.length,8192);
  assert.throws(()=>observer.port.onmessage({data:{kind:"window",token:"other",values:new Float32Array(8192).buffer}}),/receipt/);
  assert.throws(()=>gain.connect(context.destination),/Ambiguous/);
});
