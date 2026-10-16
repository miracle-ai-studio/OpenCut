import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { maskFactV1 } from "./mask-fixture.mjs";
import { prepareMaskFrameSourcesV1 } from "../src/mask-source-v1.mjs";
import { prepareSceneSourcesV1 } from "../src/scene-source-v1.mjs";
import { byteBudget } from "../src/validation.mjs";

const location = new URL("../src/gpu.mjs", import.meta.url);
const code = (await fs.readFile(location, "utf8")).replace(/from "([^"]+)"/g, (_, name) => {
  const resolved = name.startsWith("/renderer/vendor/") ? `data:text/javascript,${encodeURIComponent(`export default ${JSON.stringify(name)}`)}` : new URL(name, location).href;
  return `from ${JSON.stringify(resolved)}`;
});
const { renderGpuFrame, closeGpuRealm } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const quad = { centerX: 4, centerY: 2, width: 8, height: 4, rotationRadians: 0, opacity: 0.5, flipX: false, flipY: false };
const identity = { projectId: "p", revision: "r", sequenceId: "s", profile: "classic-v31-gpu-mask-frame-v1", width: 8, height: 4, time: { ticks: "0", timebase: 30 } };
function gpuFixture() {
  const allocations = [], uploads = [], passes = [], canvases = [], calls = [], hitCopies = [];
  const allocate = data => { const item = { ...data, destroyed: false, destroy() { this.destroyed = true; } }; allocations.push(item); return item; };
  const device = {
    limits: { maxTextureDimension2D: 4096 }, destroy() { calls.push("destroy"); }, pushErrorScope() {}, async popErrorScope() { return null; },
    queue: { writeBuffer(buffer, _offset, data) { buffer.bytes = data.slice(); }, copyExternalImageToTexture(...args) { uploads.push(args); }, submit() { calls.push("submit"); } },
    createBuffer: descriptor => allocate({ ...descriptor, mapState: "unmapped", async mapAsync() {this.mapState="mapped";}, unmap(){this.mapState="unmapped";}, getMappedRange(){return this.readback??new ArrayBuffer(this.size);} }), createTexture: descriptor => allocate({ ...descriptor, createView() { return this; } }),
    createShaderModule: descriptor => descriptor, createRenderPipeline: descriptor => ({ ...descriptor, getBindGroupLayout: index => ({ index }) }),
    createSampler: descriptor => descriptor, createBindGroup: descriptor => descriptor,
    createCommandEncoder: () => ({
      beginRenderPass(descriptor) {
        const pass = { descriptor, groups: [], pipeline: null }; passes.push(pass);
        return { setPipeline(pipeline) { pass.pipeline = pipeline; }, setVertexBuffer() {}, setBindGroup(index, binding) { pass.groups[index] = binding; }, draw() {},
          end() {
            // WebGPU forbids sampling an attachment in the same render pass.
            for (const group of pass.groups) for (const entry of group.entries) if (entry.resource?.createView) assert.notEqual(entry.resource, descriptor.colorAttachments[0].view);
          } };
      }, copyTextureToTexture() {}, copyTextureToBuffer(source,destination,size) {hitCopies.push({source,destination,size});destination.buffer.readback??=new ArrayBuffer(destination.buffer.size);new Uint8Array(destination.buffer.readback)[destination.offset+3]=[255,0,128][hitCopies.length-1]??255;}, finish: () => ({}),
    }),
  };
  const createCanvas = (width, height) => {
    const canvas = { width, height }; canvases.push(canvas);
    canvas.getContext = () => ({ canvas, fill() {}, save() {}, restore() {}, translate() {}, rotate() {}, scale() {}, drawImage() {} }); return canvas;
  };
  class Path { moveTo() {} lineTo() {} closePath() {} ellipse() {} }
  const surface = { width: 8, height: 4, getContext: () => ({ configure() {}, getCurrentTexture: () => ({}) }) };
  return { realm: { device, observation: {} }, surface, hitCopies, allocations, uploads, passes, canvases, calls, createCanvas, Path };
}
function frame(feathers = [4]) {
  const masks = feathers.map((value, i) => ({ elementId: String(i), mask: maskFactV1(i % 2 ? "ellipse" : "rectangle", value), quad: { ...quad } }));
  return { identity: structuredClone(identity), background: null, masks, layers: masks.map(entry => ({ elementId: entry.elementId, quad: { ...quad }, blendMode: "multiply" })) };
}
async function execute(gpu, fixed, options = {}) {
  const saved = [globalThis.GPUBufferUsage, globalThis.GPUTextureUsage,globalThis.GPUMapMode];
  globalThis.GPUBufferUsage = { VERTEX: 1, COPY_DST: 2, UNIFORM: 4,MAP_READ:8 };
  globalThis.GPUMapMode={READ:1};
  globalThis.GPUTextureUsage = { TEXTURE_BINDING: 1, RENDER_ATTACHMENT: 2, COPY_SRC: 4, COPY_DST: 8 };
  const admission = { maxGpuBytes: options.budget ?? "100000", maxRgbaBytes: "100000", maxDecodedPixels: "10000" };
  const source = { width: 8, height: 4, canvas: {} }, sources = new Map(fixed.layers.map(layer => [layer.elementId, source]));
  const state = { budget: byteBudget(admission.maxRgbaBytes), releases: [], sources, retainedDecoded: 0 };
  try {
    if (fixed.scene !== undefined) prepareSceneSourcesV1(fixed, state, admission, options.signal, gpu.createCanvas, gpu.Path);
    if (fixed.masks !== undefined) prepareMaskFrameSourcesV1(fixed, state, admission, options.signal, gpu.createCanvas, gpu.Path);
    return await renderGpuFrame(fixed, sources, admission, () => { throw Error("no readback"); }, options.signal, gpu.realm, gpu.surface,options.hitQuery);
  } finally { state.releases.forEach(release => release()); [globalThis.GPUBufferUsage, globalThis.GPUTextureUsage,globalThis.GPUMapMode] = saved; }
}
const shader = pass => pass.pipeline?.fragment.module.code.split("/").at(-1);
const uniforms = (pass, index) => Array.from(pass.groups[index].entries[0].resource.buffer.bytes);
test("Classic JFA runs both nearest-sampled seed sets, descending steps, half-feather distance, then mask before blend", async () => {
  const gpu = gpuFixture(); await execute(gpu, frame());
  assert.deepEqual(gpu.passes.map(shader).filter(Boolean), ["jfa_init.wgsl", "jfa_step.wgsl", "jfa_step.wgsl", "jfa_step.wgsl", "jfa_init.wgsl", "jfa_step.wgsl", "jfa_step.wgsl", "jfa_step.wgsl", "jfa_distance.wgsl", "layer.wgsl", "mask.wgsl", "blend.wgsl"]);
  assert.deepEqual(gpu.passes.filter(p => shader(p) === "jfa_init.wgsl").map(p => uniforms(p, 1)), [[8, 4, 0, 0], [8, 4, 1, 0]]);
  assert.deepEqual(gpu.passes.filter(p => shader(p) === "jfa_step.wgsl").map(p => uniforms(p, 1)[2]), [4, 2, 1, 4, 2, 1]);
  const distance = gpu.passes.find(p => shader(p) === "jfa_distance.wgsl"); assert.deepEqual(uniforms(distance, 2), [8, 4, 2, 0]);
  for (const pass of gpu.passes.filter(p => shader(p)?.startsWith("jfa_"))) for (const group of pass.groups) {
    const sampler = group.entries.find(entry => entry.binding === 1)?.resource; if (sampler) assert.equal(sampler.magFilter, "nearest");
  }
  const apply = gpu.passes.find(p => shader(p) === "mask.wgsl"), mix = gpu.passes.find(p => shader(p) === "blend.wgsl");
  assert.deepEqual(uniforms(apply, 2), [0, 0, 0, 0]); assert.equal(mix.groups[1].entries[0].resource, apply.descriptor.colorAttachments[0].view);
  assert.equal(gpu.uploads[1][1].premultipliedAlpha, false); assert.equal(gpu.uploads[1][1].texture, distance.descriptor.colorAttachments[0].view);
  assert.ok(gpu.canvases.every(canvas => canvas.width === 0 && canvas.height === 0));
  assert.equal(gpu.allocations.filter(item => item.createView && !item.destroyed).length, 7); closeGpuRealm(gpu.realm); assert.ok(gpu.allocations.every(item => item.destroyed));
});
test("warm static mask skips raster/upload/JFA while still binding mask and blend with retained storage charged", async () => {
  const gpu = gpuFixture(), cold = frame(); await execute(gpu, cold);
  const allocations = gpu.allocations.length, passes = gpu.passes.length, canvases = gpu.canvases.length;
  const next = frame(); next.identity.time.ticks = "1";
  const warm = await execute(gpu, next);
  assert.deepEqual(gpu.passes.slice(passes).map(shader).filter(Boolean), ["layer.wgsl", "mask.wgsl", "blend.wgsl"]);
  assert.equal(gpu.canvases.length, canvases); assert.equal(gpu.uploads.length, 3); // Mutable image always uploads.
  assert.equal(gpu.allocations.slice(allocations).filter(item => item.createView).length, 1);
  assert.ok(Number(warm.peakGpuBytes) >= 7 * 128);
  await assert.rejects(execute(gpu, frame(), { budget: String(7 * 128 - 1) }), error => error.code === "VISUAL_BUDGET_EXCEEDED");
  assert.ok(gpu.allocations.every(item => item.destroyed)); assert.equal(gpu.calls.includes("destroy"), false);
  await execute(gpu, frame()); closeGpuRealm(gpu.realm);
});
test("multiple masks share exactly three scratch targets; edits/revisions/old-profile frames evict obsolete masks", async () => {
  const gpu = gpuFixture(); await execute(gpu, frame([4, 8]));
  assert.equal(gpu.allocations.filter(item => item.createView && !item.destroyed).length, 8);
  const oldMasks = gpu.uploads.slice(1).map(args => args[1].texture);
  const changed = frame([4, 8]); changed.masks[0].quad.centerX++;
  await execute(gpu, changed); assert.equal(oldMasks[0].destroyed, true); assert.equal(oldMasks[1].destroyed, false);
  const revision = frame([4, 8]); revision.identity.revision = "next"; await execute(gpu, revision); assert.equal(oldMasks[1].destroyed, true);
  const plain = frame([]); delete plain.masks; plain.identity.profile = "classic-v31-gpu-normal-frame"; await execute(gpu, plain);
  assert.equal(gpu.allocations.filter(item => item.createView && !item.destroyed).length, 3); closeGpuRealm(gpu.realm);
});
test("hard masks allocate one scratch and use raw antialias alpha; disabled quads allocate no masks", async () => {
  const gpu = gpuFixture(); await execute(gpu, frame([0]));
  assert.deepEqual(gpu.passes.map(shader).filter(Boolean), ["layer.wgsl", "mask.wgsl", "blend.wgsl"]);
  assert.equal(gpu.allocations.filter(item => item.createView && !item.destroyed).length, 5);
  const previousPasses = gpu.passes.length;
  await execute(gpu, frame([1e-100])); // Classic descriptor narrows this to f32 zero.
  assert.deepEqual(gpu.passes.slice(previousPasses).map(shader).filter(Boolean), ["layer.wgsl", "mask.wgsl", "blend.wgsl"]);
  const skipped = frame(); skipped.layers[0].quad = skipped.masks[0].quad = null; const count = gpu.canvases.length;
  await execute(gpu, skipped); assert.equal(gpu.canvases.length, count); assert.equal(gpu.allocations.filter(item => item.createView && !item.destroyed).length, 3); closeGpuRealm(gpu.realm);
});
test("upload failure or cancellation destroys warm masks and partial scratch but permits realm retry", async () => {
  for (const fault of ["upload", "cancel"]) {
    const gpu = gpuFixture(); await execute(gpu, frame());
    const original = gpu.realm.device.queue.copyExternalImageToTexture, abort = new AbortController(), reason = Error(fault);
    gpu.realm.device.queue.copyExternalImageToTexture = (...args) => { if (args[0].source.width) { if (fault === "upload") throw reason; abort.abort(reason); } original(...args); };
    const changed = frame([8]);
    await assert.rejects(execute(gpu, changed, { signal: abort.signal }), error => error === reason);
    assert.ok(gpu.allocations.every(item => item.destroyed)); assert.ok(gpu.canvases.every(canvas => canvas.width === 0)); assert.equal(gpu.calls.includes("destroy"), false);
    gpu.realm.device.queue.copyExternalImageToTexture = original; await execute(gpu, frame()); closeGpuRealm(gpu.realm); closeGpuRealm(gpu.realm);
    assert.equal(gpu.calls.filter(call => call === "destroy").length, 1);
  }
});

