import { controlledWorkers } from "./font-hash-fixture.mjs";
import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createFrameFontChecks, drawClassicText, measureClassicText, rasterizeClassicText, validateClassicText, createSceneTextStyleScopeV1, rasterizeSceneClassicText, validateSceneFontSynthesisV1 } from "../src/text-layout.mjs";

const code = expected => error => error.code === expected;
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const bytes = Uint8Array.of(1, 2, 3, 4); // Font loading is mocked; this is not glyph evidence.
const text = () => ({
  content: "AV To\n中文编辑", fontSizePx: 30, lineHeightPx: 36, letterSpacingPx: 2,
  align: "center", decoration: "none", color: [1, 0.25, 0, 1],
  background: { enabled: true, color: [0, 0.5, 1, 0.5], paddingXPx: 10, paddingYPx: 6, offsetXPx: 3, offsetYPx: -4, cornerRadiusPercent: 50 },
  transform: { centerX: 320, centerY: 180, scaleX: 2, scaleY: 0.5, rotationRadians: Math.PI / 2 },
});
const font = content => ({ fontId: "fixed-font", contentHash: hash(bytes), validatedTextHash: hash(content), weight: 400, style: "normal", faceIndex: 0, glyphCoverageValidated: true });
const dimensions = { width: 640, height: 360, maxPixels: "230400", maxRasterBytes: "3686400" };
const synthesisBinding = f => ({
  schema_version: 1, binding_id: `font-synthesis-v1_${"a".repeat(64)}`, profile: "classic_chromium_italic_v1",
  actual: { id: "fixture-actual-normal-v1", family: "Fixture actual normal", weight: f.weight, style: f.style, content_hash: f.contentHash, size_bytes: f.size,
    source_url: "https://example.invalid/fixed/font.ttf", source_commit: "1".repeat(40), license_id: "OFL-1.1", license_notice: "Fixture only; not a real font" },
  requested: { weight: f.weight, style: "italic" },
});

function context() {
  const calls = [], measured = [], state = [];
  const ctx = {
    calls, measured, font: "", textAlign: "start", textBaseline: "alphabetic", letterSpacing: "0px", fillStyle: "#000",
    save() { state.push(Object.fromEntries(["font", "textAlign", "textBaseline", "letterSpacing", "fillStyle"].map(key => [key, this[key]]))); calls.push(["save"]); },
    restore() { Object.assign(this, state.pop()); calls.push(["restore"]); },
    measureText(content) {
      measured.push({ content, font: this.font, baseline: this.textBaseline, spacing: this.letterSpacing });
      return content === "AV To" ? { width: 73.5, actualBoundingBoxAscent: 11, actualBoundingBoxDescent: 9 } : { width: content ? 120 : 0, actualBoundingBoxAscent: 13, actualBoundingBoxDescent: 8 };
    },
  };
  for (const method of ["translate", "scale", "rotate", "beginPath", "roundRect", "fill", "fillText", "fillRect"]) ctx[method] = (...args) => calls.push([method, ...args]);
  return ctx;
}

test("sampled size, explicit lines, middle baselines and padding retain independent Classic geometry", () => {
  const t = text(), ctx = context(), result = measureClassicText(ctx, t, "locked", font(t.content));
  assert.deepEqual(result.block, { maxWidth: 120, height: 72, visualCenterOffset: 18 });
  assert.deepEqual(result.lines.map(line => [line.content, line.baselineY]), [["AV To", -18], ["中文编辑", 18]]);
  assert.deepEqual(result.background, { left: -67, top: -46, width: 140, height: 84, radius: 21 });
  assert.deepEqual(ctx.measured, ["AV To", "中文编辑"].map(content => ({ content, font: 'normal 400 30px "locked"', baseline: "middle", spacing: "2px" })));
  assert.equal(ctx.font, "");
  assert.equal(ctx.textBaseline, "alphabetic");
  const long = { ...t, content: "an explicit long line stays on one line\n\n尾行" };
  assert.deepEqual(measureClassicText(context(), long, "locked", font(long.content)).lines.map(line => line.baselineY), [-36, 0, 36]);
});

