// @vitest-environment jsdom
// Editor interactions: clipboard snapshots, selection, settings and atomic deletion.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { TimelinePanel } from "../timeline/panel";
import { SettingsPanel } from "../inspector/sequence-settings";
import { WorkspaceProvider } from "./context";
import { WorkspaceController } from "./controller";
import { EditorApi } from "./api";
import { projectRef, type Snapshot } from "../model/types";

beforeAll(() => {
  vi.stubGlobal("PointerEvent", MouseEvent);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const time = (ticks: number) => ({ ticks: String(ticks), timebase: 24 });
function snapshot(): Snapshot {
  return {
    format: "visual",
    revision: "head",
    curves: [],
    visual: [
      {
        sequence_id: "main",
        shapes: [],
        masks: [],
        transitions: [],
        effects: [
          {
            id: "blur",
            target_id: "clip",
            effect: { kind: "gaussian_blur", radius_px: 12 },
          },
        ],
      },
    ],
    project: {
      id: "project",
      name: "Project",
      schema_version: 2,
      default_sequence_id: "main",
      assets: [
        {
          id: "asset",
          name: "Test",
          kind: "video",
          uri: "test.mp4",
          duration: time(48),
          metadata: {},
        },
      ],
      sequences: [
        {
          id: "main",
          name: "Main",
          width: 640,
          height: 360,
          fps: { numerator: 24, denominator: 1 },
          tracks: [
            {
              id: "track",
              name: "Video",
              kind: "video",
              enabled: true,
              muted: false,
              locked: false,
              params: {},
              clips: [
                {
                  id: "clip",
                  asset_id: "asset",
                  timeline_range: { start: time(0), duration: time(24) },
                  source_range: { start: time(0), duration: time(24) },
                  params: {},
                  channels: [],
                },
              ],
            },
          ],
        },
      ],
    },
  };
}
async function setup(
  component: "timeline" | "settings",
  configure?: (s: Snapshot) => void,
) {
  const api = new EditorApi("http://localhost:8788", "token");
  let current = snapshot();
  configure?.(current);
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(current)]);
  vi.spyOn(api, "inspect").mockImplementation(async () => current);
  vi.spyOn(api, "history").mockImplementation(async () => ({
    project_id: "project",
    head_revision: current.revision,
    revisions: [],
    navigation: null,
    next_cursor: null,
  }));
  vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
  const controller = new WorkspaceController(() => api);
  await controller.connect("x", "t");
  await controller.open(projectRef(current));
  controller.select({ kind: "clip", id: "clip", trackId: "track" });
  const mutate = vi.spyOn(controller, "mutate").mockResolvedValue({} as never);
  render(
    <WorkspaceProvider controller={controller}>
      {component === "timeline" ? <TimelinePanel /> : <SettingsPanel />}
    </WorkspaceProvider>,
  );
  return {
    controller,
    mutate,
    change: async (edit: (s: Snapshot) => void) => {
      current = structuredClone(current);
      current.revision = "new-head";
      edit(current);
      await act(async () => {
        await controller.write(async () => {});
      });
    },
  };
}
it("copy freezes visual effects before later edits to the source", async () => {
  const { change, mutate } = await setup("timeline");
  fireEvent.keyDown(document.body, { key: "c", ctrlKey: true });
  await change((s) => {
    s.visual[0].effects[0].effect = { kind: "gaussian_blur", radius_px: 40 };
  });
  fireEvent.keyDown(document.body, { key: "v", ctrlKey: true });
  await waitFor(() => expect(mutate).toHaveBeenCalled());
  const operations = mutate.mock.calls[0][2]!;
  const effect = operations.find((op) => op.type === "effect.put");
  expect(effect?.type === "effect.put" ? effect.effect.effect : null).toEqual({
    kind: "gaussian_blur",
    radius_px: 12,
  });
});
it("copy retains visual effects after the source has been removed", async () => {
  const { change, mutate } = await setup("timeline");
  fireEvent.keyDown(document.body, { key: "c", ctrlKey: true });
  await change((s) => {
    s.project.sequences[0].tracks[0].clips = [];
    s.visual[0].effects = [];
  });
  fireEvent.keyDown(document.body, { key: "v", ctrlKey: true });
  await waitFor(() => expect(mutate).toHaveBeenCalled());
  expect(
    mutate.mock.calls[0][2]!.filter((op) => op.type === "effect.put"),
  ).toHaveLength(1);
});
it("settings do not overwrite an externally changed frame rate on another apply", async () => {
  const { change, mutate } = await setup("settings");
  await change((s) => {
    s.project.sequences[0].fps = { numerator: 30, denominator: 1 };
  });
  fireEvent.click(screen.getByRole("button", { name: "Apply settings" }));
  await waitFor(() => expect(mutate).toHaveBeenCalled());
  const operation = mutate.mock.calls[0][1][0];
  expect(
    operation.type === "sequence.update_settings" ? operation.fps : null,
  ).toEqual({ numerator: 30, denominator: 1 });
});

