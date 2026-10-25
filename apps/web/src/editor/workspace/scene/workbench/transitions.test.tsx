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
import { WorkspaceProvider, useEditingWorkspace } from "../../context";
import { initialSnapshot, setup } from "../fixture";
import { TransitionResources, TransitionInspector } from "./transitions";
import { TransitionProvider } from "./transition-workspace";
import { TransitionSeams } from "./transition-seams";
import { transitionPairs, transitionLimit } from "./transition-target";
import { preferences } from "../../../shared/locale";

beforeEach(() => {
  preferences({ language: "en" });
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function adjacentSnapshot() {
  const snapshot = initialSnapshot(),
    track = snapshot.project.sequences[0].tracks[0];
  snapshot.project.assets[0].kind = "video";
  snapshot.project.assets[0].duration = { ticks: "12", timebase: 1 };
  const first = track.elements[0];
  first.name = "First";
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
    trim_start: { ticks: "1", timebase: 1 },
    trim_end: { ticks: "5", timebase: 1 },
    source_duration: { ticks: "12", timebase: 1 },
  };
  first.timeline_range.duration = { ticks: "6", timebase: 1 };
  const second = structuredClone(first);
  second.id = "second";
  second.name = "Second";
  second.timeline_range.start = { ticks: "12", timebase: 2 };
  // Reverse storage order; exact adjacency cannot depend on storage order.
  track.elements.unshift(second);
  return snapshot;
}
function Contents() {
  const { scene } = useEditingWorkspace();
  if (!scene) return null;
  const sequence = (scene!.historical ?? scene!.head).project.sequences[0];
  return (
    <>
      <TransitionResources />
      <TransitionSeams sequence={sequence} trackId="track" scale={40} />
      <TransitionInspector />
    </>
  );
}
async function mount(snapshot = adjacentSnapshot()) {
  const f = await setup(snapshot);
  const apply = vi
    .spyOn(f.controller.scene, "applyPlanned")
    .mockResolvedValue({
      base_revision: "r0",
      revision: "r1",
      transaction_id: "tx",
      changed_ids: [],
      created_ids: [],
      deleted_ids: [],
    });
  render(
    <WorkspaceProvider controller={f.controller}>
      <TransitionProvider>
        <Contents />
      </TransitionProvider>
    </WorkspaceProvider>,
  );
  return { ...f, apply };
}
const seam = () =>
  screen.getByRole("button", { name: "Clip cut · First → Second" });
const input = () =>
  screen.getByLabelText("Duration (seconds)") as HTMLInputElement;
const add = () => screen.getByRole("button", { name: "Add transition" });

it("never chooses the first pair implicitly; preview and Escape keep the document and timeline selection unchanged", async () => {
  const f = await mount();
  try {
    await act(() => f.controller.scene.select("image"));
    expect(
      (
        screen.getByRole("button", {
          name: "Add transition · Smooth · 0.5 s",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.queryByLabelText("Duration (seconds)")).toBeNull();
    fireEvent.click(
      screen.getByRole("button", {
        name: "Preview transition · Smooth · 0.5 s",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Play example" }));
    fireEvent(
      screen.getByRole("dialog"),
      new Event("cancel", { bubbles: false, cancelable: true }),
    );
    expect(f.controller.getSnapshot().scene?.selectedId).toBe("image");
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_plan"),
    ).toHaveLength(0);
    expect(f.apply).not.toHaveBeenCalled();
  } finally {
    f.controller.dispose();
  }
});

it("applies only to the explicitly selected cut through one native plan and the unchanged fixed request", async () => {
  const f = await mount();
  try {
    fireEvent.click(seam());
    expect(seam().style.left).toBe("240px");
    fireEvent.change(input(), { target: { value: "0.125" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    fireEvent.click(add());
    await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
    const planned = f.call.mock.calls.filter(
      ([name]) => name === "opencut_edit_plan",
    );
    expect(planned).toHaveLength(1);
    expect(f.apply.mock.calls[0]).toEqual([planned[0][1], "scene"]);
    expect(planned[0][1]).toMatchObject({
      expected_revision: "r0",
      operations: [
        {
          type: "transition_put",
          transition: {
            outgoing_element_id: "image",
            incoming_element_id: "second",
            duration: { ticks: "1", timebase: 8 },
          },
        },
      ],
    });
  } finally {
    f.controller.dispose();
  }
});

it("replaces the original ID and passes untouched rational duration through; Escape cancels only the duration draft", async () => {
  const snapshot = adjacentSnapshot();
  snapshot.project.sequences[0].transitions.push({
    id: "original",
    outgoing_element_id: "image",
    incoming_element_id: "second",
    duration: { ticks: "1", timebase: 3 },
    transition: "legacy_crossfade_v3",
  });
  const f = await mount(snapshot);
  try {
    fireEvent.click(seam());
    const original = input().value;
    fireEvent.change(input(), { target: { value: "9" } });
    fireEvent.keyDown(input(), { key: "Escape" });
    expect(input().value).toBe(original);
    expect(seam().getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Replace transition" }));
    await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
    expect(f.apply.mock.calls[0][0].operations[0]).toMatchObject({
      transition: { id: "original", duration: { ticks: "1", timebase: 3 } },
    });
  } finally {
    f.controller.dispose();
  }
});

it.each([
  "locked",
  "gap",
  "historical",
  "image",
  "no-handles",
  "disabled",
] as const)("rejects a %s target with no plan or write", async (mode) => {
  const snapshot = adjacentSnapshot(),
    track = snapshot.project.sequences[0].tracks[0];
  if (mode === "locked") track.locked = true;
  if (mode === "gap")
    track.elements[0].timeline_range.start = { ticks: "7", timebase: 1 };
  if (mode === "image")
    track.elements[0].content = { kind: "image", asset_id: "asset" };
  if (
    mode === "no-handles" &&
    track.elements[0].source_span.profile === "classic_trimmed"
  )
    track.elements[0].source_span.trim_start = { ticks: "0", timebase: 1 };
  if (mode === "disabled") track.enabled = false;
  const f = await mount(snapshot);
  try {
    if (mode === "historical")
      await act(() => f.controller.scene.inspectRevision("r0"));
    if (mode !== "gap") fireEvent.click(seam());
    const button = screen.getByRole("button", {
      name: "Add transition · Smooth · 0.5 s",
    }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(f.apply).not.toHaveBeenCalled();
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_plan"),
    ).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});

it("bounds duration by source handles, not just clip duration, and rejects invalid drafts", async () => {
  const snapshot = adjacentSnapshot();
  expect(
    transitionLimit(
      transitionPairs(snapshot.project.sequences[0])[0],
      snapshot.project.assets,
    ).maximum,
  ).toEqual({ ticks: "2", timebase: 1 });
  const f = await mount(snapshot);
  try {
    fireEvent.click(seam());
    for (const value of ["2.1", "0", "-1", "bad"]) {
      fireEvent.change(input(), { target: { value } });
      expect((add() as HTMLButtonElement).disabled).toBe(true);
    }
    fireEvent.change(input(), { target: { value: "2" } });
    expect((add() as HTMLButtonElement).disabled).toBe(false);
    expect(f.apply).not.toHaveBeenCalled();
  } finally {
    f.controller.dispose();
  }
});

it("rejects a late plan after timeline selection changes", async () => {
  const f = await mount();
  try {
    const original = f.call.getMockImplementation()!;
    let resolve!: () => void;
    const waiting = new Promise<void>((done) => {
      resolve = done;
    });
    f.call.mockImplementation(async (name, request, options) => {
      if (name === "opencut_edit_plan") {
        await waiting;
        return { project: f.current().project } as never;
      }
      return original(name, request, options);
    });
    fireEvent.click(seam());
    fireEvent.click(add());
    await waitFor(() =>
      expect(
        f.call.mock.calls.some(([name]) => name === "opencut_edit_plan"),
      ).toBe(true),
    );
    await act(() => f.controller.scene.select("second"));
    await act(async () => {
      resolve();
      await waiting;
    });
    expect(f.apply).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Duration (seconds)")).toBeNull();
  } finally {
    f.controller.dispose();
  }
});

it("keeps native preflight failure read-only and visible", async () => {
  const f = await mount();
  try {
    const original = f.call.getMockImplementation()!;
    f.call.mockImplementation(async (name, request, options) => {
      if (name === "opencut_edit_plan")
        throw new Error("Source mapping unavailable");
      return original(name, request, options);
    });
    fireEvent.click(seam());
    fireEvent.click(add());
    await waitFor(() =>
      expect(screen.getByText("Source mapping unavailable")).toBeTruthy(),
    );
    expect(f.apply).not.toHaveBeenCalled();
  } finally {
    f.controller.dispose();
  }
});
