import test from "node:test";
import assert from "node:assert/strict";
import { sameJsonFacts } from "../src/validation.mjs";
import { staticImageHeader, admitDimensions } from "../src/image-format.mjs";
import { validateBegin } from "../host/protocol.mjs";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { PACKAGE_ROOT as SHARED_ROOT } from "@opencut/classic-audio-renderer/internal-host-runtime";
import { moduleRegistry, SHADERS, PACKAGE_ROOT } from "../host/modules.mjs";
const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==","base64");
const hasCode=code=>error=>error.code===code;
test("static encoded image gate rejects APNG, unsupported containers and truncated chunks",()=>{
  assert.deepEqual(staticImageHeader(png),{mime:"image/png",width:1,height:1});
  const acTL=Buffer.alloc(20);acTL.writeUInt32BE(8);acTL.write("acTL",4);
  assert.throws(()=>staticImageHeader(Buffer.concat([png.subarray(0,33),acTL,png.subarray(33)])),hasCode("UNSUPPORTED_RENDER_FEATURE"));
  assert.throws(()=>staticImageHeader(Buffer.from("GIF89a")),hasCode("UNSUPPORTED_RENDER_FEATURE"));
  assert.throws(()=>staticImageHeader(Buffer.from("RIFFxxxxWEBP")),hasCode("UNSUPPORTED_RENDER_FEATURE"));
  assert.throws(()=>staticImageHeader(png.subarray(0,35)),hasCode("INVALID_DECODED_IMAGE"));
  const jpeg=Buffer.from([255,216,255,192,0,8,8,0,2,0,3,1,255,217]);
  assert.deepEqual(staticImageHeader(jpeg),{mime:"image/jpeg",width:3,height:2});
  const mpf=Buffer.from([255,226,0,6,77,80,70,0]);
  assert.throws(()=>staticImageHeader(Buffer.concat([jpeg.subarray(0,2),mpf,jpeg.subarray(2)])),hasCode("UNSUPPORTED_RENDER_FEATURE"));
});
test("actual dimensions use pixel and byte admission independently of authored metadata",()=>{
  const admission={maxWidth:16,maxHeight:16,maxPixels:"128",maxDecodedBytes:"2048"};
  admitDimensions(16,8,admission);
  assert.throws(()=>admitDimensions(16,9,admission),hasCode("VISUAL_BUDGET_EXCEEDED"));
  assert.throws(()=>admitDimensions(16,8,{...admission,maxDecodedBytes:"2047"}),hasCode("VISUAL_BUDGET_EXCEEDED"));
  assert.throws(()=>admitDimensions(0,8,admission),hasCode("INVALID_VISUAL_PLAN"));
});
const begin=()=>({protocolVersion:1,type:"begin",executionId:"fixed-frame",plan:{identity:{projectId:"p",revision:"r",sequenceId:"s",time:{ticks:"1",timebase:120000},width:16,height:16,profile:"classic-v31-gpu-normal-frame"},resources:[],layers:[]},resources:[],admission:{maxEncodedBytes:1048576,maxFramePixels:"256",maxDecodedPixels:"256",maxGpuBytes:"1048576",maxRgbaBytes:"1048576",maxOutputBytes:"1048576",maxRssBytes:"536870912",deadlineMs:1000,quotaProfile:"monitored-terminate"},runtime:{chromiumExecutable:"/host/configured/browser"}});
test("fixed plan validates private handles and visual quota instead of audio DTOs",()=>{
  assert.equal(validateBegin(begin()).admission.monitorIntervalMs,100);
  const b=begin();b.plan.resources.push({assetId:"image",contentHash:"0".repeat(64),size:"12",kind:"image"});
  assert.throws(()=>validateBegin(b),hasCode("INVALID_HOST_PROTOCOL"));
  b.resources.push({assetId:"image",handle:"verified-1"});assert.equal(validateBegin(b).plan.resources.length,1);
  b.plan.layers.push({elementId:"e",resource:{...b.plan.resources[0],size:"13"},decode:{kind:"image"}});
  assert.throws(()=>validateBegin(b),hasCode("INVALID_HOST_PROTOCOL"));
});
test("execution identity includes original shaders and shared Node controls but serves only browser modules",async()=>{
  const registry=await moduleRegistry({});assert.match(registry.moduleSha256,/^[a-f0-9]{64}$/);assert.match(registry.shaderSha256,/^[a-f0-9]{64}$/);
  for(const shader of SHADERS)assert.match(registry.registry.get("/renderer/"+shader).toString(),/^export default /);
  // Compute the protocol's canonical digest from independent on-disk input
  // sets, including hash-only Node controls. Omitting a control from production
  // leaves served modules unchanged and must still fail this identity check.
  const inputs=new Map();
  const capture=async(root,prefix,relative)=>inputs.set(prefix+relative,await fs.readFile(path.join(root,relative)));
  for(const folder of ["src","host"])for(const file of await fs.readdir(path.join(PACKAGE_ROOT,folder)))if(file.endsWith(".mjs"))await capture(PACKAGE_ROOT,"/renderer/",folder+"/"+file);
  for(const file of SHADERS)await capture(PACKAGE_ROOT,"/renderer/",file);
  for(const file of await fs.readdir(path.join(SHARED_ROOT,"host")))if(file.endsWith(".mjs"))await capture(SHARED_ROOT,"/shared-host/","host/"+file);
  await capture(SHARED_ROOT,"/shared-host/","src/validation.mjs");
  const media=await fs.realpath(path.join(PACKAGE_ROOT,"node_modules/mediabunny"));
  await capture(media,"/dependencies/mediabunny/","dist/bundles/mediabunny.mjs");
  const digest=createHash("sha256");for(const [name,bytes] of [...inputs].sort(([a],[b])=>a.localeCompare(b))){digest.update(name);digest.update("\0");digest.update(bytes);digest.update("\0");}
  assert.equal(registry.moduleSha256,digest.digest("hex"));
  assert.equal(registry.registry.has("/renderer/host/runner.mjs"),false);assert.equal(registry.registry.has("/shared-host/host/runner.mjs"),false);
  assert.match(registry.csp,/connect-src 'none'/);assert.match(registry.csp,/img-src blob:/);
});

test("Rust object key sorting preserves facts while decoded array order and values remain exact",()=>{
  const browser=[{kind:"readyVideo",elementId:"video",width:16,height:8,requestedTime:{ticks:"77143",timebase:120000},actualTimestampSeconds:0.5,frameDurationSeconds:0.5},{kind:"readyImage",elementId:"image",width:2,height:1}];
  const rust=[{actualTimestampSeconds:0.5,elementId:"video",frameDurationSeconds:0.5,height:8,kind:"readyVideo",requestedTime:{timebase:120000,ticks:"77143"},width:16},{elementId:"image",height:1,kind:"readyImage",width:2}];
  assert.equal(sameJsonFacts(browser,rust),true);
  assert.equal(sameJsonFacts(browser,[rust[1],rust[0]]),false);
  const changed=structuredClone(rust);changed[0].requestedTime.ticks="77144";
  assert.equal(sameJsonFacts(browser,changed),false);
  assert.equal(sameJsonFacts({ticks:"1"},{ticks:1}),false);
});
