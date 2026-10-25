import { afterEach, expect, it, vi } from "vitest";
import { initialSnapshot, setup } from "./fixture";
import { normalizeTimelineSelection } from "./timeline-edit/selection";

afterEach(() => vi.restoreAllMocks());
const target = (id: string) => ({ track_id: "track", element_id: id });
async function fixture() {
  const snapshot = initialSnapshot(), track = snapshot.project.sequences[0].tracks[0];
  track.elements = ["a", "b", "c"].map(id => ({ ...structuredClone(track.elements[0]), id }));
  snapshot.project.sequences.push({ ...structuredClone(snapshot.project.sequences[0]), id: "other", tracks: [] });
  return setup(snapshot);
}
const bookmark = { kind: "put" as const, bookmark: { id: "marker", sequence_id: "scene", time: { ticks: "0", timebase: 1 }, note: "Selection lifecycle" } };

it("preserves the current cohort through save/Undo/Redo and uses it for the first toggle or range", async () => {
  const f = await fixture();
  try {
    const scene = f.controller.scene, read = () => f.controller.getSnapshot().scene!;
    scene.timelineEdit.selectMany([target("a"), target("b")], "b");
    await scene.bookmark(bookmark);
    expect(read().timelineEdit.selection).toEqual([target("a"), target("b")]);
    await scene.planUndo("tx1", true);
    expect(read().timelineEdit.selection).toEqual([target("a"), target("b")]);
    expect(read().selectedId).toBe("b");
    scene.timelineEdit.select(target("c"), "toggle");
    expect(read().timelineEdit.selection).toEqual([target("a"), target("b"), target("c")]);
    scene.timelineEdit.select(target("c"), "toggle");
    scene.timelineEdit.select(target("c"), "extend");
    expect(read().timelineEdit.selection).toEqual([target("b"), target("c")]);
    await scene.planRedo(read().navigation.redo!.undo_transaction_id, true);
    expect(read().timelineEdit.selection).toEqual([target("b"), target("c")]);
    expect(read().selectedId).toBe("c");
    expect(f.current().project.sequences[0].bookmarks[0].note).toBe(bookmark.bookmark.note);
  } finally { f.controller.dispose(); }
});

it("normalizes deleted targets and selects a surviving member instead of showing an empty inspector", async () => {
  const f = await fixture();
  try {
    f.controller.scene.timelineEdit.selectMany([target("a"), target("b")], "b");
    f.current().revision = "deleted-primary";
    f.current().project.sequences[0].tracks[0].elements = f.current().project.sequences[0].tracks[0].elements.filter(e => e.id !== "b");
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("a")]);
    expect(f.controller.getSnapshot().scene!.selectedId).toBe("a");
    f.current().revision = "deleted-scene";
    f.current().project.sequences.shift();
    f.current().project.default_sequence_id = "other";
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([]);
    expect(f.controller.getSnapshot().scene!.selectedId).toBeNull();
    expect(f.controller.getSnapshot().scene!.sequenceId).toBe("other");
  } finally { f.controller.dispose(); }
});

it("rebinds surviving IDs to their real track after a move and its reverse without relaxing command targets", async () => {
  const f = await fixture();
  try {
    f.controller.scene.timelineEdit.selectMany([target("a"), target("b")], "b");
    const sequence = f.current().project.sequences[0], original = sequence.tracks[0];
    const moved = original.elements.find(e => e.id === "b")!;
    original.elements = original.elements.filter(e => e.id !== "b");
    sequence.tracks.push({ ...structuredClone(original), id: "destination", elements: [moved] });
    f.current().revision = "moved";
    expect(normalizeTimelineSelection(sequence, [target("b")])).toEqual([]);
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("a"), { track_id: "destination", element_id: "b" }]);
    expect(f.controller.getSnapshot().scene!.selectedId).toBe("b");
    original.elements.push(moved);
    sequence.tracks[1].elements = [];
    f.current().revision = "reversed-move";
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("a"), target("b")]);
  } finally { f.controller.dispose(); }
});

it("uses the surviving primary as the Shift anchor after a history view generation changes", async () => {
  const f = await fixture();
  try {
    const scene = f.controller.scene;
    scene.timelineEdit.selectMany([target("a"), target("b")], "a");
    await scene.bookmark(bookmark);
    await scene.planUndo("tx1", true);
    scene.timelineEdit.select(target("c"), "extend");
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("a"), target("b"), target("c")]);
  } finally { f.controller.dispose(); }
});

it.each(["save", "undo", "redo"] as const)("keeps the last Meta-clicked anchor across %s even when it precedes the primary in document order", async stage => {
  const f = await fixture();
  try {
    const scene = f.controller.scene, read = () => f.controller.getSnapshot().scene!;
    scene.timelineEdit.select(target("b"));
    scene.timelineEdit.select(target("a"), "toggle");
    expect(read().timelineEdit.selection).toEqual([target("a"), target("b")]);
    expect(read().selectedId).toBe("b");
    await scene.bookmark(bookmark);
    if (stage !== "save") await scene.planUndo("tx1", true);
    if (stage === "redo") await scene.planRedo(read().navigation.redo!.undo_transaction_id, true);
    scene.timelineEdit.select(target("c"), "extend");
    expect(read().timelineEdit.selection).toEqual([target("a"), target("b"), target("c")]);
  } finally { f.controller.dispose(); }
});

