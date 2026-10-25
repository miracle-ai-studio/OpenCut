// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { setup as createFixture } from "../workspace/scene/fixture";
import { setupSourceAudio } from "../workspace/scene/source-audio-fixture";
import { shortcutSequenceEnd, sceneShortcutActions } from "./scene-actions";
import type { WorkspaceController } from "../workspace/controller";

const controllers: WorkspaceController[] = [];
afterEach(() => {
  controllers.splice(0).forEach((controller) => controller.dispose());
  vi.restoreAllMocks();
});
async function setup() {
  const f = await createFixture();
  controllers.push(f.controller);
  const state = {
    ...f.controller.getSnapshot(),
    scene: structuredClone(f.controller.getSnapshot().scene)!,
  };
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  vi.spyOn(f.controller.scene,"currentPlayhead").mockImplementation(()=>state.scene.playhead);
  return {
    ...f,
    state,
    scene: state.scene,
    sequence: state.scene.head.project.sequences[0],
  };
}

it("uses rational frame steps and clamps navigation through the existing session seek", async () => {
  const f = await setup(),
    seek = vi.spyOn(f.controller.scene, "seek").mockImplementation(() => {});
  const settings = f.scene.head.project.settings;
  if (settings.profile !== "classic_global") throw new Error("fixture format");
  settings.settings.fps = { numerator: 30000, denominator: 1001 };
  f.scene.playhead = { ticks: "1", timebase: 3 };
  const actions = sceneShortcutActions(f.controller);
  actions["frame-step-forward"]!.run();
  expect(seek).toHaveBeenLastCalledWith({ ticks: "3667", timebase: 10000 });
  f.scene.playhead = { ticks: "0", timebase: 1 };
  actions["frame-step-backward"]!.run();
  expect(seek).toHaveBeenLastCalledWith({ ticks: "0", timebase: 1 });
  f.scene.playhead = { ticks: "119", timebase: 10 };
  actions["jump-forward"]!.run();
  expect(seek).toHaveBeenLastCalledWith({ ticks: "12", timebase: 1 });
  expect(shortcutSequenceEnd(f.sequence)).toEqual({ ticks: "12", timebase: 1 });
  expect(
    f.call.mock.calls.some(([name]) => name === "opencut_edit_apply"),
  ).toBe(false);
});

it("uses the current history target and requests one-step undo or redo", async () => {
  const f = await setup();
  f.scene.navigation.undo = { transaction_id: "original-edit", label: "Edit" };
  const plan = vi
      .spyOn(f.controller.scene, "planUndo")
      .mockResolvedValue(undefined),
    apply = vi.spyOn(f.controller.scene, "applyHistory");
  await sceneShortcutActions(f.controller).undo!.run();
  expect(plan).toHaveBeenCalledExactlyOnceWith("original-edit", true);
  expect(apply).not.toHaveBeenCalled();
  f.scene.historical = structuredClone(f.scene.head);
  expect(sceneShortcutActions(f.controller).undo!.enabled).toBe(false);
});

it("rejects stale revision, selection and busy callbacks before a write", async () => {
  const f = await setup(),
    write = vi.spyOn(f.controller.scene, "write").mockResolvedValue({} as never);
  f.scene.selectedId = "image";
  const initial = sceneShortcutActions(f.controller);
  f.state.busy = true;
  expect(() => initial["toggle-elements-visibility-selected"]!.run()).toThrow(
    /recover/,
  );
  f.state.busy = false;
  const callback = sceneShortcutActions(f.controller)[
    "toggle-elements-visibility-selected"
  ]!;
  // Replace the aggregate as the real controller does; captured source facts
  // cannot silently follow a later selection or revision.
  f.state.scene = { ...f.scene, selectedId: null };
  expect(() => callback.run()).toThrow(/view changed/);
  f.state.scene = { ...f.scene, head: { ...f.scene.head, revision: "r2" } };
  expect(() => callback.run()).toThrow(/view changed/);
  expect(write).not.toHaveBeenCalled();
});

it("visibility is one atomic public edit and preserves all source, visual and channel fields", async () => {
  const f = await setup(),
    write = vi.spyOn(f.controller.scene, "write").mockResolvedValue({} as never);
  f.scene.selectedId = "image";
  const element = structuredClone(f.sequence.tracks[0].elements[0]);
  await sceneShortcutActions(f.controller)[
    "toggle-elements-visibility-selected"
  ]!.run();
  expect(write).toHaveBeenCalledExactlyOnceWith("Toggle element visibility", [
    {
      type: "element_put",
      sequence_id: "scene",
      track_id: "track",
      element: { ...element, enabled: false },
    },
  ]);
  expect(f.sequence.tracks[0].elements[0]).toEqual(element);
  f.sequence.tracks[0].locked = true;
  expect(
    sceneShortcutActions(f.controller)["toggle-elements-visibility-selected"]!
      .enabled,
  ).toBe(false);
});

it("retains the live mute controller for a single source and uses one atomic write for multiple sources", async () => {
  const f = await setupSourceAudio();
  controllers.push(f.controller);
  const state = {
    ...f.controller.getSnapshot(),
    scene: structuredClone(f.controller.getSnapshot().scene)!,
  };
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const mute = vi
    .spyOn(f.controller.scene.audioGain, "mute")
    .mockResolvedValue(undefined);
  const write = vi
    .spyOn(f.controller.scene, "write")
    .mockResolvedValue({} as never);
  await sceneShortcutActions(f.controller)[
    "toggle-elements-muted-selected"
  ]!.run();
  expect(mute).toHaveBeenCalledExactlyOnceWith("video", true);
  expect(write).not.toHaveBeenCalled();
  const track = state.scene.head.project.sequences[0].tracks[0];
  track.elements.push({ ...structuredClone(track.elements[0]), id: "second" });
  state.scene.timelineEdit.selection = track.elements.map((element) => ({
    track_id: track.id,
    element_id: element.id,
  }));
  await sceneShortcutActions(f.controller)[
    "toggle-elements-muted-selected"
  ]!.run();
  expect(write).toHaveBeenCalledTimes(1);
  expect(write.mock.calls[0][1]).toHaveLength(2);
  state.scene.audioPreview.phase = "playing";
  expect(
    sceneShortcutActions(f.controller)["toggle-elements-muted-selected"]!.enabled,
  ).toBe(false);
});

it("a late Stop playback result cannot seek into another editor view", async () => {
  const f = await setup();
  let finish!: () => void;
  vi.spyOn(f.controller.scene.audioPreview, "pause").mockImplementation(
    () =>
      new Promise<void>((done) => {
        finish = done;
      }),
  );
  const seek = vi.spyOn(f.controller.scene, "seek").mockImplementation(() => {});
  const pending = sceneShortcutActions(f.controller)["stop-playback"]!.run();
  f.state.scene = { ...f.scene, sequenceId: "other" };
  finish();
  await expect(pending).rejects.toThrow(/view changed/);
  expect(seek).not.toHaveBeenCalled();
});
