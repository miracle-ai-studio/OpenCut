import { controlledWorkers } from "./font-hash-fixture.mjs";
import { clearFontHashWorker } from "../src/font-hash.mjs";
import { clearGraphicSourceCache } from "../src/graphic-source.mjs";
import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

// Only orchestration, accounting and identity are tested here. Actual glyphs,
// WebGPU pixels and native coverage require the separately owned browser run.
const canvases = [], gpuCalls = [], faces = new Set();
let decodes = 0;
globalThis.OffscreenCanvas = class {
  constructor(width, height) { this.width = width; this.height = height; canvases.push(this); }
  getContext() {
    return { letterSpacing: "0px", font: "10px sans-serif", save() {}, restore() {},
      translate() {}, scale() {}, rotate() {}, beginPath() {}, roundRect() {}, fill() {}, fillText() {}, fillRect() {},
      measureText: text => ({ width: text.length * 2, actualBoundingBoxAscent: 1, actualBoundingBoxDescent: .5 }) };
  }
};
globalThis.FontFace = class {
  constructor(family, bytes, descriptors) { Object.assign(this, { family, bytes, descriptors, status: "unloaded" }); }
  async load() { this.status = "loaded"; return this; }
};
globalThis.fonts = { add: face => faces.add(face), delete: face => faces.delete(face) };
mock.module("../src/decoder.mjs", { namedExports: {
  VisualDecoderCache: class {},
  decodeVideo: async () => { throw new Error("This test declares only static images"); },
  decodeImage: async () => { decodes++; return { width: 2, height: 1, canvas: new OffscreenCanvas(2, 1) }; },
} });
mock.module("../src/gpu.mjs", { namedExports: {
  openGpuRealm: async () => ({}),
    clearGpuFrameCaches: realm => {clearGraphicSourceCache(realm);clearFontHashWorker(realm);},
    closeGpuRealm: clearGraphicSourceCache,
  renderGpuFrame: async (bound, sources, _admission, emit) => {
    gpuCalls.push({ bound: structuredClone(bound), sources: new Map(sources) });
    for (const layer of bound.layers) assert.ok(sources.get(layer.elementId).canvas.width > 0);
    await emit(0, Uint8Array.of(1, 2, 3, 255)); return { peakGpuBytes: "4" };
  },
} });
const { renderClassicFrame, renderClassicTextFrame, preparePreviewFontFrame } = await import("../src/index.mjs");
const { validateTextFramePlan, fontResource } = await import("../src/text-frame.mjs");
const hash = value => createHash("sha256").update(value).digest("hex");
const imageBytes = Uint8Array.of(1, 2, 3, 4), fontBytes = Uint8Array.of(7, 6, 5, 4);
const code = expected => error => error.code === expected;