it("resolves the retained range anchor to its new track after an authoritative refresh", async () => {
  const f = await fixture();
  try {
    const scene = f.controller.scene, sequence = f.current().project.sequences[0], original = sequence.tracks[0];
    scene.timelineEdit.select(target("b"));
    scene.timelineEdit.select(target("a"), "toggle");
    const moved = original.elements.find(e => e.id === "a")!;
    original.elements = original.elements.filter(e => e.id !== "a");
    sequence.tracks.push({ ...structuredClone(original), id: "destination", elements: [moved] });
    f.current().revision = "moved-anchor";
    await scene.refresh();
    scene.timelineEdit.select(target("c"), "extend");
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("c"), { track_id: "destination", element_id: "a" }]);
  } finally { f.controller.dispose(); }
});

it("falls back to the surviving primary when the range anchor was deleted", async () => {
  const f = await fixture();
  try {
    const scene = f.controller.scene;
    scene.timelineEdit.select(target("b"));
    scene.timelineEdit.select(target("a"), "toggle");
    f.current().project.sequences[0].tracks[0].elements.shift();
    f.current().revision = "deleted-anchor";
    await scene.refresh();
    scene.timelineEdit.select(target("c"), "extend");
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("b"), target("c")]);
  } finally { f.controller.dispose(); }
});

it.each(["empty", "bookmark", "scene", "historical"] as const)("does not revive the anchor after %s clears the cohort", async kind => {
  const f = await fixture();
  try {
    const scene = f.controller.scene;
    await scene.bookmark(bookmark);
    scene.timelineEdit.select(target("b"));
    scene.timelineEdit.select(target("a"), "toggle");
    if (kind === "empty") scene.select(null);
    else if (kind === "bookmark") scene.select("marker");
    else if (kind === "scene") { scene.chooseSequence("other"); scene.chooseSequence("scene"); }
    else { await scene.inspectRevision("r0"); scene.returnToHead(); }
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([]);
    scene.timelineEdit.select(target("c"), "extend");
    expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual([target("c")]);
  } finally { f.controller.dispose(); }
});

it.each(["undo", "redo"] as const)("rejects %s before planning or changing view when a property draft is active", async direction => {
  const f = await fixture();
  try {
    f.controller.scene.timelineEdit.selectMany([target("a"), target("b")]);
    f.controller.scene.propertyDraft("draft", true);
    const before = f.controller.getSnapshot().scene!;
    await expect(direction === "undo" ? f.controller.scene.planUndo("tx1", true) : f.controller.scene.planRedo("undo", true)).rejects.toThrow("property draft");
    expect(f.controller.getSnapshot().scene).toBe(before);
    expect(f.call.mock.calls.filter(([name]) => name === "opencut_edit_undo_plan" || name === "opencut_edit_redo_plan")).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it.each(["scene", "connection"])("does not publish a late history plan after %s changes", async kind => {
  const f = await fixture();
  try {
    let release!: () => void;
    const wait = new Promise<void>(resolve => { release = resolve; });
    const original = f.call.getMockImplementation()!;
    f.call.mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_undo_plan") await wait;
      return original(name, input, options);
    });
    f.controller.scene.timelineEdit.selectMany([target("a"), target("b")]);
    const pending = f.controller.scene.planUndo("tx1", true);
    if (kind === "scene") f.controller.scene.chooseSequence("other");
    else f.controller.disconnect();
    release();
    await pending;
    expect(f.call.mock.calls.some(([name]) => name === "opencut_edit_undo_apply")).toBe(false);
    expect(f.controller.getSnapshot().scene?.historyPlan ?? null).toBeNull();
    expect(f.controller.getSnapshot().scene?.timelineEdit.selection ?? []).toEqual([]);
  } finally { f.controller.dispose(); }
});

it.each(["undo", "redo"] as const)("keeps a property draft created during a pending %s plan without auto-applying", async direction => {
  const f = await fixture();
  try {
    let release!: () => void;
    const wait = new Promise<void>(resolve => { release = resolve; });
    const original = f.call.getMockImplementation()!;
    f.call.mockImplementation(async (name, input, options) => {
      if (name === `opencut_edit_${direction}_plan`) await wait;
      return original(name, input, options);
    });
    f.controller.scene.timelineEdit.selectMany([target("a"), target("b")]);
    const pending = direction === "undo" ? f.controller.scene.planUndo("tx1", true) : f.controller.scene.planRedo("undo", true);
    const rejected = expect(pending).rejects.toThrow("property draft");
    f.controller.scene.propertyDraft("typed-during-plan", true);
    release();
    await rejected;
    const view = f.controller.getSnapshot().scene!;
    expect(view.propertyDrafts).toEqual(["typed-during-plan"]);
    expect(view.timelineEdit.selection).toEqual([target("a"), target("b")]);
    expect(view.historyPlan).toBeNull();
    expect(f.call.mock.calls.some(([name]) => name === `opencut_edit_${direction}_apply`)).toBe(false);
  } finally { f.controller.dispose(); }
});
