// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { setupTimelineEdit } from "./timeline-fixture";
import { V4TimelinePanel } from "./panel";
import type { TimelinePanelProps } from "./panel";

beforeEach(() => {
  Object.defineProperty(window, "DragEvent", { configurable: true, value: class extends MouseEvent {
    dataTransfer: DataTransfer | null;
    constructor(type: string, init: DragEventInit) { super(type, init); this.dataTransfer = init.dataTransfer ?? null; }
  } });
  Object.defineProperty(window, "PointerEvent", { configurable: true, value: class extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit) { super(type, init); this.pointerId = init.pointerId ?? 1; }
  } });
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => true);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const top = this.classList.contains("v4-timeline-tracks") ? 100 : 0;
    return { top, left: 0, right: 1000, bottom: 400, width: 1000, height: 400 - top, x: 0, y: top, toJSON() {} };
  });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

async function setup() {
  const f = await setupTimelineEdit();
  const props: TimelinePanelProps = {
    controller: f.edit, sequence: f.state().head.project.sequences[0],
    scope: { projectId: f.state().head.project.id, revision: f.state().head.revision, sequenceId: "scene" },
    selection: [{ track_id: "track", element_id: "image" }], selectedId: "image", playhead: { ticks: "1", timebase: 3 },
    disabled: false, gainDraftActive: false, onSelect: vi.fn(), onSelectMany: vi.fn(), onSelectOther: vi.fn(), onSeek: vi.fn(), onReview: vi.fn(),
    renderAudioControls: () => <div className="v4-audio-controls"><button role="slider" aria-label="Volume image">Gain</button></div>,
  };
  return { f, props };
}