it("clipboard freezes masks and curve overrides and remaps their identities", async () => {
  const { change, mutate } = await setup("timeline", (s) => {
    s.project.sequences[0].tracks[0].clips[0].channels = [
      {
        parameter: "opacity",
        keyframes: [
          { id: "key", time: time(0), value: 1, interpolation: "linear" },
        ],
      },
    ];
    s.visual[0].masks = [
      {
        id: "mask",
        target_id: "clip",
        shape: { kind: "rectangle", x: 0.1, y: 0.2, width: 0.8, height: 0.6 },
        inverted: false,
        feather: 0.03,
      },
    ];
    s.curves = [
      {
        id: "curve",
        schema_version: 1,
        sequence_id: "main",
        clip_id: "clip",
        parameter: "opacity",
        keyframe_id: "key",
        curve: { x1: 0.1, y1: 0.2, x2: 0.3, y2: 0.4 },
      },
    ];
  });
  fireEvent.keyDown(document.body, { key: "c", ctrlKey: true });
  await change((s) => {
    s.visual[0].masks = [];
    s.curves = [];
    s.project.sequences[0].tracks[0].clips = [];
  });
  fireEvent.keyDown(document.body, { key: "v", ctrlKey: true });
  await waitFor(() => expect(mutate).toHaveBeenCalled());
  const inserted = mutate.mock.calls[0][1][0];
  if (inserted.type !== "clip.insert") throw new Error("expected clip");
  const operations = mutate.mock.calls[0][2]!;
  const mask = operations.find((op) => op.type === "mask.put");
  const curve = operations.find((op) => op.type === "keyframe.curve_set");
  expect(mask?.type === "mask.put" ? mask.mask.target_id : null).toBe(
    inserted.clip.id,
  );
  expect(mask?.type === "mask.put" ? mask.mask.id : null).not.toBe("mask");
  expect(
    curve?.type === "keyframe.curve_set" ? curve.curve.keyframe_id : null,
  ).toBe(inserted.clip.channels![0].keyframes[0].id);
  expect(inserted.clip.channels![0].keyframes[0].id).not.toBe("key");
  expect(
    curve?.type === "keyframe.curve_set" ? curve.curve.clip_id : null,
  ).toBe(inserted.clip.id);
});
it("unrelated refresh preserves a settings draft, FPS identity changes reset it", async () => {
  const { change, mutate } = await setup("settings");
  fireEvent.change(screen.getByLabelText("Width"), {
    target: { value: "800" },
  });
  await change((s) => {
    s.project.name = "Renamed";
  });
  expect((screen.getByLabelText("Width") as HTMLInputElement).value).toBe(
    "800",
  );
  await change((s) => {
    s.project.sequences[0].fps = { numerator: 30000, denominator: 1001 };
  });
  expect((screen.getByLabelText("Width") as HTMLInputElement).value).toBe(
    "640",
  );
  fireEvent.click(screen.getByRole("button", { name: "Apply settings" }));
  expect(mutate.mock.calls[0][1][0]).toMatchObject({
    fps: { numerator: 30000, denominator: 1001 },
  });
});
function multiple(s: Snapshot) {
  const track = s.project.sequences[0].tracks[0];
  track.clips.push({
    ...structuredClone(track.clips[0]),
    id: "second",
    timeline_range: { start: time(48), duration: time(24) },
  });
  s.project.sequences[0].tracks.push({
    ...structuredClone(track),
    id: "destination",
    name: "Video 2",
    clips: [],
  });
}
function laneBounds() {
  document
    .querySelectorAll<HTMLElement>("[data-track-id]")
    .forEach((lane, index) =>
      vi.spyOn(lane, "getBoundingClientRect").mockReturnValue({
        left: 160,
        right: 2000,
        top: index * 64,
        bottom: (index + 1) * 64,
        width: 1840,
        height: 64,
        x: 160,
        y: index * 64,
        toJSON() {},
      }),
    );
}
it("modifier toggle, shift range and select-all maintain one shared selection set", async () => {
  const { controller } = await setup("timeline", multiple);
  const clips = screen.getAllByRole("button", { name: "Clip Test" });
  fireEvent.pointerDown(clips[1], { button: 0, ctrlKey: true });
  expect(controller.getSnapshot().selections).toHaveLength(2);
  fireEvent.pointerDown(clips[0], { button: 0, metaKey: true });
  expect(controller.getSnapshot().selections.map((item) => item.id)).toEqual([
    "second",
  ]);
  fireEvent.pointerDown(clips[0], { button: 0, shiftKey: true });
  expect(controller.getSnapshot().selections).toHaveLength(2);
  fireEvent.keyDown(document.body, { key: "a", ctrlKey: true });
  expect(controller.getSnapshot().selections).toHaveLength(2);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(controller.getSnapshot().selections).toEqual([]);
});
it("group drag emits one atomic batch, preserves relative time and destination tracks", async () => {
  const { controller, mutate } = await setup("timeline", multiple);
  laneBounds();
  act(() =>
    controller.selectMany([
      { kind: "clip", trackId: "track", id: "clip" },
      { kind: "clip", trackId: "track", id: "second" },
    ]),
  );
  const clip = screen.getAllByRole("button", { name: "Clip Test" })[0];
  fireEvent.pointerDown(clip, { button: 0, clientX: 180, clientY: 20 });
  fireEvent.pointerMove(clip, { clientX: 260, clientY: 85 });
  fireEvent.pointerUp(clip, { clientX: 260, clientY: 85 });
  expect(mutate).toHaveBeenCalledTimes(1);
  expect(mutate.mock.calls[0][1]).toMatchObject([
    {
      type: "clip.move",
      clip_id: "clip",
      destination_track_id: "destination",
      start: { ticks: "1", timebase: 1 },
    },
    {
      type: "clip.move",
      clip_id: "second",
      destination_track_id: "destination",
      start: { ticks: "3", timebase: 1 },
    },
  ]);
});
it("locked destination, cancellation and revision refresh never submit a partial move", async () => {
  const { controller, mutate, change } = await setup("timeline", (s) => {
    multiple(s);
    s.project.sequences[0].tracks[1].locked = true;
  });
  laneBounds();
  const clip = screen.getAllByRole("button", { name: "Clip Test" })[0];
  fireEvent.pointerDown(clip, { button: 0, clientX: 180, clientY: 20 });
  fireEvent.pointerMove(clip, { clientX: 260, clientY: 85 });
  fireEvent.pointerUp(clip);
  expect(mutate).not.toHaveBeenCalled();
  expect(controller.getSnapshot().error).toContain("Unlock");
  fireEvent.pointerDown(clip, { button: 0, clientX: 180, clientY: 20 });
  fireEvent.pointerMove(clip, { clientX: 260, clientY: 20 });
  fireEvent.pointerCancel(clip);
  fireEvent.pointerUp(clip);
  expect(mutate).not.toHaveBeenCalled();
  fireEvent.pointerDown(clip, { button: 0, clientX: 180, clientY: 20 });
  fireEvent.pointerMove(clip, { clientX: 260, clientY: 20 });
  await change((s) => {
    s.project.name = "External update";
  });
  fireEvent.pointerUp(clip);
  expect(mutate).not.toHaveBeenCalled();
});
it("marquee selects visible clips and cancellation restores the original set without editing", async () => {
  const { controller, mutate } = await setup("timeline", multiple);
  const clips = screen.getAllByRole("button", { name: "Clip Test" });
  clips.forEach((clip, index) =>
    vi.spyOn(clip, "getBoundingClientRect").mockReturnValue({
      left: 180 + index * 160,
      right: 250 + index * 160,
      top: 8,
      bottom: 56,
      width: 70,
      height: 48,
      x: 180,
      y: 8,
      toJSON() {},
    }),
  );
  const lane = document.querySelector("[data-track-id]")!;
  const body = document.querySelector(".timeline-body")!;
  fireEvent.pointerDown(lane, { button: 0, clientX: 170, clientY: 1 });
  fireEvent.pointerMove(body, { clientX: 430, clientY: 60 });
  expect(controller.getSnapshot().selections).toHaveLength(2);
  fireEvent.pointerCancel(body);
  expect(controller.getSnapshot().selections.map((item) => item.id)).toEqual([
    "clip",
  ]);
  expect(mutate).not.toHaveBeenCalled();
});

