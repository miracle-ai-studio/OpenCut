import { expect, it } from "vitest";
import { decodeClassic, type ClassicSnapshot } from "./decode";
import { roundMediaTime } from "./ticks";
const main = {
  id: "track",
  name: "Video",
  type: "video",
  isMain: true,
  elements: [],
  hidden: false,
  muted: false,
};
function snapshot(version: number): ClassicSnapshot {
  const scene = {
    id: "scene",
    name: "Main",
    isMain: true,
    bookmarks: [],
    tracks:
      version < 2
        ? []
        : version < 24
          ? [structuredClone(main)]
          : version === 25
            ? { "0": structuredClone(main) }
            : { main: structuredClone(main), overlay: [], audio: [] },
  };
  const raw =
    version < 2
      ? {
          id: "p",
          name: "Legacy",
          fps: 24,
          canvasSize: { width: 160, height: 90 },
          version,
          ...(version === 1 ? { scenes: [scene] } : {}),
        }
      : {
          id: "p",
          metadata: { id: "p", name: "Legacy" },
          version,
          settings: {
            fps: version < 23 ? 24 : { numerator: 24, denominator: 1 },
            canvasSize: { width: 160, height: 90 },
            background: { type: "color", color: "#000000" },
          },
          scenes: [scene],
        };
  return {
    origin: "http://localhost:3000",
    sourceId: "p",
    raw,
    context: { legacyTracksBySceneId: {}, mediaTypesById: {} },
    media: [],
  };
}
it.each(Array.from({ length: 32 }, (_, i) => i))(
  "decodes v%i to 31 without modifying source and with deterministic replay",
  (version) => {
    const input = snapshot(version),
      original = structuredClone(input),
      first = decodeClassic(input),
      second = decodeClassic(input);
    expect(input).toEqual(original);
    expect(first).toEqual(second);
    expect(first.project.version).toBe(31);
    expect(first.project.id).toBeUndefined();
    expect(first.sourceVersion).toBe(version);
    expect(first.steps).toHaveLength(31 - version);
    expect(first.project.settings).toMatchObject({
      fps: { numerator: 24, denominator: 1 },
    });
  },
);
it("reads a legacy external v1 timeline and upgrades seconds and source type from the captured context", () => {
  const input = snapshot(1);
  input.context = {
    legacyTracksBySceneId: {
      scene: [
        {
          id: "track",
          type: "media",
          name: "Video",
          isMain: true,
          elements: [
            {
              id: "c",
              type: "media",
              mediaId: "m",
              startTime: 0,
              duration: 2,
              trimStart: 0,
              trimEnd: 0,
              name: "Red",
            },
          ],
        },
      ],
    },
    mediaTypesById: { m: "video" },
  };
  const result = decodeClassic(input),
    scenes = result.project.scenes as Array<{
      tracks: { main: { elements: Array<Record<string, unknown>> } };
    }>;
  expect(scenes[0].tracks.main.elements[0]).toMatchObject({
    id: "c",
    type: "video",
    mediaId: "m",
    duration: 240000,
  });
  expect(input.context.legacyTracksBySceneId.scene[0]).toMatchObject({
    type: "media",
  });
});
it("converts historical fractional frame rate and tick rounding against independent expected values", () => {
  const input = snapshot(22);
  (input.raw.settings as Record<string, unknown>).fps = 30000 / 1001;
  const result = decodeClassic(input);
  expect(result.project.settings).toMatchObject({
    fps: { numerator: 30000, denominator: 1001 },
  });
  expect([
    roundMediaTime({ time: 0.5 }),
    roundMediaTime({ time: -0.5 }),
    roundMediaTime({ time: 0 }),
  ]).toEqual([1, -1, 0]);
  expect(() => roundMediaTime({ time: NaN })).toThrow();
  expect(() => roundMediaTime({ time: Number.MAX_SAFE_INTEGER + 2 })).toThrow();
});
it("rejects silent element filtering and unsupported fields in legacy external timelines", () => {
  const input = snapshot(1);
  input.context.legacyTracksBySceneId.scene = [
    {
      id: "track",
      type: "media",
      elements: [{ id: "damaged", type: "unknown", duration: 2 }],
    },
  ];
  const original = structuredClone(input);
  expect(() => decodeClassic(input)).toThrow("discard source elements");
  expect(input).toEqual(original);
  input.context.legacyTracksBySceneId.scene = [
    {
      id: "track",
      type: "media",
      elements: [
        {
          id: "effect",
          type: "media",
          mediaId: "m",
          duration: 2,
          effects: [{ type: "blur" }],
        },
      ],
    },
  ];
  expect(() => decodeClassic(input)).toThrow(
    "Unsupported legacy element field",
  );
  input.context.legacyTracksBySceneId.scene = [null];
  expect(() => decodeClassic(input)).toThrow("Damaged Classic track");
  input.context.legacyTracksBySceneId.scene = [
    { id: "track", type: "audio", muted: true, elements: [] },
  ];
  expect(() => decodeClassic(input)).toThrow("track semantics");
  input.context.legacyTracksBySceneId.scene = [
    {
      id: "track",
      type: "audio",
      elements: [
        { id: "gain", type: "audio", mediaId: "m", duration: 2, volume: 0.5 },
      ],
    },
  ];
  expect(() => decodeClassic(input)).toThrow("legacy volume");
});
it("preserves unknown editing fields and dependencies for native blockers instead of erasing them", () => {
  const input = snapshot(28),
    scenes = input.raw.scenes as Array<{
      tracks: { main: { elements: unknown[] } };
    }>;
  scenes[0].tracks.main.elements = [
    {
      id: "c",
      type: "video",
      mediaId: "m",
      duration: 240000,
      startTime: 0,
      trimStart: 0,
      trimEnd: 0,
      unknownEditingField: 123,
      effects: [{ type: "future" }],
      transform: { scaleX: 1, scaleY: 1, rotate: 0, position: { x: 3, y: 4 } },
      volume: -6,
    },
  ];
  const result = decodeClassic(input),
    next = result.project.scenes as typeof scenes;
  expect(next[0].tracks.main.elements[0]).toMatchObject({
    unknownEditingField: 123,
    effects: [{ type: "future" }],
    params: { volume: -6, "transform.positionX": 3, "transform.positionY": 4 },
  });
});
it("rejects damaged identities, future versions and oversized snapshots before any new publication", () => {
  for (const mutate of [
    (s: ClassicSnapshot) => (s.raw.version = 32),
    (s: ClassicSnapshot) => (s.sourceId = "wrong"),
    (s: ClassicSnapshot) => (s.raw.id = "wrong"),
    (s: ClassicSnapshot) => (s.raw.version = -1),
    (s: ClassicSnapshot) => (s.raw.version = NaN),
    (s: ClassicSnapshot) => (s.raw.corruptDuration = Infinity),
  ]) {
    const source = snapshot(31);
    mutate(source);
    expect(() => decodeClassic(source)).toThrow();
  }
  const large = snapshot(31);
  large.raw.unknown = "x".repeat(4 * 1024 * 1024);
  expect(() => decodeClassic(large)).toThrow("4 MiB");
});
