import { checkAbort, finite, integer, natural, reject, sameJsonFacts } from "./validation.mjs";
import { hashFontSnapshot } from "./font-hash.mjs";

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

const exactFields = (value, keys) => value && typeof value === "object" && !Array.isArray(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const sceneStyles = new WeakMap();
/** This is an explicit Scene request, never a replacement for native metadata.
 * Rust authenticates the compiled source/proof; JS links that fixed request to
 * the actual byte/face identity which the existing raster path verifies again. */
export function validateSceneFontSynthesis(binding, font) {
  validateFont(font);
  const actual = binding?.actual, requested = binding?.requested;
  if (!exactFields(binding, ["schema_version", "binding_id", "profile", "actual", "requested"]) || binding.schema_version !== 1 || binding.profile !== "classic_chromium_italic_v1" || !/^font-synthesis-v1_[a-f0-9]{64}$/.test(binding.binding_id) ||
      !exactFields(actual, ["id", "family", "weight", "style", "source_url", "source_commit", "content_hash", "size_bytes", "license_id", "license_notice"]) ||
      !["id", "family", "source_url", "source_commit", "license_id", "license_notice"].every(key => typeof actual[key] === "string" && actual[key].length > 0) ||
      !/^[a-f0-9]{40}$/.test(actual.source_commit) || !exactFields(requested, ["weight", "style"]) ||
      actual.content_hash !== font.contentHash || actual.size_bytes !== font.size || natural(actual.size_bytes, "Scene synthesis font bytes") === 0n ||
      actual.style !== "normal" || font.style !== "normal" || actual.weight !== font.weight || requested.weight !== actual.weight || requested.style !== "italic") {
    reject("INVALID_VISUAL_PLAN", "Scene synthesis must preserve the actual fixed normal face and request only same-weight italic");
  }
}

/** Private, frame-local capabilities. No plain binding object can be passed to
 * the normal raster entry point to change its style; only Scene creates these
 * after checking complete owner/font mappings. Tokens never contain font bytes. */
export function createSceneTextStyleScope(entries, signal) {
  checkAbort(signal);
  const scope = { active: true }, records = new Map();
  for (const { key, font, binding } of entries) {
    if (typeof key !== "string" || !key || records.has(key)) reject("INVALID_VISUAL_PLAN", "Repeated Scene synthesis owner");
    validateSceneFontSynthesis(binding, font);
    const token = Object.freeze({});
    const record = { scope, signal, font: structuredClone(font), requested: structuredClone(binding.requested) };
    records.set(key, { token, record }); sceneStyles.set(token, record);
  }
  return {
    tokenFor(key, font) {
      if (!scope.active) reject("INVALID_VISUAL_PLAN", "Scene synthesis scope is closed");
      checkAbort(signal);
      const found = records.get(key);
      if (found && !sameJsonFacts(found.record.font, font)) reject("INVALID_VISUAL_PLAN", "Scene synthesis font changed after admission");
      return found?.token;
    },
    close() { scope.active = false; for (const { token } of records.values()) sceneStyles.delete(token); records.clear(); },
  };
}
export function sceneTextStyle(token, font, signal) {
  checkAbort(signal);
  const record = token && sceneStyles.get(token);
  if (!record?.scope.active || record.signal !== signal || !sameJsonFacts(record.font, font)) reject("INVALID_VISUAL_PLAN", "Unknown, closed or changed Scene synthesis request");
  return { ...font, ...record.requested };
}

const hashBytes = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), byte => byte.toString(16).padStart(2, "0")).join("");
const frameFontChecks = new WeakMap();
// FontFace owns an immutable copy of its constructor source. Only faces loaded
// here from verified bytes may overlap rasterization with the next frame's
// byte check; an arbitrary caller-supplied face is never a speculative source.
const verifiedFaces = new WeakMap();
const fontIdentity = font => JSON.stringify([font.fontId, font.contentHash, font.size, font.weight, font.style, font.faceIndex]);
const immutablePreviewRealms = new WeakSet(), immutablePreviewFaces = new WeakMap();
const faceDescriptors = ["family", "style", "weight", "stretch", "unicodeRange", "featureSettings", "variationSettings", "display", "ascentOverride", "descentOverride", "lineGapOverride", "sizeAdjust", "variant"];

