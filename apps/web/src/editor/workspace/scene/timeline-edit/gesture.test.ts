import { expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import { addTime, compareTime } from "../../../model/time";
import { anchoredScroll, elementRects, intersectingMedia, moveDeltaAtEdge, pixelsPerSecond, pointerTime } from "./geometry";
import { moveSelectionCommand, shortcutCommand } from "./actions";
import { TimelineGesture } from "./gesture";
import type { GestureFence } from "./gesture";

const target = { track_id: "track", element_id: "image" };
const sequence = () => initialSnapshot().project.sequences[0];
const fence: GestureFence = { api: {}, generation: 1, viewGeneration: 1, projectId: "project", revision: "r0", sequenceId: "scene" };
function start(kind: "move" | "left" | "right" = "move") {
  return new TimelineGesture({ fence, sequence: sequence(), primary: target, selected: [target], kind,
    pointerId: 1, clientX: 200, scrollLeft: 0, zoom: 1, ripple: false });
}

it("short items have their actual duration width; rectangle selection follows time and track bounds", () => {
  const seq = sequence();
  seq.tracks[0].elements[0].timeline_range = { start: { ticks: "1", timebase: 3 }, duration: { ticks: "1", timebase: 1000 } };
  const rect = elementRects(seq, 1)[0];
  expect(rect.width).toBe(pixelsPerSecond(1) / 1000);
  expect(intersectingMedia(seq, 1, { x: rect.x - 1, y: 1, width: 2, height: 10 })).toEqual([target]);
  expect(intersectingMedia(seq, 1, { x: rect.x - 1, y: 81, width: 2, height: 10 })).toEqual([]);
});

it("zoom preserves the content coordinate at its anchor and edge snaps preserve an imported exact Time", () => {
  expect(anchoredScroll(240, 100, 1, 2)).toBe(580);
  const edge = { ticks: "1", timebase: 3 };
  expect(pointerTime(0.334, [edge], 0.01)).toBe(edge);
  const seq = sequence(); seq.tracks[0].elements[0].timeline_range.start = edge;
  expect(moveDeltaAtEdge(seq, target, { ticks: "0", timebase: 1 }, 1, [target])).toEqual({ ticks: "0", timebase: 1 });
});

it("group moves retain each rational start and relative track offset with explicit destinations", () => {
  const seq = sequence();
  seq.tracks[0].elements[0].timeline_range.start = { ticks: "1", timebase: 3 };
  const second = structuredClone(seq.tracks[0]); second.id = "track2"; second.elements[0].id = "image2";
  second.elements[0].timeline_range.start = { ticks: "2", timebase: 3 };
  const third = structuredClone(second); third.id = "track3"; third.elements = [];
  seq.tracks.push(second, third);
  const selected = [target, { track_id: "track2", element_id: "image2" }];
  const command = moveSelectionCommand(seq, selected, { ticks: "1", timebase: 2 }, 1, true);
  expect(command).toEqual({ kind: "move_selection", ripple: true, moves: [
    { target, target_track_id: "track2", new_start: { ticks: "5", timebase: 6 } },
    { target: selected[1], target_track_id: "track3", new_start: { ticks: "7", timebase: 6 } },
  ] });
  expect(() => moveSelectionCommand(seq, selected, { ticks: "0", timebase: 1 }, 2, false)).toThrow("destination track");
  third.locked = true;
  expect(() => moveSelectionCommand(seq, selected, { ticks: "0", timebase: 1 }, 1, false)).toThrow("locked");
  third.locked = false; third.kind = "audio";
  expect(() => moveSelectionCommand(seq, selected, { ticks: "0", timebase: 1 }, 1, false)).toThrow("element type");
});

it("a scroll-adjusted pointerup consumes one command, with no source or frame rounding", () => {
  const g = start();
  g.update(220, 20, 0, fence);
  const result = g.finish(fence);
  expect(result?.command).toEqual({ kind: "move_selection", ripple: false, moves: [
    { target, target_track_id: "track", new_start: { ticks: "1", timebase: 2 } },
  ] });
  expect(g.finish(fence)).toBeNull();
  expect(g.update(230, 20, 0, fence)).toBeNull();
});

it("cancel and any API/generation/view/head/scene change discard an active gesture", () => {
  const canceled = start(); canceled.update(240, 0, 0, fence); canceled.cancel();
  expect(canceled.finish(fence)).toBeNull();
  for (const changed of [{ api: {} }, { generation: 2 }, { viewGeneration: 2 }, { revision: "r1" }, { sequenceId: "other" }, { projectId: "other" }]) {
    const g = start(); g.update(240, 0, 0, fence);
    expect(g.finish({ ...fence, ...changed })).toBeNull();
  }
});

it("left and right trim request scene boundaries and unchanged gestures produce no operation", () => {
  const left = start("left"), right = start("right");
  expect(left.update(240, 0, 0, fence)?.command).toEqual({ kind: "trim", target, edge: "left", boundary: { ticks: "1", timebase: 2 }, ripple: false });
  expect(right.update(160, 0, 0, fence)?.command).toEqual({ kind: "trim", target, edge: "right", boundary: { ticks: "23", timebase: 2 }, ripple: false });
  const unchanged = start(); unchanged.update(201, 0, 0, fence);
  expect(unchanged.finish(fence)).toBeNull();
});

it("S/Q/W capture the exact playhead and stable right IDs; delete contains only chosen media", () => {
  const at = { ticks: "1", timebase: 3 };
  for (const [key, retain] of [["s", "both"], ["q", "right"], ["w", "left"]]) {
    expect(shortcutCommand(key, sequence(), [target], at, true, () => "right-id")).toEqual({
      kind: "split", targets: [{ target, right_element_id: "right-id" }], at, retain, ripple: true,
    });
  }
  expect(shortcutCommand("Delete", sequence(), [target], at, false)).toEqual({ kind: "delete", targets: [target], ripple: false });
  expect(compareTime(addTime(at, { ticks: "1", timebase: 2 }), { ticks: "5", timebase: 6 })).toBe(0);
});
