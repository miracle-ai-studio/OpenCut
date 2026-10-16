import { expect, it } from "vitest";
import { initialSnapshot } from "./fixture";
import {
  sceneVisualProfile,
  viewedSequence,
  viewedVisualProfile,
} from "./visual-profile";
import type { V4SessionState } from "./types";

const modes = [
  "darken",
  "multiply",
  "color-burn",
  "lighten",
  "screen",
  "plus-lighter",
  "color-dodge",
  "overlay",
  "soft-light",
  "hard-light",
  "difference",
  "exclusion",
  "hue",
  "saturation",
  "color",
  "luminosity",
] as const;
it.each(modes)(
  "selects blend for %s before graphic/caption routing",
  (mode) => {
    const sequence = initialSnapshot().project.sequences[0];
    sequence.tracks[0].elements[0].visual!.blend_mode = mode;
    sequence.tracks[0].elements.push({
      enabled: true,
      content: { kind: "graphic" },
    } as never);
    sequence.captions.push({} as never);
    expect(sceneVisualProfile(sequence)).toBe("blend");
    expect(sceneVisualProfile(sequence, "authored")).toBe("blend");
  },
);
it.each(["element", "track"])(
  "keeps disabled %s blend out of visible sampling but in whole-scene export admission",
  (disabled) => {
    const sequence = initialSnapshot().project.sequences[0],
      track = sequence.tracks[0],
      element = track.elements[0];
    element.visual!.blend_mode = "multiply";
    if (disabled === "element") element.enabled = false;
    else track.enabled = false;
    expect(sceneVisualProfile(sequence)).toBe("text");
    expect(sceneVisualProfile(sequence, "authored")).toBe("blend");
  },
);
it("retains normal profile precedence and reads the selected historical scene", () => {
  const head = initialSnapshot(),
    historical = structuredClone(head),
    sequence = historical.project.sequences[0];
  expect(sceneVisualProfile(sequence)).toBe("text");
  sequence.captions.push({} as never);
  expect(sceneVisualProfile(sequence)).toBe("caption");
  sequence.tracks[0].elements.push({
    enabled: true,
    content: { kind: "graphic" },
  } as never);
  expect(sceneVisualProfile(sequence)).toBe("graphic");
  sequence.tracks[0].elements[0].visual!.blend_mode = "screen";
  const state = { head, historical, sequenceId: "scene" } as V4SessionState;
  expect(sceneVisualProfile(viewedSequence(state))).toBe("blend");
  state.historical = null;
  expect(sceneVisualProfile(viewedSequence(state))).toBe("text");
});

it("routes every project containing an explicit synthesis resource through the Scene consumer", () => {
  const snapshot = initialSnapshot();
  snapshot.project.fonts.push({
    id: "unreferenced-synthesis",
    origin: {
      kind: "generated",
      provider: "opencut-chromium-font-synthesis-v1",
      generation_id: `font-synthesis-v1_${"a".repeat(64)}`,
    },
  } as never);
  const state = {
    head: snapshot,
    historical: null,
    sequenceId: "scene",
  } as V4SessionState;
  expect(viewedVisualProfile(state)).toBe("scene");
  expect(viewedVisualProfile(state, "authored")).toBe("scene");
  state.historical = initialSnapshot();
  expect(viewedVisualProfile(state)).toBe("text");
});

it("routes Classic stickers through Scene image delegation for both preview and complete export", () => {
  const sequence = initialSnapshot().project.sequences[0];
  sequence.tracks[0].elements[0].content = {
    kind: "sticker",
    asset_id: "asset",
    intrinsic_size: { width: 37, height: 29 },
  };
  expect(sceneVisualProfile(sequence)).toBe("scene");
  expect(sceneVisualProfile(sequence, "authored")).toBe("scene");
  sequence.tracks[0].elements[0].enabled = false;
  expect(sceneVisualProfile(sequence)).toBe("text");
  expect(sceneVisualProfile(sequence, "authored")).toBe("scene");
});
