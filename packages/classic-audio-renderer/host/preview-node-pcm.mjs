import * as nodeModule from "node:module";
import {createHash} from "node:crypto";
import {checkAbort,integer,reject,PcmBudget} from "../src/validation.mjs";
import {unpackPreviewBlock} from "../src/preview-packet.mjs";

// Buffer storage only. Decoding stays in pinned Mediabunny, sample clocks and
// gain in Rust, and mixing in the same prepareClassicPreview implementation.
export class PcmAudioBuffer {
  static maxBytes=256*1024*1024;
  constructor({numberOfChannels,length,sampleRate}) {
    integer(numberOfChannels,"PCM channels",1,2);integer(length,"PCM frames",1);integer(sampleRate,"PCM rate",1,384000);
    if(length*numberOfChannels*4>PcmAudioBuffer.maxBytes)reject("AUDIO_BUDGET_EXCEEDED","PCM buffer exceeds admission");
    this.numberOfChannels=numberOfChannels;this.length=length;this.sampleRate=sampleRate;this.duration=length/sampleRate;
    this.channels=Array.from({length:numberOfChannels},()=>new Float32Array(length));
  }
  getChannelData(channel){return this.channels[integer(channel,"PCM channel",0,this.numberOfChannels-1)];}
  copyToChannel(values,channel,start=0){integer(start,"PCM copy start",0,this.length);this.getChannelData(channel).set(values.subarray(0,this.length-start),start);}
  copyFromChannel(values,channel,start=0){integer(start,"PCM copy start",0,this.length);values.set(this.getChannelData(channel).subarray(start,start+values.length));}
}
let pinnedMedia;
function installPinnedMedia(modules) {
  const bytes=modules.registry.get("/dependencies/mediabunny/dist/bundles/mediabunny.mjs");
  if(!bytes)reject("INVALID_AUDIO_RUNTIME","Pinned browser decoder module is missing");
  const hash=createHash("sha256").update(bytes).digest("hex"),url=`opencut-pinned:mediabunny/${hash}`;
  if(pinnedMedia){if(pinnedMedia!==hash)reject("AUDIO_RUNTIME_IDENTITY_MISMATCH","Decoder module changed inside its realm");return;}
  // Use the already captured, self-contained module, rather than importing
  // another unmeasured Node dependency graph from node_modules.
  nodeModule.registerHooks({resolve(specifier,context,next){return specifier==="mediabunny"?{url,shortCircuit:true}:next(specifier,context);},
    load(address,context,next){return address===url?{format:"module",source:bytes.toString("utf8"),shortCircuit:true}:next(address,context);}});
  pinnedMedia=hash;
}
export async function prepareNodePcmPreview(realm,unwrap) {
  const plan=realm.begin.plan;
  if(typeof nodeModule.registerHooks!=="function"||!realm.fixedModules||!plan.voices.every(voice=>voice.sourceMode==="preserve-pts"))return null;
  if(typeof realm.nodePreviewRequest!=="function")reject("INVALID_AUDIO_RUNTIME","Node preview callback is missing");
  installPinnedMedia(realm.fixedModules);
  const {openVerifiedRangeSource}=await import("../src/verified-range-source.mjs");
  const {Input,ALL_FORMATS}=await import("mediabunny");
  const request=async(op,args)=>{
    checkAbort(realm.signal);const value=unwrap(await realm.nodePreviewRequest(op,args));checkAbort(realm.signal);
    if(op==="bindAndCompilePreviewBlock"||op==="compilePreviewBlock")return unpackPreviewBlock(JSON.parse(value));
    return value;
  };
  const resolve=async(resource,budget,signal)=>openVerifiedRangeSource(resource,{
    open:resource=>request("openVerifiedRange",{resource}),
    read:(transferId,offset,count)=>request("readVerifiedBytes",{transferId,offset:String(offset),count}),
    release:transferId=>request("releaseVerifiedBytes",{transferId}),
  },budget,Math.max(65536,plan.encodedSource.cacheBytes-Math.ceil(2*65536/3)),signal,{prefetch:false,readAhead:true});
  // Recipe selection is based on observed primary codec, never filename or
  // a failed decoder. Compressed audio and DSP keep their browser recipe.
  const probeBudget=new PcmBudget(realm.begin.admission.maxPcmBytes);
  for(const resource of plan.resources){
    const bytes=await resolve(resource,probeBudget,realm.signal);let input;
    try{
      input=new Input({source:bytes.source,formats:ALL_FORMATS});const track=await input.getPrimaryAudioTrack();checkAbort(realm.signal);
      if(!track)reject("KNOWN_ABSENT_AUDIO","The fixed resource has no primary audio track");
      if(!track.codec?.startsWith("pcm-")||track.numberOfChannels>2)return null;
    }finally{input?.dispose();await bytes.close();}
  }
  PcmAudioBuffer.maxBytes=Number(realm.begin.admission.maxPcmBytes);
  globalThis.AudioBuffer=PcmAudioBuffer;
  const host={engineIdentity:realm.identity.engine,resolveRangeSource:resolve,
    createAudioContext:sampleRate=>({sampleRate,createBuffer:(numberOfChannels,length,rate)=>new PcmAudioBuffer({numberOfChannels,length,sampleRate:rate}),close:async()=>{}}),
    createOfflineContext:()=>reject("UNSUPPORTED_AUDIO_CAPABILITY","PCM recipe does not admit prepared DSP"),abortRendering:()=>realm.stop()};
  for(const op of ["compilePitchPreparation","compileSourcePositions","bindPreviewSources","bindAndCompilePreviewBlock","observePreviewDecoded","observeWindowedPreviewDecoded","previewSourceWindows","compilePreviewBlock","releaseDecodedSource"])
    host[op]=args=>request(op,op==="releaseDecodedSource"?{resource:args}:args);
  host.admitPreviewDecode=(resource,primary)=>request("admitPreviewDecode",{resource,primary});
  const {prepareClassicPreview}=await import("../src/preview.mjs");
  return prepareClassicPreview(plan,host,{signal:realm.signal,maxPcmBytes:realm.begin.admission.maxPcmBytes,maxEncodedBytes:realm.begin.admission.maxEncodedBytes});
}
