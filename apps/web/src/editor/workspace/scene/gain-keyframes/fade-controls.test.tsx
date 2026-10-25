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
import type { ToolInput } from "../../../../lib/editor-sdk";
import { GainFadeControls } from "./fade-controls";

const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("../../context", () => ({ useEditingWorkspace: () => context.value }));
beforeEach(() => preferences({ language: "en" }));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup(
  options: {
    delay?: () => Promise<void>;
    corrupt?: boolean;
    capabilityFailure?: Promise<never>;
  } = {},
) {
  const f = await setupAudioGain();
  const state = {
    ...f.controller.getSnapshot(),
    scene: structuredClone(f.controller.getSnapshot().scene),
  };
  const element = state.scene!.head.project.sequences[0].tracks[0].elements[0];
  state.scene!.selectedId = element.id;
  const calls: ToolInput<"opencut_keyframe_plan">[] = [];
  vi.spyOn(f.controller, "getSnapshot").mockImplementation(() => state);
  const apply = vi
    .spyOn(f.controller.scene, "applyPlanned")
    .mockResolvedValue({} as never);
  const report = vi.spyOn(f.controller, "report").mockImplementation(() => {});
  if (options.capabilityFailure)
    vi.mocked(f.api.sdk.hostCapabilities).mockImplementation(
      () => options.capabilityFailure!,
    );
  else
    vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
      tools: ["opencut_keyframe_plan"],
    } as never);
  const original = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, args, opts) => {
    if (name !== "opencut_keyframe_plan") return original(name, args, opts);
    const request = args as ToolInput<"opencut_keyframe_plan">;
    calls.push(request);
    await options.delay?.();
    const edit = request.edits[0];
    if (edit.kind !== "channel_put") throw Error("channel input required");
    const next = structuredClone(element);
    next.channels = [
      ...next.channels.filter((c) => c.id !== edit.channel.id),
      edit.channel,
    ];
    if (options.corrupt) next.enabled = !next.enabled;
    return {
      project_id: request.project_id,
      expected_revision: request.expected_revision,
      idempotency_key: request.idempotency_key,
      label: request.label,
      operations: [
        {
          type: "element_put",
          sequence_id: request.sequence_id,
          track_id: state.scene!.head.project.sequences[0].tracks[0].id,
          element: next,
        },
      ],
    } as never;
  });
  const refresh = () => {
    context.value = { ...state, controller: f.controller };
  };
  refresh();
  const view = render(<GainFadeControls elementId={element.id} />);
  if (!options.capabilityFailure)
    await waitFor(() =>
      expect(
        (screen.getByLabelText("Fade in (seconds)") as HTMLInputElement)
          .disabled,
      ).toBe(false),
    );
  const rerender = () => {
    refresh();
    view.rerender(<GainFadeControls elementId={element.id} />);
  };
  return { ...f, state, element, calls, apply, report, view, rerender };
}
it("saves only the confirmed field with one shared plan and original gain/other authored properties", async () => {
  const f = await setup(),
    before = structuredClone(f.element);
  fireEvent.change(screen.getByLabelText("Fade in (seconds)"), {
    target: { value: "0.5" },
  });
  fireEvent.change(screen.getByLabelText("Fade out (seconds)"), {
    target: { value: "2" },
  });
  fireEvent.keyDown(screen.getByLabelText("Fade in (seconds)"), {
    key: "Enter",
  });
  await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(1));
  expect(f.calls).toHaveLength(1);
  const c = f.calls[0].edits[0];
  expect(c.kind).toBe("channel_put");
  if (c.kind !== "channel_put" || c.channel.kind !== "scalar") throw Error();
  expect(c.channel.keys.at(-1)!.value).toBe(-6); // Draft fade-out was not submitted.
  expect(f.element).toEqual(before);
});
it("Escape restores only that draft; unchanged Enter never plans", async () => {
  const f = await setup(),
    input = screen.getByLabelText("Fade in (seconds)");
  fireEvent.change(input, { target: { value: "0.5" } });
  fireEvent.keyDown(input, { key: "Escape" });
  expect((input as HTMLInputElement).value).toBe("0");
  fireEvent.keyDown(input, { key: "Enter" });
  await act(async () => {});
  expect(f.calls).toHaveLength(0);
  expect(f.state.scene!.selectedId).toBe(f.element.id);
});
it("keeps the other unconfirmed fade across a saved revision and restores focus", async () => {
  const f = await setup();
  f.apply.mockImplementation(async request => {
    const op = request.operations[0];
    if (op.type !== "element_put") throw Error();
    f.state.scene!.head.project.sequences[0].tracks[0].elements[0] = structuredClone(op.element);
    f.state.scene!.head.revision = "saved-fade";
    f.rerender();
    return { revision: "saved-fade" } as never;
  });
  const input = screen.getByLabelText("Fade in (seconds)");
  fireEvent.change(input, { target: { value: "0.5" } });
  fireEvent.change(screen.getByLabelText("Fade out (seconds)"), { target: { value: "2" } });
  input.focus();
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  await waitFor(() => expect((screen.getByLabelText("Fade out (seconds)") as HTMLInputElement).value).toBe("2"));
  await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Fade in (seconds)")));
  const saved = f.state.scene!.head.project.sequences[0].tracks[0].elements[0];
  expect(saved.channels[0].kind === "scalar" && saved.channels[0].keys.at(-1)!.value).toBe(-6);
});
it("rejects overlap before making any native request", async () => {
  const f = await setup(),
    input = screen.getByLabelText("Fade in (seconds)");
  fireEvent.change(input, { target: { value: "999" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Fade in and fade out must fit inside the clip.",
  );
  expect(input.getAttribute("aria-invalid")).toBe("true");
  expect(input.getAttribute("aria-describedby")).toBe(
    screen.getByRole("alert").id,
  );
  expect(f.calls).toHaveLength(0);
  expect(f.apply).not.toHaveBeenCalled();
});
it.each(["selection", "revision", "connection", "lock", "playback"])(
  "a delayed plan cannot save after %s changes",
  async (kind) => {
    let done!: () => void;
    const delay = new Promise<void>((r) => {
      done = r;
    });
    const f = await setup({ delay: () => delay }),
      input = screen.getByLabelText("Fade in (seconds)");
    fireEvent.change(input, { target: { value: "0.5" } });
    fireEvent.keyDown(input, { key: "Enter" });
    if (kind === "selection") f.state.scene!.selectedId = "other";
    if (kind === "revision") f.state.scene!.head.revision = "other";
    if (kind === "connection") f.state.api = null;
    if (kind === "lock")
      f.state.scene!.head.project.sequences[0].tracks[0].locked = true;
    if (kind === "playback") f.state.scene!.audioPreview.phase = "playing";
    await act(async () => {
      done();
      await delay;
    });
    expect(f.apply).not.toHaveBeenCalled();
  },
);
it("rejects a plan that also changes an unrelated element property", async () => {
  const f = await setup({ corrupt: true }),
    input = screen.getByLabelText("Fade in (seconds)");
  fireEvent.change(input, { target: { value: "0.5" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByRole("alert");
  expect(f.apply).not.toHaveBeenCalled();
});
it("deduplicates repeated confirmation while the original plan is pending", async () => {
  let done!: () => void;
  const delay = new Promise<void>((r) => {
    done = r;
  });
  const f = await setup({ delay: () => delay }),
    input = screen.getByLabelText("Fade in (seconds)");
  fireEvent.change(input, { target: { value: "0.5" } });
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(f.calls).toHaveLength(1);
  await act(async () => {
    done();
    await delay;
  });
  expect(f.apply).toHaveBeenCalledTimes(1);
});

it("returns keyboard focus to the confirmed field after the new revision remounts", async () => {
  const f = await setup();
  f.apply.mockImplementation(async (tx) => {
    const op = tx.operations[0];
    if (op.type !== "element_put") throw Error();
    f.state.scene!.head.project.sequences[0].tracks[0].elements[0] =
      structuredClone(op.element);
    f.state.scene!.head.revision = "saved-fades";
    f.rerender();
    return { revision: "saved-fades" } as never;
  });
  const input = screen.getByLabelText("Fade in (seconds)");
  input.focus();
  fireEvent.change(input, { target: { value: "0.5" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() =>
    expect(document.activeElement).toBe(
      screen.getByLabelText("Fade in (seconds)"),
    ),
  );
  expect(
    (screen.getByLabelText("Fade in (seconds)") as HTMLInputElement).value,
  ).toBe("0.5");
});

it("discards an unmounted component's delayed plan rather than applying it", async () => {
  let done!: () => void;
  const delay = new Promise<void>((r) => {
    done = r;
  });
  const f = await setup({ delay: () => delay }),
    input = screen.getByLabelText("Fade in (seconds)");
  fireEvent.change(input, { target: { value: "0.5" } });
  fireEvent.keyDown(input, { key: "Enter" });
  f.view.unmount();
  await act(async () => {
    done();
    await delay;
  });
  expect(f.apply).not.toHaveBeenCalled();
});

it.each([true, false])(
  "routes an initial authentication failure only to its original connection (current=%s)",
  async (current) => {
    let reject!: (e: Error) => void;
    const failure = new Promise<never>((_, no) => {
      reject = no;
    });
    const f = await setup({ capabilityFailure: failure });
    if (!current) f.state.api = null;
    await act(async () => {
      reject(new Error("A valid bearer token is required"));
    });
    expect(f.report).toHaveBeenCalledTimes(current ? 1 : 0);
    expect(f.apply).not.toHaveBeenCalled();
  },
);
