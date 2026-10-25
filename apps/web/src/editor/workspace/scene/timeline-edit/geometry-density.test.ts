import { expect, it } from "vitest";
import { initialSnapshot } from "../fixture";
import { elementRects, intersectingMedia } from "./geometry";

it("compact marquee selection uses visible row bounds for media and captions", () => {
  const sequence = initialSnapshot().project.sequences[0];
  const second = structuredClone(sequence.tracks[0]);
  second.id = "second"; second.elements[0].id = "second-image";
  sequence.tracks.push(second);
  const captionTrack = structuredClone(second);
  captionTrack.id = "captions"; captionTrack.kind = "caption"; captionTrack.elements = [];
  sequence.tracks.push(captionTrack);
  sequence.captions.push({
    id: "caption", track_id: "captions", text: "Travel",
    timeline_range: { start: { ticks: "1", timebase: 1 }, duration: { ticks: "2", timebase: 1 } },
    style: {
      font: { kind: "unresolved_family", family: "Arial", policy: { mode: "strict" } },
      font_size: { unit: "classic_height90", value: 15 },
      color: { encoding: "srgb", red: 1, green: 1, blue: 1, alpha: 1 },
      text_align: "center", layout: "legacy_caption_v2",
    },
  });
  const rect = elementRects(sequence, 1, 56)[1];
  expect(rect).toMatchObject({ y: 56, height: 56 });
  const selected = intersectingMedia(sequence, 1, { x: 1, y: 57, width: 10, height: 1 }, 56);
  expect(selected).toContainEqual({ track_id: "second", element_id: "second-image" });
  expect(selected).not.toContainEqual({ track_id: "track", element_id: "image" });
  expect(intersectingMedia(sequence, 1, { x: 1, y: 112, width: 10, height: 1 }, 56)).not.toContainEqual({ track_id: "second", element_id: "second-image" });
  expect(intersectingMedia(sequence, 1, { x: 90, y: 113, width: 10, height: 1 }, 56)).toEqual([{ track_id: "captions", element_id: "caption" }]);
});
