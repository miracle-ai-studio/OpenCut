import test from "node:test";
import assert from "node:assert/strict";
import { validateBegin, validateTextBegin } from "../host/protocol.mjs";
import { browserHooks } from "../host/browser.mjs";
import { executeVisualPage, executeTextVisualPage } from "../host/page.mjs";
import { fontResource } from "../src/text-frame.mjs";

const code = expected => error => error.code === expected;
function begin() {
  const identity = { projectId: "p", revision: "r", sequenceId: "s", time: { ticks: "1", timebase: 7 }, width: 8, height: 4, profile: "classic-v31-gpu-normal-text-frame-v2" };
  const image = { assetId: "image", contentHash: "1".repeat(64), size: "4", kind: "image" };
  const font = { fontId: "font", contentHash: "2".repeat(64), size: "4", weight: 400, style: "normal", faceIndex: 0, glyphCoverageValidated: true, validatedTextHash: "3".repeat(64) };
  const text = { content: "Title", fontSizePx: 2, lineHeightPx: 2.4, letterSpacingPx: 0, align: "center", decoration: "none", color: [1, 1, 1, 1],
    transform: { centerX: 4, centerY: 2, scaleX: 1, scaleY: 1, rotationRadians: 0 },
    background: { enabled: false, color: [0, 0, 0, 0], paddingXPx: 0, paddingYPx: 0, offsetXPx: 0, offsetYPx: 0, cornerRadiusPercent: 0 } };
  return { protocolVersion: 1, type: "beginTextFrameV2", executionId: "text-frame",
    plan: { identity, media: { identity: { ...structuredClone(identity), profile: "classic-v31-gpu-normal-frame" }, resources: [image], layers: [{ elementId: "back", resource: image, decode: { kind: "image" } }] },
      texts: [{ elementId: "title", text, font, opacity: .5 }], order: ["back", "title"] },
    resources: [{ assetId: "image", handle: "private-media" }], fonts: [{ fontId: "font", handle: "private-font" }],
    admission: { maxEncodedBytes: 8, maxFramePixels: "32", maxDecodedPixels: "66", maxGpuBytes: "4096", maxRgbaBytes: "4096", maxOutputBytes: "1048576", maxRssBytes: "536870912", deadlineMs: 1000, quotaProfile: "monitored-terminate" },
    runtime: { chromiumExecutable: "/host/configured/browser" } };
}
function legacy(packet) { const old = structuredClone(packet); old.type = "begin"; old.plan = old.plan.media; delete old.fonts; return old; }
async function configured(packet) {
  const exposed = new Map(), calls = [], sent = [], cancel = new AbortController();
  const realm = { begin: packet, signal: cancel.signal, page: { exposeFunction: async (name, fn) => exposed.set(name, fn) },
    broker: { request: async (op, args) => { calls.push({ op, args }); return { accepted: true }; } },
    writer: { send: async packet => sent.push(packet) } };
  await browserHooks.configurePage(realm);
  return { realm, calls, sent, cancel, callback: exposed.get("__opencutVisualCallback"), rgba: exposed.get("__opencutVisualRgba") };
}

test("text frames select a new operation with the unchanged transport and preserve strict V1 dispatch", () => {
  const packet = begin(), old = legacy(packet);
  assert.equal(validateTextBegin(packet).admission.monitorIntervalMs, 100);
  assert.equal(validateBegin(old).plan.identity.profile, "classic-v31-gpu-normal-frame");
  assert.throws(() => validateBegin(packet), code("INVALID_HOST_PROTOCOL"));
  assert.throws(() => validateBegin({ ...packet, type: "begin" }), code("INVALID_HOST_PROTOCOL"));
  assert.throws(() => validateTextBegin(old), code("INVALID_HOST_PROTOCOL"));
  assert.throws(() => validateTextBegin({ ...packet, protocolVersion: 2 }), code("INVALID_HOST_PROTOCOL"));
});

test("V2 begin requires complete private font handles and validates the media partition with V1 rules", () => {
  const corruptions = [
    p => { delete p.fonts; }, p => { p.fonts = []; },
    p => { p.fonts.push({ ...p.fonts[0] }); },
    p => { p.fonts[0].fontId = "image"; }, p => { p.fonts[0].handle = ""; },
    p => { p.fonts[0].handle = "a".repeat(1025); }, p => { p.resources = []; },
    p => { p.plan.media.resources[0].kind = "text"; },
  ];
  for (const corrupt of corruptions) { const p = begin(); corrupt(p); assert.throws(() => validateTextBegin(p), code("INVALID_HOST_PROTOCOL")); }
  const p = begin(); p.plan.media.identity.time.ticks = "2";
  assert.throws(() => validateTextBegin(p), code("INVALID_VISUAL_PLAN"));
  const oversized = begin(); oversized.admission.maxEncodedBytes = 7;
  assert.throws(() => validateTextBegin(oversized), code("VISUAL_BUDGET_EXCEEDED"));
});