function plan() {
  const identity = { projectId: "p", revision: "r", sequenceId: "s", time: { ticks: "1", timebase: 7 }, width: 8, height: 4, profile: "classic-v31-gpu-normal-text-frame-v2" };
  const image = { assetId: "image", contentHash: hash(imageBytes), size: "4", kind: "image" };
  const text = content => ({ content, fontSizePx: 2, lineHeightPx: 2.4, letterSpacingPx: .25, align: "center", decoration: "none", color: [1, 1, 1, 1],
    transform: { centerX: 5, centerY: 1, scaleX: 2, scaleY: .5, rotationRadians: Math.PI / 2 },
    background: { enabled: false, color: [0, 0, 0, 0], paddingXPx: 0, paddingYPx: 0, offsetXPx: 0, offsetYPx: 0, cornerRadiusPercent: 0 } });
  const title = (elementId, content) => ({ elementId, text: text(content), opacity: .3,
    font: { fontId: "font", contentHash: hash(fontBytes), size: "4", weight: 400, style: "normal", faceIndex: 0, glyphCoverageValidated: true, validatedTextHash: hash(content) } });
  return { identity, media: { identity: { ...structuredClone(identity), profile: "classic-v31-gpu-normal-frame" }, resources: [image],
    layers: ["back", "front"].map(elementId => ({ elementId, resource: image, decode: { kind: "image" } })) },
    texts: [title("title", "AV"), title("second-title", "中文")], order: ["back", "title", "front", "second-title"] };
}
const admission = { maxFramePixels: "32", maxDecodedPixels: "66", maxEncodedBytes: 8, maxRgbaBytes: "1208", maxGpuBytes: "4096" };
function host(plan, options = {}) {
  const transfers = new Map(), calls = [], chunks = [];
  let mediaBound, transferNumber = 0;
  const quad = opacity => ({ centerX: 4, centerY: 2, width: 8, height: 4, rotationRadians: 0, opacity, flipX: false, flipY: false });
  return { admission: { ...admission, ...options.admission }, transfers, calls, chunks,
    rgba: async (offset, bytes) => chunks.push({ offset, bytes: Array.from(bytes) }),
    callback: async (op, args) => {
      calls.push({ op, args: structuredClone(args) });
      if (op === "admitDecode") return { maxWidth: 2, maxHeight: 1, maxPixels: "2", maxDecodedBytes: "32" };
      if (op === "resolveVerifiedBytes" || op === "resolveVerifiedFontBytes") {
        const bytes = op === "resolveVerifiedBytes" ? imageBytes : options.corruptFont ? Uint8Array.of(0, 0, 0, 0) : fontBytes;
        const transferId = String(++transferNumber); transfers.set(transferId, bytes);
        return { transferId, byteLength: String(bytes.length) };
      }
      if (op === "readVerifiedBytes") {
        const bytes = transfers.get(args.transferId), start = Number(args.offset);
        return { offset: args.offset, bytesHex: Buffer.from(bytes.subarray(start, start + args.count)).toString("hex"), eof: start + args.count === bytes.length };
      }
      if (op === "releaseVerifiedBytes") { assert.equal(transfers.delete(args.transferId), true); return {}; }
      if (op === "bindFrame") {
        mediaBound = { identity: plan.media.identity, decoded: args.decoded, clear: [0, 0, 0, 1], background: [0, 0, 0, 1],
          layers: plan.media.layers.map(layer => ({ elementId: layer.elementId, assetId: layer.resource.assetId, quad: quad(.75) })) };
        return mediaBound;
      }
      if (op === "bindTextFrame") {
        const byId = new Map(mediaBound.layers.map(layer => [layer.elementId, layer]));
        for (const text of plan.texts) byId.set(text.elementId, { elementId: text.elementId, assetId: text.font.fontId, quad: quad(Math.fround(text.opacity)) });
        const result = { ...mediaBound, identity: plan.identity, layers: plan.order.map(id => structuredClone(byId.get(id))), rasters: args.rasters };
        options.changeBound?.(result);
        options.abort?.();
        return result;
      }
      throw new Error(`Unexpected callback: ${op}`);
    },
  };
}
function reset() { canvases.length = 0; gpuCalls.length = 0; decodes = 0; assert.equal(faces.size, 0); }
function released(h) { assert.equal(h.transfers.size, 0); assert.equal(faces.size, 0); assert.ok(canvases.every(canvas => canvas.width === 0 && canvas.height === 0)); }

test("V2 reuses one media decode and shared font transfer, preserving full mixed order and exact cumulative budget", async () => {
  reset(); const p = plan(), h = host(p);
  const result = await renderClassicTextFrame(p, h, {});
  assert.equal(decodes, 1); assert.equal(gpuCalls.length, 1);
  assert.equal(h.calls.filter(call => call.op === "resolveVerifiedBytes").length, 1);
  assert.equal(h.calls.filter(call => call.op === "resolveVerifiedFontBytes").length, 1);
  assert.deepEqual(h.calls.find(call => call.op === "resolveVerifiedFontBytes").args, { font: fontResource(p.texts[0].font) });
  assert.equal(result.peakAccountedBytes, "1208"); // 128 output + 32 image + 2*512 text + 2*12 bytes.
  assert.equal(result.engineIdentity, p.identity.profile);
  assert.deepEqual(result.bound.layers.map(layer => layer.elementId), p.order);
  assert.deepEqual(result.bound.rasters.map(raster => raster.layout.lines[0].content), ["AV", "中文"]);
  assert.equal(gpuCalls[0].sources.get("back"), gpuCalls[0].sources.get("front"));
  assert.deepEqual(result.bound.layers[1].quad, { centerX: 4, centerY: 2, width: 8, height: 4, rotationRadians: 0, opacity: Math.fround(.3), flipX: false, flipY: false });
  released(h);
});