it("multi-copy and duplicate keep relative placement and copy visual state once per target", async () => {
  const { controller, mutate } = await setup("timeline", multiple);
  act(() => {
    controller.selectMany([
      { kind: "clip", id: "clip", trackId: "track" },
      { kind: "clip", id: "second", trackId: "track" },
    ]);
    controller.seek(time(144));
  });
  fireEvent.keyDown(document.body, { key: "c", ctrlKey: true });
  fireEvent.keyDown(document.body, { key: "v", ctrlKey: true });
  expect(mutate.mock.calls[0][1]).toMatchObject([
    {
      type: "clip.insert",
      clip: { timeline_range: { start: { ticks: "6", timebase: 1 } } },
    },
    {
      type: "clip.insert",
      clip: { timeline_range: { start: { ticks: "8", timebase: 1 } } },
    },
  ]);
  expect(
    mutate.mock.calls[0][2]?.filter((op) => op.type === "effect.put"),
  ).toHaveLength(1);
  act(() =>
    controller.selectMany([
      { kind: "clip", id: "clip", trackId: "track" },
      { kind: "clip", id: "second", trackId: "track" },
    ]),
  );
  fireEvent.keyDown(document.body, { key: "d", ctrlKey: true });
  expect(mutate.mock.calls[1][1]).toMatchObject([
    {
      type: "clip.insert",
      clip: { timeline_range: { start: { ticks: "3", timebase: 1 } } },
    },
    {
      type: "clip.insert",
      clip: { timeline_range: { start: { ticks: "5", timebase: 1 } } },
    },
  ]);
});
it("drag coordinates include scrolling while the pointer remains in the same screen position", async () => {
  const { mutate } = await setup("timeline", multiple);
  laneBounds();
  const clip = screen.getAllByRole("button", { name: "Clip Test" })[0];
  fireEvent.pointerDown(clip, { button: 0, clientX: 180, clientY: 20 });
  const viewport = document.querySelector(".timeline-scroll")!;
  viewport.scrollLeft = 80;
  fireEvent.scroll(viewport);
  fireEvent.pointerMove(clip, { clientX: 180, clientY: 20 });
  fireEvent.pointerUp(clip);
  expect(mutate.mock.calls[0][1][0]).toMatchObject({
    type: "clip.move",
    start: { ticks: "1", timebase: 1 },
  });
});
it("refresh prunes removed selections and scene/historical navigation clears the complete set", async () => {
  const { controller, change } = await setup("timeline", multiple);
  act(() =>
    controller.selectMany([
      { kind: "clip", id: "clip", trackId: "track" },
      { kind: "clip", id: "second", trackId: "track" },
    ]),
  );
  await change((s) => {
    s.project.sequences[0].tracks[0].clips.shift();
  });
  expect(controller.getSnapshot().selections.map((item) => item.id)).toEqual([
    "second",
  ]);
  act(() => controller.chooseSequence("main"));
  expect(controller.getSnapshot().selection).toBeNull();
  expect(controller.getSnapshot().selections).toEqual([]);
  act(() =>
    controller.select({ kind: "clip", id: "second", trackId: "track" }),
  );
  await act(async () => {
    await controller.inspectRevision("history");
  });
  expect(controller.getSnapshot().selections).toEqual([]);
});

