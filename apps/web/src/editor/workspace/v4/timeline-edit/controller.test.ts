import { afterEach, expect, it, vi } from "vitest";
import { OpenCutApiError } from "../../../../lib/editor-sdk";
import { setupTimelineEdit } from "./timeline-fixture";
import type { TimelineCommand, TimelinePlan } from "./types";

afterEach(() => vi.restoreAllMocks());
const target = { track_id: "track", element_id: "image" };
const move: TimelineCommand = { kind: "move_selection", moves: [{ target, target_track_id: "track", new_start: { ticks: "4", timebase: 3 } }], ripple: false };

it("selection has document order, toggles without writes and normalizes unavailable media", async () => {
  const f = await setupTimelineEdit();
  try {
    f.edit.select(target); expect(f.state().selectedId).toBe("image");
    f.edit.select(target, "toggle"); expect(f.state().timelineEdit.selection).toEqual([]);
    f.edit.select({ track_id: "track", element_id: "missing" }, "extend");
    expect(f.state().timelineEdit.selection).toEqual([]);
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("reviews an immutable server plan before sending its identical transaction and selection receipt", async () => {
  const f = await setupTimelineEdit();
  try {
    const plan = await f.edit.plan(move);
    expect(f.applyCalls()).toHaveLength(0);
    expect(Object.isFrozen(plan.transaction)).toBe(true);
    expect(f.planner.mock.calls[0][1].command).toEqual(move);
    await f.edit.apply();
    expect(f.applyCalls()).toHaveLength(1);
    expect(f.applyCalls()[0][1]).toBe(plan.transaction);
    expect(f.state().timelineEdit.attempt!.phase).toBe("saved");
    expect(f.state().timelineEdit.selection).toEqual([target]);
    expect(f.state().head.revision).toBe(f.current().revision);
    expect(f.busy()).toBe(false);
  } finally { f.controller.dispose(); }
});

it("cancel during planning aborts the read and never restores its late transaction", async () => {
  const f = await setupTimelineEdit();
  try {
    let resolve!: (plan: TimelinePlan) => void;
    f.planner.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
    const pending = f.edit.plan(move);
    // Planning starts after the host capability read.
    await vi.waitFor(() => expect(f.planner).toHaveBeenCalled());
    const request = f.planner.mock.calls[0][1], signal = f.planner.mock.calls[0][2]!;
    f.edit.cancel(); expect(signal.aborted).toBe(true);
    resolve(f.planFor(request));
    await expect(pending).rejects.toThrow("target changed");
    expect(f.state().timelineEdit.attempt).toBeNull();
    expect(f.edit.resume(f.api, f.current().project.id)).toBeNull();
    expect(f.applyCalls()).toHaveLength(0); expect(f.busy()).toBe(false);
  } finally { f.controller.dispose(); }
});

it("a delayed plan is rejected when API, scene or history view changes", async () => {
  for (const change of ["api", "scene", "history"] as const) {
    const f = await setupTimelineEdit();
    try {
      let resolve!: (plan: TimelinePlan) => void;
      f.planner.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
      const pending = f.edit.plan(move);
      await vi.waitFor(() => expect(f.planner).toHaveBeenCalled());
      const request = f.planner.mock.calls[0][1];
      if (change === "api") f.changeApi();
      else if (change === "scene") f.set({ sequenceId: "other" });
      else { f.changeView(); f.set({ historical: f.state().head }); }
      resolve(f.planFor(request));
      await expect(pending).rejects.toThrow("target changed");
      expect(f.applyCalls()).toHaveLength(0);
    } finally { f.controller.dispose(); }
  }
});

it("rejects stale form scopes and mismatched planner identities without an apply", async () => {
  const f = await setupTimelineEdit();
  try {
    await expect(f.edit.plan(move, "Move", { projectId: f.current().project.id, revision: "old", sequenceId: "scene" })).rejects.toThrow("target changed");
    expect(f.planner).not.toHaveBeenCalled();
    f.planner.mockImplementationOnce(async (_api, request) => {
      const plan = f.planFor(request); plan.transaction!.idempotency_key = "wrong"; return plan;
    });
    await expect(f.edit.plan(move)).rejects.toThrow("does not match");
    expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("a null transaction and cancelled review write nothing; head conflict before dispatch preserves review", async () => {
  const f = await setupTimelineEdit();
  try {
    f.planner.mockImplementationOnce(async (_api, request) => ({ ...f.planFor(request), transaction: null }));
    await f.edit.plan(move); await f.edit.apply(); expect(f.applyCalls()).toHaveLength(0);
    expect(f.state().timelineEdit.attempt).toBeNull();
    await f.edit.plan(move); f.edit.cancel(); expect(f.applyCalls()).toHaveLength(0);
    await f.edit.plan(move); f.set({ head: { ...f.state().head, revision: "advanced" } });
    await expect(f.edit.apply()).rejects.toThrow("project changed");
    expect(f.state().timelineEdit.attempt!.plan).toBeTruthy(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("real accepted-but-lost identity replays unchanged after refreshed head, reopen and permission recovery", async () => {
  const f = await setupTimelineEdit();
  try {
    const plan = await f.edit.plan(move);
    let loss = true, revoked = false;
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_v4_apply") {
        if (revoked) throw new OpenCutApiError("FORBIDDEN", "Permission revoked", null, 403, "r", []);
        const receipt = f.commit(input as NonNullable<TimelinePlan["transaction"]>);
        if (loss) { loss = false; throw new Error("Response lost after commit"); }
        return receipt as never;
      }
      return previous(name, input, options);
    });
    await expect(f.edit.apply()).rejects.toThrow("Response lost");
    expect(f.state().timelineEdit.attempt!.phase).toBe("unconfirmed");
    expect(() => f.edit.cancel()).toThrow("Recover");
    await f.refresh(); f.reopen(); revoked = true;
    await expect(f.edit.apply()).rejects.toThrow("Permission revoked");
    expect(f.state().timelineEdit.attempt!.phase).toBe("unconfirmed");
    revoked = false; await f.edit.apply();
    expect(f.planner).toHaveBeenCalledTimes(1); expect(f.applyCalls()).toHaveLength(3);
    for (const call of f.applyCalls()) expect(call[1]).toBe(plan.transaction);
    expect(f.state().head.revision).toBe(f.current().revision);
  } finally { f.controller.dispose(); }
});

it("authoritative replay conflict retains the original plan and requires explicit discard", async () => {
  const f = await setupTimelineEdit();
  try {
    const plan = await f.edit.plan(move), previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let first = true;
    vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_v4_apply") {
        if (first) { first = false; throw new Error("Unknown acceptance"); }
        throw new OpenCutApiError("REVISION_CONFLICT", "Changed", null, 409, "r", []);
      }
      return previous(name, input, options);
    });
    await expect(f.edit.apply()).rejects.toThrow("Unknown");
    await expect(f.edit.apply()).rejects.toThrow("Changed");
    expect(f.state().timelineEdit.attempt).toMatchObject({ plan, phase: "conflict" });
    expect(f.planner).toHaveBeenCalledTimes(1);
    for (const call of f.applyCalls()) expect(call[1]).toBe(plan.transaction);
    f.edit.cancel(); expect(f.state().timelineEdit.attempt).toBeNull();
  } finally { f.controller.dispose(); }
});

it("server unsupported animation errors stay visible and saved receipts survive refresh errors", async () => {
  const f = await setupTimelineEdit();
  try {
    f.planner.mockRejectedValueOnce(new OpenCutApiError("UNSUPPORTED_TIMELINE_ANIMATION", "Animated trim is not supported", null, 400, "r", []));
    await expect(f.edit.plan({ kind: "trim", target, edge: "left", boundary: { ticks: "1", timebase: 3 }, ripple: false })).rejects.toThrow("Animated trim");
    expect(f.state().timelineEdit.attempt).toMatchObject({ phase: "failed", error: "Animated trim is not supported" });
    expect(f.applyCalls()).toHaveLength(0); f.edit.cancel();
    await f.edit.plan(move); f.refresh.mockRejectedValueOnce(new Error("Refresh unavailable"));
    await f.edit.apply();
    expect(f.state().timelineEdit.attempt).toMatchObject({ phase: "saved", error: "Refresh unavailable" });
    await f.edit.refreshSaved(); expect(f.state().timelineEdit.attempt!.error).toBeNull();
    expect(f.applyCalls()).toHaveLength(1);
  } finally { f.controller.dispose(); }
});

it("text planning captures its versioned tool and keeps it on retry after the scene content changes", async () => {
  const { textTimelineSequence } = await import("./text-fixture");
  const f = await setupTimelineEdit();
  try {
    const head = structuredClone(f.state().head);
    head.project.sequences[0] = textTimelineSequence();
    f.set({head});
    f.planner.mockRejectedValueOnce(new TypeError("Lost plan response"));
    const target = {track_id:"titles",element_id:"title"};
    await expect(f.edit.plan({kind:"trim",target,edge:"right",boundary:{ticks:"2",timebase:1},ripple:false})).rejects.toThrow("Lost plan");
    const original = f.state().timelineEdit.attempt!;
    expect(original.tool).toBe("opencut_edit_element_timeline_plan_v4");
    // Retry identity belongs to the captured attempt, not current classification.
    head.project.sequences[0].tracks.find(t=>t.id === "titles")!.elements = [];
    await f.edit.retryPlan();
    expect(f.planner.mock.calls[1][1]).toBe(original.request);
    expect(f.planner.mock.calls[1][3]).toBe("opencut_edit_element_timeline_plan_v4");
  } finally {f.controller.dispose();}
});
