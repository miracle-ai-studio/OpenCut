import { checkAbort, finite, integer, natural, reject } from "./validation.mjs";

// Classic v31 text/primitives.ts, text/layout.ts and renderer/nodes/text-node.ts
// define these layout semantics (MIT; vendor/classic-v31/LICENSE). This module
// consumes sampled Rust values; it does not sample animation or infer defaults.
function rgba(value, name) {
  if (!Array.isArray(value) || value.length !== 4) reject("INVALID_VISUAL_PLAN", `Invalid ${name}`);
  for (const component of value) if (finite(component, name) < 0 || component > 1) reject("INVALID_VISUAL_PLAN", `Invalid ${name}`);
  return `rgba(${value[0] * 255}, ${value[1] * 255}, ${value[2] * 255}, ${value[3]})`;
}

export function validateClassicText(text) {
  if (!text || typeof text.content !== "string" || !["left", "center", "right"].includes(text.align) || !["none", "underline", "lineThrough"].includes(text.decoration)) {
    reject("INVALID_VISUAL_PLAN", "Invalid sampled Classic text");
  }
  if (finite(text.fontSizePx, "text font size") <= 0 || finite(text.lineHeightPx, "text line height") <= 0) reject("INVALID_VISUAL_PLAN", "Text size and line height must be positive");
  finite(text.letterSpacingPx, "text letter spacing");
  rgba(text.color, "text color");
  const background = text.background, transform = text.transform;
  if (!background || typeof background.enabled !== "boolean" || !transform) reject("INVALID_VISUAL_PLAN", "Missing sampled text background or transform");
  rgba(background.color, "text background color");
  for (const key of ["paddingXPx", "paddingYPx", "offsetXPx", "offsetYPx", "cornerRadiusPercent"]) finite(background[key], key);
  for (const key of ["centerX", "centerY", "scaleX", "scaleY", "rotationRadians"]) finite(transform[key], key);
  return text;
}

function fontString(text, fontFamily, font) {
  // The family is derived exclusively from the verified bytes. Supplying no
  // generic family is intentional: the native coverage receipt must prove all
  // codepoints before this isolated face is ever used for text measurement.
  return `${font.style} ${font.weight} ${text.fontSizePx}px "${fontFamily}"`;
}

function setTextFont(ctx, text, fontFamily, font) {
  ctx.font = fontString(text, fontFamily, font);
  // Canvas silently ignores an invalid CSS font value. Continuing would use
  // the context's previous/default family instead of the verified font bytes.
  if (typeof ctx.font !== "string" || !ctx.font.includes(fontFamily)) reject("FONT_LOAD_FAILED", "The canvas rejected the verified font selection");
}

function setLetterSpacing(ctx, value) {
  if (!("letterSpacing" in ctx)) reject("CAPABILITY_UNAVAILABLE", "The admitted canvas does not support letter spacing");
  ctx.letterSpacing = `${value}px`;
}

export function measureClassicText(ctx, text, fontFamily, font) {
  validateClassicText(text);
  const lines = text.content.split("\n");
  let lineMetrics;
  ctx.save();
  try {
    setTextFont(ctx, text, fontFamily, font);
    ctx.textBaseline = "middle";
    setLetterSpacing(ctx, text.letterSpacingPx);
    lineMetrics = lines.map(line => {
      const metrics = ctx.measureText(line);
      return {
        width: finite(metrics.width, "text line width"),
        ascent: finite(metrics.actualBoundingBoxAscent ?? text.fontSizePx * 0.8, "text line ascent"),
        descent: finite(metrics.actualBoundingBoxDescent ?? text.fontSizePx * 0.2, "text line descent"),
      };
    });
  } finally { ctx.restore(); }
  const block = {
    maxWidth: lineMetrics.reduce((maximum, metrics) => Math.max(maximum, metrics.width), 0),
    height: finite(lines.length * text.lineHeightPx, "text block height"),
    visualCenterOffset: finite((lines.length - 1) * text.lineHeightPx / 2, "text center offset"),
  };
  let background = null;
  if (text.background.enabled) {
    const bg = text.background;
    const left = text.align === "left" ? 0 : text.align === "right" ? -block.maxWidth : -block.maxWidth / 2;
    const width = finite(block.maxWidth + 2 * bg.paddingXPx, "text background width");
    const height = finite(block.height + 2 * bg.paddingYPx, "text background height");
    background = {
      left: finite(left - bg.paddingXPx + bg.offsetXPx, "text background left"),
      top: finite(-block.height / 2 - bg.paddingYPx + bg.offsetYPx, "text background top"),
      width, height,
      radius: Math.min(width, height) / 2 * Math.max(0, Math.min(100, bg.cornerRadiusPercent)) / 100,
    };
  }
  const thickness = Math.max(1, text.fontSizePx * 0.07);
  return {
    fontSizePx: text.fontSizePx, lineHeightPx: text.lineHeightPx,
    letterSpacingPx: text.letterSpacingPx, align: text.align, baseline: "middle",
    block, background,
    lines: lines.map((content, index) => {
      const metrics = lineMetrics[index], baselineY = index * text.lineHeightPx - block.visualCenterOffset;
      let decoration = null;
      if (text.decoration !== "none") {
        decoration = {
          left: text.align === "left" ? 0 : text.align === "right" ? -metrics.width : -metrics.width / 2,
          top: text.decoration === "underline" ? baselineY + metrics.descent + thickness : baselineY - (metrics.ascent - metrics.descent) * 0.35,
          width: metrics.width, height: thickness,
        };
      }
      return { content, baselineY, ...metrics, decoration };
    }),
  };
}

