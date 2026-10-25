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
import { setupAudioGain } from "../audio-gain-fixture";
import { gainFixture } from "./fixture";
import { GainKeyframeLine } from "./line";
import type { ToolInput } from "../../../../lib/editor-sdk";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value, useEditingWorkspace: () => context.value }));
beforeEach(() => {
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
    width: 300,
    height: 128,
    right: 300,
    bottom: 128,
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
    plan?: () => Promise<unknown>;
    staleSample?: boolean;
    compact?: boolean;
  } = {},
) {
  const f = await setupAudioGain(),
    state = {
      ...f.controller.getSnapshot(),
      scene: structuredClone(f.controller.getSnapshot().scene),
    },
    element = state.scene!.head.project.sequences[0].tracks[0].elements[0],
    channel = gainFixture();
  channel.target = { kind: "element", element_id: element.id };
  element.channels = [channel];
  state.scene!.playhead = { ticks: "1", timebase: 1 };
  const before = structuredClone(element),
    requests: ToolInput<"opencut_keyframe_plan">[] = [],
    reads: ToolInput<"opencut_keyframe_sample">[] = [],
    original = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_keyframe_sample", "opencut_keyframe_plan"],
  } as never);
  vi.mocked(f.api.sdk.call).mockImplementation(
    async (name, input, callOptions) => {
      if (name === "opencut_keyframe_sample") {
        const request = input as ToolInput<"opencut_keyframe_sample">;
        reads.push(request);
        return {
          revision: options.staleSample ? "wrong" : request.revision,
          channel_id: request.channel_id,
          samples: request.times.map((time) => ({
            time,
            value: -9.5,
            physical_value: -11,
          })),
        } as never;
      }
      if (name === "opencut_keyframe_plan") {
        const request = input as ToolInput<"opencut_keyframe_plan">;
        requests.push(request);
        await options.plan?.();
        const next = structuredClone(element);
        if (next.channels[0].kind !== "scalar") throw Error();
        for (const edit of request.edits)
          if (edit.kind === "put_scalar") {
            const index = next.channels[0].keys.findIndex(
              (k) => k.id === edit.key.id,
            );
            if (index >= 0) next.channels[0].keys[index] = edit.key;
            else next.channels[0].keys.push(edit.key);
          }
        return {
          project_id: request.project_id,
          expected_revision: request.expected_revision,
          idempotency_key: request.idempotency_key,
          label: request.label,
          operations: [
            {
              type: "element_put",
              sequence_id: request.sequence_id,
              track_id: "track",
              element: next,
            },
          ],
        } as never;
      }
      return original(name, input, callOptions);
    },
  );
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
    .spyOn(f.controller.scene, "applyPlanned")
    .mockResolvedValue({} as never);
  context.value = { ...state, controller: f.controller };
  const view = render(
      <GainKeyframeLine elementId={element.id} compact={options.compact} />,
    ),
    refresh = () => {
      context.value = { ...state, controller: f.controller };
      view.rerender(
        <GainKeyframeLine elementId={element.id} compact={options.compact} />,
      );
    };
  if (!options.staleSample) await screen.findByLabelText("Volume keyframes");
  return {
    ...f,
    state,
    view,
    element,
    channel,
    before,
    requests,
    reads,
    apply,
    refresh,
  };
}
it("draws the Rust physical samples and batches a whole-line offset without rewriting tangent identities", async () => {
  const f = await setup(),
    line = screen.getByRole("button", { name: "Move volume curve" });
  const displayedValues = line
    .getAttribute("points")!
    .split(" ")
    .map((point) => Number(point.split(",")[1]));
  expect(displayedValues.every((value) => Math.abs(value - 52.3) < 1e-10)).toBe(
    true,
  );
  fireEvent.pointerDown(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 40,
  });
  fireEvent.pointerMove(line, { pointerId: 1, clientX: 40, clientY: 35 });
  expect(f.requests).toHaveLength(0);
  fireEvent.pointerUp(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 35,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  const request = f.requests[0];
  expect(request.edits).toHaveLength(2);
  expect(f.apply.mock.calls[0][0].idempotency_key).toBe(
    request.idempotency_key,
  );
  for (let i = 0; i < 2; i++) {
    const edit = request.edits[i];
    if (edit.kind !== "put_scalar") throw Error();
    const key = f.channel.keys[i];
    expect({ ...edit.key, value: key.value }).toEqual(key);
    expect(edit.key.value - key.value).toBeCloseTo((80 * 5) / 104);
  }
  expect(f.element).toEqual(f.before);
});
it("a click without movement selects a key and does not rewrite its noncanonical exact zero time", async () => {
  const f = await setup(),
    point = screen.getByRole("button", {
      name: "Volume point 0.000 s · -20.0 dB",
    });
  fireEvent.pointerDown(point, {
    button: 0,
    pointerId: 1,
    clientX: 0,
    clientY: 38,
  });
  fireEvent.pointerUp(point, {
    button: 0,
    pointerId: 1,
    clientX: 0,
    clientY: 38,
  });
  expect(f.requests).toHaveLength(0);
  expect(f.apply).not.toHaveBeenCalled();
});
it("Escape cancels a planned gain edit and a later reply cannot apply it", async () => {
  let resolve!: () => void;
  const f = await setup({
      plan: () => new Promise<void>((done) => (resolve = done)),
    }),
    line = screen.getByRole("button", { name: "Move volume curve" });
  fireEvent.pointerDown(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 40,
  });
  fireEvent.pointerUp(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 35,
  });
  await waitFor(() => expect(f.requests).toHaveLength(1));
  fireEvent.keyDown(window, { key: "Escape" });
  await act(async () => resolve());
  expect(f.apply).not.toHaveBeenCalled();
});
it("seek or playback during an active gesture abandons its draft with no write", async () => {
  const f = await setup(),
    line = screen.getByRole("button", { name: "Move volume curve" });
  fireEvent.pointerDown(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 40,
  });
  fireEvent.pointerMove(line, { pointerId: 1, clientX: 40, clientY: 35 });
  f.state.scene!.audioPreview.phase = "playing";
  f.refresh();
  fireEvent.pointerUp(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 35,
  });
  expect(f.requests).toHaveLength(0);
  expect(f.apply).not.toHaveBeenCalled();
});
it("asks Rust to insert at the exact local playhead and split neighboring controls", async () => {
  const f = await setup();
  fireEvent.click(screen.getByRole("button", { name: "Add key at playhead" }));
  await waitFor(() => expect(f.requests).toHaveLength(1));
  expect(f.requests[0].edits[0]).toMatchObject({
    kind: "insert_scalar",
    channel_id: "gain",
    time: { ticks: "1", timebase: 1 },
  });
  expect(f.requests[0].edits).toHaveLength(1);
});
it("rejects a different sampled revision and never invents a client interpolation fallback", async () => {
  const f = await setup({ staleSample: true });
  await screen.findByText(
    "The sampled gain curve differs from the fixed channel.",
  );
  expect(screen.queryByLabelText("Volume keyframes")).toBeNull();
  expect(f.apply).not.toHaveBeenCalled();
});

