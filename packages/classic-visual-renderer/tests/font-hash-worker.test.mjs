import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import vm from 'node:vm';
import { controlledWorkers } from './font-hash-fixture.mjs';
import { clearFontHashWorker, hashFontSnapshot, hasFontHashWorker } from '../src/font-hash.mjs';

const input=()=>Uint8Array.of(1,2,3,4),identity=JSON.stringify(['font','a'.repeat(64),'4',400,'normal',0]);
const code=expected=>error=>error.code===expected;

test('one opted-in realm reuses one worker, transfers the same owned bytes back and advances request IDs',async t=>{
  const f=controlledWorkers(t),realm=f.newRealm();
  assert.equal(f.workers.length,1);assert.equal(f.pending.length,0);
  const first=input(),p=hashFontSnapshot(realm,identity,first);
  assert.equal(first.byteLength,0);assert.deepEqual(new Uint8Array(f.pending[0].data.buffer),input());
  f.pending[0].resolve();const result=await p;
  assert.deepEqual(result.snapshot,input());assert.equal(result.hash,createHash('sha256').update(input()).digest('hex'));
  const second=hashFontSnapshot(realm,identity,input());assert.equal(f.workers.length,1);assert.equal(f.pending[1].data.id,2);
  f.pending[1].resolve();await second;
  clearFontHashWorker(realm);clearFontHashWorker(realm);assert.equal(f.workers[0].terminations,1);
  const third=hashFontSnapshot(realm,identity,input());assert.equal(f.workers.length,2);assert.equal(f.pending[2].data.id,3);
  // A late message from the terminated worker cannot settle the replacement.
  f.pending[0].reply({});f.pending[2].resolve();await third;
});

test('font worker bounds all outstanding requests at 32 and unopted/default realms never create workers',async t=>{
  const f=controlledWorkers(t);
  assert.equal(hasFontHashWorker({}),false);
  assert.throws(()=>hashFontSnapshot({},identity,input()),code('INVALID_VISUAL_PLAN'));assert.equal(f.workers.length,0);
  const realm=f.newRealm(),promises=[];
  for(let n=0;n<32;n++)promises.push(hashFontSnapshot(realm,identity,input()));
  const rejected=Promise.all(promises.map(p=>assert.rejects(p,code('FONT_LOAD_FAILED'))));
  assert.throws(()=>hashFontSnapshot(realm,identity,input()),code('VISUAL_BUDGET_EXCEEDED'));
  assert.equal(f.pending.length,32);assert.equal(f.workers.length,1);
  clearFontHashWorker(realm);await rejected;assert.equal(f.workers[0].terminations,1);
});

test('wrong version/id/identity/length/hash/extra fields terminate the worker and settle every pending request',async t=>{
  const f=controlledWorkers(t);
  for(const corrupt of [r=>r.version=2,r=>r.id=99,r=>r.identity='other',r=>r.byteLength=5,r=>r.buffer=new ArrayBuffer(5),r=>r.hash='bad',r=>r.extra=true,r=>r.kind='error']) {
    const realm=f.newRealm(),a=hashFontSnapshot(realm,identity,input()),b=hashFontSnapshot(realm,identity,input());
    const rejected=Promise.all([a,b].map(p=>assert.rejects(p,code('FONT_LOAD_FAILED'))));
    const message=f.pending.at(-2),d=message.data;
    const reply={version:1,kind:'sha256-result',id:d.id,identity:d.identity,byteLength:d.byteLength,hash:'a'.repeat(64),buffer:d.buffer};corrupt(reply);message.reply(reply);
    await rejected;assert.equal(message.worker.terminated,true);
  }
});

