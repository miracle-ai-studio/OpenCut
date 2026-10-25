// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { setupAudioPreview } from "../audio-preview-fixture";
import { CanvasTransformOverlay } from "./overlay";
import { geometry } from "../scene-stack/fixture";
import { textInitial } from "../text/fixture";
import type { useCanvasTextAuthoring } from "../text/workspace";
import type { ToolInput } from "../../../../lib/editor-sdk";
const context = vi.hoisted(() => ({
  value: {} as unknown,
  authoring: null as ReturnType<typeof useCanvasTextAuthoring>,
}));
vi.mock("../text/workspace", () => ({
  useCanvasTextAuthoring: () => context.authoring,
}));
vi.mock("../../context", () => ({ useWorkspace: () => context.value, useEditingWorkspace: () => context.value }));
beforeEach(() => {
  context.authoring = null;
  preferences({ language: "en" });
  vi.stubGlobal(
    "PointerEvent",
    class extends MouseEvent {
      pointerId: number;
      constructor(type: string, init: PointerEventInit) {
        super(type, init);
        this.pointerId = init.pointerId ?? 1;
      }
    },
  );
  Object.defineProperties(Element.prototype, {
    setPointerCapture: { configurable: true, value: vi.fn() },
    hasPointerCapture: { configurable: true, value: () => false },
    releasePointerCapture: { configurable: true, value: vi.fn() },
  });
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    width: 640,
    height: 360,
    right: 640,
    bottom: 360,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
async function setup(
  options: {
    multiple?: boolean;
    animatedParameter?: "opacity" | "position_x_px" | "scale_x";
    sampled?: boolean;
    sticker?: boolean;
    corruptGeometry?: boolean;
  } = {},
) {
  options = { sampled: true, ...options };
  const f = await setupAudioPreview(),
    state = {
      ...f.controller.getSnapshot(),
      scene: structuredClone(f.controller.getSnapshot().scene),
    };
  state.scene!.selectedId = "image";
  state.scene!.audioPreview.phase = "paused";
  state.scene!.audioPreview.session = {
    ...f.sessionFor({
      project_id: "v4-project",
      revision: "r0",
      sequence_id: "scene",
      sample_rate: 48000,
      start_time: state.scene!.playhead,
      idempotency_key: "fixture-preview",
    }),
    state: "paused",
  };
  state.scene!.timelineEdit.selection = [
    { track_id: "track", element_id: "image" },
  ];
  const element = state.scene!.head.project.sequences[0].tracks[0].elements[0],
    calls: ToolInput<"opencut_element_properties_plan">[] = [],
    base = f.call.getMockImplementation()!;
  if (options.sticker) {
    element.content = {
      kind: "sticker",
      asset_id: "image-source",
      intrinsic_size: { width: 96, height: 48 },
    };
    element.visual!.transform.rotation_degrees = 0;
  }
  if (options.animatedParameter)
    element.channels = [
      {
        kind: "scalar",
        id: "animated-channel",
        target: { kind: "element", element_id: element.id },
        binding: { kind: "property", parameter: options.animatedParameter },
        keys: [
          {
            id: "animated-key",
            time: { ticks: "0", timebase: 1 },
            value: 0.5,
            left_handle: null,
            right_handle: null,
            segment_to_next: { kind: "linear" },
            tangent_mode: "flat",
          },
        ],
        extrapolation: { before: "hold", after: "hold" },
      },
    ];
  if (options.multiple) {
    const next = structuredClone(element);
    next.id = "second";
    next.visual!.transform.position_x_px += 100;
    state.scene!.head.project.sequences[0].tracks[0].elements.push(next);
    state.scene!.timelineEdit.selection.push({
      track_id: "track",
      element_id: "second",
    });
  }
  f.call.mockImplementation(async (name, input, optionsCall) => {
    if (name === "opencut_element_transform_sample") {
      const request = input as ToolInput<"opencut_element_transform_sample">;
      return {
        project_id: request.project_id,
        revision: request.expected_revision,
        sequence_id: request.sequence_id,
        time: request.time,
        elements: request.element_ids.map((id) => {
          const element =
            state.scene!.head.project.sequences[0].tracks[0].elements.find(
              (e) => e.id === id,
            )!;
          const sampledTransform = {
            ...element.visual!.transform,
            ...(options.animatedParameter === "scale_x" ? { scale_x: 2 } : {}),
          };
          return {
            element_id: id,
            track_id: "track",
            transform: sampledTransform,
            geometry:
              options.corruptGeometry || element.content.kind === "text"
                ? null
                : {
                    intrinsic_size: options.sticker
                      ? { width: 96, height: 48 }
                      : { width: 640, height: 360 },
                    quad: {
                      ...structuredClone(geometry.quad),
                      centerX: 320 + sampledTransform.position_x_px,
                      centerY: 180 + sampledTransform.position_y_px,
                      rotationRadians:
                        (sampledTransform.rotation_degrees * Math.PI) / 180,
                      width: options.sticker
                        ? 96
                        : geometry.quad.width *
                          Math.abs(sampledTransform.scale_x),
                      height: options.sticker
                        ? 48
                        : geometry.quad.height *
                          Math.abs(sampledTransform.scale_y),
                    },
                  },
          };
        }),
      } as never;
    }
    if (name === "opencut_scene_geometry") {
      if (options.sampled)
        throw Error("The legacy geometry operation rejects animated sources.");
      return {
        ...structuredClone(geometry),
        ...(options.sampled && options.animatedParameter === "scale_x"
          ? { quad: { ...geometry.quad, width: geometry.quad.width * 2 } }
          : {}),
        element_id: (input as ToolInput<"opencut_scene_geometry">)
          .element_id,
      } as never;
    }
    if (name === "opencut_element_properties_plan") {
      calls.push(input as ToolInput<"opencut_element_properties_plan">);
      throw Error("Canvas gestures must validate the complete transaction.");
    }
    return base(name, input, optionsCall);
  });
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: [
      "opencut_scene_geometry",
      "opencut_element_properties_plan",
      "opencut_visual_preview_edit_draft_frame",
      ...(options.sampled
        ? [
            "opencut_element_transform_sample",
            "opencut_visual_preview_edit_draft_frame",
          ]
        : []),
    ],
  } as never);
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
      .spyOn(f.controller.scene, "applyPlanned")
      .mockResolvedValue({} as never),
    select = vi.spyOn(f.controller.scene, "select").mockImplementation(() => {}),
    selectMany = vi
      .spyOn(f.controller.scene.timelineEdit, "selectMany")
      .mockImplementation(() => {}),
    preview = vi
      .spyOn(f.controller.scene.visualPreview, "previewEditDraft")
      .mockResolvedValue(undefined),
    clear = vi
      .spyOn(f.controller.scene.visualPreview, "clearEditDraft")
      .mockResolvedValue(true),
    hit = vi.spyOn(f.controller.scene.visualPreview, "hitTest").mockResolvedValue({
      projectId: "v4-project",
      revision: "r0",
      sequenceId: "scene",
      time: state.scene!.playhead,
      hits: [{ elementId: "image", alpha: 128 }],
    });
  vi.spyOn(f.controller.scene.visualPreview, "snapshot").mockReturnValue({
    phase: "ready",
    pixels: null,
    time: 0,
    generation: "1",
    error: null,
    runtime: null,
    cacheCompleted: 0, cacheTotal: 0, statistics: null,
    presentedFrames: 1,
    discardedFrames: 0,
  });
  context.value = { ...state, controller: f.controller };
  const view = render(<CanvasTransformOverlay width={640} height={360} />);
  if (options.corruptGeometry)
    await screen.findByText("Canvas geometry differs from the fixed target.");
  else if (!options.sampled)
    await screen.findByText("Adjusting clips on the canvas is unavailable in this environment.");
  else await screen.findByLabelText("Move selected element");
  const refresh = () => {
    context.value = { ...state, controller: f.controller };
    view.rerender(<CanvasTransformOverlay width={640} height={360} />);
  };
  return {
    ...f,
    state,
    view,
    apply,
    select,
    selectMany,
    preview,
    clear,
    hit,
    calls,
    refresh,
  };
}
it("preserves unrelated opacity animation through a real transform gesture and rejects transform animation specifically", async () => {
  const f = await setup({ animatedParameter: "opacity", sampled: true });
  const original = structuredClone(
    f.state.scene!.head.project.sequences[0].tracks[0].elements[0].channels,
  );
  const handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 8,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 8,
    clientX: 357,
    clientY: 160,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const tx = f.apply.mock.calls[0][0];
  const op = tx.operations[0];
  if (op.type !== "element_put") throw Error("Expected element put");
  expect(op.element.channels).toEqual(original);
  cleanup();
  await setup({ animatedParameter: "position_x_px", sampled: false });
  expect(screen.queryByLabelText("Move selected element")).toBeNull();
});
it("starts V2 scaling at the Rust sample, clears only its axis and validates the final exact candidate before applying", async () => {
  const f = await setup({ sampled: true, animatedParameter: "scale_x" }),
    handle = screen.getAllByLabelText(
      "Resize selected element horizontally",
    )[0];
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 19,
    clientX: 600,
    clientY: 180,
  });
  expect(
    screen.getByText(
      "Resizing removes animation on the resized scale axes. Other animation is preserved. Escape cancels without saving.",
    ),
  ).toBeTruthy();
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 19,
    clientX: 620,
    clientY: 180,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const transaction = f.apply.mock.calls[0][0],
    op = transaction.operations[0];
  if (op.type !== "element_put") throw Error("Expected element put");
  expect(op.element.visual!.transform.scale_x).toBeGreaterThan(2);
  expect(op.element.visual!.transform).toMatchObject({
    scale_y: 1,
    rotation_degrees: 10,
    position_x_px: 17,
    position_y_px: -20,
  });
  expect(op.element.channels).toEqual([]);
  expect(f.calls).toEqual([]); // No old property planner with sampled metadata.
  expect(
    f.call.mock.calls.some(([name]) => name === "opencut_scene_geometry"),
  ).toBe(false);
  expect(f.preview).toHaveBeenCalledTimes(1);
  expect(f.preview.mock.calls[0][0]).toMatchObject({
    transaction,
    intent: {kind:"gesture",gesture:"scale_x"},
  });
  expect(f.preview.mock.invocationCallOrder[0]).toBeLessThan(
    f.apply.mock.invocationCallOrder[0],
  );
});
it("uses the native sticker quad for its handles and commits unchanged fixed sticker dimensions without a legacy geometry request", async () => {
  const f = await setup({ sampled: true, sticker: true });
  const outline = f.view.container.querySelector("svg polygon")!;
  expect(outline.getAttribute("points")).toBe(
    "289,136 385,136 385,184 289,184",
  );
  const handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 21,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 21,
    clientX: 357,
    clientY: 170,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const op = f.apply.mock.calls[0][0].operations[0];
  if (op.type !== "element_put") throw Error("Expected element put");
  expect(op.element.content).toEqual({
    kind: "sticker",
    asset_id: "image-source",
    intrinsic_size: { width: 96, height: 48 },
  });
  expect(op.element.visual!.transform).toMatchObject({
    position_x_px: 37,
    position_y_px: -10,
    scale_x: 1,
    scale_y: 1,
  });
  expect(
    f.call.mock.calls.some(([name]) => name === "opencut_scene_geometry"),
  ).toBe(false);
});
it("rejects missing authoritative geometry rather than falling back to old metadata, while old hosts keep Sticker explicitly unavailable", async () => {
  const f = await setup({ sampled: true, corruptGeometry: true });
  expect(screen.queryByLabelText("Move selected element")).toBeNull();
  expect(f.apply).not.toHaveBeenCalled();
  expect(
    f.call.mock.calls.some(([name]) => name === "opencut_scene_geometry"),
  ).toBe(false);
  cleanup();
  await setup({ sticker: true, sampled: false });
  expect(screen.queryByLabelText("Move selected element")).toBeNull();
});
it("Sampled geometry rejects moving an animated position while allowing a cancelled single-axis resize without deleting its channel", async () => {
  const f = await setup({ sampled: true, animatedParameter: "position_x_px" });
  fireEvent.pointerDown(screen.getByLabelText("Move selected element"), {
    button: 0,
    pointerId: 10,
    clientX: 337,
    clientY: 160,
  });
  expect(
    screen.getByText(
      "This gesture would overwrite an animated position or rotation. Use the keyframe editor.",
    ),
  ).toBeTruthy();
  expect(f.preview).not.toHaveBeenCalled();
  const edge = screen.getAllByLabelText(
    "Resize selected element vertically",
  )[0];
  fireEvent.pointerDown(edge, {
    button: 0,
    pointerId: 11,
    clientX: 337,
    clientY: 0,
  });
  fireEvent.pointerMove(edge, { pointerId: 11, clientX: 337, clientY: -20 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  const op = f.preview.mock.calls[0][0].transaction.operations[0];
  if (op.type !== "element_put") throw Error("Expected element put");
  expect(op.element.channels).toEqual(
    f.state.scene!.head.project.sequences[0].tracks[0].elements[0].channels,
  );
  expect(f.preview.mock.calls[0][0].intent).toEqual({kind:"gesture",gesture:"scale_y"});
  fireEvent.keyDown(window, { key: "Escape" });
  fireEvent.pointerUp(edge, { pointerId: 11, clientX: 337, clientY: -20 });
  expect(f.apply).not.toHaveBeenCalled();
});
it("validates the complete draft before pointerup and commits exactly one original transaction", async () => {
  const f = await setup(),
    handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 357, clientY: 170 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 4,
    clientX: 357,
    clientY: 170,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const tx = f.apply.mock.calls[0][0];
  expect(tx).toMatchObject({expected_revision:"r0", operations:[{type:"element_put", element:{id:"image", visual:{transform:{
    position_x_px:37,position_y_px:-10,rotation_degrees:10,scale_x:1,scale_y:1,
  }}}}]});
  expect(f.calls).toEqual([]);
  expect(f.preview.mock.calls.at(-1)![0].transaction).toEqual(tx);
  expect(f.clear).toHaveBeenCalledWith(tx.idempotency_key);
});
it("seek cancels an awaiting draft validation before it can publish a transaction", async () => {
  let resolve!: () => void;
  const f = await setup(), handle = screen.getByLabelText("Move selected element");
  f.preview.mockImplementationOnce(() => new Promise<void>((done) => (resolve = done)));
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 4,
    clientX: 357,
    clientY: 160,
  });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  f.state.scene!.playhead = { ticks: "1", timebase: 1 };
  f.refresh();
  await act(async () => resolve());
  expect(f.apply).not.toHaveBeenCalled();
});
it("Escape clears already rendered content without a write or stale late commit", async () => {
  const f = await setup(),
    handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 2,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerMove(handle, { pointerId: 2, clientX: 357, clientY: 160 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  fireEvent.keyDown(window, { key: "Escape" });
  fireEvent.pointerUp(handle, { pointerId: 2, clientX: 357, clientY: 160 });
  expect(f.clear).toHaveBeenCalledWith(f.preview.mock.calls[0][0].draftId);
  expect(f.apply).not.toHaveBeenCalled();
});
it("selection consumes observed per-layer alpha, and rejects stale observations without a bounds fallback", async () => {
  const f = await setup(),
    surface = screen.getByLabelText("Select visible picture content");
  fireEvent.pointerDown(surface, {
    button: 0,
    pointerId: 2,
    clientX: 12.9,
    clientY: 15.1,
  });
  fireEvent.pointerUp(surface, {
    button: 0,
    pointerId: 2,
    clientX: 12.9,
    clientY: 15.1,
  });
  await waitFor(() => expect(f.hit).toHaveBeenCalledWith({ x: 12, y: 15 }));
  await waitFor(() => expect(f.select).toHaveBeenCalledWith("image"));
  f.select.mockClear();
  f.hit.mockResolvedValueOnce({
    projectId: "v4-project",
    revision: "other",
    sequenceId: "scene",
    time: f.state.scene!.playhead,
    hits: [{ elementId: "image", alpha: 255 }],
  });
  fireEvent.pointerDown(surface, {
    button: 0,
    pointerId: 3,
    clientX: 12,
    clientY: 15,
  });
  fireEvent.pointerUp(surface, {
    button: 0,
    pointerId: 3,
    clientX: 12,
    clientY: 15,
  });
  await screen.findByText(
    "The pixel hit observation differs from the fixed canvas.",
  );
  expect(f.select).not.toHaveBeenCalled();
});
it("transparent content and unavailable paused pixels never turn metadata bounds into a hit", async () => {
  const f = await setup(),
    surface = screen.getByLabelText("Select visible picture content");
  f.hit.mockResolvedValueOnce({
    projectId: "v4-project",
    revision: "r0",
    sequenceId: "scene",
    time: f.state.scene!.playhead,
    hits: [],
  });
  fireEvent.pointerDown(surface, {
    button: 0,
    pointerId: 2,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerUp(surface, {
    button: 0,
    pointerId: 2,
    clientX: 337,
    clientY: 160,
  });
  await waitFor(() => expect(f.select).toHaveBeenCalledWith(null));
  f.select.mockClear();
  f.hit.mockRejectedValueOnce(
    Error("Pause the live preview before selecting picture content."),
  );
  fireEvent.pointerDown(surface, {
    button: 0,
    pointerId: 3,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerUp(surface, {
    button: 0,
    pointerId: 3,
    clientX: 337,
    clientY: 160,
  });
  await screen.findByText(
    "Pause the live preview before selecting picture content.",
  );
  expect(f.select).not.toHaveBeenCalled();
});

it("routes a double click through the same in-flight alpha observation to the existing text editor", async () => {
  const f = await setup();
  const text = structuredClone(
    f.state.scene!.head.project.sequences[0].tracks[0].elements[0],
  );
  text.id = "title";
  text.content = { kind: "text", text: textInitial().text };
  f.state.scene!.head.project.sequences[0].tracks[0].elements.push(text);
  const beginCanvasEdit = vi.fn();
  context.authoring = {
    available: true,
    disabled: false,
    canvas: null,
    beginCanvasEdit,
  } as unknown as NonNullable<ReturnType<typeof useCanvasTextAuthoring>>;
  f.refresh();
  let finish!: (
    value: Awaited<ReturnType<typeof f.controller.scene.visualPreview.hitTest>>,
  ) => void;
  f.hit.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const surface = screen.getByLabelText("Select visible picture content");
  fireEvent.pointerDown(surface, {
    button: 0,
    pointerId: 2,
    clientX: 12,
    clientY: 15,
  });
  fireEvent.pointerUp(surface, {
    button: 0,
    pointerId: 2,
    clientX: 12,
    clientY: 15,
  });
  await waitFor(() => expect(f.hit).toHaveBeenCalledTimes(1));
  fireEvent.doubleClick(surface, { button: 0, clientX: 12, clientY: 15 });
  await act(async () =>
    finish({
      projectId: "v4-project",
      revision: "r0",
      sequenceId: "scene",
      time: f.state.scene!.playhead,
      hits: [{ elementId: "title", alpha: 128 }],
    }),
  );
  expect(f.hit).toHaveBeenCalledTimes(1);
  expect(beginCanvasEdit).toHaveBeenCalledWith("title");
  expect(f.apply).not.toHaveBeenCalled();
});
it("keeps the precise pointer texel when a scaled double-click MouseEvent rounds its coordinates", async () => {
  vi.mocked(Element.prototype.getBoundingClientRect).mockReturnValue({
    left: 0, top: 0, width: 320, height: 180, right: 320, bottom: 180,
    x: 0, y: 0, toJSON: () => ({}),
  });
  const f = await setup();
  const text = structuredClone(f.state.scene!.head.project.sequences[0].tracks[0].elements[0]);
  text.id = "title";
  text.content = { kind: "text", text: textInitial().text };
  f.state.scene!.head.project.sequences[0].tracks[0].elements.push(text);
  const beginCanvasEdit = vi.fn();
  context.authoring = { available: true, disabled: false, canvas: null, beginCanvasEdit } as unknown as NonNullable<ReturnType<typeof useCanvasTextAuthoring>>;
  f.refresh();
  let finish!: (value: Awaited<ReturnType<typeof f.controller.scene.visualPreview.hitTest>>) => void;
  f.hit.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const surface = screen.getByLabelText("Select visible picture content");
  fireEvent.pointerDown(surface, { button: 0, pointerId: 2, clientX: 12.75, clientY: 15.75 });
  fireEvent.pointerUp(surface, { button: 0, pointerId: 2, clientX: 12.75, clientY: 15.75 });
  await waitFor(() => expect(f.hit).toHaveBeenCalledTimes(1));
  expect(f.hit).toHaveBeenCalledWith({ x: 25, y: 31 });
  fireEvent.doubleClick(surface, { button: 0, clientX: 12, clientY: 15 });
  await act(async () => finish({ projectId: "v4-project", revision: "r0", sequenceId: "scene", time: f.state.scene!.playhead, hits: [{ elementId: "title", alpha: 255 }] }));
  expect(f.hit).toHaveBeenCalledTimes(1);
  expect(beginCanvasEdit).toHaveBeenCalledWith("title");
  fireEvent.doubleClick(surface, { button: 0, clientX: 99, clientY: 15 });
  await waitFor(() => expect(f.hit).toHaveBeenCalledTimes(2));
  expect(f.hit).toHaveBeenLastCalledWith({ x: 198, y: 30 });
  expect(f.apply).not.toHaveBeenCalled();
});

it("removes every transform hit target when canvas text input opens, leaving IME events to the text editor", async () => {
  const f = await setup();
  context.authoring = {
    available: true,
    disabled: false,
    canvas: {},
  } as NonNullable<ReturnType<typeof useCanvasTextAuthoring>>;
  f.refresh();
  expect(screen.queryByLabelText("Select visible picture content")).toBeNull();
  expect(screen.queryByLabelText("Move selected element")).toBeNull();
  fireEvent.keyDown(window, { key: "Process", isComposing: true });
  expect(f.hit).not.toHaveBeenCalled();
  expect(f.apply).not.toHaveBeenCalled();
});

it("previews a relative group and saves both members in one atomic transaction", async () => {
  const f = await setup({ multiple: true }),
    before = structuredClone(f.state.scene!.head.project),
    handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 370,
    clientY: 160,
  });
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 390, clientY: 170 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  const tx = f.preview.mock.calls[0][0].transaction;
  expect(tx.operations).toHaveLength(2);
  expect(
    tx.operations.map((op) => op.type === "element_put" && op.element.id),
  ).toEqual(["image", "second"]);
  const positions = tx.operations.map((op) =>
    op.type === "element_put"
      ? op.element.visual!.transform.position_x_px
      : NaN,
  );
  expect(positions).toEqual([37, 137]);
  expect(f.apply).not.toHaveBeenCalled();
  expect(f.state.scene!.head.project).toEqual(before);
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 4,
    clientX: 390,
    clientY: 170,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  expect(f.apply.mock.calls[0][0]).toEqual(tx);
});
it("modifier alpha selection toggles the shared timeline selection without a private second selection", async () => {
  const f = await setup({ multiple: true }),
    surface = screen.getByLabelText("Select visible picture content");
  fireEvent.pointerDown(surface, {
    button: 0,
    pointerId: 4,
    clientX: 20,
    clientY: 20,
    metaKey: true,
  });
  fireEvent.pointerUp(surface, {
    button: 0,
    pointerId: 4,
    clientX: 20,
    clientY: 20,
    metaKey: true,
  });
  await waitFor(() =>
    expect(f.selectMany).toHaveBeenCalledWith(
      [{ track_id: "track", element_id: "second" }],
      "second",
    ),
  );
  expect(f.select).not.toHaveBeenCalled();
  expect(f.apply).not.toHaveBeenCalled();
});
it.each(["head", "selection", "session", "playing", "busy", "api"])(
  "%s changes cancel visible draft and prevent its late continuation",
  async (change) => {
    const f = await setup(),
      handle = screen.getByLabelText("Move selected element");
    let done!: () => void;
    f.preview.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          done = resolve;
        }),
    );
    fireEvent.pointerDown(handle, {
      button: 0,
      pointerId: 4,
      clientX: 337,
      clientY: 160,
    });
    fireEvent.pointerMove(handle, { pointerId: 4, clientX: 357, clientY: 170 });
    await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
    const [draft, signal] = f.preview.mock.calls[0];
    if (change === "head") f.state.scene!.head.revision = "r1";
    else if (change === "selection") f.state.scene!.timelineEdit.selection = [];
    else if (change === "session")
      f.state.scene!.audioPreview.session!.generation = "2";
    else if (change === "playing") f.state.scene!.audioPreview.phase = "playing";
    else if (change === "busy") f.state.busy = true;
    else f.state.api = null;
    f.refresh();
    expect(signal.aborted).toBe(true);
    expect(f.clear).toHaveBeenCalledWith(draft.draftId);
    await act(async () => done());
    fireEvent.pointerUp(handle, {
      button: 0,
      pointerId: 4,
      clientX: 357,
      clientY: 170,
    });
    expect(f.apply).not.toHaveBeenCalled();
  },
);
it("an in-flight stale draft is cancelled and only the latest pointer candidate can remain", async () => {
  let finish!: () => void;
  const f = await setup(), handle = screen.getByLabelText("Move selected element");
  f.preview.mockImplementation(() => new Promise<void>((resolve) => { finish = resolve; }));
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 347, clientY: 160 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 357, clientY: 160 });
  expect(f.preview).toHaveBeenCalledTimes(1);
  expect(f.preview.mock.calls[0][1].aborted).toBe(true);
  await act(async () => finish());
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(2));
  await act(async () => finish());
  const op = f.preview.mock.calls[1][0].transaction.operations[0];
  expect(
    op.type === "element_put" && op.element.visual!.transform.position_x_px,
  ).toBe(37);
  fireEvent.keyDown(window, { key: "Escape" });
  expect(f.apply).not.toHaveBeenCalled();
});
it("Escape during pointerup validation clears pixels immediately even if the request ignores abort", async () => {
  const f = await setup(),
    handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 357, clientY: 160 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  let done!: () => void;
  f.preview.mockImplementationOnce(() => new Promise<void>((resolve) => { done = resolve; }));
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 4,
    clientX: 357,
    clientY: 160,
  });
  await waitFor(() => expect(done).toBeDefined());
  fireEvent.keyDown(window, { key: "Escape" });
  expect(f.clear).toHaveBeenCalledWith(f.preview.mock.calls[0][0].draftId);
  await act(async () => done());
  expect(f.apply).not.toHaveBeenCalled();
});
it("one rejected group member rejects the complete draft and cannot commit a partial group", async () => {
  const f = await setup({ multiple: true }), handle = screen.getByLabelText("Move selected element");
  f.preview.mockRejectedValueOnce(Error("The first member is no longer editable."));
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 370,
    clientY: 160,
  });
  fireEvent.pointerUp(handle, {
    button: 0,
    pointerId: 4,
    clientX: 390,
    clientY: 170,
  });
  await screen.findByText("The first member is no longer editable.");
  expect(f.preview).toHaveBeenCalledTimes(1);
  const [draft, signal] = f.preview.mock.calls[0];
  expect(draft.transaction.operations).toHaveLength(2);
  expect(signal.aborted).toBe(true);
  expect(f.clear).toHaveBeenCalledWith(draft.draftId);
  expect(f.apply).not.toHaveBeenCalled();
});
it("a new pointer move does not abort an already presented draft while its replacement is planned", async () => {
  const f = await setup(),
    handle = screen.getByLabelText("Move selected element");
  fireEvent.pointerDown(handle, {
    button: 0,
    pointerId: 4,
    clientX: 337,
    clientY: 160,
  });
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 347, clientY: 160 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(1));
  await act(async () => {});
  const originalSignal = f.preview.mock.calls[0][1];
  fireEvent.pointerMove(handle, { pointerId: 4, clientX: 357, clientY: 160 });
  await waitFor(() => expect(f.preview).toHaveBeenCalledTimes(2));
  expect(originalSignal.aborted).toBe(false);
  fireEvent.keyDown(window, { key: "Escape" });
  expect(f.clear).toHaveBeenCalledWith(f.preview.mock.calls[1][0].draftId);
  expect(f.apply).not.toHaveBeenCalled();
});