/** Browser preview alone may retain a face constructed from a verified private
 * snapshot. Its parsed source cannot be replaced through FontFace's API. Raw
 * byte users, including native hosts, keep the original per-frame hash path. */
export function enableImmutablePreviewFonts(realm) {
  if (!realm || realm.closed) reject("INVALID_VISUAL_PLAN", "Preview font realm is unavailable");
  immutablePreviewRealms.add(realm);
}
function currentImmutableFace(face, font, realm) {
  const owner = face && immutablePreviewFaces.get(face);
  const fontSet = globalThis.fonts ?? globalThis.document?.fonts;
  return !!owner && !realm?.closed && owner.realm === realm && owner.identity === fontIdentity(font)
    && face.status === "loaded" && owner.descriptors.every((value, i) => face[faceDescriptors[i]] === value)
    && typeof fontSet?.has === "function" && fontSet.has(face);
}

// Only the current graphic frame's already retained fonts use this path. The
// caller reserves 3N before start: retained bytes + this private N-byte snapshot
// + WebCrypto's N-byte input copy. Cold transfers retain their original path.
export function createFrameFontChecks(signal, realm, { deferLoadedFaces = false } = {}) {
  const scope = { active: true }, records = new Map();
  let started = false;
  return {
    start(texts, fontCache) {
      if (started || !scope.active) reject("INVALID_VISUAL_PLAN", "Font checks already started or closed");
      started = true;
      for (const { font } of texts) {
        checkAbort(signal); validateFont(font);
        const identity = fontIdentity(font);
        if (records.has(identity)) continue;
        const retained = fontCache.get(`${font.fontId}:${font.contentHash}:${font.size}`);
        if (!retained) continue;
        const bytes = retained.bytes;
        if (!(bytes instanceof Uint8Array) || BigInt(bytes.byteLength) !== natural(font.size, "font snapshot bytes") || bytes.byteLength === 0) reject("INVALID_VISUAL_PLAN", "Retained font bytes changed size");
        const previewOwner = immutablePreviewRealms.has(realm);
        const immutableFace = previewOwner && currentImmutableFace(retained.face, font, realm) ? retained.face : null;
        const token = Object.freeze({}), expectedHash = font.contentHash;
        const record = { scope, identity, font: structuredClone(font), bytes, snapshot: null, signal, token, deferLoadedFaces, realm, previewOwner, immutableFace, retained };
        records.set(identity, record); frameFontChecks.set(token, record);
        if (immutableFace) {
          // This frame consumes the parsed face, not the mutable cache bytes.
          // Authority and text coverage are still checked for every frame.
          record.pending = Promise.resolve({ ok: true });
          continue;
        }
        const snapshot = new Uint8Array(bytes);
        // Consume errors here even when media fails before text is reached.
        record.pending = (async () => {
          try {
            const result = await hashFontSnapshot(realm, identity, snapshot, signal); checkAbort(signal);
            if (result.hash !== expectedHash) reject("ASSET_HASH_MISMATCH", "Font bytes changed after native validation");
            record.snapshot = result.snapshot;
            return { ok: true };
          } catch (error) { return { ok: false, error }; }
        })();
      }
    },
    tokenFor(font, bytes) {
      if (!scope.active) reject("INVALID_VISUAL_PLAN", "Frame font checks are closed");
      checkAbort(signal);
      const record = records.get(fontIdentity(font));
      if (record && record.bytes !== bytes) reject("INVALID_VISUAL_PLAN", "Retained font bytes changed reference");
      return record?.token;
    },
    async verify() {
      checkAbort(signal);
      if (!scope.active) reject("INVALID_VISUAL_PLAN", "Frame font checks are closed");
      const results = await Promise.all([...records.values()].map(record => record.pending));
      checkAbort(signal);
      if (!scope.active) reject("INVALID_VISUAL_PLAN", "Frame font checks are closed");
      for (const result of results) if (!result.ok) throw result.error;
      for (const record of records.values()) if (record.immutableFace) checkedFrameFontRecord(record.token, record.font, record.bytes, signal);
    },
    async close() {
      scope.active = false;
      await Promise.all([...records.values()].map(record => record.pending));
      for (const record of records.values()) {
        frameFontChecks.delete(record.token); record.snapshot = null; record.bytes = null;
      }
      records.clear();
    },
  };
}

