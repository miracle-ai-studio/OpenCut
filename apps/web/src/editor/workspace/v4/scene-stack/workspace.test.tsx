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
import { setupV4 } from "../fixture";
import { V4SceneStackProvider } from "./workspace";
import { V4SceneStackInspector, V4SceneEffectsPanel } from "./panel";
import { createMask } from "./model";
import { geometry, font } from "./fixture";
import type { SceneGeometry, SceneEditRequest } from "./types";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
function Product() {
  return (
    <V4SceneStackProvider>
      <V4SceneStackInspector elementId="image" />
      <V4SceneEffectsPanel />
    </V4SceneStackProvider>
  );
}
async function setup(
  options: {
    read?: () => Promise<SceneGeometry>;
    plan?: () => Promise<unknown>;
  } = {},
) {
  const f = await setupV4(),
    state = {
      ...f.controller.getSnapshot(),
      v4: structuredClone(f.controller.getSnapshot().v4),
    };
  state.v4!.selectedId = "image";
  state.v4!.head.project.fonts = [font];
  const read = vi.fn(options.read ?? (async () => structuredClone(geometry))),
    plan = vi.fn(options.plan ?? (async () => ({}))),
    call = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (name, ...args) =>
    name === "opencut_scene_v1_geometry"
      ? ((await read()) as never)
      : name === "opencut_edit_v4_plan"
        ? ((await plan()) as never)
        : call(name, ...args),
  );
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_scene_v1_geometry", "opencut_edit_v4_plan"],
  } as never);
  const apply = vi
    .spyOn(f.controller.v4, "applyPlanned")
    .mockResolvedValue({} as never);
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  context.value = { ...state, controller: f.controller };
  const view = render(<Product />);
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Edit visual stack",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
  const refresh = () => {
    context.value = { ...state, controller: f.controller };
    view.rerender(<Product />);
  };
  return { ...f, state, view, read, plan, apply, refresh };
}
async function open() {
  fireEvent.click(screen.getByRole("button", { name: "Edit visual stack" }));
  await waitFor(() =>
    expect(
      (screen.getByRole("button", { name: "Save stack" }) as HTMLButtonElement)
        .disabled,
    ).toBe(false),
  );
}
it("saves a complete stack using one planned original request and preserves untouched transform/source fields", async () => {
  const f = await setup(),
    source = structuredClone(
      f.state.v4!.head.project.sequences[0].tracks[0].elements[0],
    );
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Add mask" }));
  fireEvent.change(screen.getByLabelText("Mask shape"), {
    target: { value: "heart" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add mask" }));
  fireEvent.click(screen.getByLabelText("Inverted"));
  fireEvent.change(screen.getByLabelText("Stroke width (pixels)"), {
    target: { value: "3" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add blur" }));
  fireEvent.click(screen.getByRole("button", { name: "Save stack" }));
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const request = f.apply.mock.calls[0][0],
    planned = f.call.mock.calls.find(
      ([name]) => name === "opencut_edit_v4_plan",
    )![1];
  expect(request).toEqual(planned);
  expect(request.expected_revision).toBe("r0");
  expect(request.idempotency_key).toBeTruthy();
  expect(request.operations).toHaveLength(1);
  const op = request.operations[0];
  if (op.type !== "element_put") throw Error();
  expect(op.element.visual!.masks).toHaveLength(2);
  expect(op.element.visual!.masks[1]).toMatchObject({
    inverted: true,
    stroke: { width_px: 3 },
  });
  expect(op.element.visual!.effects[0].effect).toEqual({
    kind: "classic_blur",
    intensity: 15,
  });
  expect({ ...op.element, visual: source.visual }).toEqual(source);
  expect(op.element.visual!.transform).toEqual(source.visual!.transform);
});
it("discards a late geometry reply after a seek and cannot save the previous draft", async () => {
  let resolve!: (value: SceneGeometry) => void;
  const f = await setup({
    read: () => new Promise((done) => (resolve = done)),
  });
  fireEvent.click(screen.getByRole("button", { name: "Edit visual stack" }));
  await waitFor(() => expect(f.read).toHaveBeenCalled());
  f.state.v4!.playhead = { ticks: "1", timebase: 1 };
  f.refresh();
  await act(async () => resolve(geometry));
  expect(screen.queryByRole("button", { name: "Save stack" })).toBeNull();
  expect(f.apply).not.toHaveBeenCalled();
});
it("refuses a planned write if the fixed revision or playback state changes while awaiting validation", async () => {
  let resolve!: (value: unknown) => void;
  const f = await setup({
    plan: () => new Promise((done) => (resolve = done)),
  });
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Add mask" }));
  fireEvent.click(screen.getByRole("button", { name: "Save stack" }));
  await waitFor(() => expect(f.plan).toHaveBeenCalled());
  f.state.v4!.audioPreview.phase = "playing";
  f.refresh();
  await act(async () => resolve({}));
  expect(f.apply).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "Save stack" })).toBeNull();
});
it("preserves animated mask identities and channels while editing base values", async () => {
  const f = await setup({
      read: async () => {
        throw Error("Animated targets cannot be edited statically");
      },
    }),
    element = f.state.v4!.head.project.sequences[0].tracks[0].elements[0],
    mask = createMask("split", null);
  element.visual!.masks = [mask];
  element.channels = [
    {
      kind: "scalar",
      id: "channel",
      target: { kind: "mask", element_id: "image", mask_id: mask.id },
      binding: { kind: "property", parameter: "mask_center_x" },
      keys: [],
      extrapolation: { before: "hold", after: "hold" },
    },
  ];
  f.refresh();
  await open();
  fireEvent.click(screen.getByLabelText("Inverted"));
  fireEvent.click(screen.getByRole("button", { name: "Save stack" }));
  await waitFor(() => expect(f.apply).toHaveBeenCalled());
  const op = f.apply.mock.calls[0][0].operations[0];
  if (op.type !== "element_put") throw Error();
  expect(op.element.channels).toEqual(element.channels);
  expect(op.element.visual!.masks[0].id).toBe(mask.id);
  expect(op.element.visual!.masks[0].inverted).toBe(true);
});
it("removes an animated mask and effect with their own channels in one original ElementPut request", async () => {
  const f = await setup(), element = f.state.v4!.head.project.sequences[0].tracks[0].elements[0],
    mask = createMask("split", null);
  element.visual!.masks = [mask];
  element.visual!.effects = [{ id: "blur", enabled: true, effect: { kind: "classic_blur", intensity: 15 } }];
  element.channels = [
    { kind: "scalar", id: "mask-channel", target: { kind: "mask", element_id: element.id, mask_id: mask.id }, binding: { kind: "property", parameter: "mask_center_x" }, keys: [], extrapolation: { before: "hold", after: "hold" } },
    { kind: "scalar", id: "effect-channel", target: { kind: "effect", element_id: element.id, effect_id: "blur" }, binding: { kind: "property", parameter: "classic_blur_intensity" }, keys: [], extrapolation: { before: "hold", after: "hold" } },
    { kind: "scalar", id: "opacity-channel", target: { kind: "element", element_id: element.id }, binding: { kind: "property", parameter: "opacity" }, keys: [], extrapolation: { before: "hold", after: "hold" } },
  ];
  const before = structuredClone(element);
  f.refresh(); await open();
  fireEvent.click(screen.getByRole("button", { name: "Remove mask 1" }));
  fireEvent.click(screen.getByRole("button", { name: "Remove effect" }));
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Save stack" }));
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const request = f.apply.mock.calls[0][0];
  expect(request).toEqual(f.call.mock.calls.find(([name]) => name === "opencut_edit_v4_plan")![1]);
  expect(request.operations).toHaveLength(1);
  const op = request.operations[0];
  if (op.type !== "element_put") throw Error();
  expect(op.element.visual!.masks).toEqual([]);
  expect(op.element.visual!.effects).toEqual([]);
  expect(op.element.channels).toEqual([before.channels[2]]);
  expect({ ...op.element, channels: before.channels, visual: before.visual }).toEqual(before);
  expect(element).toEqual(before);
});

it("can delete the final freeform point, save an empty open draft and append a finite new point", async () => {
  const f = await setup(), element = f.state.v4!.head.project.sequences[0].tracks[0].elements[0],
    mask = createMask("freeform", null);
  if (mask.shape.kind !== "freeform") throw Error();
  mask.shape.path.points.push({ id: "third", x: 0, y: 0.5, in_x: 0, in_y: 0.5, out_x: 0, out_y: 0.5 });
  mask.shape.path.closed = true;
  element.visual!.masks = [mask];
  f.refresh(); await open();
  fireEvent.click(screen.getAllByRole("button", { name: "Delete point" })[0]);
  expect((screen.getByLabelText("Closed path") as HTMLInputElement).checked).toBe(false);
  fireEvent.click(screen.getAllByRole("button", { name: "Delete point" })[0]);
  fireEvent.click(screen.getByRole("button", { name: "Delete point" }));
  expect(screen.queryByRole("button", { name: "Delete point" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Append point" }));
  for (const name of ["Anchor X (%)", "Anchor Y (%)", "In X (%)", "In Y (%)", "Out X (%)", "Out Y (%)"])
    expect((screen.getByLabelText(name) as HTMLInputElement).valueAsNumber).toBe(0);
  fireEvent.click(screen.getByRole("button", { name: "Delete point" }));
  fireEvent.click(screen.getByRole("button", { name: "Save stack" }));
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const op = f.apply.mock.calls[0][0].operations[0];
  if (op.type !== "element_put") throw Error();
  expect(op.element.visual!.masks[0].shape).toEqual({ kind: "freeform", path: { ...mask.shape.path, closed: false, points: [] } });
});
it("creates a real effect-layer track at the exact playhead using the original transaction port", async () => {
  const f = await setup();
  f.state.v4!.playhead = { ticks: "1001", timebase: 30000 };
  f.refresh();
  fireEvent.change(screen.getByLabelText("Duration (seconds)"), {
    target: { value: "2.25" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add effect layer" }));
  await waitFor(() => expect(f.apply).toHaveBeenCalled());
  const op = f.apply.mock.calls[0][0].operations[0];
  if (op.type !== "track_put") throw Error();
  expect(op.track.kind).toBe("effect");
  expect(op.track.elements[0].content.kind).toBe("effect_layer");
  expect(op.track.elements[0].timeline_range).toEqual({
    start: { ticks: "1001", timebase: 30000 },
    duration: { ticks: "9", timebase: 4 },
  });
});
it("text masks bind the selected imported face and freeform controls keep open/closed meaning explicit", async () => {
  const f = await setup();
  await open();
  fireEvent.change(screen.getByLabelText("Mask shape"), {
    target: { value: "text" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add mask" }));
  fireEvent.change(screen.getByLabelText("Content"), {
    target: { value: "Title" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save stack" }));
  await waitFor(() => expect(f.apply).toHaveBeenCalled());
  const tx = f.apply.mock.calls[0][0] as SceneEditRequest,
    op = tx.operations[0];
  if (op.type !== "element_put") throw Error();
  expect(op.element.visual!.masks[0].shape).toMatchObject({
    kind: "text",
    mask: {
      content: "Title",
      font: { kind: "locked", font_id: "font" },
      font_weight: "bold",
      font_style: "italic",
    },
  });
});
it("history and unresolved recovery disable both the editor and layer mutations", async () => {
  const f = await setup();
  f.state.v4!.historical = structuredClone(f.state.v4!.head);
  f.refresh();
  expect(
    (
      screen.getByRole("button", {
        name: "Edit visual stack",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(
    (
      screen.getByRole("button", {
        name: "Add effect layer",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(f.apply).not.toHaveBeenCalled();
});
