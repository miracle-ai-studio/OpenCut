// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { setupV4 } from "./fixture";
import type { ToolInput } from "../../../lib/editor-sdk";
import { V4Api } from "./api";
import { textTimelineSequence } from "./timeline-edit/text-fixture";
afterEach(() => vi.restoreAllMocks());
describe("complete V4 workspace authority", () => {
  it("selects free text and forwards its original content planner through the shared API", async () => {
    const { controller, api, current } = await setupV4();
    try {
      current().project.sequences[0] = textTimelineSequence();
      await controller.v4.refresh();
      controller.v4.select("title");
      expect(controller.getSnapshot().v4!.timelineEdit.selection).toEqual([
        { track_id: "titles", element_id: "title" },
      ]);
      const plan = vi.spyOn(api.v4, "timelinePlan").mockResolvedValueOnce({
        original_request_digest: "fixed-content-request", transaction: null,
        decision: { created_ids: [], changed_ids: [], deleted_ids: [], selection: [], ripple_shifted_ids: [], removed_transition_ids: [] },
      });
      await controller.v4.timelineEdit.plan({
        kind: "delete", targets: [{ track_id: "titles", element_id: "title" }], ripple: false,
      });
      expect(plan.mock.calls[0][2]).toBe("opencut_edit_element_timeline_plan_v4");
    } finally { controller.dispose(); }
  });
  it("restores a historical scene deleted from the current head with project-scoped recovery", async () => {
    const { controller, call, current } = await setupV4();
    try {
      current().project.sequences[0].id = "replacement-scene";
      current().project.default_sequence_id = "replacement-scene";
      current().revision = "head-with-deleted-scene";
      await controller.v4.refresh();
      await controller.v4.inspectRevision("r0");
      const viewed = controller.getSnapshot().v4!;
      expect(viewed.sequenceId).toBe("scene");
      expect(viewed.head.project.sequences.some(scene => scene.id === viewed.sequenceId)).toBe(false);
      const result = await controller.v4.restoreRevision("r0");
      const staged = call.mock.calls.find(([name]) => name === "opencut_edit_recovery_v2_stage")![1] as ToolInput<"opencut_edit_recovery_v2_stage">;
      expect(staged.context.sequence_id).toBeNull();
      expect(staged.context.intent.kind).toBe("restore");
      expect(staged.context.intent.request).toMatchObject({
        project_id: viewed.head.project.id,
        expected_revision: "head-with-deleted-scene",
        target_revision: "r0",
        asset_versions: {},
      });
      const writes = call.mock.calls.filter(([name]) => name === "opencut_project_v4_restore");
      expect(writes).toHaveLength(1);
      expect(writes[0][1]).toEqual(staged.context.intent.request);
      expect(result.base_revision).toBe("head-with-deleted-scene");
      expect(controller.getSnapshot().v4!.historical).toBeNull();
      expect(controller.getSnapshot().v4!.head.project.sequences[0].id).toBe("scene");
    } finally { controller.dispose(); }
  });
  it("normalizes media selection after a new head and cancels timeline plan reads before navigation", async () => {
    const { controller, api, current, ref } = await setupV4();
    try {
      controller.v4.select("image");
      expect(controller.getSnapshot().v4!.timelineEdit.selection).toEqual([
        { track_id: "track", element_id: "image" },
      ]);
      const plan = vi.spyOn(api.v4, "timelinePlan").mockImplementationOnce((_request, signal) =>
        new Promise((_resolve, reject) => {
          signal?.addEventListener("abort", () => reject(new Error("Cancelled timeline read")), { once: true });
        }),
      );
      const pending = controller.v4.timelineEdit.plan({
        kind: "delete", targets: [{ track_id: "track", element_id: "image" }], ripple: false,
      });
      const rejected = expect(pending).rejects.toThrow("Cancelled timeline read");
      await controller.open(ref());
      await rejected;
      expect(plan.mock.calls[0][1]?.aborted).toBe(true);
      expect(controller.getSnapshot().v4!.timelineEdit.attempt).toBeNull();
      controller.v4.select("image");
      current().revision = "external-head";
      current().project.sequences[0].tracks[0].elements = [];
      await controller.v4.refresh();
      expect(controller.getSnapshot().v4!.timelineEdit.selection).toEqual([]);
      expect(controller.getSnapshot().v4!.selectedId).toBeNull();
    } finally { controller.dispose(); }
  });
  it("an external head refresh cancels clipboard copy and rejects its late fixed-revision response", async () => {
    const { controller, api, current, call } = await setupV4();
    try {
      controller.v4.select("image");
      let resolve!: (value: Awaited<ReturnType<V4Api["timelineCopy"]>>) => void;
      const copy = vi.spyOn(api.v4, "timelineCopy").mockImplementationOnce(
        () => new Promise(value => { resolve = value; }),
      );
      const pending = controller.v4.clipboard.copy();
      const rejected = expect(pending).rejects.toThrow("The clipboard target changed");
      expect(controller.getSnapshot().v4!.clipboard.phase).toBe("copying");
      const [request, signal] = copy.mock.calls[0];
      current().revision = "external-clipboard-head";
      await controller.v4.refresh();
      expect(signal?.aborted).toBe(true);
      expect(controller.getSnapshot().v4!.clipboard).toMatchObject({
        phase: "idle", attempt: null, buffer: null,
      });
      resolve({
        schema_version: 1,
        source_project_id: request.project_id,
        source_revision: request.revision,
        source_sequence_id: request.sequence_id,
        items: [{ source_track_id: "track", source_track_kind: "video", source_element_id: "image" }],
        selection_digest: "fixed-selection-digest",
      });
      await rejected;
      expect(controller.getSnapshot().v4!.clipboard).toMatchObject({
        phase: "idle", attempt: null, buffer: null,
      });
      expect(call.mock.calls.some(([name]) => name === "opencut_edit_v4_apply")).toBe(false);
    } finally { controller.dispose(); }
  });
  it("persists bookmark changes without projecting or overwriting other V4 content", async () => {
    const { controller, call, current, ref } = await setupV4();
    try {
      const untouched = structuredClone(current().project.sequences[0].tracks);
      const bookmark = {
        id: "note",
        sequence_id: "scene",
        time: { ticks: "1", timebase: 3 },
        duration: { ticks: "2", timebase: 3 },
        note: "Canvas note",
        color: "#facc15",
      };
      await controller.v4.bookmark({ kind: "put", bookmark });
      expect(controller.getSnapshot().snapshot).toBeNull();
      expect(
        controller.getSnapshot().v4!.head.project.sequences[0].tracks,
      ).toEqual(untouched);
      const plan = call.mock.calls.find(
        ([name]) => name === "opencut_edit_v4_plan",
      )!;
      const apply = call.mock.calls.find(
        ([name]) => name === "opencut_edit_v4_apply",
      )!;
      expect(apply[1]).toEqual(plan[1]);
      expect(
        (apply[1] as ToolInput<"opencut_edit_v4_apply">).operations,
      ).toEqual([{ type: "bookmark", operation: { kind: "put", bookmark } }]);
      await controller.open(ref());
      expect(
        controller.getSnapshot().v4!.head.project.sequences[0].bookmarks,
      ).toEqual([bookmark]);
      await controller.v4.bookmark({
        kind: "move",
        bookmark_id: "note",
        sequence_id: "scene",
        time: { ticks: "2", timebase: 1 },
      });
      expect(current().project.sequences[0].bookmarks[0].time).toEqual({
        ticks: "2",
        timebase: 1,
      });
      await controller.v4.bookmark({
        kind: "delete",
        bookmark_id: "note",
        sequence_id: "scene",
      });
      expect(current().project.sequences[0].bookmarks).toEqual([]);
      expect(
        call.mock.calls.some(
          ([name]) =>
            name.startsWith("opencut_edit_apply_v3") ||
            name === "opencut_project_inspect" ||
            name === "opencut_preview_v3_create",
        ),
      ).toBe(false);
    } finally {
      controller.dispose();
    }
  });
  it("uses the original selective history request, rejects conflicts and restores persisted bookmarks on redo", async () => {
    const { controller, call, current } = await setupV4();
    try {
      await controller.v4.bookmark({
        kind: "put",
        bookmark: {
          id: "b",
          sequence_id: "scene",
          time: { ticks: "0", timebase: 1 },
          note: "Persisted",
        },
      });
      await controller.v4.planUndo("tx1");
      const input = controller.getSnapshot().v4!.historyPlan!.input;
      await controller.v4.applyHistory();
      expect(current().project.sequences[0].bookmarks).toEqual([]);
      expect(
        call.mock.calls.find(([n]) => n === "opencut_edit_v4_undo_apply")![1],
      ).toEqual(input);
      await controller.v4.planRedo(
        controller.getSnapshot().v4!.navigation.redo!.undo_transaction_id,
      );
      await controller.v4.applyHistory();
      expect(current().project.sequences[0].bookmarks[0].note).toBe(
        "Persisted",
      );
      await controller.v4.planUndo("tx1");
      controller
        .getSnapshot()
        .v4!.historyPlan!.plan.conflicts.push({
          path: "bookmark.b",
          reason: "dependent_change",
        });
      await expect(controller.v4.applyHistory()).rejects.toThrow(
        "conflict-free",
      );
      const count = call.mock.calls.filter(
        ([n]) => n === "opencut_edit_v4_undo_apply",
      ).length;
      expect(count).toBe(1);
      await controller.v4.inspectRevision("r0");
      await expect(
        controller.v4.bookmark({
          kind: "delete",
          sequence_id: "scene",
          bookmark_id: "b",
        }),
      ).rejects.toThrow("current revision");
      controller.v4.returnToHead();
      expect(controller.getSnapshot().v4!.historical).toBeNull();
    } finally {
      controller.dispose();
    }
  });
  it("a delayed historical read cannot replace the current view after returning to head", async () => {
    const { controller, api, snapshots } = await setupV4();
    try {
      let resolve!: (value: Awaited<ReturnType<V4Api["inspect"]>>) => void;
      vi.spyOn(api.v4, "inspect").mockImplementationOnce(
        () =>
          new Promise((r) => {
            resolve = r;
          }),
      );
      const pending = controller.v4.inspectRevision("r0");
      controller.v4.returnToHead();
      resolve(snapshots.get("r0")!);
      await pending;
      expect(controller.getSnapshot().v4!.historical).toBeNull();
    } finally {
      controller.dispose();
    }
  });
});
it("a mixed library can open a legacy project after a complete V4 session without V4 data leaking into legacy edits", async () => {
  const { controller, api, ref } = await setupV4();
  try {
    const legacy = {
      format: "visual" as const,
      revision: "old",
      visual: [],
      curves: [],
      project: {
        id: "legacy",
        name: "Legacy project",
        schema_version: 2,
        default_sequence_id: "main",
        assets: [],
        sequences: [
          {
            id: "main",
            name: "Main",
            width: 640,
            height: 360,
            fps: { numerator: 24, denominator: 1 },
            caption_cues: [],
            tracks: [],
          },
        ],
      },
    };
    vi.mocked(api.projects).mockResolvedValue([
      ref(),
      {
        id: "legacy",
        name: "Legacy project",
        revision: "old",
        format: "visual",
      },
    ]);
    vi.spyOn(api, "inspect").mockResolvedValue(legacy);
    vi.spyOn(api, "history").mockResolvedValue({
      project_id: "legacy",
      head_revision: "old",
      revisions: [],
      next_cursor: null,
      navigation: {
        project_id: "legacy",
        revision: "old",
        undo: null,
        redo: null,
      },
    });
    await controller.refreshProjects();
    expect(controller.getSnapshot().projects.map((p) => p.format)).toEqual([
      "v4",
      "visual",
    ]);
    await controller.open(controller.getSnapshot().projects[1]);
    expect(controller.getSnapshot().v4).toBeNull();
    expect(controller.getSnapshot().snapshot).toEqual(legacy);
    await controller.open(ref());
    expect(controller.getSnapshot().snapshot).toBeNull();
    expect(
      controller.getSnapshot().v4!.head.project.sequences[0].tracks[0]
        .elements[0].visual!.transform.position_x_px,
    ).toBe(17);
  } finally {
    controller.dispose();
  }
});
it("filters and paginates versioned summaries while keeping legacy library metadata on the shared SDK", async () => {
  const { EditorApi } = await import("../api");
  const api = new EditorApi("http://localhost:8788", "test");
  const metadata = {
    project_id: "old",
    name: "Legacy",
    revision: "old-head",
    format: "visual" as const,
    width: 1920,
    height: 1080,
    fps: { numerator: 24, denominator: 1 },
    duration: { ticks: "3", timebase: 1 },
    scene_count: 2,
    asset_count: 5,
    created_at: "2026-10-11T00:00:00Z",
    modified_at: "2026-10-12T00:00:00Z",
  };
  const call = vi
    .spyOn(api.sdk, "call")
    .mockImplementation(async (name, input) => {
      if (name === "opencut_project_library")
        return { items: [metadata], next_cursor: null } as never;
      if (name === "opencut_project_v4_list") {
        const request = input as ToolInput<"opencut_project_v4_list">;
        return (
          request.cursor
            ? {
                items: [
                  {
                    project_id: "new-2",
                    name: "New 2",
                    revision: "r2",
                    schema_version: 4,
                  },
                ],
                next_cursor: null,
              }
            : {
                items: [
                  {
                    project_id: "old",
                    name: "Legacy",
                    revision: "old-head",
                    schema_version: 3,
                  },
                  {
                    project_id: "new-1",
                    name: "New 1",
                    revision: "r1",
                    schema_version: 4,
                  },
                ],
                next_cursor: "more",
              }
        ) as never;
      }
      throw new Error(`Unexpected tool ${name}`);
    });
  try {
    const projects = await api.projects();
    expect(projects.map((p) => p.id)).toEqual(["old", "new-1", "new-2"]);
    expect(projects[0].library).toEqual(metadata);
    expect(projects[1].format).toBe("v4");
    expect(api.v4.sdk).toBe(api.sdk);
    expect(call.mock.calls.map(([name]) => name)).toEqual([
      "opencut_project_library",
      "opencut_project_v4_list",
      "opencut_project_v4_list",
    ]);
  } finally {
    api.covers.dispose();
  }
});