test("scene composes effects before intersecting multiple masks and an adjustment track before the next layer", async () => {
  const gpu=gpuFixture(),fixed=frame([4]);delete fixed.masks;
  fixed.identity.profile="classic-v31-gpu-scene-frame-v1";
  const first=maskFactV1("rectangle",4),second=maskFactV1("ellipse",2);first.id="first";second.id="second";second.inverted=true;
  const blur={id:"blur",enabled:true,effect:{kind:"classic_blur",intensity:100}};
  fixed.scene={layers:[{stack:{elementId:"0",masks:[first,second],effects:[blur]},quad:{...quad}}],steps:[{kind:"layer",element_id:"0"},{kind:"effect",element_id:"adjustment",effects:[{...blur,id:"adjustment-blur"}]}],background:{kind:"color",color:{encoding:"srgb",red:0,green:0,blue:0,alpha:1}}};
  await execute(gpu,fixed);
  const shaders=gpu.passes.map(shader).filter(Boolean),apply=gpu.passes.filter(p=>shader(p)==="mask.wgsl");
  assert.deepEqual(apply.map(p=>uniforms(p,2)[0]),[0,1]);
  assert.equal(shaders.filter(s=>s==="gaussian_blur.wgsl").length,4);
  assert.ok(shaders.indexOf("gaussian_blur.wgsl")<shaders.indexOf("mask.wgsl"));
  assert.ok(shaders.lastIndexOf("gaussian_blur.wgsl")>shaders.lastIndexOf("blend.wgsl"));
  assert.equal(apply[1].groups[0].entries[0].resource,apply[0].descriptor.colorAttachments[0].view);
  closeGpuRealm(gpu.realm);assert.ok(gpu.allocations.every(item=>item.destroyed));
});

test("content hit copies each actual masked foreground texel, skips transparency, and reports front to back", async () => {
  const gpu=gpuFixture(),fixed=frame([0,0,0]);
  const result=await execute(gpu,fixed,{hitQuery:{x:2,y:1}});
  assert.deepEqual(result.hitTest,{identity:fixed.identity,point:{x:2,y:1},hits:[{elementId:"2",alpha:128},{elementId:"0",alpha:255}]});
  assert.equal(gpu.hitCopies.length,3);
  for(const [i,copy]of gpu.hitCopies.entries()){
    assert.deepEqual(copy.source.origin,{x:2,y:1,z:0});
    assert.deepEqual(copy.size,[1,1]);assert.equal(copy.destination.offset,i*256);
    const pass=gpu.passes.filter(p=>shader(p)==="mask.wgsl")[i];
    assert.equal(copy.source.texture,pass.descriptor.colorAttachments[0].view);
  }
  const readback=gpu.allocations.find(item=>item.readback);
  assert.equal(readback.destroyed,true);assert.equal(readback.mapState,"unmapped");
  closeGpuRealm(gpu.realm);
});
