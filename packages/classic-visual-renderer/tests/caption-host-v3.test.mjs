import test from "node:test";
import assert from "node:assert/strict";
import { validateCaptionBeginV3, validateTextBegin } from "../host/protocol.mjs";
import { browserHooks } from "../host/browser.mjs";
import { executeCaptionVisualPageV3 } from "../host/page.mjs";
import { CAPTION_FRAME_PROFILE_V3 } from "../src/caption-plan-v3.mjs";
import { CAPTION_WRAP_PROFILE_V3 } from "../src/caption-layout-v3.mjs";
import { fontResource } from "../src/text-frame.mjs";
const code = expected => error => error.code === expected;
function begin() {
  const identity = { projectId: "p", revision: "r", sequenceId: "s", time: { ticks: "1", timebase: 7 }, width: 8, height: 4, profile: "classic-v31-gpu-normal-text-frame-v2" };
  const font = { fontId: "font", contentHash: "2".repeat(64), size: "4", weight: 400, style: "normal", faceIndex: 0, glyphCoverageValidated: true, validatedTextHash: "3".repeat(64) };
  const text = { content: "Caption", fontSizePx: 2, lineHeightPx: 2.4, letterSpacingPx: 0, align: "center", decoration: "none", color: [1, 1, 1, 1], transform: { centerX: 4, centerY: 2, scaleX: 1, scaleY: 1, rotationRadians: 0 }, background: { enabled: false, color: [0, 0, 0, 0], paddingXPx: 0, paddingYPx: 0, offsetXPx: 0, offsetYPx: 0, cornerRadiusPercent: 0 } };
  const execution = { identity, media: { identity: { ...identity, profile: "classic-v31-gpu-normal-frame" }, resources: [], layers: [] }, texts: [{ elementId: "caption", text, font, opacity: 1 }], order: ["caption"] };
  return { protocolVersion: 1, type: "beginCaptionFrameV3", executionId: "caption-frame", plan: { identity: { ...identity, profile: CAPTION_FRAME_PROFILE_V3 }, execution, captions: [{ elementId: "caption", maxWidthPx: 6, bottomMarginPx: .2, wrapProfile: CAPTION_WRAP_PROFILE_V3 }] }, resources: [], fonts: [{ fontId: "font", handle: "private-font" }], admission: { maxEncodedBytes: 8, maxFramePixels: "32", maxDecodedPixels: "66", maxGpuBytes: "4096", maxRgbaBytes: "4096", maxOutputBytes: "1048576", maxRssBytes: "536870912", deadlineMs: 1000, quotaProfile: "monitored-terminate" }, runtime: { chromiumExecutable: "/host/browser" } };
}
async function configured(packet) {
  const exposed = new Map(), calls = [];
  const realm = { begin: packet, signal: new AbortController().signal, page: { exposeFunction: async (name, fn) => exposed.set(name, fn) }, broker: { request: async (op, args) => { calls.push({ op, args }); return {}; } }, writer: { send: async () => {} } };
  await browserHooks.configurePage(realm);
  return { callback: exposed.get("__opencutVisualCallback"), calls };
}
test("V3 uses a distinct begin while retaining the original fixed V2 resource admission", () => {
  const packet = begin();
  assert.equal(validateCaptionBeginV3(packet).plan.identity.profile, CAPTION_FRAME_PROFILE_V3);
  assert.throws(() => validateTextBegin(packet), code("INVALID_HOST_PROTOCOL"));
  for (const change of [p => p.fonts = [], p => p.plan.execution.identity.revision = "other", p => p.plan.captions[0].bottomMarginPx = 0, p => p.plan.captions.push(p.plan.captions[0])]) {
    const corrupt = begin(); change(corrupt);
    assert.throws(() => validateCaptionBeginV3(corrupt));
  }
});
test("V3 retains font authority and admits only complete caption observation sets", async () => {
  const packet = validateCaptionBeginV3(begin()), gate = await configured(packet);
  const font = fontResource(packet.plan.execution.texts[0].font);
  await gate.callback("resolveVerifiedFontBytes", { font });
  assert.equal(gate.calls[0].args.handle, "private-font");
  const rasters = [{ elementId: "caption" }];
  const captions = [{ elementId: "caption", originalTextHash: "3".repeat(64), wrappedText: "Cap\ntion", measurements: [{ content: "Caption", width: 10 }] }];
  assert.throws(() => gate.callback("bindTextFrame", { rasters }), code("INVALID_HOST_CALLBACK"));
  for (const invalid of [{ rasters: [], captions }, { rasters, captions: [] }, { rasters, captions: [captions[0], captions[0]] }, { rasters, captions: [{ ...captions[0], originalTextHash: "4".repeat(64) }] }]) {
    assert.throws(() => gate.callback("bindCaptionFrameV3", invalid), code("INVALID_VISUAL_PLAN"));
  }
  await gate.callback("bindCaptionFrameV3", { rasters, captions });
  assert.equal(gate.calls.at(-1).op, "bindCaptionFrameV3");
});
test("V2 cannot opt into V3 callbacks and host render selects the new serialized page entry", async () => {
  const packet = begin(), old = { ...packet, type: "beginTextFrameV2", plan: packet.plan.execution };
  const gate = await configured(validateTextBegin(old));
  assert.throws(() => gate.callback("bindCaptionFrameV3", { rasters: [], captions: [] }), code("INVALID_HOST_CALLBACK"));
  let selected;
  await browserHooks.render({ begin: packet, page: { evaluate: async fn => { selected = fn; return { ok: true, receipt: {} }; } } });
  assert.equal(selected, executeCaptionVisualPageV3);
});
