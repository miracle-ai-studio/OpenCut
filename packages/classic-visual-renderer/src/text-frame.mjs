import { rasterizeClassicText, validateClassicText } from "./text-layout.mjs";
import { checkAbort, finite, integer, natural, reject, sameJsonFacts } from "./validation.mjs";

export const TEXT_FRAME_PROFILE = "classic-v31-gpu-normal-text-frame-v2";
const MEDIA_PROFILE = "classic-v31-gpu-normal-frame";
const clone = value => structuredClone(value);

export function fontResource(font) {
  return Object.fromEntries(["fontId", "contentHash", "size", "weight", "style", "faceIndex"].map(key => [key, font[key]]));
}

export function validateTextFramePlan(plan, admission) {
  if (!plan || plan.identity?.profile !== TEXT_FRAME_PROFILE || plan.media?.identity?.profile !== MEDIA_PROFILE || !Array.isArray(plan.media.resources) || !Array.isArray(plan.media.layers) || !Array.isArray(plan.texts) || !Array.isArray(plan.order)) reject("INVALID_VISUAL_PLAN", "Invalid V2 text frame plan");
  const identity = { ...plan.identity, profile: MEDIA_PROFILE };
  if (!sameJsonFacts(identity, plan.media.identity)) reject("INVALID_VISUAL_PLAN", "V2 media partition changed the fixed frame identity");
  integer(identity.width, "V2 frame width", 1, 16384); integer(identity.height, "V2 frame height", 1, 16384);
  const pixels = BigInt(identity.width) * BigInt(identity.height);
  if (pixels > natural(admission.maxFramePixels, "frame pixels") || pixels * 4n > natural(admission.maxRgbaBytes, "RGBA bytes")) reject("VISUAL_BUDGET_EXCEEDED", "V2 frame exceeds its canvas admission");
  if (plan.order.length > 60 || plan.order.length !== plan.media.layers.length + plan.texts.length) reject("INVALID_VISUAL_PLAN", "V2 frame layer collection is incomplete or exceeds admission");
  const elements = new Set(), fonts = new Map(), resources = new Set();
  let encoded = 0n;
  for (const resource of plan.media.resources) {
    if (typeof resource.assetId !== "string" || !resource.assetId || resources.has(resource.assetId)) reject("INVALID_VISUAL_PLAN", "V2 media resources must be distinct");
    resources.add(resource.assetId);
    encoded += natural(resource.size, "media encoded bytes");
  }
  for (const layer of plan.media.layers) {
    if (typeof layer.elementId !== "string" || !layer.elementId || elements.has(layer.elementId) || !["image", "video"].includes(layer.decode?.kind)) reject("INVALID_VISUAL_PLAN", "V2 media layers must be distinct media nodes");
    elements.add(layer.elementId);
  }
  for (const layer of plan.texts) {
    if (typeof layer.elementId !== "string" || !layer.elementId || elements.has(layer.elementId)) reject("INVALID_VISUAL_PLAN", "V2 text layers must be distinct");
    elements.add(layer.elementId);
    validateClassicText(layer.text); finite(layer.opacity, "text layer opacity");
    const font = layer.font;
    if (!font || typeof font.fontId !== "string" || !font.fontId || !/^[a-f0-9]{64}$/.test(font.contentHash) || !/^[a-f0-9]{64}$/.test(font.validatedTextHash) || ![400, 700].includes(font.weight) || !["normal", "italic"].includes(font.style) || font.faceIndex !== 0 || font.glyphCoverageValidated !== true || natural(font.size, "font encoded bytes") === 0n) reject("INVALID_VISUAL_PLAN", "V2 text requires fixed font identity and native coverage facts");
    if (resources.has(font.fontId)) reject("INVALID_VISUAL_PLAN", "V2 font and media identities collide");
    const resource = fontResource(font), previous = fonts.get(font.fontId);
    if (previous && !sameJsonFacts(previous, resource)) reject("INVALID_VISUAL_PLAN", "Shared font identity changed between text layers");
    if (!previous) { fonts.set(font.fontId, resource); encoded += natural(font.size, "font encoded bytes"); }
  }
  if (elements.size !== plan.order.length || new Set(plan.order).size !== elements.size || plan.order.some(id => !elements.has(id))) reject("INVALID_VISUAL_PLAN", "V2 order must cover every media and text node exactly once");
  if (resources.size + fonts.size > 32 || encoded > BigInt(integer(admission.maxEncodedBytes, "encoded byte admission", 1))) reject("VISUAL_BUDGET_EXCEEDED", "V2 aggregate encoded resources exceed admission");
  return fonts;
}

