import { checkAbort, finite, integer, natural, reject, sameJsonFacts } from "./validation.mjs";

// Adapted from Classic cf5e79e (MIT): masks/builtin/box-like.ts,
// definitions/{rectangle,ellipse}.ts and compositor/frame-descriptor.ts.
// Scale is intentionally retained but unused by Classic's box-like geometry.
export function exactMaskKeys(value, keys) {
  return value && !Array.isArray(value) && sameJsonFacts(Object.keys(value).sort(), [...keys].sort());
}
function canvasNumber(value, name) {
  // Raw MaskV4 geometry and Canvas2D paths are doubles. In particular the
  // unused Classic scale must not acquire a new f32 limit in this adapter.
  if (typeof value !== "number" || !Number.isFinite(value)) reject("INVALID_VISUAL_PLAN", `Invalid ${name}`);
  return value;
}
export function validateMask(mask) {
  if (mask === null) return;
  if (!exactMaskKeys(mask, ["id", "enabled", "shape", "inverted", "feather", "stroke"]) ||
      typeof mask.id !== "string" || !mask.id || mask.id.length > 1024 || mask.enabled !== true || mask.inverted !== false)
    reject("INVALID_VISUAL_PLAN", "Mask frame requires one enabled, non-inverted mask");
  if (!exactMaskKeys(mask.shape, ["kind", "geometry"]) || !["rectangle", "ellipse"].includes(mask.shape.kind))
    reject("INVALID_VISUAL_PLAN", "Mask shape is outside the fixed rectangle/ellipse profile");
  const g = mask.shape.geometry;
  if (!exactMaskKeys(g, ["center_x", "center_y", "width", "height", "rotation_degrees", "scale"]))
    reject("INVALID_VISUAL_PLAN", "Invalid box mask geometry");
  for (const [key, value] of Object.entries(g)) canvasNumber(value, `mask ${key}`);
  if (g.width <= 0 || g.height <= 0 || g.scale < 0)
    reject("INVALID_VISUAL_PLAN", "Invalid box mask dimensions");
  if (!exactMaskKeys(mask.feather, ["unit", "value"]) || mask.feather.unit !== "classic_pixels" || finite(mask.feather.value, "mask feather") < 0)
    reject("INVALID_VISUAL_PLAN", "Mask feather must use nonnegative Classic pixels");
  const feather = Math.fround(mask.feather.value);
  if (feather > 0 && Math.fround(feather / 2) === 0)
    reject("INVALID_VISUAL_PLAN", "Mask feather radius underflows the Classic shader");
  const stroke = mask.stroke, color = stroke?.color;
  if (!exactMaskKeys(stroke, ["color", "width_px", "align"]) || stroke.width_px !== 0 || !["inside", "center", "outside"].includes(stroke.align) ||
      !exactMaskKeys(color, ["encoding", "red", "green", "blue", "alpha"]) || color.encoding !== "srgb")
    reject("INVALID_VISUAL_PLAN", "Mask strokes are outside this profile");
  for (const key of ["red", "green", "blue", "alpha"]) if (finite(color[key], `mask stroke ${key}`) < 0 || color[key] > 1)
    reject("INVALID_VISUAL_PLAN", "Invalid mask stroke color");
}
export function validateMaskQuad(quad) {
  if (quad === null) return;
  if (!exactMaskKeys(quad, ["centerX", "centerY", "width", "height", "rotationRadians", "opacity", "flipX", "flipY"]) ||
      typeof quad.flipX !== "boolean" || typeof quad.flipY !== "boolean") reject("INVALID_VISUAL_PLAN", "Invalid logical mask quad");
  for (const key of ["centerX", "centerY", "width", "height", "rotationRadians", "opacity"]) canvasNumber(quad[key], `mask quad ${key}`);
  if (quad.width <= 0 || quad.height <= 0 || quad.opacity < 0 || quad.opacity > 1) reject("INVALID_VISUAL_PLAN", "Invalid logical mask extent or opacity");
}
export function maskPath(mask, width, height) {
  validateMask(mask);
  if (mask === null) reject("INVALID_VISUAL_PLAN", "Missing mask path");
  const g = mask.shape.geometry;
  const centerX = width / 2 + g.center_x * width, centerY = height / 2 + g.center_y * height;
  const halfWidth = Math.max(g.width, 0.01) * width / 2, halfHeight = Math.max(g.height, 0.01) * height / 2;
  const rotation = g.rotation_degrees * Math.PI / 180;
  for (const value of [centerX, centerY, halfWidth, halfHeight, rotation]) canvasNumber(value, "resolved mask geometry");
  if (mask.shape.kind === "ellipse") return [["ellipse", centerX, centerY, halfWidth, halfHeight, rotation, 0, Math.PI * 2]];
  const cos = Math.cos(rotation), sin = Math.sin(rotation);
  const corners = [[centerX - halfWidth, centerY - halfHeight], [centerX + halfWidth, centerY - halfHeight], [centerX + halfWidth, centerY + halfHeight], [centerX - halfWidth, centerY + halfHeight]];
  return [...corners.map(([x, y], index) => {
    const dx = x - centerX, dy = y - centerY;
    return [index ? "lineTo" : "moveTo", canvasNumber(centerX + dx * cos - dy * sin, "mask corner x"), canvasNumber(centerY + dx * sin + dy * cos, "mask corner y")];
  }), ["closePath"]];
}

