export { renderClassicCaptionFrameV3 } from "./text-frame-v3.mjs";
import { decodeImage, decodeVideo } from "./decoder.mjs";
export { VisualDecoderCache } from "./decoder.mjs";
export { measureClassicText } from "./text-layout.mjs";
import { openGpuRealm, renderGpuFrame } from "./gpu.mjs";
import { byteBudget, checkAbort, integer, natural, reject, sameJsonFacts } from "./validation.mjs";
import { composeTextFrame, validateTextFramePlan } from "./text-frame.mjs";
export { openGpuRealm };
const clone = value => structuredClone(value);
async function encodedResource(resource,host,budget,signal,operation="resolveVerifiedBytes",argument="resource") {
  const expected=natural(resource.size,"encoded resource size");
  if(expected===0n||expected>BigInt(host.admission.maxEncodedBytes)||expected>BigInt(Number.MAX_SAFE_INTEGER)) reject("VISUAL_BUDGET_EXCEEDED","Encoded resource exceeds admission");
  const release=budget.reserve(Number(expected)*3);let transfer;
  try {
    checkAbort(signal);transfer=await host.callback(operation,{[argument]:resource});checkAbort(signal);
    if(natural(transfer.byteLength,"verified byte length")!==expected) reject("INVALID_VISUAL_PLAN","Verified byte length changed");
    const bytes=new Uint8Array(Number(expected));let offset=0;
    while(offset<bytes.length) {
      checkAbort(signal);const count=Math.min(65536,bytes.length-offset);
      const chunk=await host.callback("readVerifiedBytes",{transferId:transfer.transferId,offset:String(offset),count});
      checkAbort(signal);
      if(chunk.offset!==String(offset)||typeof chunk.bytesHex!=="string"||chunk.bytesHex.length!==count*2||!/^(?:[a-f0-9]{2})+$/.test(chunk.bytesHex)||chunk.eof!==(offset+count===bytes.length)) reject("INVALID_VISUAL_PLAN","Verified resource chunks changed length or order");
      for(let n=0;n<count;n++)bytes[offset+n]=Number.parseInt(chunk.bytesHex.slice(n*2,n*2+2),16);offset+=count;
    }
    const digest=new Uint8Array(await crypto.subtle.digest("SHA-256",bytes));checkAbort(signal);
    const hash=Array.from(digest,b=>b.toString(16).padStart(2,"0")).join("");
    if(hash!==resource.contentHash) reject("ASSET_HASH_MISMATCH","Verified visual bytes changed identity");
    return {bytes,release};
  } catch(error) {release();throw error;}
  finally {if(transfer)await host.callback("releaseVerifiedBytes",{transferId:transfer.transferId});}
}
export async function renderClassicFrame(fixedPlan,host,realm,signal,cache,decoders) {
  return renderFrame(fixedPlan,host,realm,signal,cache,decoders);
}
export async function renderClassicTextFrameV2(fixedPlan,host,realm,signal,cache,decoders,fontCache) {
  checkAbort(signal);const plan=clone(fixedPlan);validateTextFramePlan(plan,host.admission);
  if(fontCache) {
    const keys=new Set(plan.texts.map(t=>`${t.font.fontId}:${t.font.contentHash}:${t.font.size}`));
    for(const [key,item] of fontCache)if(!keys.has(key)) { if(item.face)(globalThis.fonts??globalThis.document?.fonts)?.delete(item.face);fontCache.delete(key); }
  }
  return renderFrame(plan.media,host,realm,signal,cache,decoders,state=>composeTextFrame(plan,host,state,signal),fontCache);
}
export async function renderFrame(fixedPlan,host,realm,signal,cache,decoders,compose,fontCache) {
  checkAbort(signal);const plan=clone(fixedPlan),admission=host.admission;
  const width=integer(plan.identity.width,"frame width",1,16384),height=integer(plan.identity.height,"frame height",1,16384);
  const rgba=BigInt(width)*BigInt(height)*4n;
  if(plan.identity.profile!=="classic-v31-gpu-normal-frame"||BigInt(width)*BigInt(height)>natural(admission.maxFramePixels,"frame pixels")||rgba>natural(admission.maxRgbaBytes,"RGBA bytes")||plan.layers.length>60)reject("INVALID_VISUAL_PLAN","Fixed visual frame exceeds admission");
  const budget=byteBudget(admission.maxRgbaBytes),sources=new Map(),encoded=new Map(),decodedSources=new Map(),borrowedRequests=new Map(),canvases=new Set(),decoded=[],releases=[];
  // Output readback is admitted separately from all decoded source canvases.
  releases.push(budget.reserve(Number(rgba)));
  const resourceKey = r => `${r.assetId}:${r.contentHash}:${r.size}`;
  decoders?.retain(new Set(plan.layers.filter(l => l.decode.kind === "video" && !l.decode.noFrameReason).map(l => resourceKey(l.resource))));
  let retainedDecoded = decoders?.retainedBytes() ?? 0;
  releases.push(budget.reserve(retainedDecoded));
  // Optional session cache contains verified immutable encoded bytes only.
  // Its complete retained set is charged to every frame; changed resource sets
  // evict obsolete entries before admission. The optional decoded pool above
  // is independently retained/accounted; native one-shot canvases still release.
  if(cache) {
    const keys=new Set(plan.resources.map(r=>`${r.assetId}:${r.contentHash}:${r.size}`));
    for(const key of cache.keys())if(!keys.has(key))cache.delete(key);
    for(const bytes of cache.values())releases.push(budget.reserve(bytes.byteLength*3));
  }
  if(fontCache)for(const item of fontCache.values())releases.push(budget.reserve(item.bytes.byteLength*3));
  try {
    for(const layer of plan.layers) {
      checkAbort(signal);const permit=clone(await host.callback("admitDecode",{elementId:layer.elementId}));checkAbort(signal);
      const decode=layer.decode;
      if(decode.noFrameReason!==undefined&&decode.noFrameReason!==null) {
        if(decode.noFrameReason!=="outside_read_window")reject("INVALID_VISUAL_PLAN","Unknown prepared no-frame fact");
        decoded.push({kind:"noFrame",elementId:layer.elementId,requestedTime:clone(decode.requestedTime),reason:"outside_read_window"});continue;
      }
      let encodedSource=encoded.get(layer.resource.assetId);
      if(!encodedSource) {
        const key=`${layer.resource.assetId}:${layer.resource.contentHash}:${layer.resource.size}`,hit=cache?.get(key);
        if(hit) {
          if(host.requireResolveCached)await verifyCachedResource(layer.resource,host,signal);
          encodedSource={bytes:hit,release:()=>{}};
        }
        else {
          encodedSource=await encodedResource(layer.resource,host,budget,signal);
          if(cache) {
            const retained=[...cache.values()].reduce((n,b)=>n+b.byteLength,0)+encodedSource.bytes.byteLength;
            if(retained>Number(admission.maxEncodedBytes)){encodedSource.release();reject("VISUAL_BUDGET_EXCEEDED","Retained preview bytes exceed admission");}
            cache.set(key,encodedSource.bytes);
          }
        }
        encoded.set(layer.resource.assetId,encodedSource);
      }
      // Admission is checked before the decoder allocates; the cap remains
      // reserved until actual output dimensions replace it with factual bytes.
      const sourceKey = `${resourceKey(layer.resource)}:${JSON.stringify([decode, permit])}`;
      let source = decodedSources.get(sourceKey);
      if (!source) {
      const key = resourceKey(layer.resource);
      // Keep at most one borrowed observation per resource in a composed frame.
      // A different time or permit must not recycle/dispose an earlier layer's
      // pool canvas before all layers have finished uploading to the GPU.
      const usePool = decoders && decode.kind === "video" && (!borrowedRequests.has(key) || borrowedRequests.get(key) === sourceKey);
      if (usePool) borrowedRequests.set(key, sourceKey);
      const cap=natural(permit.maxDecodedBytes,"decoded byte cap");
      if(cap>BigInt(Number.MAX_SAFE_INTEGER))reject("VISUAL_BUDGET_EXCEEDED","Decoded admission cannot be allocated safely");
      // The cap already includes this resource's admitted retained pool. Only
      // its unused capacity is reserved again; new factual pool bytes replace
      // that temporary reservation after decode, never on top of it.
      const releaseCap=budget.reserve(Math.max(0, Number(cap) - (usePool ? decoders.retainedBytes(key) : 0)));
      try {
        source=decode.kind==="image"?await decodeImage(encodedSource.bytes,permit,signal):decode.kind==="video"?(usePool ? await decoders.decode(key,encodedSource.bytes,decode,permit,signal) : await decodeVideo(encodedSource.bytes,decode,permit,signal)):reject("UNSUPPORTED_RENDER_FEATURE","Unknown visual source");
        if (source.canvas && !source.borrowed) canvases.add(source.canvas);
      }
      finally {releaseCap();}
      const retained = decoders?.retainedBytes() ?? 0;
      if (retained > retainedDecoded) releases.push(budget.reserve(retained - retainedDecoded));
      retainedDecoded = retained;
      decodedSources.set(sourceKey, source);
      if (!source.reason) {
        sources.set(layer.elementId, source);
        if (!source.borrowed) releases.push(budget.reserve(source.width*source.height*16));
      }
      }
      checkAbort(signal);
      if(source.reason) {decoded.push({kind:"noFrame",elementId:layer.elementId,requestedTime:clone(decode.requestedTime),reason:source.reason});continue;}
      sources.set(layer.elementId,source);
      decoded.push({kind:decode.kind==="image"?"readyImage":"readyVideo",elementId:layer.elementId,width:source.width,height:source.height,...(decode.kind==="video"?{requestedTime:clone(decode.requestedTime),selectedVideoStream:null,actualTimestampSeconds:source.actualTimestampSeconds,frameDurationSeconds:source.frameDurationSeconds}:{})});
    }
    checkAbort(signal);let bound=clone(await host.callback("bindFrame",{decoded:clone(decoded)}));checkAbort(signal);
    if(!sameJsonFacts(bound.identity,plan.identity)||!sameJsonFacts(bound.decoded,decoded))reject("INVALID_VISUAL_PLAN","Bound frame changed fixed identity or decoder facts");
    if(compose)bound=await compose({bound,sources,budget,canvases,releases,encoded,retainedDecoded,fontCache,resolveFont:async resource=>{
      const key=`${resource.fontId}:${resource.contentHash}:${resource.size}`,hit=fontCache?.get(key);
      if(hit) {await verifyCachedResource(resource,host,signal,"resolveVerifiedFontBytes","font");return {bytes:hit.bytes,release:()=>{}};}
      const loaded=await encodedResource(resource,host,budget,signal,"resolveVerifiedFontBytes","font");
      if(fontCache) {
        const total=[...fontCache.values()].reduce((n,v)=>n+v.bytes.byteLength,0)+[...cache?.values()??[]].reduce((n,v)=>n+v.byteLength,0)+loaded.bytes.byteLength;
        if(total>Number(admission.maxEncodedBytes)){loaded.release();reject("VISUAL_BUDGET_EXCEEDED","Retained fonts exceed encoded admission");}
        fontCache.set(key,{bytes:loaded.bytes});
      }
      return loaded;
    }});
    const gpu = await renderGpuFrame(bound,sources,admission,async(offset,bytes)=>{checkAbort(signal);await host.rgba(offset,bytes);checkAbort(signal);},signal,realm,host.surface);
    host.observeExecution?.({ ...gpu, peakAccountedBytes: String(budget.peak), retainedDecodedBytes: String(retainedDecoded), retainedEncodedBytes: String([...cache?.values() ?? []].reduce((n,b)=>n+b.byteLength,0)) });
    checkAbort(signal);
    return {identity:clone(compose?bound.identity:plan.identity),engineIdentity:compose?bound.identity.profile:"classic-v31-gpu-normal-frame",bound,rgbaBytes:host.surface ? "0" : String(rgba),peakAccountedBytes:String(budget.peak)};
  } finally {
    for(const canvas of canvases){canvas.width=0;canvas.height=0;}
    for(const item of encoded.values())item.release();for(const release of releases)release();
  }
}

async function verifyCachedResource(resource,host,signal,operation="resolveVerifiedBytes",argument="resource") {
  checkAbort(signal);const transfer=await host.callback(operation,{[argument]:resource});
  try {if(natural(transfer.byteLength,"cached verified bytes")!==natural(resource.size,"cached resource size"))reject("INVALID_VISUAL_PLAN","Cached resource identity changed");checkAbort(signal);}
  finally {await host.callback("releaseVerifiedBytes",{transferId:transfer.transferId});}
}
