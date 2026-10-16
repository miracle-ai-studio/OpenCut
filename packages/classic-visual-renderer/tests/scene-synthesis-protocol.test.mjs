import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { blendBeginV1 } from "./blend-fixture.mjs";
import { validateBlendBeginV1, validateSceneBeginV1 } from "../host/protocol.mjs";
import { browserHooks } from "../host/browser.mjs";
import { fontResource } from "../src/text-frame.mjs";

function sceneBegin() {
  const packet = blendBeginV1(), blend = packet.plan, graphic = blend.execution;
  const font = { fontId: "project-font", contentHash: "a".repeat(64), size: "4", weight: 700, style: "normal", faceIndex: 0, glyphCoverageValidated: true,
    validatedTextHash: createHash("sha256").update("Title").digest("hex") };
  const text = { content: "Title", fontSizePx: 2, lineHeightPx: 2.4, letterSpacingPx: 0, align: "center", decoration: "none", color: [1, 1, 1, 1],
    transform: { centerX: 4, centerY: 2, scaleX: 1, scaleY: 1, rotationRadians: 0 },
    background: { enabled: false, color: [0, 0, 0, 0], paddingXPx: 0, paddingYPx: 0, offsetXPx: 0, offsetYPx: 0, cornerRadiusPercent: 0 } };
  graphic.execution.execution.texts = [{ elementId: "title", font, text, opacity: 1 }];
  graphic.execution.execution.order.push("title"); graphic.order.push("title");
  blend.blends.push({ elementId: "title", blendMode: "normal" });
  const binding = { schema_version: 1, binding_id: `font-synthesis-v1_${"b".repeat(64)}`, profile: "classic_chromium_italic_v1",
    actual: { id: "fixture-normal-v1", family: "Fixture", weight: 700, style: "normal", source_url: "https://example.invalid/font.ttf", source_commit: "1".repeat(40),
      content_hash: font.contentHash, size_bytes: font.size, license_id: "OFL-1.1", license_notice: "Mock only" }, requested: { weight: 700, style: "italic" } };
  packet.type = "beginSceneFrameV1";
  packet.plan = { identity: { ...blend.identity, profile: "classic-v31-gpu-scene-frame-v1" }, execution: blend,
    stacks: graphic.order.map(elementId => ({ elementId, masks: [], effects: [] })), maskFonts: [],
    fontSynthesis: [{ elementId: "title", ownerId: "title", ownerKind: "text", fontId: font.fontId, binding }],
    steps: graphic.order.map(element_id => ({ kind: "layer", element_id })), background: { kind: "color", color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 } } };
  packet.fonts = [{ fontId: font.fontId, handle: "private-font" }];
  return packet;
}
const invalid = error => error.code === "INVALID_VISUAL_PLAN";

test("Scene begin requires an explicit map, rejects extra or duplicate owners, and keeps delegated font metadata normal", () => {
  const packet = validateSceneBeginV1(sceneBegin());
  assert.equal(packet.plan.execution.execution.execution.execution.texts[0].font.style, "normal");
  assert.equal(packet.plan.fontSynthesis[0].binding.requested.style, "italic");
  for (const mutate of [
    p => { delete p.plan.fontSynthesis; },
    p => { p.plan.fontSynthesis.push(structuredClone(p.plan.fontSynthesis[0])); },
    p => { p.plan.fontSynthesis[0].elementId = "shape"; },
    p => { p.plan.fontSynthesis[0].ownerId = "different-owner"; },
    p => { p.plan.fontSynthesis[0].binding.requested.weight = 400; },
    p => { p.plan.fontSynthesis[0].binding.actual.size_bytes = "5"; },
  ]) { const bad = sceneBegin(); mutate(bad); assert.throws(() => validateSceneBeginV1(bad), invalid); }
  const old = { ...packet, type: "beginBlendFrameV1", plan: packet.plan.execution };
  assert.equal(validateBlendBeginV1(old).plan.identity.profile, "classic-v31-gpu-blend-frame-v1");
  old.plan = { ...old.plan, fontSynthesis: packet.plan.fontSynthesis };
  assert.throws(() => validateBlendBeginV1(old), invalid);
});

test("Scene private font reads authorize the actual normal descriptor and cannot request italic bytes", async () => {
  const packet = validateSceneBeginV1(sceneBegin()), exposed = new Map(), calls = [];
  await browserHooks.configurePage({ begin: packet, signal: new AbortController().signal,
    page: { exposeFunction: async (name, value) => exposed.set(name, value) },
    broker: { request: async (op, args) => { calls.push({ op, args }); return {}; } }, writer: { send: async () => {} } });
  const callback = exposed.get("__opencutVisualCallback"), font = fontResource(packet.plan.execution.execution.execution.execution.texts[0].font);
  await callback("resolveVerifiedFontBytes", { font });
  assert.deepEqual(calls, [{ op: "resolveVerifiedFontBytes", args: { font, handle: "private-font" } }]);
  assert.throws(() => callback("resolveVerifiedFontBytes", { font: { ...font, style: "italic" } }), error => error.code === "NOT_AUTHORIZED");
});