test("V1 retains its original media path and never calls the text or font protocol", async () => {
  reset(); const p = plan(), h = host(p);
  const result = await renderClassicFrame(p.media, h, {});
  assert.equal(result.engineIdentity, "classic-v31-gpu-normal-frame");
  assert.equal(result.peakAccountedBytes, "172");
  assert.ok(h.calls.every(call => !["resolveVerifiedFontBytes", "bindTextFrame"].includes(call.op)));
  assert.deepEqual(result.bound.layers.map(layer => layer.elementId), ["back", "front"]);
  released(h);
});

test('typed in-process transfers preserve hashes and reject malformed chunks without changing the hex fallback',async()=>{
  for(const fault of [null,'length','hash']){
    reset();const p=plan(),h=host(p);
    h.readVerifiedBytes=async(id,offset,count)=>fault==='length'?new Uint8Array(count-1):fault==='hash'?new Uint8Array(count):h.transfers.get(id).subarray(offset,offset+count);
    if(fault)await assert.rejects(renderClassicTextFrame(p,h,{}),code(fault==='hash'?'ASSET_HASH_MISMATCH':'INVALID_VISUAL_PLAN'));
    else{const result=await renderClassicTextFrame(p,h,{});assert.equal(result.peakAccountedBytes,'1208');assert.equal(gpuCalls.length,1);}
    assert.ok(!h.calls.some(c=>c.op==='readVerifiedBytes'));if(fault)assert.equal(gpuCalls.length,0);released(h);
  }
});

test("aggregate encoded, decoded pixels and cumulative canvas bytes cannot be bypassed by text partitions", async () => {
  for (const limit of [{ maxEncodedBytes: 7 }, { maxDecodedPixels: "65" }, { maxRgbaBytes: "1207" }]) {
    reset(); const p = plan(), h = host(p, { admission: limit });
    await assert.rejects(renderClassicTextFrame(p, h, {}), code("VISUAL_BUDGET_EXCEEDED"));
    assert.equal(gpuCalls.length, 0); released(h);
  }
});

test("font byte corruption and another title's coverage fail before GPU output with all resources retired", async () => {
  for (const corruptBytes of [true, false]) {
    reset(); const p = plan();
    if (!corruptBytes) p.texts[1].font.validatedTextHash = p.texts[0].font.validatedTextHash;
    const h = host(p, { corruptFont: corruptBytes });
    await assert.rejects(renderClassicTextFrame(p, h, {}), code(corruptBytes ? "ASSET_HASH_MISMATCH" : "INVALID_VISUAL_PLAN"));
    assert.equal(gpuCalls.length, 0); assert.equal(h.chunks.length, 0); released(h);
  }
});

test("final binding cannot alter media, omit text, reorder layers, change observed layout or double-apply transform/opacity", async () => {
  const corruptions = [
    result => { result.identity.revision = "other"; },
    result => { result.layers.reverse(); },
    result => { result.layers[0].quad.width--; },
    result => { result.layers[1].quad.rotationRadians = Math.PI / 2; },
    result => { result.layers[1].quad.opacity = .09; },
    result => { result.layers[1].assetId = "other-font"; },
    result => { result.rasters.pop(); },
    result => { result.rasters[0].layout.lines[0].content = "wrong"; },
    result => { result.background[0] = .5; },
  ];
  for (const changeBound of corruptions) {
    reset(); const p = plan(), h = host(p, { changeBound });
    await assert.rejects(renderClassicTextFrame(p, h, {}), code("INVALID_VISUAL_PLAN"));
    assert.equal(gpuCalls.length, 0); released(h);
  }
});