test("underline and strike follow actual middle-baseline ascent/descent and per-line alignment", () => {
  const t = { ...text(), align: "right", decoration: "underline" };
  const under = measureClassicText(context(), t, "locked", font(t.content));
  assert.deepEqual(under.lines[0].decoration, { left: -73.5, top: -6.9, width: 73.5, height: 2.1 });
  assert.deepEqual(under.lines[1].decoration, { left: -120, top: 28.1, width: 120, height: 2.1 });
  const strike = measureClassicText(context(), { ...t, align: "left", decoration: "lineThrough" }, "locked", font(t.content));
  assert.deepEqual(strike.lines[0].decoration, { left: 0, top: -18.7, width: 73.5, height: 2.1 });
  assert.equal(strike.background.left, -7);
  assert.equal(under.background.left, -127);
});

test("full-canvas raster applies T S R, preserves unscaled offsets, and draws background before glyphs", () => {
  const t = { ...text(), decoration: "underline" }, ctx = context();
  const layout = measureClassicText(ctx, t, "locked", font(t.content));
  ctx.calls.length = 0;
  drawClassicText(ctx, t, layout, "locked", font(t.content));
  assert.deepEqual(ctx.calls.slice(0, 4), [["save"], ["translate", 320, 180], ["scale", 2, 0.5], ["rotate", Math.PI / 2]]);
  assert.deepEqual(ctx.calls.filter(call => call[0] === "roundRect"), [["roundRect", -67, -46, 140, 84, 21]]);
  assert.deepEqual(ctx.calls.filter(call => call[0] === "fillText"), [["fillText", "AV To", 0, -18], ["fillText", "中文编辑", 0, 18]]);
  assert.ok(ctx.calls.findIndex(call => call[0] === "fill") < ctx.calls.findIndex(call => call[0] === "fillText"));
  assert.equal(ctx.calls.at(-1)[0], "restore");
  // Independently, local (10, 4) becomes (-4, 10), (-8, 5), then (312, 185).
  // The order above is material: T R S would put it at (318, 200).
});

test("finite negative padding is preserved while radius alone follows Classic clamping", () => {
  const t = text();
  t.background = { ...t.background, paddingXPx: -10, paddingYPx: -6, cornerRadiusPercent: 120 };
  assert.deepEqual(measureClassicText(context(), t, "locked", font(t.content)).background, { left: -47, top: -34, width: 100, height: 60, radius: 30 });
  t.background.cornerRadiusPercent = -1;
  assert.equal(measureClassicText(context(), t, "locked", font(t.content)).background.radius, 0);
  t.background.color[3] = 0;
  assert.deepEqual(measureClassicText(context(), t, "locked", font(t.content)).background, { left: -47, top: -34, width: 100, height: 60, radius: 0 });
  t.background.enabled = false;
  assert.equal(measureClassicText(context(), t, "locked", font(t.content)).background, null);
});

test("invalid physical samples and unsupported letter-spacing capability fail explicitly", () => {
  for (const change of [{ fontSizePx: 0 }, { lineHeightPx: -1 }, { letterSpacingPx: Infinity }, { align: "start" }, { decoration: "overline" }, { color: [1, 0, 0, 2] }]) {
    assert.throws(() => validateClassicText({ ...text(), ...change }), code("INVALID_VISUAL_PLAN"));
  }
  const ctx = context(); delete ctx.letterSpacing;
  const t = text();
  assert.throws(() => measureClassicText(ctx, t, "locked", font(t.content)), code("CAPABILITY_UNAVAILABLE"));
});

function installRealm(options = {}) {
  const originals = new Map(["FontFace", "OffscreenCanvas", "fonts"].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  const faces = [], canvases = [], active = new Set(), additions = [], deletions = [];
  class FontFace {
    constructor(family, source, descriptors) { this.family = family; this.source = source; this.descriptors = descriptors; this.status = "unloaded"; faces.push(this); }
    async load() { if (options.load) await options.load(this); else this.status = "loaded"; return this; }
  }
  class OffscreenCanvas {
    constructor(width, height) { this.width = width; this.height = height; this.ctx = context(); canvases.push(this); }
    getContext(kind) { assert.equal(kind, "2d"); if (options.context) return options.context(this); return this.ctx; }
  }
  const fonts = { add(face) { active.add(face); additions.push(face); }, delete(face) { deletions.push(face); return active.delete(face); } };
  for (const [key, value] of Object.entries({ FontFace, OffscreenCanvas, fonts })) Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  return { faces, canvases, active, additions, deletions, restore() { for (const [key, descriptor] of originals) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; } } };
}

