import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import { BLEND_MODES } from "../src/blend-modes.mjs";
import { prepareGraphicSources, resolveGraphicSource, isRetainedGraphicSource } from "../src/graphic-source.mjs";

// Node cannot import the isolated browser's /renderer WGSL modules. Load the
// actual GPU orchestration with only shader imports replaced by labels; this
// checks command/uniform wiring, not shader pixels or GPU conformance.
const location = new URL("../src/gpu.mjs", import.meta.url);
const code = (await fs.readFile(location, "utf8")).replace(
  /from "([^"]+)"/g,
  (_, name) => {
    const resolved = name.startsWith("/renderer/vendor/")
      ? `data:text/javascript,${encodeURIComponent(`export default ${JSON.stringify(name)}`)}`
      : new URL(name, location).href;
    return `from ${JSON.stringify(resolved)}`;
  },
);
const { renderGpuFrame, closeGpuRealm } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`
);
const quad = {
  centerX: 1,
  centerY: 1,
  width: 2,
  height: 2,
  rotationRadians: 0,
  opacity: 0.5,
  flipX: false,
  flipY: false,
};

function fakeGpu() {
  const allocations = [],
    uploads = [],
    modes = [],
    modeBuffers = [],
    calls = [];
  const allocation = (value) => {
    const object = {
      ...value,
      destroyed: false,
      destroy() {
        this.destroyed = true;
      },
    };
    allocations.push(object);
    return object;
  };
  const device = {
    destroy() { calls.push("destroy"); },
    limits: { maxTextureDimension2D: 4096 },
    queue: {
      writeBuffer: (buffer, _offset, bytes) => {
        buffer.bytes = bytes.slice();
      },
      copyExternalImageToTexture: (...args) => uploads.push(args),
      submit: () => calls.push("submit"),
    },
    pushErrorScope() {},
    async popErrorScope() {
      return null;
    },
    createBuffer: (descriptor) => allocation(descriptor),
    createTexture: (descriptor) =>
      allocation({
        ...descriptor,
        createView() {
          return this;
        },
      }),
    createShaderModule: (descriptor) => descriptor,
    createRenderPipeline: (descriptor) => ({
      ...descriptor,
      getBindGroupLayout: (index) => ({ index }),
    }),
    createSampler: () => ({}),
    createBindGroup: (descriptor) => descriptor,
    createCommandEncoder: () => ({
      beginRenderPass: () => ({
        setPipeline() {},
        setVertexBuffer() {},
        draw() {},
        end() {},
        setBindGroup(index, group) {
          if (index === 2) {
            const buffer = group.entries[0].resource.buffer;
            modes.push(buffer.bytes[0]);
            modeBuffers.push(buffer);
            assert.deepEqual(Array.from(buffer.bytes).slice(1), [0, 0, 0]);
          }
        },
      }),
      copyTextureToTexture() {},
      finish: () => ({}),
    }),
  };
  const surface = {
    width: 2,
    height: 2,
    getContext: () => ({ configure() {}, getCurrentTexture: () => ({}) }),
  };
  return {
    realm: { device, observation: {} },
    surface,
    allocations,
    uploads,
    modes,
    modeBuffers,
    calls,
  };
}
async function execute(layers, gpu, budget = "8192", sourceMap, size = [2, 2], signal) {
  const saved = [globalThis.GPUBufferUsage, globalThis.GPUTextureUsage];
  globalThis.GPUBufferUsage = { VERTEX: 1, COPY_DST: 2, UNIFORM: 4 };
  globalThis.GPUTextureUsage = {
    TEXTURE_BINDING: 1,
    RENDER_ATTACHMENT: 2,
    COPY_SRC: 4,
    COPY_DST: 8,
  };
  const source = { width: 2, height: 2, canvas: {} };
  try {
    return await renderGpuFrame(
      { identity: { width: size[0], height: size[1] }, background: null, layers },
      sourceMap ?? new Map(layers.map((layer) => [layer.elementId, source])),
      { maxGpuBytes: budget },
      () => {
        throw Error("preview must not read back");
      },
      signal,
      gpu.realm,
      gpu.surface,
    );
  } finally {
    [globalThis.GPUBufferUsage, globalThis.GPUTextureUsage] = saved;
  }
}

test("one shared GPU pipeline binds all 17 shader codes in layer order and reuses mode buffers", async () => {
  const gpu = fakeGpu();
  const layers = [...BLEND_MODES, "multiply"].map((blendMode, index) => ({
    elementId: String(index),
    quad,
    blendMode,
  }));
  await execute(layers, gpu);
  assert.deepEqual(gpu.modes, [
    ...Array.from({ length: 17 }, (_, index) => index),
    2,
  ]);
  assert.equal(gpu.modeBuffers.at(-1), gpu.modeBuffers[2]);
  assert.equal(new Set(gpu.modeBuffers).size, 17);
  assert.equal(gpu.uploads.length, 1);
  assert.deepEqual(gpu.calls, ["submit"]);
  assert.equal(gpu.allocations.filter(value => !value.destroyed).length, 3);
  closeGpuRealm(gpu.realm);
  assert.ok(gpu.allocations.every((value) => value.destroyed));
});
test("old absent mode stays normal, null quad stays skipped, and unknown values fail before allocation", async () => {
  const gpu = fakeGpu();
  await execute(
    [
      { elementId: "normal", quad },
      { elementId: "skipped", quad: null, blendMode: "screen" },
    ],
    gpu,
  );
  assert.deepEqual(gpu.modes, [0]);
  for (const blendMode of [null, "unknown", 17]) {
    const bad = fakeGpu();
    await assert.rejects(
      execute([{ elementId: "bad", quad: null, blendMode }], bad),
      (error) => error.code === "INVALID_VISUAL_PLAN",
    );
    assert.equal(bad.allocations.length, 0);
    assert.equal(bad.calls.length, 0);
  }
});
test("additional blend uniforms are charged to GPU admission and released on failure", async () => {
  const normal = fakeGpu(),
    blended = fakeGpu();
  const old = await execute([{ elementId: "a", quad }], normal);
  const newer = await execute(
    [{ elementId: "a", quad, blendMode: "multiply" }],
    blended,
  );
  assert.equal(Number(newer.peakGpuBytes) - Number(old.peakGpuBytes), 16);
  const limited = fakeGpu();
  await assert.rejects(
    execute(
      [{ elementId: "a", quad, blendMode: "multiply" }],
      limited,
      old.peakGpuBytes,
    ),
    (error) => error.code === "VISUAL_BUDGET_EXCEEDED",
  );
  assert.equal(limited.calls.length, 0);
  assert.ok(limited.allocations.every((value) => value.destroyed));
});

function retainedSource(gpu, fill = [1, 0, 0, 1]) {
  const layer = { elementId: 'static', graphic: { sourceWidth: 2, sourceHeight: 2,
    geometry: { kind: 'rectangle', cornerRadiusPercent: 0 }, fill,
    stroke: { widthPx: 0, align: 'center', color: [0, 0, 0, 0] } } };
  const prepared = prepareGraphicSources(gpu.realm, {
    identity: { projectId: 'p', revision: 'r', sequenceId: 's' }, graphics: [layer],
  }, { maxDecodedPixels: '100', maxRgbaBytes: '4096' });
  return resolveGraphicSource(prepared, layer, { maxPixels: '100', maxRasterBytes: '4096' }, (width, height) => {
    const canvas = { width, height };
    canvas.getContext = () => ({ canvas, clearRect() {}, save() {}, restore() {}, translate() {}, fill() {} });
    return canvas;
  }, null, class { roundRect() {} });
}

test('only controlled static graphic sources skip upload; scratch targets are reused and warm bytes stay charged', async () => {
  const gpu = fakeGpu(), source = retainedSource(gpu);
  const mutable = { width: 2, height: 2, canvas: { width: 2, height: 2 }, borrowed: true, borrowedCanvas: true };
  assert.equal(isRetainedGraphicSource(mutable), false);
  const layers = [{ elementId: 'static', quad }, { elementId: 'video', quad, blendMode: 'screen' }];
  const sources = new Map([['static', source], ['video', mutable]]);
  const cold = await execute(layers, gpu, '8192', sources);
  const textures = gpu.allocations.filter(item => item.createView && !item.destroyed);
  assert.equal(textures.length, 4);
  const warm = await execute(layers, gpu, '8192', sources);
  assert.equal(warm.peakGpuBytes, cold.peakGpuBytes);
  assert.equal(gpu.uploads.filter(([input]) => input.source === source.canvas).length, 1);
  assert.equal(gpu.uploads.filter(([input]) => input.source === mutable.canvas).length, 2);
  assert.deepEqual(gpu.allocations.filter(item => item.createView && !item.destroyed), textures);
  closeGpuRealm(gpu.realm); closeGpuRealm(gpu.realm);
  assert.equal(source.canvas.width, 0);
  assert.ok(gpu.allocations.every(item => item.destroyed));
  assert.equal(gpu.calls.filter(call => call === 'destroy').length, 1);
  await assert.rejects(execute(layers, gpu, '8192', sources), error => error.code === 'VISUAL_GPU_FAILED');
});

test('changed raster, inactive source and resized frame evict only the obsolete GPU storage', async () => {
  const gpu = fakeGpu(), layers = [{ elementId: 'static', quad }];
  const first = retainedSource(gpu);
  await execute(layers, gpu, '8192', new Map([['static', first]]));
  const oldTexture = gpu.uploads[0][1].texture;
  const targets = gpu.allocations.filter(item => item.createView && !item.destroyed && item !== oldTexture);
  const second = retainedSource(gpu, [0, 1, 0, 1]);
  assert.equal(first.canvas.width, 0);
  await execute(layers, gpu, '8192', new Map([['static', second]]));
  assert.equal(oldTexture.destroyed, true);
  assert.ok(targets.every(item => !item.destroyed));
  const sourceTexture = gpu.uploads[1][1].texture;
  await execute(layers, gpu, '8192', new Map([['static', second]]), [4, 2]);
  assert.ok(targets.every(item => item.destroyed));
  assert.equal(sourceTexture.destroyed, false);
  await execute([], gpu, '8192', new Map(), [4, 2]);
  assert.equal(sourceTexture.destroyed, true);
  closeGpuRealm(gpu.realm);
  assert.ok(gpu.allocations.every(item => item.destroyed));
});

test('lower warm admission, cancellation and upload failure clear caches without destroying a reusable realm', async () => {
  for (const fault of ['budget', 'cancel', 'upload']) {
    const gpu = fakeGpu(), source = retainedSource(gpu), layers = [{ elementId: 'static', quad }];
    const sources = new Map([['static', source]]);
    const first = await execute(layers, gpu, '8192', sources);
    const reason = Error('cancelled cache frame');
    const upload = gpu.realm.device.queue.copyExternalImageToTexture;
    if (fault === 'upload') {
      sources.set('static', { width: 2, height: 2, canvas: {} });
      gpu.realm.device.queue.copyExternalImageToTexture = () => { throw reason; };
    }
    await assert.rejects(execute(layers, gpu, fault === 'budget' ? String(Number(first.peakGpuBytes) - 1) : '8192', sources,
      [2, 2], fault === 'cancel' ? { aborted: true, reason } : undefined),
      error => fault === 'budget' ? error.code === 'VISUAL_BUDGET_EXCEEDED' : error === reason);
    assert.ok(gpu.allocations.every(item => item.destroyed));
    assert.equal(source.canvas.width, 0);
    assert.equal(gpu.calls.includes('destroy'), false);
    gpu.realm.device.queue.copyExternalImageToTexture = upload;
    const retry = retainedSource(gpu);
    await execute(layers, gpu, '8192', new Map([['static', retry]]));
    assert.equal(retry.canvas.width, 2);
    closeGpuRealm(gpu.realm);
  }
});

test('malformed GPU admission on a warm realm releases existing textures and sources before retry',async()=>{
  const gpu=fakeGpu(),source=retainedSource(gpu),layers=[{elementId:'static',quad}];
  await execute(layers,gpu,'8192',new Map([['static',source]]));
  const allocationCount=gpu.allocations.length,submitCount=gpu.calls.length;
  await assert.rejects(execute(layers,gpu,'invalid',new Map([['static',source]])),error=>error.code==='INVALID_VISUAL_PLAN');
  assert.equal(gpu.allocations.length,allocationCount);assert.equal(gpu.calls.length,submitCount);
  assert.ok(gpu.allocations.every(item=>item.destroyed));assert.equal(source.canvas.width,0);assert.equal(gpu.calls.includes('destroy'),false);
  const retry=retainedSource(gpu);
  await execute(layers,gpu,'8192',new Map([['static',retry]]));
  assert.equal(retry.canvas.width,2);closeGpuRealm(gpu.realm);
});

test('pops both submitted GPU scopes before waiting and still rejects a late memory error', { timeout: 1000 }, async () => {
  const gpu = fakeGpu(); let calls = 0, validation;
  gpu.realm.device.popErrorScope = () => {
    if (++calls === 1) return new Promise(resolve => { validation = resolve; });
    validation(null); return Promise.resolve({ message: 'memory rejected' });
  };
  await assert.rejects(execute([{ elementId: 'layer', quad, blendMode: 'normal' }], gpu), error => error.code === 'VISUAL_GPU_FAILED');
  assert.equal(calls, 2);
  assert.ok(gpu.allocations.every(value => value.destroyed));
});
