import { expect, it, vi } from "vitest";
import type { EditorApi } from "../../api";
import type { Element } from "../types";
import {
  gestureIntent,
  gestureProblem,
  sampleTransforms,
  transformElementForGesture,
} from "./planning";
import type { TransformTarget } from "./planning";

function target(): TransformTarget {
  return {
    trackId: "track",
    element: {
      id: "clip",
      content: { kind: "image", asset_id: "source" },
      visual: {
        transform: {
          position_x_px: 10,
          position_y_px: 20,
          scale_x: 1,
          scale_y: 1,
          rotation_degrees: 30,
        },
      },
      channels: [
        "scale_x",
        "scale_y",
        "rotation_degrees",
        "position_x_px",
        "opacity",
      ].map((parameter) => ({
        kind: "scalar",
        id: parameter,
        target: { kind: "element", element_id: "clip" },
        binding: { kind: "property", parameter },
        keys: [
          {
            id: `key-${parameter}`,
            value: 0.5,
            time: { ticks: "1", timebase: 3 },
            segment_to_next: { kind: "linear" },
            tangent_mode: "flat",
            left_handle: null,
            right_handle: null,
          },
        ],
        extrapolation: { before: "hold", after: "hold" },
      })),
    } as Element,
  };
}
it("axis intent removes exactly that element's scale channel and keeps original authored values and all other channels", () => {
  const original = target(),
    before = structuredClone(original),
    sample = {
      position_x_px: 90,
      position_y_px: 80,
      scale_x: 2,
      scale_y: 3,
      rotation_degrees: 100,
    };
  for (const [intent, removed] of [
    ["scale_x", ["scale_x"]],
    ["scale_y", ["scale_y"]],
    ["scale_xy", ["scale_x", "scale_y"]],
  ] as const) {
    const result = transformElementForGesture(original, sample, intent, false);
    expect(result.channels).toEqual(
      before.element.channels.filter((c) => !removed.some((id) => id === c.id)),
    );
    expect(result.visual!.transform).toEqual({
      position_x_px: 10,
      position_y_px: 20,
      scale_x: intent === "scale_y" ? 1 : 2,
      scale_y: intent === "scale_x" ? 1 : 3,
      rotation_degrees: 30,
    });
  }
  expect(original).toEqual(before);
  expect(gestureProblem([original], "scale_x")).toBeNull();
  expect(gestureProblem([original], "translate")).toMatch(/animated position/);
  expect(gestureProblem([original], "rotate")).toMatch(
    /animated position or rotation/,
  );
  expect(gestureProblem([original, original], "scale_xy")).toMatch(
    /Group transforms/,
  );
  expect(gestureIntent("right", false)).toBe("scale_x");
  expect(gestureIntent("top", false)).toBe("scale_y");
  expect(gestureIntent("top-left", false)).toBe("scale_xy");
  expect(gestureIntent("right", true)).toBe("scale_xy");
});
it("fixed samples reject wrong order, revision and late completion after the gesture fence changes", async () => {
  const t = target(),
    scope = { projectId: "project", revision: "r0", sequenceId: "scene" },
    time = { ticks: "1", timebase: 3 };
  const result = {
    schema_version: 2,
    project_id: "project",
    revision: "r0",
    sequence_id: "scene",
    time: { ticks: "2", timebase: 6 },
    elements: [
      {
        element_id: "clip",
        track_id: "track",
        transform: t.element.visual!.transform,
        geometry: {
          intrinsic_size: { width: 640, height: 360 },
          quad: {
            centerX: 330,
            centerY: 200,
            width: 640,
            height: 360,
            rotationRadians: Math.PI / 6,
            opacity: 1,
            flipX: false,
            flipY: false,
          },
        },
      },
    ],
  };
  const call = vi.fn().mockResolvedValue(result),
    api = { sdk: { call } } as unknown as EditorApi;
  expect(
    await sampleTransforms(
      api,
      scope,
      time,
      [t],
      new AbortController().signal,
      () => true,
    ),
  ).toEqual(result.elements);
  call.mockResolvedValueOnce({ ...result, revision: "other" });
  await expect(
    sampleTransforms(
      api,
      scope,
      time,
      [t],
      new AbortController().signal,
      () => true,
    ),
  ).rejects.toThrow(/fixed target/);
  call.mockResolvedValueOnce({
    ...result,
    elements: [{ ...result.elements[0], element_id: "other" }],
  });
  await expect(
    sampleTransforms(
      api,
      scope,
      time,
      [t],
      new AbortController().signal,
      () => true,
    ),
  ).rejects.toThrow(/fixed target/);
  await expect(
    sampleTransforms(
      api,
      scope,
      time,
      [t],
      new AbortController().signal,
      () => false,
    ),
  ).rejects.toMatchObject({ name: "AbortError" });
});