it("file drops use the lane coordinate at the current zoom, including scroll, and reject locked or busy tracks", async () => {
  const { f, props } = await setup();
  try {
    props.onFiles = vi.fn(async () => {});
    const view = render(<V4TimelinePanel {...props} />);
    const lane = view.container.querySelector<HTMLElement>("[data-file-track=track]")!;
    const file = new File(["png"], "image.png", { type: "image/png" });
    const transfer = { files: [file], types: ["Files"], dropEffect: "none" };
    vi.spyOn(lane, "getBoundingClientRect").mockReturnValue({ left: -28, top: 100 } as DOMRect);
    fireEvent.change(screen.getByLabelText("Timeline zoom"), { target: { value: "2" } });
    fireEvent.drop(lane, { dataTransfer: transfer, clientX: 132 });
    expect(props.onFiles).toHaveBeenCalledExactlyOnceWith([file], { scope: props.scope, trackId: "track", at: { ticks: "1", timebase: 1 } });
    props.sequence.tracks[0].locked = true;
    view.rerender(<V4TimelinePanel {...props} />);
    fireEvent.drop(lane, { dataTransfer: transfer, clientX: 132 });
    expect(props.onFiles).toHaveBeenCalledOnce();
    expect(screen.getByRole("alert").textContent).toContain("unlocked");
    props.sequence.tracks[0].locked = false; props.disabled = true;
    view.rerender(<V4TimelinePanel {...props} />);
    fireEvent.drop(lane, { dataTransfer: transfer, clientX: 132 });
    expect(props.onFiles).toHaveBeenCalledOnce();
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("real pointer cancellation writes nothing; pointerup plans once and consumes the following clip click", async () => {
  const { f, props } = await setup();
  try {
    const view = render(<V4TimelinePanel {...props} />);
    const clip = screen.getByRole("button", { name: "Whole image · image" });
    const panel = view.container.querySelector(".v4-timeline-panel")!;
    const viewport = view.container.querySelector(".v4-timeline-viewport")!;
    for (const how of ["escape", "cancel", "lost", "blur"]) {
      fireEvent.pointerDown(clip, { button: 0, pointerId: 1, clientX: 200, clientY: 130 });
      fireEvent.pointerMove(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
      if (how === "escape") fireEvent.keyDown(panel, { key: "Escape" });
      else if (how === "cancel") fireEvent.pointerCancel(viewport, { pointerId: 1 });
      else if (how === "lost") fireEvent.lostPointerCapture(viewport, { pointerId: 1 });
      else fireEvent.blur(window);
      fireEvent.pointerUp(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
    }
    expect(f.planner).not.toHaveBeenCalled();
    fireEvent.pointerDown(clip, { button: 0, pointerId: 2, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(viewport, { pointerId: 2, clientX: 240, clientY: 130 });
    fireEvent.pointerUp(viewport, { pointerId: 2, clientX: 240, clientY: 130 });
    fireEvent.lostPointerCapture(viewport, { pointerId: 2 });
    fireEvent.click(clip, { detail: 1 });
    await waitFor(() => expect(f.planner).toHaveBeenCalledTimes(1));
    expect(f.planner.mock.calls[0][1].command).toEqual({ kind: "move_selection", ripple: false, moves: [
      { target: props.selection[0], target_track_id: "track", new_start: { ticks: "1", timebase: 2 } },
    ] });
    expect(f.applyCalls()).toHaveLength(0); expect(props.onSeek).not.toHaveBeenCalled();
    await waitFor(() => expect(props.onReview).toHaveBeenCalledOnce());
  } finally { f.controller.dispose(); }
});

it("gain owns its focus; modifiers only select; keyboard split uses the original playhead", async () => {
  const { f, props } = await setup();
  try {
    const view = render(<V4TimelinePanel {...props} />);
    const slider = screen.getByRole("slider", { name: "Volume image" });
    fireEvent.pointerDown(slider, { button: 0, pointerId: 1, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(slider, { pointerId: 1, clientX: 240, clientY: 130 });
    fireEvent.pointerUp(slider, { pointerId: 1, clientX: 240, clientY: 130 });
    fireEvent.keyDown(slider, { key: "s" });
    expect(f.planner).not.toHaveBeenCalled();
    const clip = screen.getByRole("button", { name: "Whole image · image" });
    clip.focus();
    fireEvent.click(clip, { detail: 0 });
    expect(document.activeElement).toBe(view.container.querySelector(".v4-timeline-panel"));
    fireEvent.pointerDown(clip, { button: 0, ctrlKey: true, pointerId: 2, clientX: 200, clientY: 130 });
    fireEvent.pointerUp(clip, { pointerId: 2, clientX: 240, clientY: 130 });
    expect(props.onSelect).toHaveBeenCalledWith(props.selection[0], "toggle");
    expect(f.planner).not.toHaveBeenCalled();
    fireEvent.keyDown(view.container.querySelector(".v4-timeline-panel")!, { key: "s" });
    await waitFor(() => expect(f.planner).toHaveBeenCalledTimes(1));
    const command = f.planner.mock.calls[0][1].command;
    expect(command.kind).toBe("split");
    if (command.kind === "split") expect(command.at).toEqual(props.playhead);
  } finally { f.controller.dispose(); }
});

it("same-scope API identity change cancels a drag; bookmarks and track accessibility remain discoverable", async () => {
  const { f, props } = await setup();
  try {
    props.sequence.bookmarks.push({ id: "mark", sequence_id: "scene", time: { ticks: "1", timebase: 1 }, duration: null, note: "Note", color: null });
    const view = render(<V4TimelinePanel {...props} />);
    const clip = screen.getByRole("button", { name: "Whole image · image" });
    const viewport = view.container.querySelector(".v4-timeline-viewport")!;
    fireEvent.pointerDown(clip, { button: 0, pointerId: 1, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
    f.changeApi(); view.rerender(<V4TimelinePanel {...props} />);
    fireEvent.pointerUp(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
    expect(f.planner).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Bookmark at 00:01.00: Note" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "Main Track · video" }).getAttribute("aria-pressed")).toBe("false");
  } finally { f.controller.dispose(); }
});

it("rubberband uses scrolled content rectangles and Escape restores the captured selection without planning", async () => {
  const { f, props } = await setup();
  try {
    const first = props.sequence.tracks[0];
    first.elements[0].timeline_range = { start: { ticks: "1", timebase: 1 }, duration: { ticks: "1", timebase: 1 } };
    const second = structuredClone(first); second.id = "second-track"; second.elements[0].id = "second-image";
    props.sequence.tracks.push(second);
    const view = render(<V4TimelinePanel {...props} />);
    const viewport = view.container.querySelector(".v4-timeline-viewport")!;
    const lane = view.container.querySelector(".v4-track-lane")!;
    fireEvent.pointerDown(lane, { button: 0, pointerId: 1, clientX: 173, clientY: 110 });
    fireEvent.pointerMove(viewport, { pointerId: 1, clientX: 400, clientY: 230 });
    expect(props.onSelectMany).toHaveBeenLastCalledWith([
      { track_id: "track", element_id: "image" }, { track_id: "second-track", element_id: "second-image" },
    ], "second-image");
    fireEvent.keyDown(view.container.querySelector(".v4-timeline-panel")!, { key: "Escape" });
    expect(props.onSelectMany).toHaveBeenLastCalledWith(props.selection);
    fireEvent.pointerUp(viewport, { pointerId: 1, clientX: 400, clientY: 230 });
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});
