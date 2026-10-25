// Classic masks/builtin/definitions/text.ts and text/primitives.ts. Font bytes
// and complete glyph coverage are proved natively; Canvas never selects a system
// family. This bridge only lays out an already sampled, fixed mask after binding.
import { rasterizePreparedClassicText, rasterizeSceneClassicText, validateSceneFontSynthesis, createSceneTextStyleScope, sceneTextStyle } from "./text-layout.mjs";
import { fontResource } from "./text-frame.mjs";
import {
  checkAbort,
  finite,
  integer,
  natural,
  reject,
  sameJsonFacts,
} from "./validation.mjs";
const frames = new WeakMap();
const exact = (value, keys) =>
  value &&
  typeof value === "object" &&
  !Array.isArray(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((k) => Object.hasOwn(value, k));
const key = (font) => `${font.fontId}:${font.contentHash}:${font.size}`;
const textPlan = (plan) => plan.execution.execution.execution.execution;
const ownerKey = (kind, elementId, ownerId) => JSON.stringify([kind, elementId, ownerId]);
/** The top-level Scene map is the only requested-style authority. Delegated
 * text/font plans continue to describe the real normal face. Caption directives
 * cannot acquire synthesis by sharing a font or an element id. */
export function sceneSynthesisEntries(plan) {
  if (plan.identity?.profile !== "classic-v31-gpu-scene-frame-v1" || !Array.isArray(plan.fontSynthesis) || plan.fontSynthesis.length > 3900) reject("INVALID_VISUAL_PLAN", "Missing bounded Scene synthesis map");
  if (!Array.isArray(plan.maskFonts) || plan.maskFonts.length > 3840) reject("INVALID_VISUAL_PLAN", "Missing bounded Scene mask font proofs");
  const texts = new Map(textPlan(plan).texts.map(layer => [layer.elementId, layer]));
  const captions = new Set((plan.execution.execution.execution.captions ?? []).map(caption => caption.elementId));
  const masks = new Map();
  for (const stack of plan.stacks) for (const mask of stack.masks) {
    if (masks.has(mask.id)) reject("INVALID_VISUAL_PLAN", "Scene mask owner must be unique");
    masks.set(mask.id, { elementId: stack.elementId, mask });
  }
  const seen = new Set(), bindings = new Map(), entries = [];
  for (const record of plan.fontSynthesis) {
    if (!exact(record, ["elementId", "ownerId", "ownerKind", "fontId", "binding"]) || !["text", "text_mask"].includes(record.ownerKind) || !["elementId", "ownerId", "fontId"].every(field => typeof record[field] === "string" && record[field].length > 0)) reject("INVALID_VISUAL_PLAN", "Invalid Scene synthesis owner");
    const key = ownerKey(record.ownerKind, record.elementId, record.ownerId);
    if (seen.has(key)) reject("INVALID_VISUAL_PLAN", "Repeated Scene synthesis owner");
    seen.add(key);
    let font;
    if (record.ownerKind === "text") {
      const layer = texts.get(record.elementId);
      if (!layer || record.ownerId !== record.elementId || captions.has(record.elementId)) reject("INVALID_VISUAL_PLAN", "Scene synthesis owner is not a fixed independent text");
      font = layer.font;
    } else {
      const owned = masks.get(record.ownerId), proof = plan.maskFonts.find(proof => proof.maskId === record.ownerId);
      if (!owned || owned.elementId !== record.elementId || !owned.mask.enabled || owned.mask.shape?.kind !== "text" || !owned.mask.shape.mask.content.trim() || !proof) reject("INVALID_VISUAL_PLAN", "Scene synthesis owner is not an active fixed text mask");
      const text = owned.mask.shape.mask;
      if (text.font.font_id !== record.fontId || text.font_style !== record.binding?.requested?.style || (text.font_weight === "bold" ? 700 : 400) !== record.binding?.requested?.weight) reject("INVALID_VISUAL_PLAN", "Scene text mask changed requested synthesis typography");
      font = proof.font;
    }
    if (font.fontId !== record.fontId) reject("INVALID_VISUAL_PLAN", "Scene synthesis changed its project font resource");
    validateSceneFontSynthesis(record.binding, font);
    if (bindings.has(record.fontId) && !sameJsonFacts(bindings.get(record.fontId), record.binding)) reject("INVALID_VISUAL_PLAN", "Scene synthesis changed a shared resource binding");
    bindings.set(record.fontId, record.binding);
    entries.push({ key, font, binding: record.binding });
  }
  return entries;
}
export function createSceneSynthesis(plan, signal) {
  const scope = createSceneTextStyleScope(sceneSynthesisEntries(plan), signal);
  return {
    close: () => scope.close(),
    tokenForMask: (elementId, maskId, font) => scope.tokenFor(ownerKey("text_mask", elementId, maskId), font),
    wrapTextHooks(hooks) {
      return { ...hooks, rasterize(text, font, bytes, dimensions, rasterSignal, retained, layer) {
        if (rasterSignal !== signal) reject("INVALID_VISUAL_PLAN", "Scene synthesis changed frame cancellation scope");
        const token = scope.tokenFor(ownerKey("text", layer.elementId, layer.elementId), font);
        return token === undefined ? hooks.rasterize(text, font, bytes, dimensions, rasterSignal, retained, layer) : rasterizeSceneClassicText(text, font, bytes, dimensions, rasterSignal, retained, token);
      } };
    },
  };
}
function validatedFont(font) {
  if (
    !exact(font, [
      "fontId",
      "contentHash",
      "size",
      "weight",
      "style",
      "faceIndex",
      "glyphCoverageValidated",
      "validatedTextHash",
    ]) ||
    typeof font.fontId !== "string" ||
    !font.fontId ||
    !/^[a-f0-9]{64}$/.test(font.contentHash) ||
    !/^[a-f0-9]{64}$/.test(font.validatedTextHash) ||
    ![400, 700].includes(font.weight) ||
    !["normal", "italic"].includes(font.style) ||
    font.faceIndex !== 0 ||
    font.glyphCoverageValidated !== true ||
    natural(font.size, "scene mask font bytes") === 0n
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Scene text mask needs a fixed static face and native complete-text coverage",
    );
}
export function validateSceneTextMask(text) {
  if (
    !exact(text, [
      "content",
      "font",
      "font_size",
      "font_weight",
      "font_style",
      "decoration",
      "letter_spacing_px",
      "line_height_multiplier",
      "center_x",
      "center_y",
      "rotation_degrees",
      "scale",
    ]) ||
    typeof text.content !== "string" ||
    text.content.length > 65536 ||
    new TextEncoder().encode(text.content).byteLength > 65536 ||
    !exact(text.font, ["kind", "font_id"]) ||
    text.font.kind !== "locked" ||
    typeof text.font.font_id !== "string" ||
    !text.font.font_id ||
    text.font.font_id.length > 128 ||
    /[\u0000-\u001f\u007f]/.test(text.font.font_id) ||
    !["normal", "bold"].includes(text.font_weight) ||
    !["normal", "italic"].includes(text.font_style) ||
    !["none", "underline", "line_through"].includes(text.decoration) ||
    !exact(text.font_size, ["unit", "value"]) ||
    !["classic_height90", "canvas_pixels"].includes(text.font_size.unit)
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Text mask requires complete sampled typography and locked font",
    );
  if (
    finite(text.font_size.value, "text mask font size") <= 0 ||
    finite(text.line_height_multiplier, "text mask line height") <= 0 ||
    finite(text.scale, "text mask scale") < 0
  )
    reject("INVALID_VISUAL_PLAN", "Invalid text mask positive geometry");
  for (const field of [
    "letter_spacing_px",
    "center_x",
    "center_y",
    "rotation_degrees",
  ])
    finite(text[field], field);
}
/** Complete encoded-font union for Scene begin validation, including fonts that
 * are only used by masks. Values use the existing private font resource DTO. */
export function sceneFontResources(plan) {
  const synthesis = new Map(sceneSynthesisEntries(plan).map(entry => [entry.key, entry]));
  const fonts = new Map();
  for (const layer of textPlan(plan).texts) {
    const resource = fontResource(layer.font);
    fonts.set(resource.fontId, resource);
  }
  if (!Array.isArray(plan.maskFonts) || plan.maskFonts.length > 3840)
    reject("INVALID_VISUAL_PLAN", "Missing bounded Scene mask font proofs");
  const expected = [];
  for (const stack of plan.stacks)
    for (const mask of stack.masks)
      if (
        mask.enabled &&
        mask.shape?.kind === "text" &&
        mask.shape.mask?.content?.trim()
      ) {
        validateSceneTextMask(mask.shape.mask);
        expected.push({ mask, elementId: stack.elementId });
      }
  if (expected.length !== plan.maskFonts.length)
    reject("INVALID_VISUAL_PLAN", "Scene mask font coverage is incomplete");
  const seen = new Set();
  for (let i = 0; i < expected.length; i++) {
    const { mask, elementId } = expected[i],
      proof = plan.maskFonts[i];
    if (
      !exact(proof, ["maskId", "font"]) ||
      proof.maskId !== mask.id ||
      seen.has(proof.maskId)
    )
      reject(
        "INVALID_VISUAL_PLAN",
        "Scene mask font owner changed or repeated",
      );
    seen.add(proof.maskId);
    validatedFont(proof.font);
    const text = mask.shape.mask;
    if (
      proof.font.fontId !== text.font.font_id ||
      proof.font.weight !== (text.font_weight === "bold" ? 700 : 400) ||
      (proof.font.style !== text.font_style && !synthesis.has(ownerKey("text_mask", elementId, mask.id)))
    )
      reject(
        "INVALID_VISUAL_PLAN",
        "Scene mask face differs from sampled typography",
      );
    const resource = fontResource(proof.font),
      previous = fonts.get(resource.fontId);
    if (previous && !sameJsonFacts(previous, resource))
      reject("INVALID_VISUAL_PLAN", "Scene shared font identity changed");
    fonts.set(resource.fontId, resource);
  }
  return fonts;
}
function rasterText(mask, width, height) {
  const m = mask.shape.mask,
    fontSize =
      m.font_size.unit === "classic_height90"
        ? (m.font_size.value * height) / 90
        : m.font_size.value;
  return {
    content: m.content,
    fontSizePx: fontSize,
    lineHeightPx: fontSize * m.line_height_multiplier,
    letterSpacingPx: m.letter_spacing_px,
    align: "center",
    decoration: m.decoration === "line_through" ? "lineThrough" : m.decoration,
    color: [1, 1, 1, 1],
    background: {
      enabled: false,
      color: [0, 0, 0, 0],
      paddingXPx: 0,
      paddingYPx: 0,
      offsetXPx: 0,
      offsetYPx: 0,
      cornerRadiusPercent: 0,
    },
    transform: {
      centerX: width / 2 + m.center_x * width,
      centerY: height / 2 + m.center_y * height,
      scaleX: m.scale,
      scaleY: m.scale,
      rotationRadians: (m.rotation_degrees * Math.PI) / 180,
    },
  };
}
function strokeText(canvas, body, text, layout, font, mask, drawingFont = font) {
  const c = canvas.getContext("2d");
  if (!c)
    reject("CAPABILITY_UNAVAILABLE", "Text mask stroke Canvas unavailable");
  const family = `opencut_${font.contentHash}_${font.weight}_${font.style}`,
    transform = text.transform,
    color = mask.stroke.color;
  c.save();
  try {
    c.translate(transform.centerX, transform.centerY);
    c.scale(transform.scaleX, transform.scaleY);
    if (transform.rotationRadians) c.rotate(transform.rotationRadians);
    c.font = `${drawingFont.style} ${drawingFont.weight} ${text.fontSizePx}px "${family}"`;
    if (!c.font.includes(family))
      reject("FONT_LOAD_FAILED", "Text mask stroke rejected the verified font");
    c.textAlign = "center";
    c.textBaseline = "middle";
    if (!("letterSpacing" in c))
      reject(
        "CAPABILITY_UNAVAILABLE",
        "Text mask stroke requires letter spacing",
      );
    c.letterSpacing = `${text.letterSpacingPx}px`;
    c.strokeStyle = `rgba(${color.red * 255},${color.green * 255},${color.blue * 255},${color.alpha})`;
    c.lineWidth = mask.stroke.width_px;
    c.lineJoin = "round";
    c.lineCap = "round";
    for (const line of layout.lines)
      c.strokeText(line.content, 0, line.baselineY);
  } finally {
    c.restore();
  }
  if (mask.stroke.align !== "center") {
    c.globalCompositeOperation =
      mask.stroke.align === "inside" ? "destination-in" : "destination-out";
    c.drawImage(body, 0, 0);
    c.globalCompositeOperation = "source-over";
  }
}
/** Font resolution happens before final Scene binding. Shared title/mask faces
 * use the renderFrame resolveFont memo, so each fixed face is authorized once. */
export async function resolveSceneTextMaskFonts(plan, state, signal) {
  sceneFontResources(plan);
  const fonts = new Map();
  for (const proof of plan.maskFonts) {
    checkAbort(signal);
    const id = proof.font.fontId;
    if (fonts.has(id)) continue;
    const resource = await state.resolveFont(fontResource(proof.font));
    checkAbort(signal);
    if (![...state.encoded.values()].includes(resource))
      state.encoded.set(Symbol(`scene-mask-font:${id}`), resource);
    fonts.set(id, resource);
  }
  return fonts;
}
export async function prepareSceneTextMasks(
  plan,
  frame,
  state,
  admission,
  signal,
  createCanvas = (w, h) => new OffscreenCanvas(w, h),
  resolved,
  synthesis,
) {
  checkAbort(signal);
  plan = structuredClone(plan);
  sceneFontResources(plan);
  const identity = structuredClone(frame.identity),
    scene = structuredClone(frame.scene);
  if (
    !sameJsonFacts(identity, plan.identity) ||
    !sameJsonFacts(
      scene.layers.map((layer) => layer.stack),
      plan.stacks,
    )
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Text mask frame differs from the fixed Scene plan",
    );
  const unchanged = () => {
    checkAbort(signal);
    if (
      !sameJsonFacts(frame.identity, identity) ||
      !sameJsonFacts(frame.scene, scene)
    )
      reject(
        "INVALID_VISUAL_PLAN",
        "Text mask frame changed while awaiting fixed resources",
      );
  };
  const loaded =
      resolved ?? (await resolveSceneTextMaskFonts(plan, state, signal)),
    rasters = new Map(),
    ephemeral = new Map();
  unchanged();
  const styles = synthesis ?? createSceneSynthesis(plan, signal);
  try {
    for (const layer of scene.layers)
      for (const mask of layer.stack.masks) {
        if (
          !mask.enabled ||
          mask.shape?.kind !== "text" ||
          !mask.shape.mask.content.trim() ||
          layer.quad === null
        )
          continue;
        const proof = plan.maskFonts.find((p) => p.maskId === mask.id);
        if (!proof)
          reject("INVALID_VISUAL_PLAN", "Text mask omitted native font proof");
        const width = integer(
            Math.round(layer.quad.width),
            "text mask local width",
            0,
            16384,
          ),
          height = integer(
            Math.round(layer.quad.height),
            "text mask local height",
            0,
            16384,
          ),
          pixels = BigInt(width) * BigInt(height);
        if (pixels > natural(admission.maxDecodedPixels, "text mask pixels"))
          reject(
            "VISUAL_BUDGET_EXCEEDED",
            "Text mask local pixels exceed admission",
          );
        state.releases.push(state.budget.reserve(Number(pixels * 16n)));
        let body,
          stroke = null;
        if (width && height) {
          const font = proof.font,
            bytes = loaded.get(font.fontId)?.bytes;
          if (!(bytes instanceof Uint8Array))
            reject(
              "INVALID_VISUAL_PLAN",
              "Scene text font bytes are unavailable",
            );
          const fontKey = key(font);
          let retained =
            state.fontCache?.get(fontKey) ?? ephemeral.get(fontKey);
          if (!retained) {
            retained = { bytes };
            ephemeral.set(fontKey, retained);
          }
          const token = styles.tokenForMask(layer.stack.elementId, mask.id, font);
          const text = rasterText(mask, width, height),
            source = await (token === undefined ? rasterizePreparedClassicText : rasterizeSceneClassicText)(
              text,
              font,
              bytes,
              {
                width,
                height,
                maxPixels: admission.maxDecodedPixels,
                maxRasterBytes: admission.maxRgbaBytes,
                frameFontCheck: state.frameFontChecks?.tokenFor(font, bytes),
              },
              signal,
              retained,
              token,
            );
          body = source.canvas;
          state.canvases.add(body);
          unchanged();
          if (mask.stroke.width_px > 0) {
            stroke = createCanvas(width, height);
            state.canvases.add(stroke);
            strokeText(stroke, body, text, source.layout, font, mask, token === undefined ? font : sceneTextStyle(token, font, signal));
          }
        } else {
          body = createCanvas(width, height);
          state.canvases.add(body);
        }
        const entry = Object.freeze({
          body,
          stroke,
          localWidth: width,
          localHeight: height,
        });
        rasters.set(mask.id, {
          entry,
          mask: structuredClone(mask),
          quad: structuredClone(layer.quad),
        });
      }
    unchanged();
    frames.set(frame, { rasters, identity, scene });
  } finally {
    if (!synthesis) styles.close();
    for (const item of ephemeral.values())
      if (item.face)
        (globalThis.fonts ?? globalThis.document?.fonts)?.delete(item.face);
  }
}
export function sceneTextMaskSource(frame, maskId) {
  const record = frames.get(frame);
  if (
    !record ||
    !sameJsonFacts(record.identity, frame.identity) ||
    !sameJsonFacts(record.scene, frame.scene)
  )
    reject(
      "INVALID_VISUAL_PLAN",
      "Text mask source requires its unchanged fixed Scene frame",
    );
  const item = record.rasters.get(maskId);
  if (!item) return null;
  const layer = frame.scene.layers.find((l) =>
      l.stack.masks.some((m) => m.id === maskId),
    ),
    mask = layer?.stack.masks.find((m) => m.id === maskId);
  if (!sameJsonFacts(item.mask, mask) || !sameJsonFacts(item.quad, layer.quad))
    reject(
      "INVALID_VISUAL_PLAN",
      "Text mask source changed after fixed font rasterization",
    );
  return item.entry;
}
export function releaseSceneTextMasks(frame) {
  frames.delete(frame);
}
