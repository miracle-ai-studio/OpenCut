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
import { WorkspaceProvider } from "../../context";
import { setupAudioGain } from "../audio-gain-fixture";
import { preferences } from "../../../shared/locale";
import { fadeChannel, simpleFades } from "../gain-keyframes/fades";
import { VolumeControls } from "./volume-controls";
import type { ToolInput } from "../../../../lib/editor-sdk";
import type { WorkspaceController } from "../../controller";

const controllers: WorkspaceController[] = [];
beforeEach(() => {
  preferences({ language: "en" });
  Object.defineProperty(window, "PointerEvent", {
    configurable: true,
    value: class extends MouseEvent {
      pointerId: number;
      constructor(type: string, init: PointerEventInit) {
        super(type, init);
        this.pointerId = init.pointerId ?? 1;
      }
    },
  });
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => true);
});
afterEach(() => {
  cleanup();
  controllers.splice(0).forEach((controller) => controller.dispose());
  vi.restoreAllMocks();
});

async function setup(
  options: {
    multi?: boolean;
    animated?: boolean;
    legacy?: boolean;
    corrupt?: boolean;
    delay?: Promise<void>;
    realSave?: boolean;
    capabilities?: Promise<readonly string[]>;
  } = {},
) {
  const f = await setupAudioGain();
  controllers.push(f.controller);
  const state = f.controller.getSnapshot().scene!,
    seq = state.head.project.sequences[0],
    track = seq.tracks[0],
    first = track.elements[0];
  if (first.content.kind !== "video") throw Error();
  first.channels = [];
  first.content.audio.gain = options.legacy
    ? { unit: "linear_legacy", value: 1.6921544822993657 }
    : { unit: "decibels", value: -6 };
  if (options.animated)
    first.channels = [fadeChannel(first, simpleFades(first)!, "fadeIn", "1")!];
  const ids = [first.id],
    selection = [{ track_id: track.id, element_id: first.id }];
  if (options.multi) {
    const second = structuredClone(first);
    second.id = "second-video";
    second.name = "Second clip";
    if (second.content.kind !== "video") throw Error();
    second.channels = [];
    second.content.audio.gain = { unit: "decibels", value: 0 };
    second.content.audio.muted = true;
    seq.tracks.push({
      ...structuredClone(track),
      id: "second-track",
      elements: [second],
      muted: true,
    });
    ids.push(second.id);
    selection.push({ track_id: "second-track", element_id: second.id });
  }
  f.controller.scene.timelineEdit.selectMany(selection);
  if (options.capabilities)
    vi.mocked(f.api.sdk.hostCapabilities).mockImplementation(async () => ({ tools: await options.capabilities! }) as never);
  else vi.mocked(f.api.sdk.hostCapabilities).mockResolvedValue({
    tools: ["opencut_element_properties_plan", "opencut_keyframe_plan"],
  } as never);
  const original = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  const plans: { name: string; request: unknown }[] = [];
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, args, opts) => {
    if (
      name !== "opencut_element_properties_plan" &&
      name !== "opencut_keyframe_plan"
    )
      return original(name, args, opts);
    plans.push({ name, request: structuredClone(args) });
    const request = args as ToolInput<"opencut_element_properties_plan">;
    const keyframe =
      name === "opencut_keyframe_plan"
        ? (args as ToolInput<"opencut_keyframe_plan">)
        : null;
    const edit = keyframe?.edits[0];
    const targetId =
      edit?.kind === "channel_put" && edit.owner.kind === "element"
        ? edit.owner.element_id
        : request.element_id;
    const owner = seq.tracks.find((track) =>
      track.elements.some((e) => e.id === targetId),
    )!;
    const element = structuredClone(
      owner.elements.find((e) => e.id === targetId)!,
    );
    if (element.content.kind !== "video") throw Error();
    if (edit?.kind === "channel_put")
      element.channels = [
        ...element.channels.filter((c) => c.id !== edit.channel.id),
        edit.channel,
      ];
    else {
      if (request.audio_gain) element.content.audio.gain = request.audio_gain;
      if (request.muted !== null) element.content.audio.muted = request.muted!;
    }
    if (options.corrupt && plans.length === (options.multi ? 2 : 1))
      element.enabled = !element.enabled;
    await options.delay;
    return {
      project_id: request.project_id,
      expected_revision: request.expected_revision,
      idempotency_key: request.idempotency_key,
      label: request.label,
      operations: [
        {
          type: "element_put",
          sequence_id: request.sequence_id,
          track_id: owner.id,
          element,
        },
      ],
    } as never;
  });
  const apply = vi.spyOn(f.controller.scene, "applyPlanned");
  if (!options.realSave) apply.mockResolvedValue({ revision: "new-revision" } as never);
  const view = render(
    <WorkspaceProvider controller={f.controller}>
      <VolumeControls elementIds={ids} />
    </WorkspaceProvider>,
  );
  if (!options.capabilities) await waitFor(() =>
    expect(
      (screen.getByLabelText("Volume (percent)") as HTMLInputElement).disabled,
    ).toBe(false),
  );
  return { ...f, state, seq, first, ids, plans, apply, view };
}

