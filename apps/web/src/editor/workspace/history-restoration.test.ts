// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";
import { WorkspaceController } from "./controller";
import { EditorApi } from "./api";
import { projectRef, type Snapshot, type History } from "../model/types";
import {
  flushTimelinePreferences,
  readTimelinePreferences,
} from "../shared/view-preferences";
const original: Snapshot = {
  format: "visual",
  revision: "r0",
  visual: [],
  curves: [],
  project: {
    id: "history-restoration",
    name: "History",
    schema_version: 2,
    default_sequence_id: "main",
    assets: [],
    sequences: ["main", "scene"].map((id) => ({
      id,
      name: id,
      width: 640,
      height: 360,
      fps: { numerator: 24, denominator: 1 },
      caption_cues: [],
      tracks: [
        {
          id: "v",
          name: "V",
          kind: "video",
          enabled: true,
          muted: false,
          locked: false,
          params: {},
          clips: ["a", "b"].map((id, i) => ({
            id,
            asset_id: null,
            source_range: null,
            params: {},
            channels: [],
            timeline_range: {
              start: { ticks: String(i * 3), timebase: 1 },
              duration: { ticks: "3", timebase: 1 },
            },
          })),
        },
      ],
    })),
  },
};
beforeEach(() => {
  flushTimelinePreferences();
  localStorage.clear();
  vi.restoreAllMocks();
});

it("split calls the versioned planner with captured selection/time/mode and restores generated selection only after the acknowledged head", async () => {
  const f = await fixture();
  f.controller.toggleRipple();
  const next = structuredClone(original);
  next.revision = "split-head";
  next.project.sequences[1].tracks[0].clips[0].id = "right";
  const previousCall = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (name, input) => {
    if (name === "opencut_edit_classic_plan_v1")
      return {
        plan: {
          transaction: {
            project_id: original.project.id,
            expected_revision: "r0",
            idempotency_key: "split",
            label: "Split",
            operations: [],
          },
        },
        selection: {
          clip_ids: ["right"],
          caption_ids: [],
          shape_ids: [],
          playhead: { ticks: "1", timebase: 1 },
        },
      } as never;
    if (name === "opencut_edit_v3_apply") {
      f.remote(next);
      return { transaction_id: "split", revision: next.revision } as never;
    }
    return previousCall(name, input);
  });
  await f.controller.split("right");
  const request = f.call.mock.calls.find(
    ([name]) => name === "opencut_edit_classic_plan_v1",
  )![1];
  expect(request).toMatchObject({
    project_id: original.project.id,
    expected_revision: "r0",
    sequence_id: "scene",
    ripple: true,
    command: {
      kind: "split",
      retain: "right",
      clip_ids: ["a", "b"],
      caption_ids: [],
      shape_ids: [],
      at: { ticks: "2", timebase: 1 },
    },
  });
  expect(f.controller.getSnapshot().selections).toEqual([
    { kind: "clip", id: "right", trackId: "v" },
  ]);
  expect(f.controller.getSnapshot().playhead).toEqual({
    ticks: "1",
    timebase: 1,
  });
  expect(
    readTimelinePreferences(original.project.id, "scene").playhead,
  ).toEqual({ ticks: "1", timebase: 1 });
  f.controller.dispose();
});

it("scene navigation during split planning never submits the old plan", async () => {
  const f = await fixture();
  let complete!: (value: never) => void;
  const pending = new Promise<never>((resolve) => {
    complete = resolve;
  });
  const previousCall = f.call.getMockImplementation()!;
  f.call.mockImplementation((name, input) =>
    name === "opencut_edit_classic_plan_v1"
      ? pending
      : previousCall(name, input),
  );
  const edit = f.controller.split("both");
  f.controller.chooseSequence("main");
  complete({ plan: { transaction: {} }, selection: null } as never);
  await expect(edit).rejects.toMatchObject({ name: "AbortError" });
  expect(
    f.call.mock.calls.some(([name]) => name === "opencut_edit_v3_apply"),
  ).toBe(false);
  expect(f.controller.getSnapshot().sequenceId).toBe("main");
  f.controller.dispose();
});

