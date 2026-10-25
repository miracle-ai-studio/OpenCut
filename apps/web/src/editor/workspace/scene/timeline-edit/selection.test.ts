import { expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import {
  normalizeTimelineSelection,
  orderedTimelineTargets,
  selectTimelineTarget,
  timelineSelectionReason,
} from "./selection";

it("multi-selection uses stable ranges, drops missing targets and explains locks", () => {
  const sequence = initialSnapshot().project.sequences[0],
    first = { track_id: "track", element_id: "image" };
  const second = structuredClone(sequence.tracks[0].elements[0]);
  second.id = "second";
  sequence.tracks[0].elements.push(second);
  const target = { track_id: "track", element_id: "second" };
  expect(
    normalizeTimelineSelection(sequence, [
      target,
      first,
      target,
      { track_id: "track", element_id: "caption" },
    ]),
  ).toEqual([first, target]);
  expect(selectTimelineTarget(sequence, [first], target, "extend")).toEqual([
    first,
    target,
  ]);
  const third = structuredClone(second); third.id = "third";
  sequence.tracks[0].elements.push(third);
  const last = { track_id: "track", element_id: "third" };
  expect(selectTimelineTarget(sequence, [first], last, "extend", first)).toEqual([first, target, last]);
  expect(selectTimelineTarget(sequence, [first, target, last], target, "extend", first)).toEqual([first, target]);
  expect(selectTimelineTarget(sequence, [last], first, "extend", last)).toEqual([first, target, last]);
  sequence.tracks[0].locked = true;
  expect(timelineSelectionReason(sequence, [first, target])).toContain(
    "Unlock",
  );
  expect(timelineSelectionReason(sequence, [])).toContain("Select");
});

it("text participates in ordered multi-selection, compatible moves and keyboard commands without media source fields", async () => {
  const { textTimelineSequence } = await import("./text-fixture");
  const { moveSelectionCommand, shortcutCommand } = await import("./actions");
  const { intersectingMedia } = await import("./geometry");
  const sequence = textTimelineSequence(),
    target = { track_id: "titles", element_id: "title" };
  expect(normalizeTimelineSelection(sequence, [target])).toEqual([target]);
  expect(
    intersectingMedia(sequence, 1, { x: 0, y: 80, width: 500, height: 80 }),
  ).toContainEqual(target);
  expect(
    moveSelectionCommand(
      sequence,
      [target],
      { ticks: "1", timebase: 3 },
      1,
      false,
    ).kind,
  ).toBe("move_selection");
  expect(() =>
    moveSelectionCommand(
      sequence,
      [target],
      { ticks: "1", timebase: 3 },
      -1,
      false,
    ),
  ).toThrow("element type");
  for (const key of ["s", "q", "w", "Delete"])
    expect(
      shortcutCommand(
        key,
        sequence,
        [target],
        { ticks: "1", timebase: 2 },
        false,
        () => "right",
      )?.kind,
    ).toBe(key === "Delete" ? "delete" : "split");
  sequence.tracks.find((t) => t.id === "titles")!.locked = true;
  expect(timelineSelectionReason(sequence, [target])).toContain("Unlock");
});

it("selects graphics, effect layers, stickers and independent captions in stable track order", async () => {
  const { moveSelectionCommand } = await import("./actions");
  const { intersectingMedia } = await import("./geometry");
  const sequence = initialSnapshot().project.sequences[0];
  const graphicTrack = structuredClone(sequence.tracks[0]);
  graphicTrack.id = "g";
  graphicTrack.kind = "graphic";
  const graphic = graphicTrack.elements[0];
  graphic.id = "graphic";
  graphic.content = {
    kind: "graphic",
    graphic: {
      geometry: { kind: "ellipse" },
      source_size: { width: 80, height: 60 },
      fill: { encoding: "srgb", red: 1, green: 0, blue: 0, alpha: 1 },
      stroke: {
        color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 },
        width_px: 0,
        align: "center",
      },
    },
  };
  sequence.tracks.push(graphicTrack);
  const effects = structuredClone(graphicTrack);
  effects.id = "e";
  effects.kind = "effect";
  effects.elements[0].id = "effect";
  effects.elements[0].content = {
    kind: "effect_layer",
    effect: {
      id: "blur",
      enabled: true,
      effect: { kind: "classic_blur", intensity: 10 },
    },
  };
  sequence.tracks.push(effects);
  const captions = structuredClone(graphicTrack);
  captions.id = "c";
  captions.kind = "caption";
  captions.elements = [];
  sequence.tracks.push(captions);
  sequence.captions.push({
    id: "caption",
    track_id: "c",
    text: "Caption",
    timeline_range: {
      start: { ticks: "1", timebase: 1 },
      duration: { ticks: "2", timebase: 1 },
    },
    style: {
      font: {
        kind: "unresolved_family",
        family: "Arial",
        policy: { mode: "strict" },
      },
      font_size: { unit: "classic_height90", value: 15 },
      color: { encoding: "srgb", red: 1, green: 1, blue: 1, alpha: 1 },
      text_align: "center",
      layout: "legacy_caption_v2",
    },
  });
  const targets = [
    { track_id: "g", element_id: "graphic" },
    { track_id: "e", element_id: "effect" },
    { track_id: "c", element_id: "caption" },
  ];
  expect(orderedTimelineTargets(sequence)).toEqual([
    { track_id: "track", element_id: "image" },
    ...targets,
  ]);
  expect(normalizeTimelineSelection(sequence, [...targets].reverse())).toEqual(
    targets,
  );
  for (const target of targets)
    expect(
      intersectingMedia(sequence, 1, { x: 0, y: 0, width: 1000, height: 1000 }),
    ).toContainEqual(target);
  expect(
    moveSelectionCommand(
      sequence,
      [targets[2]],
      { ticks: "1", timebase: 1 },
      0,
      false,
    ),
  ).toMatchObject({
    kind: "move_selection",
    moves: [
      {
        target: targets[2],
        target_track_id: "c",
        new_start: { ticks: "2", timebase: 1 },
      },
    ],
  });
  expect(() =>
    moveSelectionCommand(
      sequence,
      [targets[1]],
      { ticks: "0", timebase: 1 },
      -1,
      false,
    ),
  ).toThrow("element type");
});
