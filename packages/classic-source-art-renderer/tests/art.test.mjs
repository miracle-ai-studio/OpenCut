import test from "node:test";
import assert from "node:assert/strict";
import { peakBuckets, thumbnailSize, admitPixels } from "../src/art.mjs";
import { imageDimensions } from "../src/image-header.mjs";
import { validateAdmission, validateBegin } from "../host/runner.mjs";
test("all four channels contribute independent 128-sample maxima and a short tail",()=>{
 const channels=Array.from({length:4},()=>new Float32Array(257));
 channels[0][0]=.25;channels[1][127]=-.75;channels[2][128]=-1.25;channels[3][256]=.125;
 assert.deepEqual([...peakBuckets({sampleRate:48000,numberOfChannels:4,length:257,getChannelData:c=>channels[c]},4112)], [.75,1.25,.125]);
 assert.throws(()=>peakBuckets({sampleRate:48000,numberOfChannels:4,length:257,getChannelData:c=>channels[c]},4111));
 channels[3][2]=Infinity;assert.throws(()=>peakBuckets({sampleRate:48000,numberOfChannels:4,length:257,getChannelData:c=>channels[c]},4112));
});
test("empty actual decode is a single zero bucket and cancellation discards computation",()=>{
 assert.deepEqual([...peakBuckets({sampleRate:44100,numberOfChannels:1,length:0,getChannelData:()=>new Float32Array(0)},4)],[0]);
 const abort=new AbortController();abort.abort();assert.throws(()=>peakBuckets({sampleRate:44100,numberOfChannels:1,length:1,getChannelData:()=>new Float32Array(1)},4,abort.signal),{code:"CANCELLED"});
});
test("Classic JPEG fit rounds sequential dimensions and never upscales",()=>{
 assert.deepEqual(thumbnailSize(1920,1080),{width:1280,height:720});assert.deepEqual(thumbnailSize(16,9),{width:16,height:9});assert.deepEqual(thumbnailSize(1080,1920),{width:405,height:720});
 assert.throws(()=>admitPixels(50000,50000,1,{maxPcmBytes:256*1024*1024,maxJpegBytes:1024*1024}),{code:"SOURCE_ART_BUDGET_EXCEEDED"});
});
test("image allocation requires a supported bounded header",()=>{
 const png=new Uint8Array(24),v=new DataView(png.buffer);v.setUint32(0,0x89504e47);v.setUint32(4,0x0d0a1a0a);v.setUint32(12,0x49484452);v.setUint32(16,16);v.setUint32(20,9);
 assert.deepEqual(imageDimensions(png),{width:16,height:9});assert.throws(()=>imageDimensions(new Uint8Array(4)),{code:"UNSUPPORTED_SOURCE_ART_IMAGE"});
});
test("source execution policy is hard bounded rather than inherited from an audio job",()=>{
 const policy={quotaProfile:"monitored-terminate",deadlineMs:60000,maxEncodedBytes:33554432,maxPcmBytes:268435456,maxRssBytes:"2147483648"};
 assert.equal(validateAdmission(policy).monitorIntervalMs,100);
 const begin={executionId:"fixed",type:"begin",admission:policy,
   plan:{resource:{assetId:"video",contentHash:"a".repeat(64),kind:"video",size:4*1024*1024*1024},
     query:{kind:"thumbnail"},identity:{algorithm:"classic-thumbnail-1s-jpeg08-v1",decode_profile:"fixed-cas-streamed-concat-webaudio-canvas"},rangeCacheBytes:8*1024*1024,waveformWorkingBytes:121438208,maxWaveformBytes:40479400},resources:[{assetId:"video",handle:"verified"}]};
 assert.equal(validateBegin(begin).plan.resource.size,4*1024*1024*1024);
 const waveform={...begin,plan:{...begin.plan,query:{kind:"waveform",sampleRate:48000},identity:{...begin.plan.identity,algorithm:"source-peaks128-v1"}}};
 assert.equal(validateBegin(waveform).plan.resource.size,4*1024*1024*1024);
 assert.throws(()=>validateBegin({...waveform,plan:{...waveform.plan,waveformWorkingBytes:1}}));
 assert.throws(()=>validateBegin({...begin,plan:{...begin.plan,resource:{...begin.plan.resource,kind:"image"}}}));
 assert.throws(()=>validateBegin({...begin,plan:{...begin.plan,rangeCacheBytes:32*1024*1024}}));
 for(const [key,value] of [["deadlineMs",60001],["maxPcmBytes",268435457],["maxRssBytes","2147483649"]])assert.throws(()=>validateAdmission({...policy,[key]:value}));
});
