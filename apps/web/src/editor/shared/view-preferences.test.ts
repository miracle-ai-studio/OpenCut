// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CoverSceneViews,
  flushTimelinePreferences,
  readTimelinePreferences,
  readWorkspaceLayouts,
  saveWorkspaceLayout,
  readWelcomePreferences,
  saveWelcomePreferences,
  readLibraryPreferences,
  saveLibraryPreferences,
  saveTimelinePreferences,
} from "./view-preferences";

it("cover scene views persist only bounded identities, isolate hosts/projects and preserve copying", () => {
  const views = new CoverSceneViews("http://localhost:8788");
  views.save("project", "other");
  expect(new CoverSceneViews("http://localhost:8788").read("project")).toBe(
    "other",
  );
  expect(
    new CoverSceneViews("http://localhost:8789").read("project"),
  ).toBeNull();
  views.save("copy", views.read("project")!);
  expect(views.read("copy")).toBe("other");
  for (let n = 0; n < 66; n++) views.save("p" + n, "s");
  const raw = JSON.parse(localStorage.getItem("opencut.cover-scenes")!);
  expect(raw).toHaveLength(64);
  expect(
    raw.every(
      (v: object) =>
        Object.keys(v).sort().join(",") === "host,projectId,sequenceId",
    ),
  ).toBe(true);
  expect(views.read("project")).toBeNull();
  views.save("x".repeat(513), "s");
  expect(JSON.parse(localStorage.getItem("opencut.cover-scenes")!)).toEqual(
    raw,
  );
});
it("cover scene views reject credentials and recover the session when storage is unavailable", () => {
  new CoverSceneViews("http://user:secret@localhost:8788").save("p", "s");
  expect(localStorage.getItem("opencut.cover-scenes")).toBeNull();
  const views = new CoverSceneViews("http://localhost:8788");
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("quota");
  });
  views.save("project", "other");
  expect(views.read("project")).toBe("other");
});

beforeEach(() => {
  flushTimelinePreferences();
  localStorage.clear();
  vi.useFakeTimers();
});
afterEach(() => {
  flushTimelinePreferences();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
describe("optional timeline views", () => {
  it("persists ripple per scene and project while keeping the stored projection finite", () => {
    saveTimelinePreferences("p", "main", { ripple: true });
    flushTimelinePreferences();
    expect(readTimelinePreferences("p", "main").ripple).toBe(true);
    expect(readTimelinePreferences("p", "other").ripple).toBe(false);
    expect(readTimelinePreferences("another", "main").ripple).toBe(false);
    const stored = JSON.parse(localStorage.getItem("opencut.timeline")!);
    expect(Object.keys(stored[0]).sort()).toEqual([
      "playhead",
      "projectId",
      "ripple",
      "scrollLeft",
      "sequenceId",
      "zoom",
    ]);
  });
  it("coalesces frequent playback updates without losing scene isolation or exact ticks", () => {
    const write = vi.spyOn(Storage.prototype, "setItem");
    for (let i = 0; i < 20; i++)
      saveTimelinePreferences("project", "main", {
        playhead: {
          ticks: String(9007199254740993n + BigInt(i)),
          timebase: 24,
        },
      });
    saveTimelinePreferences("project", "other", { zoom: 3, scrollLeft: 240 });
    expect(write).not.toHaveBeenCalled();
    expect(readTimelinePreferences("project", "main").playhead.ticks).toBe(
      "9007199254741012",
    );
    vi.advanceTimersByTime(200);
    expect(write).toHaveBeenCalledTimes(1);
    expect(readTimelinePreferences("project", "other")).toMatchObject({
      zoom: 3,
      scrollLeft: 240,
      playhead: { ticks: "0", timebase: 1 },
    });
    expect(readTimelinePreferences("elsewhere", "main").zoom).toBe(1);
  });
  it("ignores corrupt preferences and strips unrelated stored data before writing", () => {
    localStorage.setItem(
      "opencut.timeline",
      JSON.stringify([
        {
          projectId: "p",
          sequenceId: "bad",
          zoom: 2,
          scrollLeft: 0,
          playhead: { ticks: "1e99", timebase: 1 },
        },
        {
          projectId: "p",
          sequenceId: "good",
          zoom: 2,
          scrollLeft: 30,
          playhead: { ticks: "1", timebase: 24 },
          project: { secrets: "discard" },
        },
      ]),
    );
    expect(readTimelinePreferences("p", "bad").zoom).toBe(1);
    saveTimelinePreferences("p", "good", { zoom: 4 });
    flushTimelinePreferences();
    const stored = JSON.parse(localStorage.getItem("opencut.timeline")!);
    expect(stored).toHaveLength(1);
    expect(stored[0]).not.toHaveProperty("project");
    expect(stored[0].playhead).toEqual({ ticks: "1", timebase: 24 });
  });
  it("keeps only64 recent views and merges another tab's unrelated view at flush", () => {
    for (let i = 0; i < 65; i++)
      saveTimelinePreferences(String(i), "main", { zoom: 2 });
    localStorage.setItem(
      "opencut.timeline",
      JSON.stringify([
        {
          projectId: "another-tab",
          sequenceId: "main",
          zoom: 3,
          scrollLeft: 0,
          playhead: { ticks: "0", timebase: 1 },
        },
      ]),
    );
    flushTimelinePreferences();
    expect(JSON.parse(localStorage.getItem("opencut.timeline")!)).toHaveLength(
      64,
    );
    saveTimelinePreferences("64", "main", { zoom: 4 });
    localStorage.setItem(
      "opencut.timeline",
      JSON.stringify([
        {
          projectId: "another-tab",
          sequenceId: "main",
          zoom: 3,
          scrollLeft: 0,
          playhead: { ticks: "0", timebase: 1 },
        },
      ]),
    );
    flushTimelinePreferences();
    expect(readTimelinePreferences("another-tab", "main").zoom).toBe(3);
    expect(readTimelinePreferences("64", "main").zoom).toBe(4);
  });
  it("survives unavailable browser storage and rejects nonfinite state", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("Unavailable");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("Unavailable");
    });
    saveTimelinePreferences("p", "main", { zoom: Infinity });
    expect(readTimelinePreferences("p", "main").zoom).toBe(1);
    saveTimelinePreferences("p", "main", { zoom: 2 });
    expect(() => flushTimelinePreferences()).not.toThrow();
  });
});

