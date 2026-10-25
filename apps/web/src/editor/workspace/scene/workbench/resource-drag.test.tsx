// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../../context";
import { initialSnapshot } from "../fixture";
import { setupMedia } from "../media-fixture";
import {
  ResourceDragProvider,
  RESOURCE_DRAG_TYPE,
  useResourceDrag,
} from "./resource-drag";
import {
  resourceDrop,
  type DragResource,
  type ResourceHit,
} from "./resource-drop-model";
import { placeMediaDirect } from "../media-dialog";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const time = (ticks: string, timebase = 1) => ({ ticks, timebase });
const hit: ResourceHit = { trackId: "track", at: time("10"), tolerance: 0.2 };
function transfer() {
  const values = new Map<string, string>();
  return {
    setData: (key: string, value: string) => values.set(key, value),
    getData: (key: string) => values.get(key) ?? "",
    effectAllowed: "none",
    types: [RESOURCE_DRAG_TYPE],
  };
}
function DragHarness({
  resource,
  target = hit,
}: {
  resource: DragResource;
  target?: ResourceHit;
}) {
  const drag = useResourceDrag()!;
  return (
    <>
      <button draggable onDragStart={(event) => drag.start(event, resource)}>
        Resource
      </button>
      <div
        data-testid="target"
        onDragOver={() => drag.hover(target)}
        onDrop={(event) =>
          void drag.drop(target, event.dataTransfer.getData(RESOURCE_DRAG_TYPE))
        }
      />
      <output>
        {drag.error ?? drag.target?.reason ?? (drag.drag ? "dragging" : "idle")}
      </output>
    </>
  );
}
async function mount(resource: DragResource, target = hit) {
  const f = await setupMedia();
  render(
    <WorkspaceProvider controller={f.controller}>
      <ResourceDragProvider>
        <DragHarness resource={resource} target={target} />
      </ResourceDragProvider>
    </WorkspaceProvider>,
  );
  return f;
}
it("dropping media preserves the exact target and rational time, selects the saved element and seeks it", async () => {
  const f = await mount(
      { kind: "media", assetId: "asset", name: "Image" },
      { ...hit, at: time("301", 3) },
    ),
    dataTransfer = transfer();
  try {
    fireEvent.dragStart(screen.getByRole("button", { name: "Resource" }), {
      dataTransfer,
    });
    fireEvent.dragOver(screen.getByTestId("target"), { dataTransfer });
    expect(f.requests).toHaveLength(0);
    fireEvent.drop(screen.getByTestId("target"), { dataTransfer });
    await waitFor(() =>
      expect(f.controller.getSnapshot().scene!.media.attempt).toBeNull(),
    );
    await waitFor(() => expect(f.requests).toHaveLength(1));
    await waitFor(() =>
      expect(f.controller.getSnapshot().scene!.selectedId).toBe(
        f.requests[0].placements[0].element_id,
      ),
    );
    expect(f.requests[0].placements[0]).toMatchObject({
      track_id: "track",
      start: time("301", 3),
    });
    expect(f.controller.getSnapshot().scene!.playhead).toEqual(time("301", 3));
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
    ).toHaveLength(1);
  } finally {
    cleanup();
    f.controller.dispose();
  }
});
it.each(["escape", "foreign", "revision"])(
  "does not write after %s invalidates a drag",
  async (kind) => {
    const f = await mount({ kind: "media", assetId: "asset", name: "Image" }),
      dataTransfer = transfer();
    try {
      fireEvent.dragStart(screen.getByRole("button", { name: "Resource" }), {
        dataTransfer,
      });
      if (kind === "escape") fireEvent.keyDown(window, { key: "Escape" });
      if (kind === "foreign")
        dataTransfer.setData(RESOURCE_DRAG_TYPE, "foreign-document");
      if (kind === "revision") {
        f.current().revision = "external-change";
        await act(() => f.controller.scene.refresh());
      }
      fireEvent.drop(screen.getByTestId("target"), { dataTransfer });
      await act(async () => {});
      expect(f.requests).toHaveLength(0);
      expect(
        f.call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
      ).toHaveLength(0);
    } finally {
      cleanup();
      f.controller.dispose();
    }
  },
);
it("captures catalog audio placement once and rejects repeated Drop while acquisition is pending", async () => {
  let finish!: () => void;
  const place = vi.fn(
      (_position: import("./resource-drop-model").ResourcePosition) =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    ),
    f = await mount(
      { kind: "catalog-audio", name: "Sound", place },
      { ...hit, trackId: null },
    ),
    dataTransfer = transfer();
  try {
    fireEvent.dragStart(screen.getByRole("button", { name: "Resource" }), {
      dataTransfer,
    });
    fireEvent.drop(screen.getByTestId("target"), { dataTransfer });
    fireEvent.drop(screen.getByTestId("target"), { dataTransfer });
    expect(place).toHaveBeenCalledOnce();
    expect(place.mock.calls[0][0]).toMatchObject({
      projectId: f.current().project.id,
      revision: f.current().revision,
      sequenceId: "scene",
      trackId: null,
      at: time("10"),
    });
    await act(async () => finish());
  } finally {
    cleanup();
    f.controller.dispose();
  }
});
it("rejects locked, incompatible and occupied media destinations before native planning", async () => {
  const f = await setupMedia();
  try {
    const state = f.controller.getSnapshot().scene!,
      asset = state.head.project.assets[0],
      position = {
        projectId: state.head.project.id,
        revision: state.head.revision,
        sequenceId: state.sequenceId,
        trackId: "track",
        at: time("0"),
      };
    await expect(
      placeMediaDirect(f.controller, asset, position),
    ).rejects.toThrow("overlaps");
    await expect(
      placeMediaDirect(f.controller, asset, {
        ...position,
        trackId: "missing",
      }),
    ).rejects.toThrow("compatible");
    const sequence = structuredClone(state.head.project.sequences[0]);
    sequence.tracks[0].locked = true;
    expect(
      resourceDrop(
        { kind: "media", assetId: asset.id, name: asset.name },
        sequence,
        state.head.project.assets,
        hit,
      ).reason,
    ).toMatch(/locked/);
    expect(f.requests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
it("offers effects on a visual clip or an empty effect layer, never an incompatible lane", () => {
  const snapshot = initialSnapshot(),
    sequence = snapshot.project.sequences[0],
    assets = snapshot.project.assets,
    resource: DragResource = {
      kind: "effect",
      name: "Blur",
      intensity: 15,
      duration: time("5"),
    };
  expect(
    resourceDrop(resource, sequence, assets, { ...hit, elementId: "image" }),
  ).toMatchObject({ kind: "effect-clip", elementId: "image", reason: null });
  expect(resourceDrop(resource, sequence, assets, hit).reason).toMatch(
    /visual clip/,
  );
  expect(
    resourceDrop(resource, sequence, assets, { ...hit, trackId: null }),
  ).toMatchObject({ kind: "effect-layer", trackId: null, reason: null });
});
it("requires a real adjacent video cut within the hit tolerance and sufficient source handles", () => {
  const snapshot = initialSnapshot(),
    sequence = snapshot.project.sequences[0],
    assets = snapshot.project.assets,
    track = sequence.tracks[0],
    first = track.elements[0];
  assets[0].kind = "video";
  assets[0].duration = time("12");
  first.timeline_range = { start: time("0"), duration: time("6") };
  first.content = {
    kind: "video",
    asset_id: "asset",
    video_stream: 0,
    source_audio_enabled: false,
    audio: { stream: null, gain: { unit: "decibels", value: 0 }, muted: false },
    retime: {
      rate: { numerator: 1, denominator: 1 },
      maintain_pitch: true,
      mapping: { kind: "exact_rational" },
    },
  };
  first.source_span = {
    profile: "classic_trimmed",
    trim_start: time("1"),
    trim_end: time("5"),
    source_duration: time("12"),
  };
  const second = structuredClone(first);
  second.id = "second";
  second.timeline_range.start = time("6");
  track.elements.push(second);
  const resource: DragResource = {
      kind: "transition",
      name: "Crossfade",
      duration: time("1", 2),
    },
    cut = { ...hit, at: time("6") };
  expect(resourceDrop(resource, sequence, assets, cut)).toMatchObject({
    kind: "transition",
    at: time("23", 4),
    reason: null,
    seam: { outgoingId: "image", incomingId: "second" },
  });
  expect(
    resourceDrop(resource, sequence, assets, { ...cut, at: time("7") }).reason,
  ).toContain("cut");
  expect(
    resourceDrop({ ...resource, duration: time("3") }, sequence, assets, cut)
      .reason,
  ).toMatch(/source handles/);
});
