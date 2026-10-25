import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {moduleRegistry} from '../host/modules.mjs';
import {prepareNodePcmPreview,PcmAudioBuffer} from '../host/preview-node-pcm.mjs';
import {previewReadCredits,unwrapPreviewEnvelope} from '../host/preview-browser.mjs';

function wav(){
 const data=Buffer.alloc(60);data.write('RIFF');data.writeUInt32LE(52,4);data.write('WAVEfmt ',8);data.writeUInt32LE(16,16);data.writeUInt16LE(1,20);data.writeUInt16LE(2,22);data.writeUInt32LE(48000,24);data.writeUInt32LE(192000,28);data.writeUInt16LE(4,32);data.writeUInt16LE(16,34);data.write('data',36);data.writeUInt32LE(16,40);
 [-32768,32767,-1,0,0,-32768,32767,1].forEach((value,i)=>data.writeInt16LE(value,44+i*2));return data;
}
test('Node PCM uses the pinned decoder and shared mixer with independent signed-16 expected samples',async()=>{
 const bytes=wav(),resource={assetId:'fixed',kind:'audio',contentHash:createHash('sha256').update(bytes).digest('hex'),size:String(bytes.length)};
 const identity={projectId:'project',revision:'revision',sequenceId:'scene',profile:'classic-realtime-v31-web-audio',sampleRate:48000,channels:2};
 const plan={identity,frameCount:'4',encodedSource:{mode:'verified-range',cacheBytes:65536},resources:[resource],voices:[{elementId:'voice',assetId:'fixed',sourceMode:'preserve-pts'}]};
 const operations=[];let opens=0,releases=0;
 const realm={begin:{plan,admission:{maxPcmBytes:4*1024*1024,maxEncodedBytes:1024*1024}},fixedModules:await moduleRegistry({}),identity:{engine:'captured decoder and sampler'},signal:new AbortController().signal,stop:async()=>{},nodePreviewRequest:async(op,args)=>{
  operations.push(op);let value={};
  if(op==='openVerifiedRange')value={transferId:`transfer-${++opens}`,contentHash:resource.contentHash,byteLength:resource.size,maxChunkBytes:65536};
  if(op==='readVerifiedBytes')value={offset:args.offset,eof:Number(args.offset)+args.count===bytes.length,bytes:new Uint8Array(bytes.subarray(Number(args.offset),Number(args.offset)+args.count))};
  if(op==='releaseVerifiedBytes')releases++;
  if(op==='admitPreviewDecode')value={preparation:null,stream:{workingFrames:32,maxChannels:2,maxChunkFrames:16,sourceSampleRate:48000,targetSampleRate:48000}};
  if(op==='bindAndCompilePreviewBlock'){
   assert.deepEqual(args.sourceFormats[0].segments,[{segmentId:0,timestampSeconds:0,frameCount:'4'}]);
   value=JSON.stringify({block:{identity,startSample:'0',frameCount:4,voices:[{elementId:'voice',blockOffset:0,frameCount:4,positionRef:0,amplitudeRef:0}],receipts:[{trackId:'track',elementId:'voice',decision:'ready'}]},positionTables:[[0,1,2,3].map(i=>[0,String(i),String(Math.min(i+1,3)),0])],amplitudeTables:[[1,1,1,1]]});
  }
  return {ok:true,value};
 }};
 const producer=await prepareNodePcmPreview(realm,unwrapPreviewEnvelope);
 try{
  assert(producer);const result=await producer.next({startSample:'0',frameCount:4,sourceWindows:[{assetId:'fixed',startSeconds:0,endSeconds:4/48000}]});
  assert.deepEqual([...result.channels[0]],[-1,-1/32768,0,32767/32768]);assert.deepEqual([...result.channels[1]],[32767/32768,0,-1,1/32768]);
  assert.equal(producer.receipt.decoders[0].primary.codec,'pcm-s16');assert.equal(opens,2);assert(operations.includes('bindAndCompilePreviewBlock'));
 }finally{await producer.close();}
 assert.equal(releases,2);
 const prepared=structuredClone(plan);prepared.voices[0].sourceMode='prepared-concat';
 assert.equal(await prepareNodePcmPreview({...realm,begin:{...realm.begin,plan:prepared}},unwrapPreviewEnvelope),null);
});
test('source read credits reserve control capacity and cancel queued work',async()=>{
 const controller=new AbortController(),acquire=previewReadCredits(controller.signal),held=await Promise.all(Array.from({length:6},()=>acquire()));
 let entered=false;const waiting=acquire().then(release=>{entered=true;return release;});await Promise.resolve();assert.equal(entered,false);
 held[0]();const release=await waiting;assert.equal(entered,true);release();
 const extra=await acquire(),cancelled=acquire();controller.abort();await assert.rejects(cancelled,{code:'CANCELLED'});extra();for(const release of held.slice(1))release();
});
test('PCM storage keeps Float32 bits, bounded allocation and independent channels',()=>{
 const buffer=new PcmAudioBuffer({numberOfChannels:2,length:2,sampleRate:44100});buffer.copyToChannel(new Float32Array([-0,.25]),0);assert(Object.is(buffer.getChannelData(0)[0],-0));assert.deepEqual([...buffer.getChannelData(1)],[0,0]);
 assert.throws(()=>new PcmAudioBuffer({numberOfChannels:2,length:1e9,sampleRate:48000}),{code:'AUDIO_BUDGET_EXCEEDED'});
});