it("applies smoothing as one atomic curve plan and discards a reply after the head changes", async () => {
  let resolve!: () => void;
  const f = await setup({
    plan: () => new Promise<void>((done) => (resolve = done)),
  });
  fireEvent.click(screen.getByRole("button", { name: "Smooth changes" }));
  await waitFor(() => expect(f.requests).toHaveLength(1));
  expect(f.requests[0].edits).toEqual([
    {
      kind: "curve",
      channel_id: "gain",
      left_key_id: "first",
      right_key_id: "second",
      curve: { x1: 0.4, y1: 0, x2: 0.2, y2: 1 },
      reference_span_value: 1,
    },
  ]);
  f.state.scene!.head.revision = "another-edit";
  f.refresh();
  await act(async () => resolve());
  expect(f.apply).not.toHaveBeenCalled();
  expect(f.element).toEqual(f.before);
});

it("Alt starts a fine drag without changing the original times or curve handles", async () => {
  const f = await setup();
  const point = screen.getByRole("button", {
    name: "Volume point 0.000 s · -20.0 dB",
  });
  fireEvent.pointerDown(point, {
    button: 0,
    pointerId: 1,
    clientX: 12,
    clientY: 38,
    altKey: true,
  });
  fireEvent.pointerUp(point, {
    button: 0,
    pointerId: 1,
    clientX: 12,
    clientY: 28,
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  expect(f.requests[0].edits).toHaveLength(1);
  const edit = f.requests[0].edits[0];
  if (edit.kind !== "put_scalar") throw Error();
  expect(edit.key.value).toBeCloseTo(-20 + (80 * 10) / 104 / 5);
  expect({ ...edit.key, value: -20 }).toEqual(f.channel.keys[0]);
});

it("zooms a short opening independently of the whole clip without saving a project edit", async () => {
  const f = await setup();
  const point = screen.getByRole("button", {
    name: "Volume point 2.000 s · -5.0 dB",
  });
  expect(
    Number(point.querySelector(".gain-key-dot")!.getAttribute("cx")),
  ).toBeCloseTo(288);
  fireEvent.click(screen.getByRole("button", { name: "Whole clip" }));
  expect(
    Number(point.querySelector(".gain-key-dot")!.getAttribute("cx")),
  ).toBeCloseTo(58);
  expect(f.requests).toHaveLength(0);
  expect(f.apply).not.toHaveBeenCalled();
});

it("keeps the sampled graph mounted without new RPCs during playhead and busy updates", async () => {
  const f = await setup();
  const graph = screen.getByLabelText("Volume keyframes");
  const reads = f.reads.length;
  f.state.scene!.playhead = { ticks: "2", timebase: 1 };
  f.state.scene!.audioPreview.phase = "playing";
  f.state.busy = true;
  f.refresh();
  expect(screen.getByLabelText("Volume keyframes")).toBe(graph);
  expect(f.reads).toHaveLength(reads);
});
it("draws the authored curve while the pointer is down without planning every movement", async () => {
  const f = await setup();
  const key = screen.getAllByRole("button", { name: /Volume point/ })[0];
  fireEvent.pointerDown(key, {
    button: 0,
    pointerId: 1,
    clientX: 20,
    clientY: 40,
  });
  fireEvent.pointerMove(key, { pointerId: 1, clientX: 20, clientY: 32 });
  expect(
    f.view.container.querySelector("path.gain-sampled-curve"),
  ).not.toBeNull();
  expect(f.requests).toHaveLength(0);
  fireEvent.pointerUp(key, { pointerId: 1, clientX: 20, clientY: 32 });
  await waitFor(() => expect(f.requests).toHaveLength(1));
});

it("samples the displayed historical revision and keeps it read-only",async()=>{
 const f=await setup();
 f.state.scene!.historical={...structuredClone(f.state.scene!.head),revision:"displayed-history"};f.refresh();
 await waitFor(()=>expect(f.reads.at(-1)!.revision).toBe("displayed-history"));
 expect((screen.getByRole("button",{name:"Add key at playhead"}) as HTMLButtonElement).disabled).toBe(true);
 expect(f.apply).not.toHaveBeenCalled();
});
