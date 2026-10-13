import { expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import { normalizeTimelineSelection, selectTimelineTarget, timelineSelectionReason } from "./selection";

it("multi-selection is deduplicated in track/element order, excludes captions and explains locked targets", () => {
  const sequence = initialSnapshot().project.sequences[0], first = { track_id: "track", element_id: "image" };
  const second = structuredClone(sequence.tracks[0].elements[0]); second.id = "second";
  sequence.tracks[0].elements.push(second);
  const target = { track_id: "track", element_id: "second" };
  expect(normalizeTimelineSelection(sequence, [target, first, target, { track_id: "track", element_id: "caption" }])).toEqual([first, target]);
  expect(selectTimelineTarget(sequence, [first], target, "extend")).toEqual([first, target]);
  sequence.tracks[0].locked = true;
  expect(timelineSelectionReason(sequence, [first, target])).toContain("Unlock");
  expect(timelineSelectionReason(sequence, [])).toContain("Select");
});

it("text participates in ordered multi-selection, compatible moves and keyboard commands without media source fields", async () => {
  const { textTimelineSequence } = await import("./text-fixture");
  const { moveSelectionCommand, shortcutCommand } = await import("./actions");
  const { intersectingMedia } = await import("./geometry");
  const sequence = textTimelineSequence(), target = { track_id: "titles", element_id: "title" };
  expect(normalizeTimelineSelection(sequence, [target])).toEqual([target]);
  expect(intersectingMedia(sequence, 1, {x:0,y:80,width:500,height:80})).toContainEqual(target);
  expect(moveSelectionCommand(sequence, [target], {ticks:"1",timebase:3}, 1, false).kind).toBe("move_selection");
  expect(() => moveSelectionCommand(sequence, [target], {ticks:"1",timebase:3}, -1, false)).toThrow("element type");
  for (const key of ["s", "q", "w", "Delete"]) expect(shortcutCommand(key,sequence,[target],{ticks:"1",timebase:2},false,()=>"right")?.kind).toBe(key === "Delete" ? "delete" : "split");
  sequence.tracks.find(t=>t.id === "titles")!.locked = true;
  expect(timelineSelectionReason(sequence,[target])).toContain("Unlock");
});