test("cancellation after native binding releases full canvases and emits no tentative output", async () => {
  reset(); const p = plan(), cancel = new AbortController(), reason = new Error("owner cancelled");
  const h = host(p, { abort: () => cancel.abort(reason) });
  await assert.rejects(renderClassicTextFrame(p, h, {}, cancel.signal), error => error === reason);
  assert.equal(gpuCalls.length, 0); assert.equal(h.chunks.length, 0); released(h);
});

test("V2 plan keeps resource and element namespaces, exact media identity and complete order fixed", () => {
  const corruptions = [
    p => { p.media.identity.time.ticks = "2"; },
    p => { p.texts[1].elementId = "title"; },
    p => { p.texts[1].font.contentHash = "f".repeat(64); },
    p => { p.texts[0].font.fontId = "image"; },
    p => { p.order[1] = "front"; },
    p => { p.texts[0].opacity = Infinity; },
    p => { p.media.layers[0].decode.kind = "text"; },
  ];
  for (const corrupt of corruptions) {
    const p = structuredClone(plan()); corrupt(p);
    assert.throws(() => validateTextFramePlan(p, admission), code("INVALID_VISUAL_PLAN"));
  }
});

test("persistent frames reuse verified bytes and loaded faces but renew native authorization and evict inactive fonts", async () => {
  reset();const p=plan(),mediaCache=new Map(),fontCache=new Map();
  const first=host(p);first.requireResolveCached=true;
  await renderClassicTextFrame(p,first,{},undefined,mediaCache,undefined,fontCache);
  assert.equal(first.calls.filter(c=>c.op==="readVerifiedBytes").length,2);
  assert.equal(faces.size,1);assert.equal(fontCache.size,1);assert.equal(first.transfers.size,0);
  const second=host(p);second.requireResolveCached=true;
  const receipt=await renderClassicTextFrame(p,second,{},undefined,mediaCache,undefined,fontCache);
  assert.equal(receipt.peakAccountedBytes,"1208");
  assert.equal(second.calls.filter(c=>c.op==="readVerifiedBytes").length,0);
  assert.equal(second.calls.filter(c=>c.op==="resolveVerifiedFontBytes").length,1);
  assert.equal(second.calls.filter(c=>c.op==="resolveVerifiedBytes").length,1);
  assert.equal(faces.size,1);assert.equal(second.transfers.size,0);
  const inactive=structuredClone(p);inactive.texts=[];inactive.order=["back","front"];
  const last=host(inactive);last.requireResolveCached=true;
  await renderClassicTextFrame(inactive,last,{},undefined,mediaCache,undefined,fontCache);
  assert.equal(fontCache.size,0);assert.equal(faces.size,0);
  assert.ok(canvases.every(c=>c.width===0&&c.height===0));
});