function checkedFrameFontRecord(token, font, bytes, signal) {
  const record = token && frameFontChecks.get(token);
  if (!record?.scope.active || record.identity !== fontIdentity(font) || record.bytes !== bytes || record.signal !== signal) reject("INVALID_VISUAL_PLAN", "Unknown or changed frame font check");
  if (record.immutableFace && (record.retained.face !== record.immutableFace || !currentImmutableFace(record.immutableFace, font, record.realm))) reject("FONT_LOAD_FAILED", "Retained preview face changed after admission");
  return record;
}
async function frameFontSnapshot(token, font, bytes, signal) {
  const record = checkedFrameFontRecord(token, font, bytes, signal);
  const result = await record.pending; checkAbort(signal);
  checkedFrameFontRecord(token, font, bytes, signal);
  if (!result.ok) throw result.error;
  return record.snapshot;
}

/** Full-frame raster from a native-validated static font and sampled text. */
export async function rasterizeClassicText(text, font, bytes, dimensions, signal, retained) {
  return rasterizePreparedClassicText(text, font, bytes, dimensions, signal, retained);
}

/** V3-only hook runs after the original native coverage/hash has been checked. */
export async function rasterizePreparedClassicText(text, font, bytes, dimensions, signal, retained, prepare) {
  return rasterizeText(text, font, bytes, dimensions, signal, retained, prepare);
}

/** Scene-only style override; FontFace, hashing, coverage and layout receipt
 * retain the actual font identity. Classic Canvas performs synthesis itself. */
export async function rasterizeSceneClassicText(text, font, bytes, dimensions, signal, retained, token) {
  sceneTextStyle(token, font, signal);
  return rasterizeText(text, font, bytes, dimensions, signal, retained, undefined, token);
}