test("verified bytes and exact-content coverage load one isolated face and release it after rasterization", async () => {
  const realm = installRealm(), t = text(), f = font(t.content);
  try {
    const raster = await rasterizeClassicText(t, f, bytes, dimensions);
    assert.equal(raster.canvas, realm.canvases[0]);
    assert.equal(raster.width, 640); assert.equal(raster.height, 360);
    assert.equal(realm.faces.length, 1);
    assert.equal(realm.faces[0].family, `opencut_${f.contentHash}_400_normal`);
    assert.equal(realm.faces[0].source, bytes);
    assert.deepEqual(realm.faces[0].descriptors, { style: "normal", weight: "400" });
    assert.equal(realm.active.size, 0);
    assert.deepEqual(realm.additions, realm.deletions);
    assert.equal(raster.layout.fontContentHash, f.contentHash);
    assert.equal(raster.layout.validatedTextHash, f.validatedTextHash);
    assert.deepEqual(raster.layout.transform, t.transform);
    assert.ok(realm.canvases[0].ctx.measured.every(item => !item.font.includes("sans-serif")));
  } finally { realm.restore(); }
});

test("changed bytes, mismatched coverage, unvalidated faces and canvas budgets fail before font allocation", async () => {
  const realm = installRealm(), t = text(), f = font(t.content);
  try {
    await assert.rejects(rasterizeClassicText(t, f, Uint8Array.of(5), dimensions), code("ASSET_HASH_MISMATCH"));
    await assert.rejects(rasterizeClassicText(t, { ...f, validatedTextHash: hash("other text") }, bytes, dimensions), code("INVALID_VISUAL_PLAN"));
    for (const change of [{ glyphCoverageValidated: false }, { faceIndex: 1 }, { weight: 500 }, { style: "oblique" }]) await assert.rejects(rasterizeClassicText(t, { ...f, ...change }, bytes, dimensions), code("INVALID_VISUAL_PLAN"));
    await assert.rejects(rasterizeClassicText(t, f, bytes, { ...dimensions, maxPixels: "230399" }), code("VISUAL_BUDGET_EXCEEDED"));
    await assert.rejects(rasterizeClassicText(t, f, bytes, { ...dimensions, maxRasterBytes: "3686399" }), code("VISUAL_BUDGET_EXCEEDED"));
    const missing = { ...dimensions }; delete missing.maxPixels;
    await assert.rejects(rasterizeClassicText(t, f, bytes, missing), code("INVALID_VISUAL_PLAN"));
    assert.equal(realm.faces.length, 0); assert.equal(realm.canvases.length, 0);
  } finally { realm.restore(); }
});

test("browser font rejection and cancellation never publish fallback glyphs or retain a font", async () => {
  const t = text(), f = font(t.content);
  let realm = installRealm({ load() { throw new Error("sanitizer rejected"); } });
  try {
    await assert.rejects(rasterizeClassicText(t, f, bytes, dimensions), code("FONT_LOAD_FAILED"));
    assert.equal(realm.canvases.length, 0); assert.equal(realm.additions.length, 0);
  } finally { realm.restore(); }
  const abort = new AbortController(), reason = new Error("cancelled by owner");
  realm = installRealm({ load(face) { face.status = "loaded"; abort.abort(reason); } });
  try {
    await assert.rejects(rasterizeClassicText(t, f, bytes, dimensions, abort.signal), error => error === reason);
    assert.equal(realm.canvases.length, 0); assert.equal(realm.active.size, 0);
  } finally { realm.restore(); }
});

test("raster failure releases loaded font and owned canvas", async () => {
  const t = text(), realm = installRealm({ context() { return null; } });
  try {
    await assert.rejects(rasterizeClassicText(t, font(t.content), bytes, dimensions), code("CAPABILITY_UNAVAILABLE"));
    assert.equal(realm.active.size, 0); assert.deepEqual(realm.additions, realm.deletions);
    assert.equal(realm.canvases[0].width, 0); assert.equal(realm.canvases[0].height, 0);
  } finally { realm.restore(); }
});

