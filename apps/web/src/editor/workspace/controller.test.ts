import { describe, expect, it, vi } from "vitest";
import { WorkspaceController } from "./controller";
import { EditorApi } from "./api";
import { projectRef, type Snapshot, type History } from "../model/types";
import { flushTimelinePreferences, readTimelinePreferences, saveTimelinePreferences } from "../shared/view-preferences";
const snapshot = (id: string, revision = "head"): Snapshot => ({
  format: "visual",
  revision,
  visual: [],
  curves: [],
  project: {
    id,
    name: id,
    default_sequence_id: "main",
    sequences: [],
    assets: [],
  },
});
const history = (id: string): History => ({
  project_id: id,
  head_revision: "head",
  next_cursor: null,
  revisions: [],
  navigation: { project_id: id, revision: "head", undo: null, redo: null },
});
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { resolve, promise };
};
function setup() {
  const api = new EditorApi("http://localhost:8788", "test");
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([
    projectRef(snapshot("a")),
    projectRef(snapshot("b")),
  ]);
  vi.spyOn(api, "inspect").mockImplementation(async (project) =>
    snapshot(project.id),
  );
  vi.spyOn(api.sdk, "call").mockImplementation(async (name, input) => {
    if (name === "opencut_project_inspect") {
      const request = input as {project_id: string};
      return {kind: "historical", snapshot: await api.inspect(projectRef(snapshot(request.project_id)))} as never;
    }
    throw Error(`Unexpected tool ${name}`);
  });
  vi.spyOn(api, "history").mockImplementation(async (id) => history(id));
  vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
  const controller = new WorkspaceController(() => api);
  return { api, controller };
}
describe("workspace isolation and transactions", () => {
  it("restores each scene's playhead and keeps history browsing out of saved views", async () => {
    const stored = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
    });
    const { api, controller } = setup();
    const current = snapshot("timeline-views");
    current.project.sequences = ["main", "other"].map((id) => ({
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
          clips: [
            {
              id: "c",
              asset_id: null,
              source_range: null,
              params: {},
              channels: [],
              timeline_range: {
                start: { ticks: "0", timebase: 1 },
                duration: { ticks: "6", timebase: 1 },
              },
            },
          ],
        },
      ],
    }));
    vi.mocked(api.inspect).mockResolvedValue(current);
    saveTimelinePreferences(current.project.id, "main", {
      playhead: { ticks: "2", timebase: 1 },
    });
    saveTimelinePreferences(current.project.id, "other", {
      playhead: { ticks: "1", timebase: 1 },
    });
    try {
      await controller.connect("x", "t");
      await controller.open(projectRef(current));
      expect(controller.getSnapshot().playhead).toEqual({
        ticks: "2",
        timebase: 1,
      });
      controller.chooseSequence("other");
      expect(controller.getSnapshot().playhead).toEqual({
        ticks: "1",
        timebase: 1,
      });
      controller.seek({ ticks: "3", timebase: 2 });
      await controller.inspectRevision("past");
      controller.seek({ ticks: "4", timebase: 1 });
      expect(
        readTimelinePreferences(current.project.id, "other").playhead,
      ).toEqual({ ticks: "3", timebase: 2 });
      controller.returnToHead();
      controller.chooseSequence("main");
      expect(controller.getSnapshot().playhead).toEqual({
        ticks: "2",
        timebase: 1,
      });
    } finally {
      controller.dispose();
      flushTimelinePreferences();
      vi.unstubAllGlobals();
    }
  });
  it("deleting the viewed project fences an already running sync failure", async () => {
    const { api, controller } = setup();
    await controller.connect("x", "t");
    await controller.open(projectRef(snapshot("a")));
    const gate = deferred<void>();
    vi.mocked(api.inspect).mockImplementationOnce(async () => {
      await gate.promise;
      throw new Error("Old project disappeared");
    });
    const oldSync = controller.sync();
    vi.spyOn(api.sdk, "call").mockResolvedValue({
      receipt: { projects: [], staging_upload_ids: [] },
      cleanup_pending: [],
    } as never);
    vi.mocked(api.projects).mockResolvedValue([projectRef(snapshot("b"))]);
    await controller.deleteProjects([projectRef(snapshot("a"))], "delete-a");
    gate.resolve();
    await oldSync;
    expect(controller.getSnapshot().snapshot).toBeNull();
    expect(controller.getSnapshot().error).toBeNull();
    expect(controller.getSnapshot().projects.map((p) => p.id)).toEqual(["b"]);
  });
  it("a delayed deletion cannot clear another project opened while it was pending", async () => {
    const { api, controller } = setup();
    await controller.connect("x", "t");
    await controller.open(projectRef(snapshot("a")));
    const deletion = deferred<never>();
    const normal = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation((name, input, options) =>
      name === "opencut_project_delete" ? deletion.promise : normal(name, input, options));
    const pending = controller.deleteProjects(
      [projectRef(snapshot("a"))],
      "delete-a",
    );
    await controller.open(projectRef(snapshot("b")));
    deletion.resolve({
      receipt: { projects: [], staging_upload_ids: [] },
      cleanup_pending: [],
    } as never);
    await pending;
    expect(controller.getSnapshot().snapshot?.project.id).toBe("b");
    expect(controller.getSnapshot().error).toBeNull();
  });

  it("restores a requested scene and falls back when that scene is deleted", async () => {
    const { api, controller } = setup();
    const head = snapshot("a");
    const scene = (id: string) => ({
      id,
      name: id,
      width: 640,
      height: 360,
      fps: { numerator: 25, denominator: 1 },
      tracks: [],
      caption_cues: [],
    });
    head.project.sequences = [scene("main"), scene("second")];
    vi.mocked(api.inspect).mockImplementation(async () =>
      structuredClone(head),
    );
    await controller.connect("x", "t");
    await controller.open(projectRef(head), "second");
    controller.seek({ ticks: "10", timebase: 1 });
    expect(controller.getSnapshot().sequenceId).toBe("second");
    const seek = controller.getSnapshot().seekId;
    head.project.sequences = [scene("main")];
    head.revision = "deleted";
    await controller.sync();
    expect(controller.getSnapshot().sequenceId).toBe("main");
    expect(controller.getSnapshot().playhead.ticks).toBe("0");
    expect(controller.getSnapshot().seekId).toBeGreaterThan(seek);
  });
  it("ignores a connection that finishes after disconnect", async () => {
    const { api, controller } = setup(),
      pending = deferred<ReturnType<typeof projectRef>[]>();
    vi.mocked(api.projects).mockReturnValue(pending.promise);
    const connection = controller.connect("x", "t");
    await Promise.resolve();
    controller.disconnect();
    pending.resolve([projectRef(snapshot("a"))]);
    await connection;
    expect(controller.getSnapshot().connection).toBe("disconnected");
  });
  it("keeps the newest opened project when responses arrive backwards", async () => {
    const { api, controller } = setup();
    await controller.connect("x", "t");
    const pending = deferred<Snapshot>();
    vi.mocked(api.inspect).mockImplementation(async (project) =>
      project.id === "a" ? pending.promise : snapshot("b"),
    );
    const first = controller.open(projectRef(snapshot("a")));
    await controller.open(projectRef(snapshot("b")));
    pending.resolve(snapshot("a"));
    await first;
    expect(controller.getSnapshot().snapshot?.project.id).toBe("b");
  });
  it("does not reopen a project after disconnect during creation", async () => {
    const { api, controller } = setup();
    await controller.connect("x", "t");
    const pending = deferred<Awaited<ReturnType<EditorApi["create"]>>>();
    vi.spyOn(api, "create").mockReturnValue(pending.promise);
    const creating = controller.create("a", 1920, 1080, 24);
    controller.disconnect();
    pending.resolve({project_id:"a",revision:"head"});
    await creating;
    expect(controller.getSnapshot().snapshot).toBeNull();
  });
  it("does not reenter history after returning to the head", async () => {
    const { api, controller } = setup();
    await controller.connect("x", "t");
    await controller.open(projectRef(snapshot("a")));
    const pending = deferred<Snapshot>();
    vi.mocked(api.inspect).mockReturnValue(pending.promise);
    const inspecting = controller.inspectRevision("old");
    controller.returnToHead();
    pending.resolve(snapshot("a", "old"));
    await inspecting;
    expect(controller.getSnapshot().historical).toBeNull();
  });
  it("advances the playback clock without issuing a new seek", () => {
    const { controller } = setup();
    controller.seek({ ticks: "1", timebase: 1 });
    const seek = controller.getSnapshot().seekId;
    controller.present({ ticks: "2", timebase: 1 });
    expect(controller.getSnapshot().seekId).toBe(seek);
    expect(controller.getSnapshot().playhead.ticks).toBe("2");
  });
});