it("shows capability loading instead of a changed-target error while the mixed form initializes", async () => {
  let resolve!: (tools: readonly string[]) => void;
  const capabilities = new Promise<readonly string[]>(done => { resolve = done; });
  const f = await setup({ multi: true, capabilities });
  expect(screen.getByRole("status").textContent).toBe("Checking volume editing availability…");
  expect(screen.queryByText("The audio target changed. Select the clip again.")).toBeNull();
  expect((screen.getByLabelText("Volume (percent)") as HTMLInputElement).disabled).toBe(true);
  await act(async () => { resolve(["opencut_element_properties_plan", "opencut_keyframe_plan"]); });
  await waitFor(() => expect((screen.getByLabelText("Volume (percent)") as HTMLInputElement).disabled).toBe(false));
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByText("The audio target changed. Select the clip again.")).toBeNull();
  expect(screen.getByRole("slider").getAttribute("aria-valuenow")).toBeNull();
  expect(f.plans).toHaveLength(0);
});

it("ends capability loading on failure without enabling save or claiming the target changed", async () => {
  let reject!: (error: Error) => void;
  const capabilities = new Promise<readonly string[]>((_resolve, fail) => { reject = fail; });
  const f = await setup({ capabilities });
  await act(async () => { reject(Error("Capabilities unavailable")); });
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByText("The audio target changed. Select the clip again.")).toBeNull();
  expect((screen.getByLabelText("Volume (percent)") as HTMLInputElement).disabled).toBe(true);
  expect(f.plans).toHaveLength(0);
});

it.each(["percent", "db", "muted"])("restores %s focus after the real controller refreshes its saved revision", async field => {
  const f = await setup({ realSave: true });
  const input = field === "muted" ? screen.getByLabelText("Mute selected clips") : screen.getByLabelText(field === "percent" ? "Volume (percent)" : "Volume (decibels)");
  input.focus();
  if (field === "muted") fireEvent.click(input);
  else {
    fireEvent.change(input, { target: { value: field === "percent" ? "200" : "0" } });
    fireEvent.keyDown(input, { key: "Enter" });
  }
  await waitFor(() => expect(f.controller.getSnapshot().scene!.head.revision).not.toBe("r0"));
  await waitFor(() => expect(document.activeElement?.getAttribute("data-volume-field")).toBe(field));
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]);
  expect(f.controller.getSnapshot().scene!.recovery?.record).toBeNull();
  expect(f.apply).toHaveBeenCalledOnce();
});