describe("bounded editor presentation preferences", () => {
  it("projects only panel sizes and guide flags, discarding foreign fields", () => {
    localStorage.setItem(
      "opencut.layout",
      JSON.stringify({
        stage: {
          library: 25,
          preview: 50,
          properties: 25,
          project: { secret: true },
        },
        workspace: { stage: 65, timeline: 35 },
        token: "ignored",
      }),
    );
    expect(readWorkspaceLayouts()).toEqual({
      stage: { library: 25, preview: 50, properties: 25 },
      workspace: { stage: 65, timeline: 35 },
    });
    saveWorkspaceLayout("stage", {
      library: 30,
      preview: 45,
      properties: 25,
      token: 500,
    });
    expect(JSON.parse(localStorage.getItem("opencut.layout")!)).toEqual({
      stage: { library: 30, preview: 45, properties: 25 },
      workspace: { stage: 65, timeline: 35 },
    });
    localStorage.setItem(
      "opencut.welcome",
      JSON.stringify({
        completed: true,
        lastSeenRelease: "0.3.0",
        project: { secret: true },
      }),
    );
    saveWelcomePreferences({ completed: false });
    expect(JSON.parse(localStorage.getItem("opencut.welcome")!)).toEqual({
      completed: false,
      lastSeenRelease: "0.3.0",
    });
    localStorage.setItem(
      "opencut.library",
      JSON.stringify({
        layout: "list",
        sort: "name",
        ascending: true,
        project: { secret: true },
      }),
    );
    saveLibraryPreferences(readLibraryPreferences());
    expect(JSON.parse(localStorage.getItem("opencut.library")!)).toEqual({
      layout: "list",
      sort: "name",
      ascending: true,
    });
  });
  it("rejects unusable panels and malformed guide data without overwriting a valid layout", () => {
    saveWorkspaceLayout("stage", { library: 25, preview: 50, properties: 25 });
    saveWorkspaceLayout("stage", { library: -10, preview: 85, properties: 25 });
    saveWorkspaceLayout("workspace", { stage: 90, timeline: 90 });
    expect(readWorkspaceLayouts().stage).toEqual({
      library: 25,
      preview: 50,
      properties: 25,
    });
    expect(readWorkspaceLayouts().workspace).toBeUndefined();
    localStorage.setItem(
      "opencut.welcome",
      '{"completed":"yes","lastSeenRelease":"../project"}',
    );
    expect(readWelcomePreferences()).toEqual({
      completed: false,
      lastSeenRelease: null,
    });
  });
  it("keeps guide and resizing usable when optional browser preferences are blocked", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("Unavailable");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("Unavailable");
    });
    expect(readWorkspaceLayouts()).toEqual({});
    expect(readWelcomePreferences().completed).toBe(false);
    expect(() =>
      saveWorkspaceLayout("workspace", { stage: 65, timeline: 35 }),
    ).not.toThrow();
    expect(() => saveWelcomePreferences({ completed: true })).not.toThrow();
  });
});

describe("preview guide preferences", () => {
  it("bounds grids and custom lines and discards project or credential data", async () => {
    const { readPreviewGuides, savePreviewGuides } =
      await import("./view-preferences");
    localStorage.setItem(
      "opencut.preview",
      JSON.stringify({ kind: "grid", rows: 25, cols: 3, lines: [] }),
    );
    expect(readPreviewGuides().kind).toBe("none");
    localStorage.setItem(
      "opencut.preview",
      JSON.stringify({
        kind: "custom",
        rows: 3,
        cols: 3,
        lines: [{ axis: "x", position: 33 }],
        project: { id: "must-discard" },
        token: "must-discard",
      }),
    );
    savePreviewGuides(readPreviewGuides());
    expect(JSON.parse(localStorage.getItem("opencut.preview")!)).toEqual({
      kind: "custom",
      rows: 3,
      cols: 3,
      lines: [{ axis: "x", position: 33 }],
    });
    localStorage.setItem(
      "opencut.preview",
      JSON.stringify({
        kind: "custom",
        rows: 3,
        cols: 3,
        lines: Array.from({ length: 33 }, () => ({ axis: "x", position: 50 })),
      }),
    );
    expect(readPreviewGuides().kind).toBe("none");
  });
});