export async function composeTextFrame(plan, host, state, signal) {
  const { bound: media, sources, budget, canvases, releases, encoded, retainedDecoded, resolveFont } = state;
  const fonts = new Map(), rasters = [];
  // Retained video pools account for three RGBA canvases. Other distinct source
  // objects each own a single observed image canvas; shared observations count
  // once, independently of how many layers reference them.
  let pixels = BigInt(retainedDecoded / 4);
  for (const source of new Set(sources.values())) if (!source.borrowed) pixels += BigInt(source.width) * BigInt(source.height);
  const textPixels = BigInt(plan.identity.width) * BigInt(plan.identity.height);
  const totalPixels = pixels + textPixels * BigInt(plan.texts.length);
  if (totalPixels > natural(host.admission.maxDecodedPixels, "V2 decoded pixels")) reject("VISUAL_BUDGET_EXCEEDED", "V2 media and text rasters exceed aggregate pixel admission");
  for (const layer of plan.texts) {
    checkAbort(signal);
    // Reserve retained full-canvas storage before loading or allocating it.
    releases.push(budget.reserve(Number(textPixels * 16n)));
    let font = fonts.get(layer.font.fontId);
    if (!font) {
      font = await resolveFont(fontResource(layer.font));
      encoded.set(Symbol(layer.font.fontId), font);
      fonts.set(layer.font.fontId, font);
    }
    const source = await rasterizeClassicText(layer.text, layer.font, font.bytes, {
      width: plan.identity.width, height: plan.identity.height,
      maxPixels: host.admission.maxDecodedPixels, maxRasterBytes: host.admission.maxRgbaBytes,
    }, signal);
    canvases.add(source.canvas); sources.set(layer.elementId, source);
    rasters.push({ elementId: layer.elementId, width: source.width, height: source.height, layout: clone(source.layout) });
  }
  checkAbort(signal);
  const bound = clone(await host.callback("bindTextFrame", { rasters: clone(rasters) }));
  checkAbort(signal);
  if (!sameJsonFacts(bound.identity, plan.identity) || !sameJsonFacts(bound.decoded, media.decoded) || !sameJsonFacts(bound.rasters, rasters) || !sameJsonFacts(bound.clear, media.clear) || !sameJsonFacts(bound.background, media.background) || !Array.isArray(bound.layers) || bound.layers.length !== plan.order.length) reject("INVALID_VISUAL_PLAN", "Bound V2 frame changed fixed identity or observed raster facts");
  const mediaLayers = new Map(media.layers.map(layer => [layer.elementId, layer]));
  const textLayers = new Map(plan.texts.map(layer => [layer.elementId, layer]));
  for (let index = 0; index < bound.layers.length; index++) {
    const actual = bound.layers[index], id = plan.order[index];
    if (actual.elementId !== id) reject("INVALID_VISUAL_PLAN", "Bound V2 frame changed complete layer order");
    const original = mediaLayers.get(id);
    if (original) {
      if (actual.assetId !== original.assetId || !sameJsonFacts(actual.quad, original.quad)) reject("INVALID_VISUAL_PLAN", "Bound V2 frame changed a delegated media layer");
    } else {
      const text = textLayers.get(id);
      const quad = {
        centerX: plan.identity.width / 2, centerY: plan.identity.height / 2,
        width: plan.identity.width, height: plan.identity.height, rotationRadians: 0,
        opacity: Math.fround(text.opacity), flipX: false, flipY: false,
      };
      if (actual.assetId !== text.font.fontId || !sameJsonFacts(actual.quad, quad)) reject("INVALID_VISUAL_PLAN", "Text raster must use a full-canvas identity quad and one opacity multiplier");
    }
  }
  return bound;
}
