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
import { V4GainKeyframeLine } from "./line";
import type { ToolInput } from "../../../../lib/editor-sdk";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
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
    height: 76,
    right: 300,
    bottom: 76,
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
  options: { plan?: () => Promise<unknown>; staleSample?: boolean } = {},
) {
  const f = await setupAudioGain(),
    state = {
      ...f.controller.getSnapshot(),
      v4: structuredClone(f.controller.getSnapshot().v4),
    },
    element = state.v4!.head.project.sequences[0].tracks[0].elements[0],
    channel = gainFixture();
  channel.target = { kind: "element", element_id: element.id };
  element.channels = [channel];
  state.v4!.playhead = { ticks: "1", timebase: 1 };
  const before = structuredClone(element),
    requests: ToolInput<"opencut_keyframe_v1_plan">[] = [],
    reads: ToolInput<"opencut_keyframe_v1_sample">[] = [],
    original = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_keyframe_v1_sample", "opencut_keyframe_v1_plan"],
  } as never);
  vi.mocked(f.api.sdk.call).mockImplementation(
    async (name, input, callOptions) => {
      if (name === "opencut_keyframe_v1_sample") {
        const request = input as ToolInput<"opencut_keyframe_v1_sample">;
        reads.push(request);
        return {
          schema_version: 1,
          revision: options.staleSample ? "wrong" : request.revision,
          channel_id: request.channel_id,
          samples: request.times.map((time) => ({
            time,
            value: -9.5,
            physical_value: -11,
          })),
        } as never;
      }
      if (name === "opencut_keyframe_v1_plan") {
        const request = input as ToolInput<"opencut_keyframe_v1_plan">;
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
    .spyOn(f.controller.v4, "applyPlanned")
    .mockResolvedValue({} as never);
  context.value = { ...state, controller: f.controller };
  const view = render(<V4GainKeyframeLine elementId={element.id} />),
    refresh = () => {
      context.value = { ...state, controller: f.controller };
      view.rerender(<V4GainKeyframeLine elementId={element.id} />);
    };
  if (!options.staleSample) await screen.findByLabelText("Gain keyframes");
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
    line = screen.getByRole("button", { name: "Move volume automation" });
  expect(line.getAttribute("points")).toContain("29.45");
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
    expect(edit.key.value - key.value).toBeCloseTo((80 * 5) / 76);
  }
  expect(f.element).toEqual(f.before);
});
it("a click without movement selects a key and does not rewrite its noncanonical exact zero time", async () => {
  const f = await setup(),
    point = screen.getByRole("button", { name: "Gain keyframe first" });
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
    line = screen.getByRole("button", { name: "Move volume automation" });
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
    line = screen.getByRole("button", { name: "Move volume automation" });
  fireEvent.pointerDown(line, {
    button: 0,
    pointerId: 1,
    clientX: 40,
    clientY: 40,
  });
  fireEvent.pointerMove(line, { pointerId: 1, clientX: 40, clientY: 35 });
  f.state.v4!.audioPreview.phase = "playing";
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
it("creates a key at the exact local playhead from the authoritative raw sample", async () => {
  const f = await setup();
  fireEvent.click(screen.getByRole("button", { name: "Add key at playhead" }));
  await waitFor(() => expect(f.requests).toHaveLength(1));
  expect(f.requests[0].edits[0]).toMatchObject({
    kind: "put_scalar",
    channel_id: "gain",
    key: {
      time: { ticks: "1", timebase: 1 },
      value: -9.5,
      left_handle: null,
      right_handle: null,
    },
  });
  expect(f.requests[0].edits).toHaveLength(1);
});
it("rejects a different sampled revision and never invents a client interpolation fallback", async () => {
  const f = await setup({ staleSample: true });
  await screen.findByText(
    "The sampled gain curve differs from the fixed channel.",
  );
  expect(screen.queryByLabelText("Gain keyframes")).toBeNull();
  expect(f.apply).not.toHaveBeenCalled();
});
