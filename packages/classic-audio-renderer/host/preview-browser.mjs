import {prepareNodePcmPreview} from "./preview-node-pcm.mjs";
import {deflateSync} from "node:zlib";
import { AudioExecutionError, checkAbort, integer, natural, reject } from "../src/validation.mjs";
import { preparePreviewPage, nextPreviewPage } from "./preview-page.mjs";
import { packPreviewBinding, compactPreviewReceipt } from "./preview-binding.mjs";
import { PROTOCOL_LIMITS, compiledFactsJson } from "./protocol.mjs";

export function encodePreviewFacts(text){
  if(typeof text!=="string"||Buffer.byteLength(text,"utf8")>PROTOCOL_LIMITS.replyBytes)reject("AUDIO_BUDGET_EXCEEDED","Compiled preview facts exceed the reply budget");
  const bytes=Buffer.from(text,"utf8");
  return {codec:"deflate-base64",decodedBytes:bytes.length,data:deflateSync(bytes,{level:1}).toString("base64")};
}

const safeCode = error => typeof error?.code === "string" && /^[A-Z][A-Z0-9_]{0,63}$/.test(error.code) ? error.code : "AUDIO_PREVIEW_FAILED";
export async function previewEnvelope(work) {
  try { return { ok: true, value: await work() }; }
  catch (error) { return { ok: false, error: { code: safeCode(error) } }; }
}
export function unwrapPreviewEnvelope(result) {
  if (result?.ok === true && Object.keys(result).every(key => ["ok", "value"].includes(key))) return result.value;
  if (result?.ok === false && Object.keys(result).length === 2 && Object.keys(result.error ?? {}).length === 1 && /^[A-Z][A-Z0-9_]{0,63}$/.test(result.error?.code)) throw new AudioExecutionError(result.error.code, "Isolated preview operation failed");
  throw new AudioExecutionError("AUDIO_PROTOCOL_ERROR", "Invalid isolated preview result");
}

