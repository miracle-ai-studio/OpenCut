import { decodeImage, decodeVideo } from "./decoder.mjs";
import { openGpuRealm, renderGpuFrame } from "./gpu.mjs";
import { byteBudget, checkAbort, integer, natural, reject, sameJsonFacts } from "./validation.mjs";
export { openGpuRealm };
const clone = value => structuredClone(value);
async function encodedResource(resource,host,budget,signal) {
  const expected=natural(resource.size,"encoded resource size");
  if(expected===0n||expected>BigInt(host.admission.maxEncodedBytes)||expected>BigInt(Number.MAX_SAFE_INTEGER)) reject("VISUAL_BUDGET_EXCEEDED","Encoded resource exceeds admission");
  const release=budget.reserve(Number(expected)*3);let transfer;
  try {
    checkAbort(signal);transfer=await host.callback("resolveVerifiedBytes",{resource});checkAbort(signal);
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
export async function renderClassicFrame(fixedPlan,host,realm,signal) {
  checkAbort(signal);const plan=clone(fixedPlan),admission=host.admission;
  const width=integer(plan.identity.width,"frame width",1,16384),height=integer(plan.identity.height,"frame height",1,16384);
  const rgba=BigInt(width)*BigInt(height)*4n;
  if(plan.identity.profile!=="classic-v31-gpu-normal-frame"||BigInt(width)*BigInt(height)>natural(admission.maxFramePixels,"frame pixels")||rgba>natural(admission.maxRgbaBytes,"RGBA bytes")||plan.layers.length>60)reject("INVALID_VISUAL_PLAN","Fixed visual frame exceeds admission");
  const budget=byteBudget(admission.maxRgbaBytes),sources=new Map(),encoded=new Map(),decoded=[],releases=[];
  // Output readback is admitted separately from all decoded source canvases.
  releases.push(budget.reserve(Number(rgba)));
  try {
    for(const layer of plan.layers) {
      checkAbort(signal);const permit=clone(await host.callback("admitDecode",{elementId:layer.elementId}));checkAbort(signal);
      const decode=layer.decode;
      if(decode.noFrameReason!==undefined&&decode.noFrameReason!==null) {
        if(decode.noFrameReason!=="outside_read_window")reject("INVALID_VISUAL_PLAN","Unknown prepared no-frame fact");
        decoded.push({kind:"noFrame",elementId:layer.elementId,requestedTime:clone(decode.requestedTime),reason:"outside_read_window"});continue;
      }
      let encodedSource=encoded.get(layer.resource.assetId);
      if(!encodedSource) {encodedSource=await encodedResource(layer.resource,host,budget,signal);encoded.set(layer.resource.assetId,encodedSource);}
      // Admission is checked before the decoder allocates; the cap remains
      // reserved until actual output dimensions replace it with factual bytes.
      const cap=natural(permit.maxDecodedBytes,"decoded byte cap");
      if(cap>BigInt(Number.MAX_SAFE_INTEGER))reject("VISUAL_BUDGET_EXCEEDED","Decoded admission cannot be allocated safely");
      const releaseCap=budget.reserve(Number(cap));let source;
      try {source=decode.kind==="image"?await decodeImage(encodedSource.bytes,permit,signal):decode.kind==="video"?await decodeVideo(encodedSource.bytes,decode,permit,signal):reject("UNSUPPORTED_RENDER_FEATURE","Unknown visual source");}
      finally {releaseCap();}
      checkAbort(signal);
      if(source.reason) {decoded.push({kind:"noFrame",elementId:layer.elementId,requestedTime:clone(decode.requestedTime),reason:source.reason});continue;}
      releases.push(budget.reserve(source.width*source.height*16));sources.set(layer.elementId,source);
      decoded.push({kind:decode.kind==="image"?"readyImage":"readyVideo",elementId:layer.elementId,width:source.width,height:source.height,...(decode.kind==="video"?{requestedTime:clone(decode.requestedTime),selectedVideoStream:null,actualTimestampSeconds:source.actualTimestampSeconds,frameDurationSeconds:source.frameDurationSeconds}:{})});
    }
    checkAbort(signal);const bound=clone(await host.callback("bindFrame",{decoded:clone(decoded)}));checkAbort(signal);
    if(!sameJsonFacts(bound.identity,plan.identity)||!sameJsonFacts(bound.decoded,decoded))reject("INVALID_VISUAL_PLAN","Bound frame changed fixed identity or decoder facts");
    await renderGpuFrame(bound,sources,admission,async(offset,bytes)=>{checkAbort(signal);await host.rgba(offset,bytes);checkAbort(signal);},signal,realm);
    checkAbort(signal);
    return {identity:clone(plan.identity),engineIdentity:"classic-v31-gpu-normal-frame",bound,rgbaBytes:String(rgba),peakAccountedBytes:String(budget.peak)};
  } finally {
    for(const source of sources.values()){source.canvas.width=0;source.canvas.height=0;}
    for(const item of encoded.values())item.release();for(const release of releases)release();
  }
}