export function maskSourceLayout(entry, identity, admission) {
  validateMask(entry.mask); validateMaskQuad(entry.quad);
  if (entry.mask === null || entry.quad === null) return null;
  const width = integer(identity.width, "mask frame width", 1, 16384), height = integer(identity.height, "mask frame height", 1, 16384);
  // Classic rounds only the backing canvas. Path coordinates and drawImage
  // dimensions keep the bound logical display extent.
  const localWidth = integer(Math.round(entry.quad.width), "local mask width", 0, 16384);
  const localHeight = integer(Math.round(entry.quad.height), "local mask height", 0, 16384);
  const pixels = BigInt(width) * BigInt(height) + BigInt(localWidth) * BigInt(localHeight);
  if (pixels > natural(admission.maxDecodedPixels, "mask pixels") || pixels * 4n > natural(admission.maxRgbaBytes, "mask canvas bytes"))
    reject("VISUAL_BUDGET_EXCEEDED", "Mask canvases exceed explicit admission");
  return { width, height, localWidth, localHeight, pixels, peakRgbaBytes: Number(pixels * 4n) };
}

// A frame-local private preparation binds generated sources to the final host
// wrapper. No caller-owned Canvas can opt into the retained GPU mask cache.
const preparedFrames = new WeakMap(), issuedSources = new WeakSet();
function freezeFacts(value) {
  if (value && typeof value === "object") { for (const item of Object.values(value)) freezeFacts(item); Object.freeze(value); }
  return value;
}
export function prepareMaskFrameSources(frame, state, admission, signal, createCanvas = (w, h) => new OffscreenCanvas(w, h), Path = globalThis.Path2D) {
  checkAbort(signal);
  const layouts = frame.masks.map(entry => maskSourceLayout(entry, frame.identity, admission));
  const pixels = layouts.reduce((sum, layout) => sum + (layout?.pixels ?? 0n), 0n);
  // Include the already admitted media/text/graphic source set as well.
  let sourcePixels = BigInt(state.retainedDecoded / 4);
  for (const source of new Set(state.sources.values())) if (!source.borrowed) sourcePixels += BigInt(source.width) * BigInt(source.height);
  if (pixels + sourcePixels > natural(admission.maxDecodedPixels, "aggregate mask pixels"))
    reject("VISUAL_BUDGET_EXCEEDED", "Masks and visual sources exceed aggregate pixel admission");
  for (const layout of layouts) if (layout) state.releases.push(state.budget.reserve(layout.peakRgbaBytes));
  const scope = [frame.identity.projectId, frame.identity.revision, frame.identity.sequenceId, frame.identity.profile, frame.identity.width, frame.identity.height];
  const sources = new Map();
  for (const [index, entry] of frame.masks.entries()) {
    const layout = layouts[index]; if (!layout) continue;
    const facts = freezeFacts(structuredClone(entry));
    // Classic's frame descriptor narrows feather to f32 before choosing the
    // positive-feather path. A tiny positive f64 can therefore be a hard mask.
    const source = Object.freeze({ ...layout, feather: Math.fround(facts.mask.feather.value), key: JSON.stringify([scope, facts]), facts, createCanvas, Path });
    issuedSources.add(source); sources.set(entry.elementId, source);
  }
  preparedFrames.set(frame, { sources, identity: structuredClone(frame.identity), masks: structuredClone(frame.masks) });
}
export function maskFrameSources(frame) {
  if (frame.masks === undefined) return new Map();
  const prepared = preparedFrames.get(frame);
  if (!prepared || !sameJsonFacts(prepared.identity, frame.identity) || !sameJsonFacts(prepared.masks, frame.masks))
    reject("INVALID_VISUAL_PLAN", "Mask sources require unchanged final frame preparation");
  return new Map(prepared.sources);
}
export function releaseMaskFrameSources(frame) { preparedFrames.delete(frame); }
export function rasterizeMaskSource(source, signal) {
  checkAbort(signal);
  if (!issuedSources.has(source)) reject("INVALID_VISUAL_PLAN", "Unrecognized mask source");
  const { width, height, localWidth, localHeight, facts: { mask, quad }, createCanvas, Path } = source;
  if (typeof Path !== "function") reject("CAPABILITY_UNAVAILABLE", "Mask Path2D is unavailable");
  let canvas, local, complete = false;
  try {
    canvas = createCanvas(width, height);
    const ctx = canvas.getContext("2d"); if (!ctx) reject("CAPABILITY_UNAVAILABLE", "Mask canvas is unavailable");
    // A rounded-zero local extent has no samples, matching the clipped canvas.
    if (localWidth && localHeight) {
      local = createCanvas(localWidth, localHeight);
      const localCtx = local.getContext("2d"); if (!localCtx) reject("CAPABILITY_UNAVAILABLE", "Local mask canvas is unavailable");
      const path = new Path(); for (const [op, ...args] of maskPath(mask, quad.width, quad.height)) path[op](...args);
      localCtx.fillStyle = "white"; localCtx.fill(path);
      checkAbort(signal); ctx.save();
      try {
        if (quad.rotationRadians !== 0 || quad.flipX || quad.flipY) {
          ctx.translate(quad.centerX, quad.centerY); ctx.rotate(quad.rotationRadians);
          ctx.scale(quad.flipX ? -1 : 1, quad.flipY ? -1 : 1); ctx.translate(-quad.centerX, -quad.centerY);
        }
        ctx.drawImage(local, quad.centerX - quad.width / 2, quad.centerY - quad.height / 2, quad.width, quad.height);
      } finally { ctx.restore(); }
    }
    checkAbort(signal); complete = true; return canvas;
  } finally {
    if (local) { local.width = 0; local.height = 0; }
    if (!complete && canvas) { canvas.width = 0; canvas.height = 0; }
  }
}
