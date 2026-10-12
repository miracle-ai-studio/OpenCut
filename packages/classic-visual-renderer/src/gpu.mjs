import vertexSource from "/renderer/vendor/classic-v31/fullscreen.wgsl";
import layerSource from "/renderer/vendor/classic-v31/layer.wgsl";
import blendSource from "/renderer/vendor/classic-v31/blend.wgsl";
import { byteBudget, checkAbort, finite, integer, reject } from "./validation.mjs";

const FORMAT = "bgra8unorm";
const QUAD = new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]);
function uniformBytes(quad, canvas) {
  // Exact Classic LayerUniforms layout: 10 floats plus 8-byte padding.
  return new Float32Array([canvas.width, canvas.height, quad.centerX, quad.centerY,
    quad.width, quad.height, quad.rotationRadians, quad.opacity,
    quad.flipX ? 1 : 0, quad.flipY ? 1 : 0, 0, 0]);
}
export async function openGpuRealm() {
  if (!globalThis.navigator?.gpu) reject("CAPABILITY_UNAVAILABLE", "WebGPU is unavailable in the admitted browser");
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) reject("CAPABILITY_UNAVAILABLE", "No WebGPU adapter is available");
  const info = adapter.info ?? await adapter.requestAdapterInfo?.() ?? {};
  const observedDevice = JSON.stringify({ vendor: info.vendor ?? "", architecture: info.architecture ?? "", device: info.device ?? "", description: info.description ?? "", ...(typeof adapter.isFallbackAdapter === "boolean" ? { fallback: adapter.isFallbackAdapter } : {}) });
  const device = await adapter.requestDevice();
  return { device, observation: { gpuBackend: "webgpu", gpuDevice: observedDevice, textureFormat: FORMAT } };
}
export async function renderGpuFrame(frame, sources, admission, emit, signal, realm, surface) {
  checkAbort(signal);
  const owned = !realm;
  realm ??= await openGpuRealm();
  const { device } = realm;
  const budget = byteBudget(admission.maxGpuBytes), allocated = [], buffers = [], releases = [];
  let readback; const canvases = [];
  let scopes = 0;
  try {
    checkAbort(signal);
    const { width, height } = frame.identity;
    integer(width, "canvas width", 1, device.limits.maxTextureDimension2D);
    integer(height, "canvas height", 1, device.limits.maxTextureDimension2D);
    device.pushErrorScope("out-of-memory"); device.pushErrorScope("validation"); scopes = 2;
    releases.push(budget.reserve(QUAD.byteLength));
    const vertices = device.createBuffer({ size: QUAD.byteLength, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST });
    buffers.push(vertices); device.queue.writeBuffer(vertices, 0, QUAD);
    if(!realm.pipelines) {
      const vertex = device.createShaderModule({ code: vertexSource });
      const layer = device.createShaderModule({ code: layerSource });
      const blend = device.createShaderModule({ code: blendSource });
      const pipeline = module => device.createRenderPipeline({ layout: "auto", vertex: { module: vertex, entryPoint: "vertex_main", buffers: [{ arrayStride: 8, attributes: [{ shaderLocation: 0, offset: 0, format: "float32x2" }] }] }, fragment: { module, entryPoint: "fragment_main", targets: [{ format: FORMAT }] }, primitive: { topology: "triangle-list" } });
      const layerPipeline = pipeline(layer), blendPipeline = pipeline(blend);
      const sampler = device.createSampler({ addressModeU: "clamp-to-edge", addressModeV: "clamp-to-edge", magFilter: "linear", minFilter: "linear", mipmapFilter: "nearest" });
      realm.pipelines={layerPipeline,blendPipeline,sampler};
    }
    const {layerPipeline,blendPipeline,sampler}=realm.pipelines;
    function texture(w, h) {
      integer(w, "texture width", 1, device.limits.maxTextureDimension2D); integer(h, "texture height", 1, device.limits.maxTextureDimension2D);
      releases.push(budget.reserve(w * h * 4));
      const result = device.createTexture({ size: [w, h], format: FORMAT, usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC | GPUTextureUsage.COPY_DST });
      allocated.push(result); return result;
    }
    function group(p, index, tex) { return device.createBindGroup({ layout: p.getBindGroupLayout(index), entries: [{ binding: 0, resource: tex.createView() }, { binding: 1, resource: sampler }] }); }
    function uniforms(p, index, bytes) {
      releases.push(budget.reserve(bytes.byteLength));
      const result = device.createBuffer({ size: bytes.byteLength, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
      buffers.push(result); device.queue.writeBuffer(result, 0, bytes);
      return device.createBindGroup({ layout: p.getBindGroupLayout(index), entries: [{ binding: 0, resource: { buffer: result } }] });
    }
    let scene = texture(width, height), nextScene = texture(width, height);
    const target = texture(width, height);
    const encoder = device.createCommandEncoder();
    const clear = encoder.beginRenderPass({ colorAttachments: [{ view: scene.createView(), loadOp: "clear", storeOp: "store", clearValue: { r: 0, g: 0, b: 0, a: 1 } }] }); clear.end();
    const normal = uniforms(blendPipeline, 2, new Uint32Array([0, 0, 0, 0]));
    const items = [];
    if (frame.background) {
      const bg = texture(1, 1), color = frame.background;
      // Match Classic ColorNode: Canvas fill then unpremultiplied external
      // upload. Straight byte writes would bypass premul/unpremul quantization.
      color.forEach(v => finite(v, "background component"));
      const backgroundCanvas = new OffscreenCanvas(1, 1), context = backgroundCanvas.getContext("2d");
      if (!context) reject("VISUAL_GPU_FAILED", "Background source canvas is unavailable");
      context.fillStyle = `rgba(${color[0] * 255}, ${color[1] * 255}, ${color[2] * 255}, ${color[3]})`;
      context.fillRect(0, 0, 1, 1);
      device.queue.copyExternalImageToTexture({ source: backgroundCanvas, flipY: false }, { texture: bg, colorSpace: "srgb", premultipliedAlpha: false }, [1, 1]);
      canvases.push(backgroundCanvas);
      items.push({ texture: bg, quad: { centerX: width / 2, centerY: height / 2, width, height, rotationRadians: 0, opacity: 1, flipX: false, flipY: false } });
    }
    const uploaded = new Map();
    for (const layer of frame.layers) {
      checkAbort(signal);
      if (layer.quad === null) continue;
      const quad = layer.quad;
      const source = sources.get(layer.elementId);
      if (!source) reject("INVALID_VISUAL_PLAN", "Bound quad has no decoded source");
      for (const key of ["centerX", "centerY", "width", "height", "rotationRadians", "opacity"]) finite(quad[key], key);
      if (quad.width <= 0 || quad.height <= 0 || typeof quad.flipX !== "boolean" || typeof quad.flipY !== "boolean") reject("UNSUPPORTED_RENDER_FEATURE", "Degenerate visual quads are unsupported");
      let tex = uploaded.get(source);
      if (!tex) {
        tex = texture(source.width, source.height);
        device.queue.copyExternalImageToTexture({ source: source.canvas, flipY: false }, { texture: tex, colorSpace: "srgb", premultipliedAlpha: false }, [source.width, source.height]);
        uploaded.set(source, tex);
      }
      items.push({ texture: tex, quad });
    }
    for (const item of items) {
      checkAbort(signal);
      const pass = encoder.beginRenderPass({ colorAttachments: [{ view: target.createView(), loadOp: "clear", storeOp: "store", clearValue: { r: 0, g: 0, b: 0, a: 0 } }] });
      pass.setPipeline(layerPipeline); pass.setVertexBuffer(0, vertices);
      pass.setBindGroup(0, group(layerPipeline, 0, item.texture)); pass.setBindGroup(1, uniforms(layerPipeline, 1, uniformBytes(item.quad, frame.identity)));
      pass.draw(6); pass.end();
      const mix = encoder.beginRenderPass({ colorAttachments: [{ view: nextScene.createView(), loadOp: "clear", storeOp: "store", clearValue: { r: 0, g: 0, b: 0, a: 0 } }] });
      mix.setPipeline(blendPipeline); mix.setVertexBuffer(0, vertices);
      mix.setBindGroup(0, group(blendPipeline, 0, scene)); mix.setBindGroup(1, group(blendPipeline, 1, target)); mix.setBindGroup(2, normal);
      mix.draw(6); mix.end(); [scene, nextScene] = [nextScene, scene];
    }
    if (surface) {
      releases.push(budget.reserve(width * height * 4));
      const changed = realm.previewSurface !== surface || surface.width !== width || surface.height !== height;
      if (surface.width !== width) surface.width = width;
      if (surface.height !== height) surface.height = height;
      const context = surface.getContext("webgpu");
      if (!context) reject("VISUAL_GPU_FAILED", "Preview GPU surface is unavailable");
      if (changed) {
        context.configure({ device, format: FORMAT, alphaMode: "opaque", usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.RENDER_ATTACHMENT });
        realm.previewSurface = surface;
      }
      encoder.copyTextureToTexture({ texture: scene }, { texture: context.getCurrentTexture() }, [width, height]);
      device.queue.submit([encoder.finish()]);
      const validationFailure = await device.popErrorScope(); scopes--;
      const memoryFailure = await device.popErrorScope(); scopes--;
      if (validationFailure || memoryFailure) reject("VISUAL_GPU_FAILED", "GPU rejected preview presentation");
      checkAbort(signal);
      return { ...realm.observation, peakGpuBytes: String(budget.peak) };
    }
    const rowBytes = width * 4, aligned = Math.ceil(rowBytes / 256) * 256;
    releases.push(budget.reserve(aligned * height));
    readback = device.createBuffer({ size: aligned * height, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ }); buffers.push(readback);
    encoder.copyTextureToBuffer({ texture: scene }, { buffer: readback, bytesPerRow: aligned, rowsPerImage: height }, [width, height]);
    device.queue.submit([encoder.finish()]);
    const validationFailure = await device.popErrorScope(); scopes--;
    const memoryFailure = await device.popErrorScope(); scopes--;
    if (validationFailure || memoryFailure) reject("VISUAL_GPU_FAILED", "GPU rejected the compiled frame execution");
    await readback.mapAsync(GPUMapMode.READ); checkAbort(signal);
    const bytes = new Uint32Array(readback.getMappedRange());
    // BGRA -> RGBA swaps the red/blue lanes without four scalar byte writes
    // per pixel. Both the padded rows and emitted chunks remain exact.
    let offset = 0; const chunk = new Uint8Array(Math.min(65536, rowBytes * height));
    const words = new Uint32Array(chunk.buffer); let fill = 0;
    for (let row = 0; row < height; row++) {
      for (let x = 0; x < width; x++) {
        const pixel = bytes[row * aligned / 4 + x];
        words[fill / 4] = (pixel & 0xff00ff00) | ((pixel & 0xff) << 16) | ((pixel >>> 16) & 0xff);
        fill += 4;
        if (fill === chunk.length) { checkAbort(signal); await emit(offset, chunk); offset += fill; fill = 0; }
      }
    }
    if (fill) { checkAbort(signal); await emit(offset, chunk.subarray(0, fill)); }
    readback.unmap();
    return { ...realm.observation, peakGpuBytes: String(budget.peak) };
  } finally {
    if (readback?.mapState === "mapped") readback.unmap();
    for (const b of buffers) b.destroy(); for (const t of allocated) t.destroy();
    for (const canvas of canvases) { canvas.width = 0; canvas.height = 0; }
    while (scopes-- > 0) await device.popErrorScope().catch(() => undefined);
    for (const release of releases) release(); if (owned) device.destroy();
  }
}