const { renderClassicCaptionFrame } = await import("../src/caption-frame.mjs");
function captionPlan() {
  const execution = plan();
  execution.texts[0].text.content = "AV To";
  execution.texts[0].font.validatedTextHash = hash("AV To");
  return { identity: { ...execution.identity, profile: "classic-v31-gpu-normal-caption-frame-v3" }, execution,
    captions: [{ elementId: "title", maxWidthPx: 6.4, bottomMarginPx: .2, wrapProfile: "classic-v31-greedy-whitespace-v1" }] };
}
function captionHost(p, options = {}) {
  const base = host(p.execution, options), callback = base.callback;
  base.captionCalls = [];
  base.callback = async (op, args) => {
    if (op === "bindCaptionFrameV3") {
      base.captionCalls.push(structuredClone(args));
      return callback("bindTextFrame", { rasters: args.rasters });
    }
    return callback(op, args);
  };
  return base;
}
test("V3 wraps only captions, binds original content identity and reuses one fixed font/media transfer", async () => {
  reset(); const p = captionPlan(), h = captionHost(p);
  const result = await renderClassicCaptionFrame(p, h, {});
  assert.equal(result.identity.profile, "classic-v31-gpu-normal-caption-frame-v3");
  assert.equal(result.execution.engineIdentity, "classic-v31-gpu-normal-text-frame-v2");
  assert.equal(h.captionCalls.length, 1);
  assert.equal(result.captions[0].originalTextHash, hash("AV To"));
  assert.equal(result.captions[0].wrappedText, "AV\nTo");
  assert.equal(result.execution.bound.rasters[0].layout.validatedTextHash, hash("AV\nTo"));
  assert.deepEqual(result.execution.bound.rasters.map(raster => raster.layout.lines.map(line => line.content)), [["AV", "To"], ["中文"]]);
  assert.equal(result.execution.bound.rasters[0].layout.transform.centerX, 4);
  assert.equal(result.execution.bound.rasters[0].layout.transform.centerY, 4 - .2 - 2.4);
  assert.equal(h.calls.filter(call => call.op === "resolveVerifiedFontBytes").length, 1);
  assert.equal(result.execution.peakAccountedBytes, "1208"); released(h);
});
test("V3 fixed caption directives and native original text coverage are mandatory before output", async () => {
  for (const corrupt of [p => { p.captions[0].bottomMarginPx = 0; }, p => { p.execution.texts[0].font.validatedTextHash = hash("wrong"); }]) {
    reset(); const p = captionPlan(); corrupt(p); const h = captionHost(p);
    await assert.rejects(renderClassicCaptionFrame(p, h, {}), code("INVALID_VISUAL_PLAN"));
    assert.equal(gpuCalls.length, 0); assert.equal(h.chunks.length, 0); released(h);
  }
});

test("V3 caption exit releases inactive font bytes and faces before a tight media-only decode admission", async () => {
  reset(); const p = captionPlan(), mediaCache = new Map(), fontCache = new Map();
  await renderClassicCaptionFrame(p, captionHost(p), {}, undefined, mediaCache, undefined, fontCache);
  assert.equal(fontCache.size, 1); assert.equal(faces.size, 1);
  const inactive = structuredClone(p);
  inactive.captions = []; inactive.execution.texts = []; inactive.execution.order = ["back", "front"];
  const last = captionHost(inactive, { admission: { maxRgbaBytes: "172" } });
  const result = await renderClassicCaptionFrame(inactive, last, {}, undefined, mediaCache, undefined, fontCache);
  assert.equal(result.execution.peakAccountedBytes, "172");
  assert.equal(fontCache.size, 0); released(last);
});

const {renderClassicGraphicFrame}=await import('../src/graphic-frame.mjs');
function graphicTextPlan() {
  const execution=captionPlan();
  return {identity:{...execution.identity,profile:'classic-v31-gpu-normal-graphic-frame-v1'},execution,graphics:[],order:execution.execution.order};
}
function graphicTextHost(p,options={}) {
  const base=captionHost(p.execution,options),callback=base.callback;let child;
  base.callback=async(op,args)=>{
    if(op==='bindCaptionFrameV3'){child=await callback(op,args);return child;}
    if(op==='bindGraphicFrameV1')return {...child,identity:p.identity,captions:base.captionCalls.at(-1).captions,graphics:args.graphics};
    return callback(op,args);
  };
  return base;
}
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
function gateFontHashes(t) {
  const fixture=controlledWorkers(t),digest=crypto.subtle.digest.bind(crypto.subtle),inputs=[];
  t.mock.method(crypto.subtle,'digest',(algorithm,input)=>{
    inputs.push(new Uint8Array(input));assert.notEqual(input.byteLength,4);return digest(algorithm,input);
  });
  return {...fixture,realm:fixture.newRealm(),inputs};
}
function cachedResources(p) {
  const media=p.execution.execution.media.resources[0],font=p.execution.execution.texts[0].font;
  return {mediaCache:new Map([[`${media.assetId}:${media.contentHash}:${media.size}`,imageBytes.slice()]]),fontCache:new Map([[`${font.fontId}:${font.contentHash}:${font.size}`,{bytes:fontBytes.slice()}]])};
}
function clearTestFonts(cache) {for(const value of cache.values())if(value.face)fonts.delete(value.face);cache.clear();}