export function drawClassicText(ctx, text, layout, fontFamily, font) {
  const transform = text.transform;
  ctx.save();
  try {
    // Text is first rasterized over the full canvas using Classic T · S · R.
    // The GPU subsequently uploads this canvas with an identity full-frame quad.
    ctx.translate(transform.centerX, transform.centerY);
    ctx.scale(transform.scaleX, transform.scaleY);
    if (transform.rotationRadians) ctx.rotate(transform.rotationRadians);
    setTextFont(ctx, text, fontFamily, font);
    ctx.textAlign = text.align;
    ctx.textBaseline = "middle";
    setLetterSpacing(ctx, text.letterSpacingPx);
    ctx.fillStyle = rgba(text.color, "text color");
    if (layout.background) {
      const rect = layout.background;
      ctx.fillStyle = rgba(text.background.color, "text background color");
      ctx.beginPath();
      ctx.roundRect(rect.left, rect.top, rect.width, rect.height, rect.radius);
      ctx.fill();
      ctx.fillStyle = rgba(text.color, "text color");
    }
    for (const line of layout.lines) {
      ctx.fillText(line.content, 0, line.baselineY);
      if (line.decoration) {
        const rect = line.decoration;
        ctx.fillRect(rect.left, rect.top, rect.width, rect.height);
      }
    }
  } finally { ctx.restore(); }
}

function validateFont(font) {
  if (!font || typeof font.fontId !== "string" || !font.fontId || !/^[a-f0-9]{64}$/.test(font.contentHash) || !/^[a-f0-9]{64}$/.test(font.validatedTextHash) || ![400, 700].includes(font.weight) || !["normal", "italic"].includes(font.style) || font.faceIndex !== 0 || font.glyphCoverageValidated !== true) {
    reject("INVALID_VISUAL_PLAN", "Text requires a validated static font face and complete glyph coverage");
  }
}

/** Full-frame raster from a native-validated static font and sampled text. */
export async function rasterizeClassicText(text, font, bytes, dimensions, signal) {
  checkAbort(signal);
  validateClassicText(text);
  validateFont(font);
  const width = integer(dimensions.width, "text canvas width", 1, 16384);
  const height = integer(dimensions.height, "text canvas height", 1, 16384);
  const pixels = BigInt(width) * BigInt(height);
  if (pixels > natural(dimensions.maxPixels, "text canvas pixel admission") || pixels * 16n > natural(dimensions.maxRasterBytes, "text canvas byte admission")) reject("VISUAL_BUDGET_EXCEEDED", "Text canvas exceeds its explicit admission");
  if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0) reject("INVALID_VISUAL_PLAN", "Missing verified font bytes");
  const hash = async value => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", value)), byte => byte.toString(16).padStart(2, "0")).join("");
  const digest = await hash(bytes);
  checkAbort(signal);
  if (digest !== font.contentHash) reject("ASSET_HASH_MISMATCH", "Font bytes changed after native validation");
  if (await hash(new TextEncoder().encode(text.content)) !== font.validatedTextHash) reject("INVALID_VISUAL_PLAN", "Glyph coverage belongs to different text content");
  checkAbort(signal);
  const fontSet = globalThis.fonts ?? globalThis.document?.fonts;
  if (!fontSet || typeof globalThis.FontFace !== "function") reject("CAPABILITY_UNAVAILABLE", "The admitted realm has no font loading support");
  const family = `opencut_${font.contentHash}_${font.weight}_${font.style}`;
  let face, canvas, added = false, complete = false;
  try {
    try {
      face = new FontFace(family, bytes, { style: font.style, weight: String(font.weight) });
      await face.load();
    }
    catch { checkAbort(signal); reject("FONT_LOAD_FAILED", "The browser rejected the verified static font bytes"); }
    checkAbort(signal);
    if (face.status !== "loaded") reject("FONT_LOAD_FAILED", "The browser did not load the verified static font face");
    fontSet.add(face); added = true;
    canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) reject("CAPABILITY_UNAVAILABLE", "Text rasterization canvas is unavailable");
    const layout = measureClassicText(ctx, text, family, font);
    drawClassicText(ctx, text, layout, family, font);
    checkAbort(signal);
    complete = true;
    return {
      canvas, width, height,
      layout: { fontId: font.fontId, fontContentHash: font.contentHash, validatedTextHash: font.validatedTextHash, fontWeight: font.weight, fontStyle: font.style, faceIndex: font.faceIndex, ...layout, transform: structuredClone(text.transform) },
    };
  } finally {
    if (added) fontSet.delete(face);
    if (canvas && !complete) { canvas.width = 0; canvas.height = 0; }
  }
}