export async function configurePreviewPage(realm) {
  const fixed = new Map(realm.begin.plan.resources.map(r => [r.assetId, r])), handles = new Map(realm.begin.resources.map(r => [r.assetId, r.handle]));
  const voices = new Set(realm.begin.plan.voices.map(v => v.elementId));
  const readCredit=previewReadCredits(realm.signal);
  const request=(op, args, inProcess) => previewEnvelope(async () => {
    checkAbort(realm.signal);
    if (!["openVerifiedRange", "readVerifiedBytes", "releaseVerifiedBytes", "releaseDecodedSource", "admitPreviewDecode", "observePreviewDecoded", "observeWindowedPreviewDecoded", "compilePitchPreparation", "compileSourcePositions", "bindPreviewSources", "bindAndCompilePreviewBlock", "previewSourceWindows", "compilePreviewBlock"].includes(op) || !args || typeof args !== "object") reject("INVALID_HOST_CALLBACK", "Unknown preview callback");
    if (["openVerifiedRange", "releaseDecodedSource", "admitPreviewDecode", "observePreviewDecoded"].includes(op)) {
      const resource = fixed.get(args.resource?.assetId);
      if (!resource || ["contentHash", "kind", "size"].some(k => args.resource[k] !== resource[k])) reject("NOT_AUTHORIZED", "Preview resource changed");
      if (["admitPreviewDecode"].includes(op)) {
        const primary = args.primary;
        if (!primary || Object.keys(primary).length !== 4 || typeof primary.codec !== "string" || !/^[a-z0-9][a-z0-9.-]{0,127}$/.test(primary.codec)) reject("INVALID_HOST_CALLBACK", "Invalid preview primary selection");
        integer(primary.trackId, "preview track ID", 0, 0xffffffff); integer(primary.sampleRate, "preview source rate", 1, 384000); integer(primary.channels, "preview source channels", 1, 64);
      }
      if (op === "observePreviewDecoded") {
        integer(args.sampleRate, "observed preview rate", 1, 384000); natural(args.frameCount, "observed preview frames"); integer(args.channels, "observed preview channels", 1, 2);
        args = { resource, sampleRate: args.sampleRate, frameCount: args.frameCount, channels: args.channels };
      } else args = op === "openVerifiedRange" ? { resource, handle: handles.get(resource.assetId) }
        : op === "releaseDecodedSource" ? { resource } : { resource, primary: args.primary };
    }
    if (op === "observeWindowedPreviewDecoded") {
      const observed = args.decoded, resource = fixed.get(observed?.resource?.assetId);
      if (!resource || ["contentHash", "kind", "size"].some(key => observed.resource[key] !== resource[key])) reject("NOT_AUTHORIZED", "Preview resource changed");
      integer(observed.sampleRate, "observed preview rate", 1, 384000); natural(observed.frameCount, "observed preview frames");
      integer(observed.channels, "observed preview channels", 1, 2); natural(args.residentFrameCount, "resident preview frames");
      args = { decoded: { resource, sampleRate: observed.sampleRate, frameCount: observed.frameCount, channels: observed.channels }, residentFrameCount: args.residentFrameCount };
    }
    if (["compilePitchPreparation", "compileSourcePositions"].includes(op) && !voices.has(args.elementId)) reject("NOT_AUTHORIZED", "Preview element changed");
    if (op === "compilePitchPreparation" && args.residentFrameCount !== null) natural(args.residentFrameCount, "resident prepared frames");
    if (["compileSourcePositions", "previewSourceWindows", "compilePreviewBlock"].includes(op)) integer(args.frameCount, "preview callback frames", 1, 4096);
    if (op === "bindAndCompilePreviewBlock") {
      if (Object.keys(args).length !== 2 || !realm.previewCommand || args.window?.startSample !== realm.previewCommand.startSample || args.window?.frameCount !== realm.previewCommand.frameCount)
        reject("INVALID_HOST_CALLBACK", "Combined source binding differs from the captured command");
      const binding = packPreviewBinding(realm.begin.plan, args.sourceFormats);
      const value = await realm.broker.request("bindAndCompilePreviewBlockPacked", { binding, window: args.window });
      if (typeof value !== "string" || Buffer.byteLength(value, "utf8") > PROTOCOL_LIMITS.replyBytes)
        reject("AUDIO_PROTOCOL_ERROR", "Invalid combined compiled preview facts");
      return inProcess ? value : encodePreviewFacts(value);
    }
    if (op === "bindPreviewSources") {
      if (Object.keys(args).length !== 2 || !Object.hasOwn(args, "sourceFormats") || !Object.hasOwn(args, "window")) reject("INVALID_HOST_CALLBACK", "Invalid preview source binding fields");
      const binding = packPreviewBinding(realm.begin.plan, args.sourceFormats);
      if (args.window !== null && (!realm.previewCommand || args.window.startSample !== realm.previewCommand.startSample || args.window.frameCount !== realm.previewCommand.frameCount))
        reject("INVALID_HOST_CALLBACK", "Source window differs from the active consumer command");
      const value = await realm.broker.request("bindPreviewSourcesShared", { binding, window: args.window });
      if (args.window === null) realm.previewBinding = binding;
      return value;
    }
    if (op === "readVerifiedBytes") { natural(args.offset, "preview byte offset"); integer(args.count, "preview byte chunk", 1, 65536); }
    if (["previewSourceWindows", "compilePreviewBlock"].includes(op) && (!realm.previewCommand || args.startSample !== realm.previewCommand.startSample || args.frameCount !== realm.previewCommand.frameCount))
      reject("INVALID_HOST_CALLBACK", "Preview callback differs from the active consumer command");
    const release=op==="readVerifiedBytes"?await readCredit():()=>{};
    let value;
    try{value=await realm.broker.request(op === "compilePreviewBlock" ? "compilePreviewBlockPacked" : op, args);}finally{release();}
    if (op === "compileSourcePositions") {
      // Keep the exact authoritative positions, but avoid Playwright recursively
      // marshalling thousands of position objects for every preparation chunk.
      if (!Array.isArray(value) || value.length !== args.frameCount) reject("AUDIO_PROTOCOL_ERROR", "Invalid prepared source positions");
      return compiledFactsJson(value);
    }
    if(op==="readVerifiedBytes"){
      if(!value||Object.keys(value).length!==3||value.offset!==args.offset||typeof value.eof!=="boolean"||typeof value.bytesHex!=="string"||value.bytesHex.length!==args.count*2||!/^[0-9a-f]+$/.test(value.bytesHex))reject("AUDIO_PROTOCOL_ERROR","Invalid verified source bytes");
      const bytes=Buffer.from(value.bytesHex,"hex");
      return inProcess ? {offset:value.offset,eof:value.eof,bytes:new Uint8Array(bytes.buffer,bytes.byteOffset,bytes.byteLength)}
        : {offset:value.offset,eof:value.eof,bytesBase64:bytes.toString("base64")};
    }
    if (op === "compilePreviewBlock") {
      // Native returns its exact compiled JSON text. Do not parse/stringify
      // here: that adds work and normalizes signed-zero floating facts.
      if (typeof value !== "string") reject("AUDIO_PROTOCOL_ERROR", "Missing compiled preview JSON");
      if (Buffer.byteLength(value, "utf8") > PROTOCOL_LIMITS.replyBytes) reject("AUDIO_BUDGET_EXCEEDED", "Compiled preview facts exceed the reply budget");
      return inProcess ? value : encodePreviewFacts(value);
    }
    return value;
  });
  // One authorization/command validator, with transport-specific projection.
  // Node already owns the exact native text/bytes; CDP requires compact values.
  realm.previewRequest=(op,args)=>request(op,args,false);
  realm.nodePreviewRequest=(op,args)=>request(op,args,true);
  await realm.page.exposeFunction("__opencutPreviewCallback",realm.previewRequest);
  await realm.page.exposeFunction("__opencutTerminatePreviewRealm", () => realm.stop());
}
export async function preparePreviewRealm(realm) {
  realm.nodePreview=await prepareNodePcmPreview(realm,unwrapPreviewEnvelope);
  if(realm.nodePreview){
    const stop=realm.stop.bind(realm);realm.stop=async()=>{try{await realm.nodePreview.close();}finally{await stop();}};
    return compactPreviewReceipt(realm.begin.plan,realm.nodePreview.receipt,realm.previewBinding);
  }
  const receipt = unwrapPreviewEnvelope(await realm.page.evaluate(preparePreviewPage, { plan: realm.begin.plan, engineIdentity: realm.identity.engine, admission: realm.begin.admission }));
  return compactPreviewReceipt(realm.begin.plan, receipt, realm.previewBinding);
}
export async function nextPreviewRealm(realm, command) {
  integer(command.frameCount, "preview PCM frames", 1, 4096);
  realm.previewCommand = command;
  try {
    const result = realm.nodePreview?await (async()=>{
      const next=await realm.nodePreview.next(command);
      return {receipt:next.receipt,frameCount:command.frameCount,channelsBase64:next.channels.map(samples=>{
        const bytes=Buffer.alloc(samples.length*4);for(let i=0;i<samples.length;i++)bytes.writeFloatLE(samples[i],i*4);return bytes.toString("base64");
      })};
    })():unwrapPreviewEnvelope(await realm.page.evaluate(nextPreviewPage, command));
    checkAbort(realm.signal);
    if (realm.previewCommand !== command || !result || Object.keys(result).length !== 3 || !Object.hasOwn(result, "receipt") ||
        result.frameCount !== command.frameCount || !Array.isArray(result.channelsBase64) || result.channelsBase64.length !== 2 ||
        result.channelsBase64.some(bytes => typeof bytes !== "string" || bytes.length !== 4 * Math.ceil(command.frameCount * 4 / 3) || !/^[A-Za-z0-9+/]*={0,2}$/.test(bytes)))
      reject("INVALID_OUTPUT_PCM", "Preview PCM exceeds captured command credit");
    const channels = result.channelsBase64.map(text => Buffer.from(text, "base64"));
    if (channels.some((bytes, index) => bytes.length !== command.frameCount * 4 || bytes.toString("base64") !== result.channelsBase64[index]))
      reject("INVALID_OUTPUT_PCM", "Invalid canonical preview PCM bytes");
    // Admit both channels before publishing either. Recheck cancellation for
    // each unchanged private packet; Rust still fences every received packet.
    for (let channel = 0; channel < 2; channel++) {
      checkAbort(realm.signal);
      await realm.writer.send({ type: "pcm", commandId: command.id, generation: command.generation,
        overlayEpoch: command.overlayEpoch, startSample: command.startSample, channel,
        frames: command.frameCount, bytesHex: channels[channel].toString("hex") });
    }
    return result.receipt;
  }
  finally { realm.previewCommand = undefined; }
}


// Two control callback credits remain available while verified source reads
// are in flight. The per-source two-line cache still owns its byte budget.
export function previewReadCredits(signal){
 let active=0;const waiting=[];
 signal?.addEventListener("abort",()=>{for(const task of waiting.splice(0))task.reject(new AudioExecutionError("CANCELLED","Preview source read cancelled"));},{once:true});
 const release=()=>{const next=waiting.shift();if(next)next.resolve(release);else active--;};
 return ()=>{
  checkAbort(signal);if(active<6){active++;return Promise.resolve(release);}
  if(waiting.length>=64)return Promise.reject(new AudioExecutionError("HOST_CALLBACK_BUDGET","Preview source wait queue is full"));
  return new Promise((resolve,reject)=>waiting.push({resolve,reject}));
 };
}