it("shows mixed volume and mute without treating either as zero, then combines all independently checked plans into one atomic apply", async () => {
  const f = await setup({ multi: true, legacy: true, animated: true });
  const before = structuredClone(f.seq.tracks.map((track) => track.elements));
  const input = screen.getByLabelText("Volume (percent)") as HTMLInputElement;
  expect(input.value).toBe("");
  expect(input.placeholder).toBe("—");
  expect(screen.getByRole("slider").getAttribute("aria-valuenow")).toBeNull();
  expect(
    (screen.getByLabelText("Mute selected clips") as HTMLInputElement)
      .indeterminate,
  ).toBe(true);
  fireEvent.change(input, { target: { value: "200" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  expect(f.plans.map((plan) => plan.name)).toEqual([
    "opencut_keyframe_plan",
    "opencut_element_properties_plan",
  ]);
  const [transaction, sequence, owner] = f.apply.mock.calls[0];
  expect(sequence).toBe(f.state.sequenceId);
  expect(owner).toBeTruthy();
  expect(transaction.operations).toHaveLength(2);
  const one = transaction.operations[0],
    two = transaction.operations[1];
  if (
    one.type !== "element_put" ||
    two.type !== "element_put" ||
    one.element.content.kind !== "video" ||
    two.element.content.kind !== "video"
  )
    throw Error();
  expect(one.element.content).toEqual(before[0][0].content);
  expect(simpleFades(one.element)!.channel!.keys.map((k) => k.value)).toEqual([
    0, 2, 2,
  ]);
  expect(two.element.content.audio.gain).toEqual({
    unit: "decibels",
    value: 20 * Math.log10(2),
  });
  expect(two.element.content.audio.muted).toBe(true);
  expect(f.seq.tracks[1].muted).toBe(true);
});
it("ignores an unchanged rounded display and cancels typed drafts without planning or losing a precision bit", async () => {
  const f = await setup({ legacy: true });
  const input = screen.getByLabelText("Volume (percent)") as HTMLInputElement;
  expect(input.value).toBe("169.215");
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.change(input, { target: { value: "200" } });
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  fireEvent.keyDown(input, { key: "Escape" });
  expect(input.value).toBe("169.215");
  expect(f.plans).toHaveLength(0);
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(0);
  expect(
    f.first.content.kind === "video" && f.first.content.audio.gain.value,
  ).toBe(1.6921544822993657);
});
it("refuses a zero fade plateau and lets cancel release the draft, leaving the original fade editable", async () => {
  const f = await setup({ legacy: true, animated: true });
  const input = screen.getByLabelText("Volume (percent)");
  fireEvent.change(input, { target: { value: "0" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect((await screen.findByRole("alert")).textContent).toContain("mute");
  expect(f.plans).toHaveLength(0);
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Cancel volume draft" }));
  expect((input as HTMLInputElement).disabled).toBe(false);
  expect(simpleFades(f.first)).not.toBeNull();
});
it("performs no partial write when a later target's plan changes another authored property", async () => {
  const f = await setup({ multi: true, corrupt: true });
  const input = screen.getByLabelText("Volume (percent)");
  fireEvent.change(input, { target: { value: "150" } });
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByRole("alert");
  expect(screen.getByText("The volume plan differs from the captured clips and original curves.")).toBeTruthy();
  expect(f.plans).toHaveLength(2);
  expect(f.apply).not.toHaveBeenCalled();
});
it.each(["selection", "connection", "revision"])(
  "rejects a late plan after the full %s fence changes",
  async (field) => {
    let finish!: () => void;
    const delay = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const f = await setup({ multi: true, delay });
    const input = screen.getByLabelText("Volume (percent)");
    fireEvent.change(input, { target: { value: "150" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(f.plans).toHaveLength(1));
    if (field === "selection")
      f.controller.getSnapshot().scene!.timelineEdit.selection = f.controller.getSnapshot().scene!.timelineEdit.selection.slice(1);
    if (field === "revision") f.controller.getSnapshot().scene!.externalRevision = "elsewhere";
    if (field === "connection")
      vi.spyOn(f.api, "connectionIdentity", "get").mockReturnValue(
        "other-connection",
      );
    await act(async () => finish());
    await screen.findByRole("alert");
    expect(f.apply).not.toHaveBeenCalled();
    expect(f.plans).toHaveLength(1);
  },
);
it("previews slider values through multiple moves but submits only on release, and Esc aborts a later gesture", async () => {
  const f = await setup();
  const slider = screen.getByRole("slider");
  vi.spyOn(slider, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: 200,
    bottom: 28,
    width: 200,
    height: 28,
    toJSON() {},
  });
  fireEvent.pointerDown(slider, { button: 0, pointerId: 1, clientX: 10 });
  fireEvent.pointerMove(slider, { pointerId: 1, clientX: 20 });
  fireEvent.pointerMove(slider, { pointerId: 1, clientX: 30 });
  expect(f.plans).toHaveLength(0);
  expect(slider.getAttribute("aria-valuenow")).toBe("150");
  fireEvent.pointerUp(slider, { pointerId: 1, clientX: 30 });
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  fireEvent.pointerDown(slider, { button: 0, pointerId: 2, clientX: 50 });
  fireEvent.keyDown(slider, { key: "Escape" });
  fireEvent.pointerUp(slider, { pointerId: 2, clientX: 50 });
  expect(f.apply).toHaveBeenCalledOnce();
});