test('abort, worker errors and message errors terminate and settle all pending work without waiting for a response',async t=>{
  const f=controlledWorkers(t);
  for(const fault of ['abort','error','messageerror']) {
    const realm=f.newRealm(),cancel=new AbortController(),reason=Error('cancelled');
    const a=hashFontSnapshot(realm,identity,input(),cancel.signal),b=hashFontSnapshot(realm,identity,input());
    const rejected=Promise.all([a,b].map(p=>assert.rejects(p,error=>fault==='abort'?error===reason:error.code==='FONT_LOAD_FAILED')));
    const worker=f.workers.at(-1);
    if(fault==='abort')cancel.abort(reason);else if(fault==='error')worker.onerror({preventDefault(){}});else worker.onmessageerror({});
    await rejected;assert.equal(worker.terminated,true);
    const retry=hashFontSnapshot(realm,identity,input());f.pending.at(-1).resolve();await retry;
  }
});

// Exercise the actual worker source with controlled platform endpoints. This is
// protocol/ownership evidence, not a claim about browser thread performance.
test('worker source hashes the transferred buffer and returns it, rejecting malformed input without bytes',async()=>{
  const source=await readFile(new URL('../src/font-hash-worker.mjs',import.meta.url),'utf8'),messages=[];
  const self={postMessage(message,transfer){messages.push(structuredClone(message,transfer?{transfer}:undefined));}};
  vm.runInNewContext(source,{self,crypto,ArrayBuffer,Uint8Array});
  const bytes=input(),request=structuredClone({version:1,kind:'sha256',id:7,identity,byteLength:4,buffer:bytes.buffer},{transfer:[bytes.buffer]});
  await self.onmessage({data:request});assert.equal(request.buffer.byteLength,0);
  assert.equal(messages[0].id,7);assert.deepEqual(new Uint8Array(messages[0].buffer),input());
  assert.equal(messages[0].hash,createHash('sha256').update(input()).digest('hex'));
  await self.onmessage({data:{version:2}});assert.deepEqual(messages[1],{version:1,kind:'error'});
});

const gpuLocation=new URL('../src/gpu.mjs',import.meta.url);
const gpuCode=(await readFile(gpuLocation,'utf8')).replace(/from "([^"]+)"/g,(_,name)=>`from ${JSON.stringify(name.startsWith('/renderer/vendor/')?`data:text/javascript,${encodeURIComponent('export default "shader"')}`:new URL(name,gpuLocation).href)}`);
const {openGpuRealm,closeGpuRealm,clearGpuFrameCaches}=await import(`data:text/javascript;base64,${Buffer.from(gpuCode).toString('base64')}`);

test('GPU opt-in prewarms a worker; default stays CSP-compatible; clear/close terminate pending work',async t=>{
  const f=controlledWorkers(t),original=Object.getOwnPropertyDescriptor(navigator,'gpu');let destroys=0;
  Object.defineProperty(navigator,'gpu',{configurable:true,value:{requestAdapter:async()=>({info:{},requestDevice:async()=>({destroy(){destroys++;}})})}});
  t.after(()=>{if(original)Object.defineProperty(navigator,'gpu',original);else delete navigator.gpu;});
  const ordinary=await openGpuRealm();assert.equal(f.workers.length,0);assert.equal(hasFontHashWorker(ordinary),false);closeGpuRealm(ordinary);
  const realm=await openGpuRealm({fontHashWorker:true});assert.equal(f.workers.length,1);assert.equal(f.pending.length,0);
  const first=hashFontSnapshot(realm,identity,input()),firstRejected=assert.rejects(first,code('FONT_LOAD_FAILED'));
  clearGpuFrameCaches(realm);await firstRejected;assert.equal(f.workers[0].terminations,1);assert.equal(destroys,1);
  const second=hashFontSnapshot(realm,identity,input()),secondRejected=assert.rejects(second,code('FONT_LOAD_FAILED'));
  closeGpuRealm(realm);closeGpuRealm(realm);await secondRejected;assert.equal(f.workers[1].terminations,1);assert.equal(destroys,2);
  assert.throws(()=>hashFontSnapshot(realm,identity,input()),code('INVALID_VISUAL_PLAN'));
});
