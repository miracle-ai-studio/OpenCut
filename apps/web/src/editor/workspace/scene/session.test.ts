import { renderCapability } from "./fixture";
// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { setup } from "./fixture";
import type { ToolInput } from "../../../lib/editor-sdk";
import { Api } from "./api";
import { textTimelineSequence } from "./timeline-edit/text-fixture";
afterEach(() => vi.restoreAllMocks());
describe("complete V4 workspace authority", () => {
  it.each(["undo", "redo"] as const)("publishes review intent only for explicit or conflicting %s, outside the native request", async direction => {
    const f = await setup();
    try {
      const apply = vi.spyOn(f.controller.scene, "applyHistory").mockResolvedValue({} as never);
      const invoke = () => direction === "undo" ? f.controller.scene.planUndo("tx1", true) : f.controller.scene.planRedo("undo-tx", true);
      await invoke();
      expect(f.controller.getSnapshot().scene!.historyPlan!.review).toBe(false);
      expect(apply).toHaveBeenCalledTimes(1);
      const original = f.call.getMockImplementation()!;
      f.call.mockImplementation(async (name, args, options) => {
        const result = await original(name, args, options);
        if (name === "opencut_edit_undo_plan" || name === "opencut_edit_redo_plan")
          return { ...result, conflicts: [{ path: "clip", reason: "dependent_change" }] } as never;
        return result;
      });
      await invoke();
      expect(f.controller.getSnapshot().scene!.historyPlan!.review).toBe(true);
      expect(apply).toHaveBeenCalledTimes(1);
      const request = f.call.mock.calls.find(([name]) => name === `opencut_edit_${direction}_plan`)![1];
      expect(request).not.toHaveProperty("review");
    } finally { f.controller.dispose(); }
  });
  it("keeps unchanged authorized polling out of presentation renders while publishing a new head",async()=>{
    const {controller,current}=await setup();
    try {
      await controller.sync();
      const before=controller.getSnapshot(),changed=vi.fn();
      const unsubscribe=controller.subscribe(changed);
      await controller.sync();
      expect(controller.getSnapshot()).toBe(before);expect(changed).not.toHaveBeenCalled();
      current().revision="external-poll-change";
      await controller.sync();
      expect(controller.getSnapshot().scene!.head.revision).toBe("external-poll-change");
      expect(changed).toHaveBeenCalled();unsubscribe();
    }finally{controller.dispose();}
  });
  it("opens before slow output discovery and fences late capability results without replacing live selection", async () => {
    const { controller, api, ref } = await setup();
    const capability = {...renderCapability(),frame:{available:false,unavailable_reason:"Delayed backend discovery fixture"}};
    let finish!: (value: typeof capability) => void;
    const delayed = () => new Promise<typeof capability>(resolve => { finish = resolve; });
    const originalCall = vi.mocked(api.sdk.call).getMockImplementation()!;
    let delayCapability = true;
    vi.mocked(api.sdk.call).mockImplementation((name, input, options) => {
      if (name === "opencut_render_capability" && delayCapability) {
        delayCapability = false;
        return delayed() as never;
      }
      return originalCall(name, input, options);
    });
    try {
      await controller.open(ref());
      expect(controller.getSnapshot().scene!.head.project.id).toBe(ref().id);
      expect(controller.getSnapshot().scene!.frameCapability).toBeNull();
      controller.scene.select("image");
      finish(capability);
      await vi.waitFor(() => expect(controller.getSnapshot().scene!.frameCapability).toEqual(capability));
      expect(controller.getSnapshot().scene!.selectedId).toBe("image");

      delayCapability = true;
      await controller.open(ref());
      controller.disconnect();
      finish(capability);
      await new Promise(resolve => setTimeout(resolve, 0));
      expect(controller.getSnapshot().scene).toBeNull();
    } finally {
      finish?.(capability);
      controller.dispose();
    }
  });
  it("skips unchanged history reads and preserves the original reviewed plan on an external head", async () => {
    const { controller, api, current, call } = await setup();
    try {
      const history = vi.spyOn(api.scene, "history");
      const unchanged=controller.getSnapshot().scene!.head;
      await controller.scene.refresh();
      expect(history).not.toHaveBeenCalled();
      expect(controller.getSnapshot().scene!.head).toBe(unchanged);
      vi.spyOn(api.scene, "timelinePlan").mockImplementationOnce(async request => ({
        original_request_digest: "reviewed-request", transaction: {
          project_id: request.project_id, expected_revision: request.expected_revision,
          idempotency_key: request.idempotency_key, label: request.label,
          operations: [{ type: "element_delete", sequence_id: request.sequence_id, element_id: "image" }],
        }, decision: { created_ids: [], changed_ids: [], deleted_ids: ["image"], selection: [], ripple_shifted_ids: [], removed_transition_ids: [] },
      }));
      await controller.scene.timelineEdit.plan({ kind: "delete", targets: [{ track_id: "track", element_id: "image" }], ripple: false });
      const original = controller.getSnapshot().scene!.timelineEdit.attempt!;
      current().revision = "another-client-head";
      await controller.scene.refresh();
      const observed = controller.getSnapshot().scene!;
      expect(observed.head.revision).toBe("another-client-head");
      expect(observed.timelineEdit.attempt).toMatchObject({ phase: "conflict", request: original.request, plan: original.plan });
      expect(call.mock.calls.some(([name]) => name === "opencut_edit_apply")).toBe(false);
      await controller.scene.timelineEdit.discard();
      expect(controller.getSnapshot().scene!.timelineEdit.attempt).toBeNull();
    } finally { controller.dispose(); }
  });
  it("selects free text and forwards its original content planner through the shared API", async () => {
    const { controller, api, current } = await setup();
    try {
      current().project.sequences[0] = textTimelineSequence();
      current().revision = "text-content-head";
      await controller.scene.refresh();
      controller.scene.select("title");
      expect(controller.getSnapshot().scene!.timelineEdit.selection).toEqual([
        { track_id: "titles", element_id: "title" },
      ]);
      const plan = vi.spyOn(api.scene, "timelinePlan").mockResolvedValueOnce({
        original_request_digest: "fixed-content-request", transaction: null,
        decision: { created_ids: [], changed_ids: [], deleted_ids: [], selection: [], ripple_shifted_ids: [], removed_transition_ids: [] },
      });
      await controller.scene.timelineEdit.plan({
        kind: "delete", targets: [{ track_id: "titles", element_id: "title" }], ripple: false,
      });
      expect(plan.mock.calls[0][0]).toMatchObject({ command: {
        kind: "delete", targets: [{ track_id: "titles", element_id: "title" }], ripple: false,
      } });
    } finally { controller.dispose(); }
  });
  it("restores a historical scene deleted from the current head with project-scoped recovery", async () => {
    const { controller, call, current } = await setup();
    try {
      current().project.sequences[0].id = "replacement-scene";
      current().project.default_sequence_id = "replacement-scene";
      current().revision = "head-with-deleted-scene";
      await controller.scene.refresh();
      await controller.scene.inspectRevision("r0");
      const viewed = controller.getSnapshot().scene!;
      expect(viewed.sequenceId).toBe("scene");
      expect(viewed.head.project.sequences.some(scene => scene.id === viewed.sequenceId)).toBe(false);
      const result = await controller.scene.restoreRevision("r0");
      const staged = call.mock.calls.find(([name]) => name === "opencut_edit_recovery_stage")![1] as ToolInput<"opencut_edit_recovery_stage">;
      expect(staged.context.sequence_id).toBeNull();
      expect(staged.context.intent.kind).toBe("restore");
      expect(staged.context.intent.request).toMatchObject({
        project_id: viewed.head.project.id,
        expected_revision: "head-with-deleted-scene",
        target_revision: "r0",
        asset_versions: {},
      });
      const writes = call.mock.calls.filter(([name]) => name === "opencut_project_restore");
      expect(writes).toHaveLength(1);
      expect(writes[0][1]).toEqual(staged.context.intent.request);
      expect(result.base_revision).toBe("head-with-deleted-scene");
      expect(controller.getSnapshot().scene!.historical).toBeNull();
      expect(controller.getSnapshot().scene!.head.project.sequences[0].id).toBe("scene");
    } finally { controller.dispose(); }
  });
  it("normalizes media selection after a new head and cancels timeline plan reads before navigation", async () => {
    const { controller, api, current, ref } = await setup();
    try {
      controller.scene.select("image");
      expect(controller.getSnapshot().scene!.timelineEdit.selection).toEqual([
        { track_id: "track", element_id: "image" },
      ]);
      const plan = vi.spyOn(api.scene, "timelinePlan").mockImplementationOnce((_request, signal) =>
        new Promise((_resolve, reject) => {
          signal?.addEventListener("abort", () => reject(new Error("Cancelled timeline read")), { once: true });
        }),
      );
      const pending = controller.scene.timelineEdit.plan({
        kind: "delete", targets: [{ track_id: "track", element_id: "image" }], ripple: false,
      });
      const rejected = expect(pending).rejects.toThrow("Cancelled timeline read");
      // The plan read begins after the host capability read.
      await vi.waitFor(() => expect(plan).toHaveBeenCalled());
      await controller.open(ref());
      await rejected;
      expect(plan.mock.calls[0][1]?.aborted).toBe(true);
      expect(controller.getSnapshot().scene!.timelineEdit.attempt).toBeNull();
      controller.scene.select("image");
      current().revision = "external-head";
      current().project.sequences[0].tracks[0].elements = [];
      await controller.scene.refresh();
      expect(controller.getSnapshot().scene!.timelineEdit.selection).toEqual([]);
      expect(controller.getSnapshot().scene!.selectedId).toBeNull();
    } finally { controller.dispose(); }
  });
  it("an external head refresh cancels clipboard copy and rejects its late fixed-revision response", async () => {
    const { controller, api, current, call } = await setup();
    try {
      controller.scene.select("image");
      let resolve!: (value: Awaited<ReturnType<Api["timelineCopy"]>>) => void;
      const copy = vi.spyOn(api.scene, "timelineCopy").mockImplementationOnce(
        () => new Promise(value => { resolve = value; }),
      );
      const pending = controller.scene.clipboard.copy();
      const rejected = expect(pending).rejects.toThrow("The clipboard target changed");
      expect(controller.getSnapshot().scene!.clipboard.phase).toBe("copying");
      const [request, signal] = copy.mock.calls[0];
      current().revision = "external-clipboard-head";
      await controller.scene.refresh();
      expect(signal?.aborted).toBe(true);
      expect(controller.getSnapshot().scene!.clipboard).toMatchObject({
        phase: "idle", attempt: null, buffer: null,
      });
      resolve({
        source_project_id: request.project_id,
        source_revision: request.revision,
        source_sequence_id: request.sequence_id,
        items: [{ source_track_id: "track", source_track_kind: "video", source_element_id: "image" }],
        selection_digest: "fixed-selection-digest",
      });
      await rejected;
      expect(controller.getSnapshot().scene!.clipboard).toMatchObject({
        phase: "idle", attempt: null, buffer: null,
      });
      expect(call.mock.calls.some(([name]) => name === "opencut_edit_apply")).toBe(false);
    } finally { controller.dispose(); }
  });
  it("persists bookmark changes without projecting or overwriting other V4 content", async () => {
    const { controller, call, current, ref } = await setup();
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
      await controller.scene.bookmark({ kind: "put", bookmark });
      expect(controller.getSnapshot().snapshot).toBeNull();
      expect(
        controller.getSnapshot().scene!.head.project.sequences[0].tracks,
      ).toEqual(untouched);
      const plan = call.mock.calls.find(
        ([name]) => name === "opencut_edit_plan",
      )!;
      const apply = call.mock.calls.find(
        ([name]) => name === "opencut_edit_apply",
      )!;
      expect(apply[1]).toEqual(plan[1]);
      expect(
        (apply[1] as ToolInput<"opencut_edit_apply">).operations,
      ).toEqual([{ type: "bookmark", operation: { kind: "put", bookmark } }]);
      await controller.open(ref());
      expect(
        controller.getSnapshot().scene!.head.project.sequences[0].bookmarks,
      ).toEqual([bookmark]);
      await controller.scene.bookmark({
        kind: "move",
        bookmark_id: "note",
        sequence_id: "scene",
        time: { ticks: "2", timebase: 1 },
      });
      expect(current().project.sequences[0].bookmarks[0].time).toEqual({
        ticks: "2",
        timebase: 1,
      });
      await controller.scene.bookmark({
        kind: "delete",
        bookmark_id: "note",
        sequence_id: "scene",
      });
      expect(current().project.sequences[0].bookmarks).toEqual([]);
      expect(
        call.mock.calls.some(
          ([name]) =>
            name === "opencut_job_submit" ||
            name === "opencut_render_submit" ||
            name === "opencut_preview_create",
        ),
      ).toBe(false);
    } finally {
      controller.dispose();
    }
  });
  it("uses the original selective history request, rejects conflicts and restores persisted bookmarks on redo", async () => {
    const { controller, call, current } = await setup();
    try {
      await controller.scene.bookmark({
        kind: "put",
        bookmark: {
          id: "b",
          sequence_id: "scene",
          time: { ticks: "0", timebase: 1 },
          note: "Persisted",
        },
      });
      await controller.scene.planUndo("tx1");
      expect(controller.getSnapshot().scene!.historyPlan!.review).toBe(true);
      const input = controller.getSnapshot().scene!.historyPlan!.input;
      await controller.scene.applyHistory();
      expect(current().project.sequences[0].bookmarks).toEqual([]);
      expect(
        call.mock.calls.find(([n]) => n === "opencut_edit_undo_apply")![1],
      ).toEqual(input);
      await controller.scene.planRedo(
        controller.getSnapshot().scene!.navigation.redo!.undo_transaction_id,
      );
      expect(controller.getSnapshot().scene!.historyPlan!.review).toBe(true);
      await controller.scene.applyHistory();
      expect(current().project.sequences[0].bookmarks[0].note).toBe(
        "Persisted",
      );
      await controller.scene.planUndo("tx1");
      controller
        .getSnapshot()
        .scene!.historyPlan!.plan.conflicts.push({
          path: "bookmark.b",
          reason: "dependent_change",
        });
      await expect(controller.scene.applyHistory()).rejects.toThrow(
        "conflict-free",
      );
      const count = call.mock.calls.filter(
        ([n]) => n === "opencut_edit_undo_apply",
      ).length;
      expect(count).toBe(1);
      await controller.scene.inspectRevision("r0");
      await expect(
        controller.scene.bookmark({
          kind: "delete",
          sequence_id: "scene",
          bookmark_id: "b",
        }),
      ).rejects.toThrow("current revision");
      controller.scene.returnToHead();
      expect(controller.getSnapshot().scene!.historical).toBeNull();
    } finally {
      controller.dispose();
    }
  });
  it("a delayed historical read cannot replace the current view after returning to head", async () => {
    const { controller, api, snapshots } = await setup();
    try {
      let resolve!: (value: Awaited<ReturnType<Api["inspect"]>>) => void;
      vi.spyOn(api.scene, "inspect").mockImplementationOnce(
        () =>
          new Promise((r) => {
            resolve = r;
          }),
      );
      const pending = controller.scene.inspectRevision("r0");
      controller.scene.returnToHead();
      resolve(snapshots.get("r0")!);
      await pending;
      expect(controller.getSnapshot().scene!.historical).toBeNull();
    } finally {
      controller.dispose();
    }
  });
});
it("a mixed library can open a legacy project after a complete V4 session without V4 data leaking into legacy edits", async () => {
  const { controller, api, ref } = await setup();
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
    const originalCall = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation((name, input, options) => {
      if (name === "opencut_project_inspect" && (input as {project_id:string}).project_id === "legacy")
        return Promise.resolve({kind:"historical",snapshot:legacy}) as never;
      return originalCall(name, input, options);
    });
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
      "editor",
      "visual",
    ]);
    await controller.open(controller.getSnapshot().projects[1]);
    expect(controller.getSnapshot().scene).toBeNull();
    expect(controller.getSnapshot().snapshot).toEqual({...legacy,readOnly:true});
    expect(controller.getSnapshot().historical).toBeNull();
    await controller.open(ref());
    expect(controller.getSnapshot().snapshot).toBeNull();
    expect(
      controller.getSnapshot().scene!.head.project.sequences[0].tracks[0]
        .elements[0].visual!.transform.position_x_px,
    ).toBe(17);
  } finally {
    controller.dispose();
  }
});
it("paginates one project directory with complete metadata on the shared SDK", async () => {
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
      if (name === "opencut_project_list") {
        const request = input as ToolInput<"opencut_project_list">;
        return request.cursor
          ? {items: [{...metadata,project_id:"new-2",name:"New 2",revision:"r2",format:"editor"}],next_cursor:null} as never
          : {items: [metadata,{...metadata,project_id:"new-1",name:"New 1",revision:"r1",format:"editor"}],next_cursor:"more"} as never;
      }
      throw new Error(`Unexpected tool ${name}`);
    });
  try {
    const projects = await api.projects();
    expect(projects.map((p) => p.id)).toEqual(["old", "new-1", "new-2"]);
    expect(projects[0].library).toEqual(metadata);
    expect(projects[1].format).toBe("editor");
    expect(api.scene.sdk).toBe(api.sdk);
    expect(call.mock.calls.map(([name]) => name)).toEqual([
      "opencut_project_list",
      "opencut_project_list",
    ]);
  } finally {
    api.covers.dispose();
  }
});