it("clip deletion removes related nodes before their lock targets in one transaction", async () => {
  const { mutate } = await setup("timeline", (s) => {
    s.visual[0].masks = [
      {
        id: "mask",
        target_id: "clip",
        shape: { kind: "rectangle", x: 0.1, y: 0.1, width: 0.8, height: 0.8 },
        inverted: false,
        feather: 0,
      },
    ];
    s.curves = [
      {
        id: "curve",
        schema_version: 1,
        sequence_id: "main",
        clip_id: "clip",
        parameter: "opacity",
        keyframe_id: "key",
        curve: { x1: 0, y1: 0, x2: 1, y2: 1 },
      },
    ];
  });
  fireEvent.keyDown(document.body, { key: "Delete" });
  expect(mutate).toHaveBeenCalledTimes(1);
  expect(mutate.mock.calls[0][1]).toEqual([]);
  expect(mutate.mock.calls[0][2]?.map((op) => op.type)).toEqual([
    "effect.delete",
    "mask.delete",
    "keyframe.curve_delete",
    "base.apply",
  ]);
  expect(mutate.mock.calls[0][3]).toBe("head");
});

it("track removal requires review, cancellation writes nothing and a changed head cannot submit the reviewed track", async () => {
  const { mutate, change } = await setup("timeline");
  fireEvent.click(screen.getByRole("button", { name: "Delete track Video" }));
  expect(screen.getByText("Delete Video and its 1 elements?")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(mutate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Delete track Video" }));
  await change((s) => {
    s.project.sequences[0].tracks[0].clips = [];
  });
  const confirm = screen.getByRole("button", {
    name: "Delete track",
  });
  expect((confirm as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(confirm);
  expect(mutate).not.toHaveBeenCalled();
});

it("locked and over-limit tracks cannot submit a partial deletion", async () => {
  const { mutate, change } = await setup("timeline", (s) => {
    s.project.sequences[0].tracks[0].locked = true;
  });
  const open = screen.getByRole("button", { name: "Delete track Video" });
  expect((open as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(open);
  expect(screen.queryByRole("dialog")).toBeNull();
  await change((s) => {
    const track = s.project.sequences[0].tracks[0];
    track.locked = false;
    track.clips = Array.from({ length: 128 }, (_, index) => ({
      ...structuredClone(track.clips[0]),
      id: `clip-${index}`,
      timeline_range: { start: time(index * 24), duration: time(24) },
    }));
    s.visual[0].effects = [];
  });
  fireEvent.click(open);
  fireEvent.click(screen.getByRole("button", { name: "Delete track" }));
  await screen.findByText(
    "This track exceeds the atomic edit limit. Remove some elements first.",
  );
  expect(mutate).not.toHaveBeenCalled();
});