test("canvas ignoring a CSS font selection fails instead of measuring with the default family", async () => {
  const t = text(), realm = installRealm({ context(canvas) {
    Object.defineProperty(canvas.ctx, "font", { get: () => "10px sans-serif", set() {}, configurable: true });
    return canvas.ctx;
  } });
  try {
    await assert.rejects(rasterizeClassicText(t, font(t.content), bytes, dimensions), code("FONT_LOAD_FAILED"));
    assert.equal(realm.canvases[0].ctx.measured.length, 0);
    assert.equal(realm.active.size, 0);
    assert.equal(realm.canvases[0].width, 0); assert.equal(realm.canvases[0].height, 0);
  } finally { realm.restore(); }
});

function controlledFontHashes(t) {
  const fixture=controlledWorkers(t);fixture.pending.realm=fixture.newRealm();
  // A warm full-font digest must never run in the calling realm.
  const digest=crypto.subtle.digest.bind(crypto.subtle);
  t.mock.method(crypto.subtle,'digest',(algorithm,input)=>{assert.notEqual(input.byteLength,4);return digest(algorithm,input);});
  return fixture.pending;
}

test("Scene italic uses the actual normal FontFace and Classic requested Canvas style without changing the old receipt", async () => {
  const realm = installRealm(), t = text(), f = { ...font(t.content), weight: 700, size: String(bytes.length) };
  const binding = synthesisBinding(f), scope = createSceneTextStyleScopeV1([{ key: "title", font: f, binding }]);
  try {
    const token = scope.tokenFor("title", f), retained = { bytes };
    const result = await rasterizeSceneClassicText(t, f, bytes, dimensions, undefined, retained, token);
    assert.deepEqual(realm.faces[0].descriptors, { style: "normal", weight: "700" });
    assert.equal(realm.faces[0].family, `opencut_${f.contentHash}_700_normal`);
    assert.ok(realm.canvases[0].ctx.measured.every(item => item.font.startsWith("italic 700 ")));
    assert.equal(result.layout.fontStyle, "normal");
    assert.equal(result.layout.fontWeight, 700);
    assert.equal(result.layout.fontContentHash, f.contentHash);
    // Reusing the same actual face for a normal call never inherits requested
    // italic. Passing an extra token to the old entry cannot enable synthesis.
    await rasterizeClassicText(t, f, bytes, dimensions, undefined, retained, token);
    assert.equal(realm.faces.length, 1);
    assert.ok(realm.canvases[1].ctx.measured.every(item => item.font.startsWith("normal 700 ")));
    await assert.rejects(rasterizeClassicText(t, f, bytes, { ...dimensions, fontSynthesis: binding }), code("INVALID_VISUAL_PLAN"));
  } finally { scope.close(); realm.restore(); }
});

test("Scene synthesis rejects changed native identity, synthetic bold, unsupported profiles and forged or expired tokens", async () => {
  const t = text(), f = { ...font(t.content), size: String(bytes.length) }, valid = synthesisBinding(f);
  for (const change of [
    b => { b.actual.style = "italic"; }, b => { b.actual.content_hash = "f".repeat(64); },
    b => { b.actual.size_bytes = "5"; }, b => { b.actual.weight = 700; },
    b => { b.requested.weight = 700; }, b => { b.requested.style = "normal"; },
    b => { b.profile = "arbitrary-skew"; }, b => { b.binding_id = "fake"; },
    b => { b.actual.surprise = true; }, b => { b.requested.surprise = true; },
  ]) { const bad = structuredClone(valid); change(bad); assert.throws(() => validateSceneFontSynthesisV1(bad, f), code("INVALID_VISUAL_PLAN")); }
  await assert.rejects(rasterizeSceneClassicText(t, f, bytes, dimensions, undefined, undefined, {}), code("INVALID_VISUAL_PLAN"));
  const signal = new AbortController().signal, scope = createSceneTextStyleScopeV1([{ key: "title", font: f, binding: valid }], signal), token = scope.tokenFor("title", f);
  assert.throws(() => scope.tokenFor("title", { ...f, fontId: "other" }), code("INVALID_VISUAL_PLAN"));
  await assert.rejects(rasterizeSceneClassicText(t, f, bytes, dimensions, undefined, undefined, token), code("INVALID_VISUAL_PLAN"));
  scope.close();
  await assert.rejects(rasterizeSceneClassicText(t, f, bytes, dimensions, signal, undefined, token), code("INVALID_VISUAL_PLAN"));
});

