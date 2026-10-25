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
import { LegacyKeyframeConversion } from "./legacy-conversion";
import type { ScalarChannel } from "./types";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useWorkspace: () => context.value }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup(linear = false) {
  const f = await createFixture(),
    state = {
      ...f.controller.getSnapshot(),
      scene: structuredClone(f.controller.getSnapshot().scene),
    };
  state.scene!.selectedId = "image";
  const element = state.scene!.head.project.sequences[0].tracks[0].elements[0];
  const channel: ScalarChannel = {
    id: "legacy",
    kind: "scalar",
    target: { kind: "element", element_id: "image" },
    binding: { kind: "property", parameter: "position_x_px" },
    extrapolation: { before: "hold", after: linear ? "linear" : "hold" },
    keys: [
      {
        id: "first",
        time: { ticks: "0", timebase: 1 },
        value: 0,
        left_handle: null,
        right_handle: null,
        segment_to_next: {
          kind: "legacy_normalized_bezier",
          curve: { x1: 0.2, y1: 0, x2: 0.8, y2: 1 },
        },
        tangent_mode: "broken",
      },
      {
        id: "last",
        time: { ticks: "2", timebase: 1 },
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
    tools: ["opencut_keyframe_legacy_conversion_plan"],
  } as never);
  const original = f.call.getMockImplementation()!,
    plan = vi.fn(async (request: unknown) => {
      const r = request as Record<string, unknown>;
      return {
        plan: {
          project_id: r.project_id,
          expected_revision: r.expected_revision,
          idempotency_key: r.idempotency_key,
          label: r.label,
          operations: [
            {
              type: "element_put",
              sequence_id: state.scene!.sequenceId,
              track_id: state.scene!.head.project.sequences[0].tracks[0].id,
              element,
            },
          ],
        },
        profile: "opencut-legacy-decimal12-lookup32-linear-v1",
        converted_segments: 1,
        inserted_keys: 31,
        max_absolute_error: 1e-11,
        error_bound: 1e-10,
      };
    });
  f.call.mockImplementation(async (name, ...args) =>
    name === "opencut_keyframe_legacy_conversion_plan"
      ? ((await plan(args[0])) as never)
      : original(name, ...args),
  );
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
      .spyOn(f.controller.scene, "applyPlanned")
      .mockResolvedValue({} as never),
    hold = vi.fn();
  const component = () => (
    <LegacyKeyframeConversion
      channel={channel}
      owner={{ kind: "element", element_id: "image" }}
      disabled={false}
      isBlocked={() => false}
      onChooseHold={hold}
    />
  );
  context.value = { ...state, controller: f.controller };
  const view = render(component());
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Review conversion" }),
    ).toHaveProperty("disabled", linear),
  );
  return {
    f,
    state,
    channel,
    plan,
    apply,
    hold,
    refresh() {
      context.value = { ...state, controller: f.controller };
      view.rerender(component());
    },
  };
}
it("requires a separate error review and explicit atomic apply", async () => {
  const s = await setup();
  try {
    fireEvent.click(screen.getByRole("button", { name: "Review conversion" }));
    await screen.findByRole("button", { name: "Apply reviewed conversion" });
    expect(s.apply).not.toHaveBeenCalled();
    expect(s.plan.mock.calls[0][0]).toMatchObject({
      channel_id: "legacy",
      left_key_ids: ["first"],
      maximum_absolute_error: 0.000001,
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Apply reviewed conversion" }),
    );
    await waitFor(() => expect(s.apply).toHaveBeenCalledTimes(1));
    expect(s.apply.mock.calls[0][0].idempotency_key).toBe(
      (s.plan.mock.calls[0][0] as { idempotency_key: string }).idempotency_key,
    );
  } finally {
    s.f.controller.dispose();
  }
});
it("changing tolerance or fixed revision discards the review before save", async () => {
  const s = await setup();
  try {
    fireEvent.click(screen.getByRole("button", { name: "Review conversion" }));
    await screen.findByRole("button", { name: "Apply reviewed conversion" });
    fireEvent.change(
      screen.getByLabelText("Maximum absolute error (property units)"),
      { target: { value: "0.001" } },
    );
    expect(
      screen.queryByRole("button", { name: "Apply reviewed conversion" }),
    ).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Review conversion" }));
    await screen.findByRole("button", { name: "Apply reviewed conversion" });
    s.state.scene!.head.revision = "other-head";
    s.refresh();
    expect(
      screen.queryByRole("button", { name: "Apply reviewed conversion" }),
    ).toBeNull();
    expect(s.apply).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});
it("rejects a host error bound above the caller tolerance", async () => {
  const s = await setup();
  try {
    const implementation = s.plan.getMockImplementation()!;
    s.plan.mockImplementationOnce(async (request) => ({
      ...(await implementation(request)),
      error_bound: 0.01,
    }));
    fireEvent.click(screen.getByRole("button", { name: "Review conversion" }));
    await screen.findByRole("alert");
    expect(
      screen.queryByRole("button", { name: "Apply reviewed conversion" }),
    ).toBeNull();
    expect(s.apply).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});
it("offers an explicit Hold transaction rather than silently changing extrapolation", async () => {
  const s = await setup(true);
  try {
    fireEvent.click(
      screen.getByRole("button", { name: "Set extrapolation to Hold" }),
    );
    expect(s.hold).toHaveBeenCalledWith(
      expect.objectContaining({
        extrapolation: { before: "hold", after: "hold" },
      }),
      { kind: "element", element_id: "image" },
    );
    expect(s.plan).not.toHaveBeenCalled();
    expect(s.apply).not.toHaveBeenCalled();
  } finally {
    s.f.controller.dispose();
  }
});

it("lets the user choose the outgoing segment and disables an empty batch", async () => {
  const s = await setup();
  try {
    fireEvent.click(screen.getByRole("checkbox"));
    expect(
      screen.getByRole("button", { name: "Review conversion" }),
    ).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Review conversion" }));
    await screen.findByRole("button", { name: "Apply reviewed conversion" });
    expect(s.plan.mock.calls[0][0]).toMatchObject({ left_key_ids: ["first"] });
  } finally {
    s.f.controller.dispose();
  }
});