test('one invocation prepares fresh font snapshots before a later authoritative frame, with unchanged budget and single-use scope',async t=>{
  reset();const p=graphicTextPlan(),{mediaCache,fontCache}=cachedResources(p),{pending,realm}=gateFontHashes(t);
  const f=p.execution.execution.texts[0].font,signal=new AbortController().signal;
  assert.equal(await preparePreviewFontFrame([f],p.identity,{...admission,maxRgbaBytes:'151'},realm,signal,mediaCache,undefined,fontCache),null);
  assert.equal(pending.length,0);
  const preparing=preparePreviewFontFrame([f],p.identity,admission,realm,signal,mediaCache,undefined,fontCache);
  assert.equal(pending.length,1);assert.equal(gpuCalls.length,0);pending[0].resolve();const prepared=await preparing;
  const next=structuredClone(p);
  const retime=value=>{if(!value||typeof value!=='object')return;if(value.identity?.time)value.identity.time={ticks:'3',timebase:7};for(const child of Object.values(value))retime(child);};retime(next);
  try {
    const h=graphicTextHost(next);h.preparedFontFrame=prepared.token;
    const receipt=await renderClassicGraphicFrame(next,h,realm,signal,mediaCache,undefined,fontCache);
    assert.equal(pending.length,1);assert.deepEqual(receipt.identity.time,{ticks:'3',timebase:7});
    assert.equal(receipt.execution.execution.peakAccountedBytes,'1208');assert.equal(gpuCalls.length,1);
    const reused=graphicTextHost(next);reused.preparedFontFrame=prepared.token;
    await assert.rejects(renderClassicGraphicFrame(next,reused,realm,signal,mediaCache,undefined,fontCache),code('INVALID_VISUAL_PLAN'));
    assert.equal(gpuCalls.length,1);
  } finally {await prepared?.close();clearTestFonts(fontCache);}
});

test('graphic warm hash starts before media binding and does not block it; shared font hashes once per frame with unchanged budget',async t=>{
  reset();const p=graphicTextPlan(),mediaCache=new Map(),fontCache=new Map();
  const first=await renderClassicGraphicFrame(p,graphicTextHost(p),{},undefined,mediaCache,undefined,fontCache);
  const {pending,inputs,realm}=gateFontHashes(t),mediaSeen=deferred(),mediaFinish=deferred(),fontSeen=deferred();
  const h=graphicTextHost(p),callback=h.callback;gpuCalls.length=0;
  h.callback=async(op,args)=>{
    if(op==='bindFrame'){assert.equal(pending.length,1);mediaSeen.resolve();await mediaFinish.promise;}
    if(op==='resolveVerifiedFontBytes')fontSeen.resolve();
    return callback(op,args);
  };
  try {
    const rendering=renderClassicGraphicFrame(p,h,realm,undefined,mediaCache,undefined,fontCache);
    await mediaSeen.promise;assert.equal(gpuCalls.length,0);
    mediaFinish.resolve();await fontSeen.promise;assert.equal(gpuCalls.length,0);
    pending[0].resolve();const warm=await rendering;
    assert.equal(pending.length,1);assert.equal(warm.execution.execution.peakAccountedBytes,first.execution.execution.peakAccountedBytes);
    assert.equal(warm.execution.execution.peakAccountedBytes,'1208');
    assert.equal(h.calls.filter(c=>c.op==='resolveVerifiedFontBytes').length,1);
    assert.ok(inputs.some(input=>Buffer.from(input).toString()==='AV To'));assert.ok(inputs.some(input=>Buffer.from(input).toString()==='中文'));
    const next=renderClassicGraphicFrame(p,graphicTextHost(p),realm,undefined,mediaCache,undefined,fontCache);
    await new Promise(resolve=>setImmediate(resolve));
    assert.equal(pending.length,2);pending[1].resolve();await next;
    fontCache.values().next().value.bytes[0]^=255;const before=gpuCalls.length;
    const corrupt=renderClassicGraphicFrame(p,graphicTextHost(p),realm,undefined,mediaCache,undefined,fontCache);
    const rejected=assert.rejects(corrupt,code('ASSET_HASH_MISMATCH'));await new Promise(resolve=>setImmediate(resolve));assert.equal(pending.length,3);pending[2].resolve();await rejected;
    assert.equal(gpuCalls.length,before);assert.ok(canvases.every(canvas=>canvas.width===0));
  } finally {clearTestFonts(fontCache);}
});