it("a racing remote head removing the split scene cannot receive the old view override", async () => {
  const f = await fixture();
  const remote = structuredClone(original);
  remote.revision = "remote-head";
  remote.project.sequences.splice(1, 1);
  const previousCall = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (name, input) => {
    if (name === "opencut_edit_classic_plan_v1")
      return {
        plan: { transaction: {} },
        selection: {
          clip_ids: ["right"],
          caption_ids: [],
          shape_ids: [],
          playhead: { ticks: "1", timebase: 1 },
        },
      } as never;
    if (name === "opencut_edit_v3_apply") {
      f.remote(remote);
      return { transaction_id: "split", revision: "split-head" } as never;
    }
    return previousCall(name, input);
  });
  await f.controller.split("right");
  expect(f.controller.getSnapshot().snapshot?.revision).toBe("remote-head");
  expect(f.controller.getSnapshot().sequenceId).toBe("main");
  expect(f.controller.getSnapshot().selections).toEqual([]);
  expect(f.controller.getSnapshot().error).toBeNull();
  f.controller.dispose();
});
async function fixture(removeScene = false) {
  let head = structuredClone(original);
  let navigation: History["navigation"] = {
    project_id: head.project.id,
    revision: "r0",
    undo: null,
    redo: null,
  };
  const removed = structuredClone(original);
  removed.revision = "r1";
  if (removeScene) removed.project.sequences.splice(1, 1);
  else removed.project.sequences[1].tracks[0].clips = [];
  const api = new EditorApi("http://localhost:8788", "test");
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(head)]);
  vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
  vi.spyOn(api, "inspect").mockImplementation(async () =>
    structuredClone(head),
  );
  vi.spyOn(api, "history").mockImplementation(async () => ({
    project_id: head.project.id,
    head_revision: head.revision,
    revisions: [],
    next_cursor: null,
    navigation: structuredClone(navigation),
  }));
  vi.spyOn(api, "apply").mockImplementation(async () => {
    head = structuredClone(removed);
    navigation = {
      project_id: head.project.id,
      revision: head.revision,
      undo: { transaction_id: "delete", label: "Delete" },
      redo: null,
    };
    return { transaction_id: "delete", revision: head.revision } as never;
  });
  const call = vi.spyOn(api.sdk, "call").mockImplementation(async (name) => {
    if (name.endsWith("undo_plan")) return { conflicts: [] } as never;
    if (name.endsWith("undo_apply")) {
      head = { ...structuredClone(original), revision: "r2" };
      navigation = {
        project_id: head.project.id,
        revision: head.revision,
        undo: null,
        redo: {
          transaction_id: "delete",
          undo_transaction_id: "undo-delete",
          label: "Delete",
        },
      };
    } else {
      head = { ...structuredClone(removed), revision: "r3" };
      navigation = {
        project_id: head.project.id,
        revision: head.revision,
        undo: { transaction_id: "delete", label: "Delete" },
        redo: null,
      };
    }
    return {
      transaction: {
        transaction_id: name.endsWith("undo_apply")
          ? "undo-delete"
          : "redo-delete",
        revision: head.revision,
      },
    } as never;
  });
  const create = async () => {
    const controller = new WorkspaceController(() => api);
    await controller.connect("x", "t");
    await controller.open(projectRef(head), "scene");
    return controller;
  };
  const controller = await create();
  controller.selectMany([
    { kind: "clip", trackId: "v", id: "a" },
    { kind: "clip", trackId: "v", id: "b" },
  ]);
  controller.seek({ ticks: "2", timebase: 1 });
  return {
    api,
    controller,
    create,
    call,
    remote: (value: Snapshot) => {
      head = structuredClone(value);
      navigation = { ...navigation!, revision: head.revision };
    },
  };
}
for (const removeScene of [false, true])
  it(`restores deleted ${removeScene ? "active scene" : "selected clip set"}, playhead and redo without content writes, including after reopening`, async () => {
    const f = await fixture(removeScene);
    await f.controller.mutate("Delete", []);
    expect(f.controller.getSnapshot().selections).toEqual([]);
    expect(f.controller.getSnapshot().sequenceId).toBe(
      removeScene ? "main" : "scene",
    );
    f.controller.dispose();
    const controller = await f.create();
    await controller.undo();
    expect(controller.getSnapshot().sequenceId).toBe("scene");
    expect(controller.getSnapshot().selections.map((item) => item.id)).toEqual([
      "a",
      "b",
    ]);
    expect(controller.getSnapshot().playhead).toEqual({
      ticks: "2",
      timebase: 1,
    });
    await controller.redo();
    expect(controller.getSnapshot().selections).toEqual([]);
    expect(controller.getSnapshot().sequenceId).toBe(
      removeScene ? "main" : "scene",
    );
    await controller.undo();
    expect(controller.getSnapshot().selections).toHaveLength(2);
    expect(f.api.apply).toHaveBeenCalledTimes(1);
    expect(f.call).toHaveBeenCalledTimes(5); // undo plan/apply twice, redo once
  });
it("does not overwrite a user-selected scene while undo is pending", async () => {
  const f = await fixture(true);
  await f.controller.mutate("Delete", []);
  const inspect = vi.mocked(f.api.inspect).getMockImplementation()!;
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  vi.mocked(f.api.inspect).mockImplementation(async (...args) => {
    await pending;
    return inspect(...args);
  });
  const undo = f.controller.undo();
  f.controller.chooseSequence("main");
  release();
  await undo;
  expect(f.controller.getSnapshot().sequenceId).toBe("main");
  expect(f.controller.getSnapshot().selections).toEqual([]);
});
it("respects a new user selection made while undo is pending", async () => {
  const f = await fixture(true);
  await f.controller.mutate("Delete", []);
  const inspect = vi.mocked(f.api.inspect).getMockImplementation()!;
  let release!: () => void;
  const pending = new Promise<void>((resolve) => {
    release = resolve;
  });
  vi.mocked(f.api.inspect).mockImplementation(async (...args) => {
    await pending;
    return inspect(...args);
  });
  const undo = f.controller.undo();
  f.controller.select({ kind: "clip", trackId: "v", id: "b" });
  release();
  await undo;
  expect(f.controller.getSnapshot().sequenceId).toBe("main");
  expect(f.controller.getSnapshot().selections.map((item) => item.id)).toEqual([
    "b",
  ]);
});
it("does not restore hints onto a racing remote head and prunes stale selection IDs", async () => {
  const f = await fixture();
  await f.controller.mutate("Delete", []);
  const remote = structuredClone(original);
  remote.revision = "remote";
  remote.project.sequences[1].tracks[0].clips = [];
  vi.mocked(f.api.inspect).mockResolvedValue(remote);
  await f.controller.undo();
  expect(f.controller.getSnapshot().snapshot?.revision).toBe("remote");
  expect(f.controller.getSnapshot().selections).toEqual([]);
});