test("Scene scope closing while FontFace loads cannot publish a late raster or leak a font", async () => {
  let entered, release;
  const loading = new Promise(resolve => { entered = resolve; }), pending = new Promise(resolve => { release = resolve; });
  const realm = installRealm({ async load(face) { entered(); await pending; face.status = "loaded"; } });
  const t = text(), f = { ...font(t.content), size: String(bytes.length) }, scope = createSceneTextStyleScopeV1([{ key: "title", font: f, binding: synthesisBinding(f) }]);
  try {
    const result = rasterizeSceneClassicText(t, f, bytes, dimensions, undefined, undefined, scope.tokenFor("title", f));
    await loading; scope.close(); release();
    await assert.rejects(result, code("INVALID_VISUAL_PLAN"));
    assert.equal(realm.additions.length, 0); assert.equal(realm.canvases.length, 0);
  } finally { scope.close(); release(); realm.restore(); }
});

test("Scene synthesis retains full-byte hash, exact-text coverage, cancellation and raster budgets", async () => {
  const realm = installRealm(), t = text(), f = { ...font(t.content), size: String(bytes.length) }, cancel = new AbortController();
  const scope = createSceneTextStyleScopeV1([{ key: "title", font: f, binding: synthesisBinding(f) }], cancel.signal), token = scope.tokenFor("title", f);
  try {
    await assert.rejects(rasterizeSceneClassicText(t, f, Uint8Array.of(9, 8, 7, 6), dimensions, cancel.signal, undefined, token), code("ASSET_HASH_MISMATCH"));
    await assert.rejects(rasterizeSceneClassicText({ ...t, content: "uncovered" }, f, bytes, dimensions, cancel.signal, undefined, token), code("INVALID_VISUAL_PLAN"));
    await assert.rejects(rasterizeSceneClassicText(t, f, bytes, { ...dimensions, maxRasterBytes: "1" }, cancel.signal, undefined, token), code("VISUAL_BUDGET_EXCEEDED"));
    const reason = new Error("Scene was replaced"); cancel.abort(reason);
    await assert.rejects(rasterizeSceneClassicText(t, f, bytes, dimensions, cancel.signal, undefined, token), error => error === reason);
    assert.equal(realm.faces.length, 0); assert.equal(realm.canvases.length, 0);
  } finally { scope.close(); realm.restore(); }
});
function fontChecks(source,signal,realm) {
  const f={...font(text().content),size:String(source.length)};
  const cache=new Map([[`${f.fontId}:${f.contentHash}:${f.size}`,{bytes:source}]]);
  const checks=createFrameFontChecks(signal,realm);checks.start([{font:f},{font:f}],cache);
  return {checks,f,token:checks.tokenFor(f,source),cache};
}

test('per-frame font checks hash each retained font once and render from the private snapshot, not later mutable bytes',async t=>{
  const realm=installRealm(),pending=controlledFontHashes(t),source=bytes.slice(),signal=new AbortController().signal;
  const {checks,f,token}=fontChecks(source,signal,pending.realm);
  try {
    assert.equal(pending.length,1);assert.deepEqual(Object.keys(token),[]);
    source.fill(0);pending[0].resolve();
    const first=await rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},signal);
    const second=await rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},signal);
    assert.equal(pending.length,1);assert.notEqual(realm.faces[0].source,source);assert.deepEqual(realm.faces[0].source,bytes);
    assert.deepEqual(realm.faces[1].source,bytes);first.canvas.width=second.canvas.width=0;
    await checks.close();
    // A new frame re-hashes the mutable cache; the old snapshot is not a cache proof.
    const next=fontChecks(source,signal,pending.realm);assert.equal(pending.length,2);pending[1].resolve();
    await assert.rejects(rasterizeClassicText(text(),next.f,source,{...dimensions,frameFontCheck:next.token},signal),code('ASSET_HASH_MISMATCH'));
    await next.checks.close();
  } finally {await checks.close();realm.restore();}
});