test('complete active-font reservation precedes every speculative hash or media callback after inactive eviction',async t=>{
  reset();const p=graphicTextPlan(),{mediaCache,fontCache}=cachedResources(p),{pending,realm}=gateFontHashes(t);
  // Inactive entries are actually evicted. The current active bytes must still
  // fit before any speculative hashing or media callback starts.
  fontCache.set('inactive',{bytes:fontBytes.slice()});
  const h=graphicTextHost(p,{admission:{maxRgbaBytes:'151'}});
  await assert.rejects(renderClassicGraphicFrame(p,h,realm,undefined,mediaCache,undefined,fontCache),code('VISUAL_BUDGET_EXCEEDED'));
  assert.equal(pending.length,0);assert.equal(h.calls.length,0);assert.equal(canvases.length,0);assert.equal(gpuCalls.length,0);
  assert.equal(fontCache.has('inactive'),false);
});

test('media failure/cancellation waits for speculative hash cleanup and digest failure never reaches GPU',async t=>{
  const {pending,realm,workers}=gateFontHashes(t);
  for(const fault of ['media','cancel','digest']) {
    reset();const p=graphicTextPlan(),{mediaCache,fontCache}=cachedResources(p),h=graphicTextHost(p),callback=h.callback;
    const reason=Error(fault),controller=new AbortController();let settled=false;
    h.callback=async(op,args)=>{
      if(op==='admitDecode'&&fault!=='digest'){if(fault==='cancel')controller.abort(reason);throw reason;}
      return callback(op,args);
    };
    const rendering=renderClassicGraphicFrame(p,h,realm,controller.signal,mediaCache,undefined,fontCache);
    const rejected=assert.rejects(rendering,error=>fault==='digest'?error.code==='FONT_LOAD_FAILED':error===reason).then(()=>{settled=true;});
    await new Promise(resolve=>setImmediate(resolve));const gate=pending.at(-1);
    assert.equal(settled,fault!=='digest');assert.equal(gpuCalls.length,0);assert.equal(faces.size,0);
    if(fault==='digest')gate.reject(reason);else assert.equal(workers.at(-1).terminated,true);await rejected;
    assert.equal(settled,true);assert.equal(gpuCalls.length,0);assert.equal(h.chunks.length,0);released(h);
  }
});

test("preview empty gaps retain only the previous font set and ordinary eviction remains explicit",async()=>{
 const {retainTextFrameFonts}=await import('../src/text-frame.mjs');
 const a={fontId:'a',contentHash:'a'.repeat(64),size:'3'},b={fontId:'b',contentHash:'b'.repeat(64),size:'4'};
 const faceA={},faceB={};faces.add(faceA);faces.add(faceB);
 const cache=new Map([[`${a.fontId}:${a.contentHash}:${a.size}`,{bytes:new Uint8Array(3),face:faceA}]]);
 retainTextFrameFonts(cache,[],true);assert.equal(cache.size,1);assert.ok(faces.has(faceA));
 cache.set(`${b.fontId}:${b.contentHash}:${b.size}`,{bytes:new Uint8Array(4),face:faceB});
 retainTextFrameFonts(cache,[b],true);assert.equal(cache.size,1);assert.ok(!faces.has(faceA));
 retainTextFrameFonts(cache,[]);assert.equal(cache.size,0);assert.ok(!faces.has(faceB));
});
