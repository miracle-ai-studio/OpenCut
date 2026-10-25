import { afterEach, expect, it, vi } from "vitest";
import { setupAudioGain } from "./audio-gain-fixture";
import type { ToolInput } from "../../../lib/editor-sdk";

afterEach(() => vi.restoreAllMocks());

it("keeps property owners isolated and refuses track gestures and unrelated writes until cancellation", async () => {
  const f = await setupAudioGain();
  try {
    const scene = f.controller.scene;
    scene.propertyDraft("old", true);
    scene.propertyDraft("new", true);
    scene.propertyDraft("old", false);
    expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual(["new"]);
    expect(() => scene.audioGain.begin("video")).toThrow("property draft");
    await expect(scene.audioGain.mute("video", true)).rejects.toThrow("property draft");
    scene.select(null);
    expect(f.controller.getSnapshot().scene!.selectedId).toBe("video");
    expect(f.controller.getSnapshot().scene!.gainDraft).toBeNull();
    expect(f.gainRequests).toHaveLength(0);
    const head = f.controller.getSnapshot().scene!.head;
    const request: ToolInput<"opencut_edit_apply"> = { project_id: head.project.id, expected_revision: head.revision, idempotency_key: "blocked", label: "Blocked", operations: [] };
    await expect(scene.applyPlanned(request, "scene")).rejects.toThrow("property draft");
    expect(f.call.mock.calls.some(([name]) => name === "opencut_edit_recovery_stage")).toBe(false);
    scene.propertyDraft("new", false);
    const id = scene.audioGain.begin("video");
    expect(f.controller.getSnapshot().scene!.gainDraft!.id).toBe(id);
    scene.audioGain.cancel(id);
  } finally { f.controller.dispose(); }
});

it("hands an unknown property save to the original durable recovery without permitting another transaction", async () => {
  const f = await setupAudioGain();
  try {
    const scene = f.controller.scene, state = f.controller.getSnapshot().scene!;
    const element = structuredClone(state.head.project.sequences[0].tracks[0].elements[0]);
    if (element.content.kind !== "video") throw Error();
    element.content.audio.muted = true;
    const request: ToolInput<"opencut_edit_apply"> = { project_id: state.head.project.id, expected_revision: state.head.revision,
      idempotency_key: "original-property", label: "Volume", operations: [{ type: "element_put", sequence_id: "scene", track_id: "track", element }] };
    const original = f.call.getMockImplementation()!;
    let lost = true;
    f.call.mockImplementation(async (name, args, options) => {
      if (name === "opencut_edit_apply" && lost) { lost = false; throw Error("Response lost"); }
      return original(name, args, options);
    });
    scene.propertyDraft("property", true);
    await expect(scene.applyPlanned(request, "scene", "property")).rejects.toThrow("Response lost");
    expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]);
    const record = structuredClone(f.controller.getSnapshot().scene!.recovery!.record);
    expect(record!.context.intent).toEqual({ kind: "public_apply", request });
    await expect(scene.applyPlanned({ ...request, idempotency_key: "unrelated" }, "scene")).rejects.toThrow("original edit");
    expect(f.controller.getSnapshot().scene!.recovery!.record).toEqual(record);
    const result = await scene.recovery.retry();
    expect(result.kind).toBe("public_apply");
    expect(f.controller.getSnapshot().scene!.recovery!.record).toBeNull();
    expect(f.current().project.sequences[0].tracks[0].elements[0]).toEqual(element);
    const dispatches = f.call.mock.calls.filter(([name]) => name === "opencut_edit_apply");
    expect(dispatches).toHaveLength(2);
    expect(dispatches.map(([, args]) => args)).toEqual([request, request]);
  } finally { f.controller.dispose(); }
});
