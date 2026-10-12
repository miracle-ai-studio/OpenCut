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