async function rasterizeText(text, font, bytes, dimensions, signal, retained, prepare, sceneToken) {
  checkAbort(signal);
  if (Object.hasOwn(dimensions, "fontSynthesis") || Object.hasOwn(dimensions, "sceneFontSynthesis")) reject("INVALID_VISUAL_PLAN", "Font synthesis requires the explicit Scene raster entry point");
  validateClassicText(text);
  validateFont(font);
  const width = integer(dimensions.width, "text canvas width", 1, 16384);
  const height = integer(dimensions.height, "text canvas height", 1, 16384);
  const pixels = BigInt(width) * BigInt(height);
  if (pixels > natural(dimensions.maxPixels, "text canvas pixel admission") || pixels * 16n > natural(dimensions.maxRasterBytes, "text canvas byte admission")) reject("VISUAL_BUDGET_EXCEEDED", "Text canvas exceeds its explicit admission");
  if (!(bytes instanceof Uint8Array) || bytes.byteLength === 0) reject("INVALID_VISUAL_PLAN", "Missing verified font bytes");
  const family = `opencut_${font.contentHash}_${font.weight}_${font.style}`;
  const record = dimensions.frameFontCheck === undefined ? null : checkedFrameFontRecord(dimensions.frameFontCheck, font, bytes, signal);
  const previousFace = retained?.face;
  const defer = record?.deferLoadedFaces && previousFace?.status === "loaded"
    && verifiedFaces.get(previousFace) === fontIdentity(font)
    && previousFace.family === family && previousFace.style === font.style
    && previousFace.weight === String(font.weight);
  let fontBytes = bytes;
  if (record) {
    // A speculative raster uses the prior immutable FontFace, never unchecked
    // source bytes. Its owning offscreen frame must verify() before completion.
    if (!record.immutableFace && (!defer || record.previewOwner)) fontBytes = await frameFontSnapshot(dimensions.frameFontCheck, font, bytes, signal);
  }
  else {
    const digest = await hashBytes(bytes); checkAbort(signal);
    if (digest !== font.contentHash) reject("ASSET_HASH_MISMATCH", "Font bytes changed after native validation");
  }
  if (await hashBytes(new TextEncoder().encode(text.content)) !== font.validatedTextHash) reject("INVALID_VISUAL_PLAN", "Glyph coverage belongs to different text content");
  checkAbort(signal);
  if (sceneToken !== undefined) sceneTextStyle(sceneToken, font, signal);
  if (dimensions.frameFontCheck !== undefined) checkedFrameFontRecord(dimensions.frameFontCheck, font, bytes, signal);
  const fontSet = globalThis.fonts ?? globalThis.document?.fonts;
  if (!fontSet || typeof globalThis.FontFace !== "function") reject("CAPABILITY_UNAVAILABLE", "The admitted realm has no font loading support");
  let face, canvas, added = false, complete = false;
  const sealPreviewFace = !!record?.previewOwner && !record.immutableFace;
  try {
    try {
      face = record?.immutableFace ?? (sealPreviewFace ? null : retained?.face)
        ?? new FontFace(family, fontBytes, { style: font.style, weight: String(font.weight) });
      if (face !== previousFace) await face.load();
    }
    catch { checkAbort(signal); reject("FONT_LOAD_FAILED", "The browser rejected the verified static font bytes"); }
    checkAbort(signal);
    if (sceneToken !== undefined) sceneTextStyle(sceneToken, font, signal);
    if (dimensions.frameFontCheck !== undefined) checkedFrameFontRecord(dimensions.frameFontCheck, font, bytes, signal);
    if (face.status !== "loaded") reject("FONT_LOAD_FAILED", "The browser did not load the verified static font face");
    if (face !== previousFace) {
      fontSet.add(face); added = true;
      if (retained) {
        retained.face = face;
        if (previousFace) fontSet.delete(previousFace);
      }
    }
    if (sealPreviewFace && retained && record.snapshot === fontBytes) {
      immutablePreviewFaces.set(face, { realm: record.realm, identity: fontIdentity(font), descriptors: faceDescriptors.map(key => face[key]) });
      record.immutableFace = face;
    }
    canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) reject("CAPABILITY_UNAVAILABLE", "Text rasterization canvas is unavailable");
    if (prepare) {
      const prepared = await prepare(ctx, text, family, font);
      checkAbort(signal);
      text = prepared.text;
      validateClassicText(text);
      font = { ...font, validatedTextHash: await hashBytes(new TextEncoder().encode(text.content)) };
    }
    const drawingFont = sceneToken === undefined ? font : sceneTextStyle(sceneToken, font, signal);
    const layout = measureClassicText(ctx, text, family, drawingFont);
    drawClassicText(ctx, text, layout, family, drawingFont);
    checkAbort(signal);
    if (!previousFace || sealPreviewFace) verifiedFaces.set(face, fontIdentity(font));
    complete = true;
    return {
      canvas, width, height,
      layout: { fontId: font.fontId, fontContentHash: font.contentHash, validatedTextHash: font.validatedTextHash, fontWeight: font.weight, fontStyle: font.style, faceIndex: font.faceIndex, ...layout, transform: structuredClone(text.transform) },
    };
  } finally {
    if (added && !retained) fontSet.delete(face);
    if (canvas && !complete) { canvas.width = 0; canvas.height = 0; }
  }
}
