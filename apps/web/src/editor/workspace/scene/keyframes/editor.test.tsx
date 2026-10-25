// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../../../shared/locale";
import { setup as createFixture } from "../fixture";
import { KeyframeEditor } from "./editor";
import type { ScalarChannel } from "./types";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
beforeEach(() => {
  preferences({ language: "en" });
  localStorage.clear();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup() {
  const f = await createFixture(),
    state = {
      ...f.controller.getSnapshot(),
      scene: structuredClone(f.controller.getSnapshot().scene),
    };
  state.scene!.selectedId = "image";
  const element = state.scene!.head.project.sequences[0].tracks[0].elements[0];
  const channel: ScalarChannel = {
    id: "channel",
    kind: "scalar",
    target: { kind: "element", element_id: "image" },
    binding: { kind: "property", parameter: "position_x_px" },
    extrapolation: { before: "hold", after: "linear" },
    keys: [
      {
        id: "first",
        time: { ticks: "1", timebase: 3 },
        value: 0,
        left_handle: null,
        right_handle: null,
        segment_to_next: { kind: "linear" },
        tangent_mode: "broken",
      },
      {
        id: "second",
        time: { ticks: "4", timebase: 3 },
        value: 100,
        left_handle: null,
        right_handle: null,
        segment_to_next: { kind: "linear" },
        tangent_mode: "broken",
      },
    ],
  };
  element.channels = [channel];
  vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_keyframe_plan"],
  } as never);
  const original = f.call.getMockImplementation()!,
    plan = vi.fn(async (request: unknown) => {
      const r = request as {
        project_id: string;
        expected_revision: string;
        idempotency_key: string;
        label: string;
      };
      return {
        ...r,
        operations: [
          {
            type: "element_put",
            sequence_id: state.scene!.sequenceId,
            track_id: state.scene!.head.project.sequences[0].tracks[0].id,
            element,
          },
        ],
      };
    });
  f.call.mockImplementation(async (name, ...args) =>
    name === "opencut_keyframe_plan"
      ? ((await plan(args[0])) as never)
      : original(name, ...args),
  );
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
    .spyOn(f.controller.scene, "applyPlanned")
    .mockResolvedValue({} as never);
  context.value = { ...state, controller: f.controller };
  const view = render(<KeyframeEditor elementId="image" />);
  await screen.findByRole("button", { name: "Add keyframe" });
  return {
    f,
    state,
    plan,
    apply,
    view,
    refresh() {
      context.value = { ...state, controller: f.controller };
      view.rerender(<KeyframeEditor elementId="image" />);
    },
  };
}
it("submits one exact group move to the Rust planner and existing atomic recovery coordinator", async () => {
  const s = await setup();
  try {
    const boxes = screen.getAllByRole("checkbox");
    fireEvent.click(boxes[0]);
    fireEvent.click(boxes[1]);
    fireEvent.change(screen.getByLabelText("Move by (seconds)"), {
      target: { value: "0.5" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Move selected keyframes" }),
    );
    await waitFor(() => expect(s.apply).toHaveBeenCalledTimes(1));
    expect(s.plan.mock.calls[0][0]).toMatchObject({
      expected_revision: s.state.scene!.head.revision,
      edits: [
        {
          kind: "move",
          keys: [
            { channel_id: "channel", key_id: "first" },
            { channel_id: "channel", key_id: "second" },
          ],
          delta: { ticks: "60000", timebase: 120000 },
        },
      ],
    });
  } finally {
    s.f.controller.dispose();
  }
});
it("failed planning retries the identical request and key even after draft fields change", async () => {
  const s = await setup();
  try {
    s.plan.mockRejectedValueOnce(new Error("Temporary failure"));
    fireEvent.click(screen.getAllByRole("checkbox")[0]);
    fireEvent.change(screen.getByLabelText("Move by (seconds)"), {
      target: { value: "1" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Move selected keyframes" }),
    );
    await screen.findByText("Temporary failure");
    const request = s.plan.mock.calls[0][0];
    fireEvent.change(screen.getByLabelText("Move by (seconds)"), {
      target: { value: "2" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Retry saving keyframes" }),
    );
    await waitFor(() => expect(s.plan).toHaveBeenCalledTimes(2));
    expect(s.plan.mock.calls[1][0]).toBe(request);
    expect(s.apply).toHaveBeenCalledTimes(1);
  } finally {
    s.f.controller.dispose();
  }
});
it("Pop Y controls remain greater than one and a stale plan never writes", async () => {
  const s = await setup();
  try {
    fireEvent.click(screen.getAllByRole("checkbox")[0]);
    fireEvent.click(screen.getAllByRole("checkbox")[1]);
    fireEvent.change(screen.getByLabelText("Animation easing"), {
      target: { value: "pop" },
    });
    let release: (value: never) => void = () => {};
    s.plan.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Apply between selected keyframes" }),
    );
    await waitFor(() => expect(s.plan).toHaveBeenCalledTimes(1));
    expect(s.plan.mock.calls[0][0]).toMatchObject({
      edits: [
        { kind: "curve", curve: { x1: 0.175, y1: 0.885, x2: 0.32, y2: 1.275 } },
      ],
    });
    s.state.scene!.head.revision = "new-revision";
    s.refresh();
    release({
      project_id: s.state.scene!.head.project.id,
      expected_revision: "old",
      operations: [{ type: "element_put" }],
    } as never);
    await waitFor(() => expect(s.apply).not.toHaveBeenCalled());
  } finally {
    s.f.controller.dispose();
  }
});

it("exposes the effect layer's content effect as an animation target without requiring visual metadata", async () => {
  const s = await setup();
  try {
    const element = s.state.scene!.head.project.sequences[0].tracks[0].elements[0];
    element.content = {
      kind: "effect_layer",
      effect: {
        id: "layer-blur",
        enabled: true,
        effect: { kind: "classic_blur", intensity: 10 },
      },
    };
    element.visual = null;
    element.channels = [];
    s.refresh();
    fireEvent.change(screen.getByLabelText("Apply animation to"), {
      target: { value: "effect:layer-blur" },
    });
    fireEvent.change(screen.getByLabelText("Property"), {
      target: { value: "classic_blur_intensity" },
    });
    fireEvent.change(screen.getByLabelText("Value"), {
      target: { value: "25" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Add animated property" }),
    );
    await waitFor(() => expect(s.plan).toHaveBeenCalledOnce());
    expect(s.plan.mock.calls[0][0]).toMatchObject({
      edits: [
        {
          kind: "channel_put",
          owner: { kind: "element", element_id: "image" },
          channel: {
            target: {
              kind: "effect",
              element_id: "image",
              effect_id: "layer-blur",
            },
            binding: { kind: "property", parameter: "classic_blur_intensity" },
            keys: [{ value: 25 }],
          },
        },
      ],
    });
    expect(s.apply).toHaveBeenCalledOnce();
  } finally {
    s.f.controller.dispose();
  }
});