test('frame font tokens reject forged, changed identity/reference/signal, stale scope and different text coverage',async t=>{
  const realm=installRealm(),pending=controlledFontHashes(t),source=bytes.slice(),signal=new AbortController().signal;
  const {checks,f,token}=fontChecks(source,signal,pending.realm);pending[0].resolve();
  try {
    for(const change of [{fontId:'other'},{contentHash:'a'.repeat(64)},{size:'5'},{weight:700},{style:'italic'}]) {
      await assert.rejects(rasterizeClassicText(text(),{...f,...change},source,{...dimensions,frameFontCheck:token},signal),code('INVALID_VISUAL_PLAN'));
    }
    for(const invalid of [{},null,structuredClone(token)])await assert.rejects(rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:invalid},signal),code('INVALID_VISUAL_PLAN'));
    await assert.rejects(rasterizeClassicText(text(),f,source.slice(),{...dimensions,frameFontCheck:token},signal),code('INVALID_VISUAL_PLAN'));
    await assert.rejects(rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},new AbortController().signal),code('INVALID_VISUAL_PLAN'));
    await assert.rejects(rasterizeClassicText(text(),{...f,validatedTextHash:hash('wrong')},source,{...dimensions,frameFontCheck:token},signal),code('INVALID_VISUAL_PLAN'));
    await checks.close();
    await assert.rejects(rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},signal),code('INVALID_VISUAL_PLAN'));
    assert.equal(realm.faces.length,0);assert.equal(realm.canvases.length,0);
  } finally {await checks.close();realm.restore();}
});

test('closing a frame invalidates tokens immediately but waits for pending hashes, including failure and cancellation',async t=>{
  const realm=installRealm(),pending=controlledFontHashes(t);
  try {
    for(const failure of ['closed','digest','cancel']) {
      const source=bytes.slice(),controller=new AbortController(),reason=Error(failure),{checks,f,token}=fontChecks(source,controller.signal,pending.realm);
      const current=pending.at(-1);let finished=false;
      const raster=rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},controller.signal);
      const rejected=assert.rejects(raster,error=>failure==='cancel'?error===reason:error.code==='INVALID_VISUAL_PLAN');
      if(failure==='cancel')controller.abort(reason);
      const closing=checks.close().then(()=>{finished=true;});await Promise.resolve();assert.equal(finished,false);
      await assert.rejects(rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},controller.signal),error=>failure==='cancel'?error===reason:error.code==='INVALID_VISUAL_PLAN');
      if(failure==='digest')current.reject(reason);else current.resolve();
      await rejected;await closing;assert.equal(finished,true);
    }
    assert.equal(realm.faces.length,0);assert.equal(realm.canvases.length,0);
  } finally {realm.restore();}
});

test('pending font digest uses the expected hash captured at frame start despite mutable descriptor changes',async t=>{
  const realm=installRealm(),pending=controlledFontHashes(t),source=bytes.slice(),signal=new AbortController().signal;
  const originalHash='a'.repeat(64),f={...font(text().content),size:String(source.length),contentHash:originalHash};
  const checks=createFrameFontChecks(signal,pending.realm);
  checks.start([{font:f}],new Map([[`${f.fontId}:${f.contentHash}:${f.size}`,{bytes:source}]]));
  const token=checks.tokenFor(f,source);
  try {
    assert.equal(pending.length,1);
    f.contentHash=hash(source);pending[0].resolve();
    // Let the digest's continuation run while the mutable descriptor points
    // at the actual bytes, then restore the token's original fixed identity.
    await new Promise(resolve=>setImmediate(resolve));f.contentHash=originalHash;
    await assert.rejects(rasterizeClassicText(text(),f,source,{...dimensions,frameFontCheck:token},signal),code('ASSET_HASH_MISMATCH'));
    assert.equal(realm.faces.length,0);assert.equal(realm.canvases.length,0);
  } finally {await checks.close();realm.restore();}
});
