// @vitest-environment jsdom
import { useState, useSyncExternalStore } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { TimelineEditControls } from "./controls";
import { TimelineEditReview } from "./review";
import { setupTimelineEdit } from "./timeline-fixture";

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const target = { track_id: "track", element_id: "image" };
function ProductHarness({ fixture: f }: { fixture: Awaited<ReturnType<typeof setupTimelineEdit>> }) {
  const state = useSyncExternalStore(f.subscribe, f.state);
  const [review, setReview] = useState(false);
  return <>
    <TimelineEditControls controller={f.edit} sequence={state.head.project.sequences[0]}
      scope={{ projectId: state.head.project.id, revision: state.head.revision, sequenceId: state.sequenceId }}
      playhead={state.playhead} selection={state.timelineEdit.selection} attempt={state.timelineEdit.attempt}
      selectedTrackId="track" disabled={f.busy() || !!state.historical || !!state.timelineEdit.attempt}
      onReview={() => setReview(true)} />
    {review && state.timelineEdit.attempt && <TimelineEditReview controller={f.edit}
      sequence={state.head.project.sequences[0]} attempt={state.timelineEdit.attempt}
      disabled={f.busy() || !!state.historical} onClose={() => setReview(false)} />}
  </>;
}

it("cancel writes nothing; untouched rational move is reviewed, retained on close and submitted once", async () => {
  const f = await setupTimelineEdit();
  try {
    const head = structuredClone(f.state().head);
    head.project.sequences[0].tracks[0].elements[0].timeline_range.start = { ticks: "1", timebase: 3 };
    f.set({ head }); f.edit.select(target);
    render(<ProductHarness fixture={f} />);
    fireEvent.click(screen.getByRole("button", { name: "Move selection" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Move selection" }));
    fireEvent.click(screen.getByRole("button", { name: "Review timeline changes" }));
    const review = await screen.findByRole("dialog", { name: "Review timeline changes" });
    expect(f.planner.mock.calls[0][1].command).toEqual({ kind: "move_selection", moves: [{
      target, target_track_id: "track", new_start: { ticks: "1", timebase: 3 },
    }], ripple: false });
    expect(f.applyCalls()).toHaveLength(0);
    fireEvent.click(within(review).getAllByRole("button", { name: "Close" })[0]);
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Review timeline changes" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply timeline changes" }));
    await screen.findByText("Timeline changes saved.");
    expect(f.applyCalls()).toHaveLength(1);
    expect(f.applyCalls()[0][1]).toBe(f.state().timelineEdit.attempt!.plan!.transaction);
  } finally { f.controller.dispose(); }
});

it("split captures unique right IDs once; rejected animation keeps the exact request available for retry", async () => {
  const f = await setupTimelineEdit();
  try {
    const head = structuredClone(f.state().head), second = structuredClone(head.project.sequences[0].tracks[0].elements[0]);
    second.id = "image-two"; second.name = "Second image";
    head.project.sequences[0].tracks[0].elements.push(second);
    f.set({ head, playhead: { ticks: "1", timebase: 3 } });
    f.edit.select(target); f.edit.select({ track_id: "track", element_id: second.id }, "extend");
    f.planner.mockRejectedValueOnce(new Error("Animated split is not supported"));
    render(<ProductHarness fixture={f} />);
    fireEvent.click(screen.getByRole("button", { name: "Split selection" }));
    const form = screen.getByRole("dialog", { name: "Split selection" });
    fireEvent.change(within(form).getByLabelText("Keep after split"), { target: { value: "right" } });
    fireEvent.click(within(form).getByRole("checkbox", { name: "Ripple affected tracks" }));
    fireEvent.click(within(form).getByRole("button", { name: "Review timeline changes" }));
    await within(form).findByText("Animated split is not supported");
    const original = f.planner.mock.calls[0][1], command = original.command;
    expect(command.kind).toBe("split");
    if (command.kind !== "split") throw new Error("Expected split fixture request");
    expect(command).toMatchObject({ at: { ticks: "1", timebase: 3 }, retain: "right", ripple: true });
    expect(new Set(command.targets.map(item => item.right_element_id)).size).toBe(2);
    expect(command.targets.map(item => item.target)).toEqual([target, { track_id: "track", element_id: second.id }]);
    expect(f.applyCalls()).toHaveLength(0);
    fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Review timeline changes" }));
    fireEvent.click(screen.getByRole("button", { name: "Retry timeline plan" }));
    await waitFor(() => expect(f.planner).toHaveBeenCalledTimes(2));
    expect(f.planner.mock.calls[1][1]).toBe(original);
    expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("head changes disable captured trim forms; empty-track controls preserve tagged track_kind and main-track guard", async () => {
  const f = await setupTimelineEdit();
  try {
    f.edit.select(target); render(<ProductHarness fixture={f} />);
    fireEvent.click(screen.getByRole("button", { name: "Trim element" }));
    const form = screen.getByRole("dialog", { name: "Trim element" });
    fireEvent.change(within(form).getByLabelText("Trim edge"), { target: { value: "right" } });
    act(() => f.set({ head: { ...f.state().head, revision: "other" } }));
    expect((within(form).getByRole("button", { name: "Review timeline changes" }) as HTMLButtonElement).disabled).toBe(true);
    expect(within(form).getByText("The timeline target changed. Close this form and review a new operation.")).toBeTruthy();
    expect(f.planner).not.toHaveBeenCalled();
    fireEvent.click(within(form).getByRole("button", { name: "Cancel" }));
    expect((screen.getByRole("button", { name: "Delete empty track" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Add track" }));
    const add = screen.getByRole("dialog", { name: "Add track" });
    fireEvent.change(within(add).getByLabelText("Track name"), { target: { value: "Audio lane" } });
    fireEvent.change(within(add).getByLabelText("Track type"), { target: { value: "audio" } });
    fireEvent.click(within(add).getByRole("button", { name: "Review timeline changes" }));
    await screen.findByRole("dialog", { name: "Review timeline changes" });
    expect(f.planner.mock.calls[0][1].command).toMatchObject({ kind: "add_track", name: "Audio lane", track_kind: "audio" });
    expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("text selection exposes formal move, trim, split, delete and only text destinations", async () => {
  const { textTimelineSequence } = await import("./text-fixture");
  const f = await setupTimelineEdit();
  try {
    const head = structuredClone(f.state().head); head.project.sequences[0] = textTimelineSequence(); f.set({head});
    f.edit.select({track_id:"titles",element_id:"title"});
    render(<ProductHarness fixture={f} />);
    for (const name of ["Move selection","Trim element","Split selection","Delete selection"]) expect(screen.getByRole("button",{name}).hasAttribute("disabled")).toBe(false);
    fireEvent.click(screen.getByRole("button",{name:"Move selection"}));
    const destination = screen.getByRole("combobox",{name:"Target track for Independent title"});
    expect(within(destination).getAllByRole("option").map(option=>option.getAttribute("value"))).toEqual(["titles","titles-above"]);
    fireEvent.click(screen.getByRole("button",{name:"Review timeline changes"}));
    await screen.findByRole("dialog",{name:"Review timeline changes"});
    expect(f.planner.mock.calls[0][3]).toBe("opencut_edit_element_timeline_plan_v4");
    expect(f.applyCalls()).toHaveLength(0);
  } finally {f.controller.dispose();}
});