test("font callbacks use their independent fixed descriptor and private handle namespace", async () => {
  const p = validateTextBegin(begin()), gate = await configured(p), font = fontResource(p.plan.texts[0].font);
  assert.deepEqual(await gate.callback("resolveVerifiedFontBytes", { font }), { accepted: true });
  assert.deepEqual(gate.calls, [{ op: "resolveVerifiedFontBytes", args: { font, handle: "private-font" } }]);
  for (const bad of [{ ...font, fontId: "other" }, { ...font, style: "italic" }, { ...font, weight: 700 }, { ...font, contentHash: "f".repeat(64) }, { ...font, handle: "injected" }, p.plan.texts[0].font]) {
    assert.throws(() => gate.callback("resolveVerifiedFontBytes", { font: bad }), code("NOT_AUTHORIZED"));
  }
  assert.throws(() => gate.callback("resolveVerifiedBytes", { resource: font }), code("NOT_AUTHORIZED"));
  assert.throws(() => gate.callback("admitDecode", { elementId: "title" }), code("NOT_AUTHORIZED"));
  await gate.callback("resolveVerifiedBytes", { resource: p.plan.media.resources[0] });
  assert.equal(gate.calls.at(-1).args.handle, "private-media");
  await gate.callback("readVerifiedBytes", { transferId: "font-1", offset: "0", count: 4 });
  await gate.callback("releaseVerifiedBytes", { transferId: "font-1" });
  assert.throws(() => gate.callback("readVerifiedBytes", { transferId: "font-1", offset: "0", count: 65537 }), code("INVALID_VISUAL_PLAN"));
});

test("V2 callback gate requires a complete text observation set and V1 rejects added callbacks", async () => {
  const gate = await configured(validateTextBegin(begin()));
  for (const rasters of [[], [{ elementId: "other" }], [{ elementId: "title" }, { elementId: "title" }]]) {
    assert.throws(() => gate.callback("bindTextFrame", { rasters }), code("INVALID_VISUAL_PLAN"));
  }
  const rasters = [{ elementId: "title", width: 8, height: 4, layout: { nativeValidates: true } }];
  await gate.callback("bindTextFrame", { rasters });
  assert.deepEqual(gate.calls, [{ op: "bindTextFrame", args: { rasters } }]);
  const old = await configured(validateBegin(legacy(begin())));
  assert.throws(() => old.callback("bindTextFrame", { rasters }), code("INVALID_HOST_CALLBACK"));
  assert.throws(() => old.callback("resolveVerifiedFontBytes", { font: fontResource(begin().plan.texts[0].font) }), code("INVALID_HOST_CALLBACK"));
  const reason = new Error("cancelled"); gate.cancel.abort(reason);
  assert.throws(() => gate.callback("bindTextFrame", { rasters }), error => error === reason);
});

test("serialized page adapters select the exact renderer export and always destroy their GPU realm", async () => {
  const calls = [], packets = [];
  let destroyed = 0;
  const render = version => async (plan, host, gpu) => {
    calls.push({ version, plan, admission: host.admission, gpu });
    await host.rgba(4, Uint8Array.of(0, 15, 128, 255));
    return { version };
  };
  globalThis.__opencutVisualModule = { renderClassicFrame: render(1), renderClassicTextFrameV2: render(2), closeGpuRealm: realm => realm.device.destroy() };
  globalThis.__opencutVisualGpu = { device: { destroy: () => destroyed++ } };
  globalThis.__opencutVisualRgba = async packet => packets.push(packet);
  try {
    const p = begin();
    assert.deepEqual(await executeVisualPage({ plan: p.plan.media, admission: p.admission }), { ok: true, receipt: { version: 1 } });
    assert.deepEqual(await executeTextVisualPage({ plan: p.plan, admission: p.admission }), { ok: true, receipt: { version: 2 } });
    assert.deepEqual(calls.map(call => call.version), [1, 2]);
    assert.equal(calls[1].plan, p.plan);
    assert.equal(destroyed, 2);
    assert.deepEqual(packets, Array(2).fill({ offsetBytes: "4", bytesHex: "000f80ff" }));
    globalThis.__opencutVisualModule.renderClassicTextFrameV2 = async () => { throw Object.assign(new Error("font rejected"), { code: "INVALID_FONT_RESOURCE" }); };
    assert.deepEqual(await executeTextVisualPage({ plan: p.plan, admission: p.admission }), { ok: false, error: { code: "INVALID_FONT_RESOURCE" } });
    assert.equal(destroyed, 3);
  } finally {
    delete globalThis.__opencutVisualModule; delete globalThis.__opencutVisualGpu; delete globalThis.__opencutVisualRgba;
  }
});

test("host render dispatch distinguishes V2 from the original page entry", async () => {
  const functions = [];
  const page = { evaluate: async fn => { functions.push(fn); return { ok: true, receipt: {} }; } };
  await browserHooks.render({ begin: begin(), page });
  await browserHooks.render({ begin: legacy(begin()), page });
  assert.deepEqual(functions, [executeTextVisualPage, executeVisualPage]);
});
