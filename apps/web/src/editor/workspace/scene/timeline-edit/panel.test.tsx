// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { setupTimelineEdit } from "./timeline-fixture";
import { TimelinePanel } from "./panel";
import { timelineZoomPosition } from "./geometry";
import type { TimelinePanelProps } from "./panel";
import { preferences } from "../../../shared/locale";
import { EditorShortcutsProvider } from "../../../shortcuts";
import type { ReactNode } from "react";

// Product keyboard edits are routed by the editor shortcut registry.
const Shortcuts = ({ children }: { children: ReactNode }) => <EditorShortcutsProvider><div data-shortcuts-root>{children}</div></EditorShortcutsProvider>;

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
    renderAudioControls: () => <div className="editor-audio-controls"><button role="slider" aria-label="Volume image">Gain</button></div>,
  };
  return { f, props };
}

it("compact rows preserve cross-track pointer targeting and cancel an unfinished drag", async () => {
  const { f, props } = await setup();
  try {
    const second = structuredClone(props.sequence.tracks[0]);
    second.id = "second"; second.name = "Second"; second.elements = [];
    props.sequence.tracks.push(second);
    const view = render(<TimelinePanel {...props} />);
    // Start in comfortable density, then switch while dragging.
    fireEvent.click(screen.getByRole("button", { name: "Compact tracks" }));
    const clip = screen.getByRole("button", { name: "Whole image · Image" });
    const viewport = view.container.querySelector(".editor-timeline-viewport")!;
    fireEvent.pointerDown(clip, { button: 0, pointerId: 1, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
    fireEvent.click(screen.getByRole("button", { name: "Compact tracks" }));
    fireEvent.pointerUp(viewport, { pointerId: 1, clientX: 240, clientY: 170 });
    expect(f.planner).not.toHaveBeenCalled();
    expect(view.container.querySelector<HTMLElement>(".editor-track")!.style.height).toBe("56px");
    fireEvent.pointerDown(clip, { button: 0, pointerId: 2, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(viewport, { pointerId: 2, clientX: 240, clientY: 170 });
    fireEvent.pointerUp(viewport, { pointerId: 2, clientX: 240, clientY: 170 });
    await waitFor(() => expect(f.planner).toHaveBeenCalledOnce());
    expect(f.planner.mock.calls[0][1].command).toMatchObject({ kind: "move_selection", moves: [{ target: props.selection[0], target_track_id: "second" }] });
    await waitFor(() => expect(f.applyCalls()).toHaveLength(1));
  } finally { f.controller.dispose(); }
});
it("moves the playback cursor from the output source without rebuilding clips or using a stale keyboard position",async()=>{
  const {f,props}=await setup(),listeners=new Set<()=>void>();
  let at={ticks:"1",timebase:3};
  props.presentation={time:()=>at,subscribe:listener=>{listeners.add(listener);return()=>{listeners.delete(listener);};}};
  try {
    vi.spyOn(performance,"now").mockReturnValue(1000);
    const view=render(<TimelinePanel {...props}/>);
    const clip=screen.getByRole('button',{name:'Whole image · Image'}),ruler=screen.getByRole('slider',{name:'Scene playhead'});
    vi.mocked(performance.now).mockReturnValue(1200);
    act(()=>{at={ticks:"3",timebase:2};listeners.forEach(listener=>listener());});
    expect(ruler.getAttribute('aria-valuenow')).toBe('1.5');
    expect(screen.getByRole('button',{name:'Whole image · Image'})).toBe(clip);
    expect(props.playhead).toEqual({ticks:"1",timebase:3});
    fireEvent.keyDown(ruler,{key:'ArrowRight'});
    const target=vi.mocked(props.onSeek).mock.calls[0][0];
    expect(Number(target.ticks)/target.timebase).toBeGreaterThan(1.5);
    view.unmount();expect(listeners.size).toBe(0);
  }finally{f.controller.dispose();}
});

it("track shortcuts plan through the same journal and obey locks and edit holds", async () => {
  const { f, props } = await setup();
  try {
    const track = props.sequence.tracks[0];
    track.name = "Main";
    const view = render(<TimelinePanel {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Mute track · Main" }));
    expect(props.onReview).not.toHaveBeenCalled();
    expect(f.planner.mock.calls[0][1].command).toEqual({ kind: "set_track_muted", track_id: track.id, muted: true });
    await waitFor(() => expect(f.applyCalls()).toHaveLength(1));
    props.sequence = structuredClone(props.sequence);
    props.sequence.tracks[0].locked = true;
    view.rerender(<TimelinePanel {...props} />);
    expect(screen.getByRole("button", { name: "Disable track · Main" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Mute track · Main" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Unlock track · Main" }).hasAttribute("disabled")).toBe(false);
    props.gainDraftActive = true;
    view.rerender(<TimelinePanel {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Unlock track · Main" }));
    expect(f.planner).toHaveBeenCalledOnce();
  } finally { f.controller.dispose(); }
});

it("fits the complete timeline without changing selection or writing an edit", async () => {
  const { f, props } = await setup();
  try {
    props.sequence.tracks[0].elements[0].timeline_range.duration = { ticks: "42", timebase: 1 };
    const view = render(<TimelinePanel {...props} />);
    const viewport = view.container.querySelector<HTMLElement>(".editor-timeline-viewport")!;
    Object.defineProperty(viewport, "clientWidth", { configurable: true, value: 1000 });
    viewport.scrollLeft = 800;
    fireEvent.click(screen.getByRole("button", { name: "Fit timeline" }));
    const clip = view.container.querySelector<HTMLElement>("[data-element-id=image]")!;
    expect(parseFloat(clip.style.left) + parseFloat(clip.style.width)).toBeLessThanOrEqual(1000 - 136 - 64 + .01);
    await waitFor(() => expect(viewport.scrollLeft).toBe(0));
    expect(f.planner).not.toHaveBeenCalled(); expect(props.onSelectMany).not.toHaveBeenCalled();
  } finally { f.controller.dispose(); }
});

it("exponential zoom keeps a visible playhead fixed and uses the content center when it is offscreen", async () => {
  const { f, props } = await setup();
  try {
    props.sequence.tracks[0].elements[0].timeline_range.duration = { ticks: "100", timebase: 1 };
    props.playhead = { ticks: "10", timebase: 1 };
    const view = render(<TimelinePanel {...props} />);
    const viewport = view.container.querySelector<HTMLElement>(".editor-timeline-viewport")!;
    Object.defineProperty(viewport, "clientWidth", { configurable: true, value: 1000 });
    viewport.scrollLeft = 400;
    const slider = screen.getByLabelText("Timeline zoom");
    fireEvent.change(slider, { target: { value: timelineZoomPosition(2) } });
    expect(viewport.scrollLeft).toBeCloseTo(1200);
    // Return at the same visible anchor, then pan so the playhead is offscreen.
    fireEvent.change(slider, { target: { value: timelineZoomPosition(1) } });
    expect(viewport.scrollLeft).toBeCloseTo(400);
    viewport.scrollLeft = 2000;
    fireEvent.change(slider, { target: { value: timelineZoomPosition(2) } });
    expect(viewport.scrollLeft).toBeCloseTo(4432);
    expect(f.planner).not.toHaveBeenCalled();
    expect(props.onSelectMany).not.toHaveBeenCalled();
    expect(props.onSeek).not.toHaveBeenCalled();
  } finally { f.controller.dispose(); }
});

it("slider midpoint gives geometric scale and rapid wheel zoom preserves the pointer time", async () => {
  const { f, props } = await setup();
  try {
    props.sequence.tracks[0].elements[0].timeline_range.duration = { ticks: "100", timebase: 1 };
    const view = render(<TimelinePanel {...props} />);
    const viewport = view.container.querySelector<HTMLElement>(".editor-timeline-viewport")!;
    Object.defineProperty(viewport, "clientWidth", { configurable: true, value: 1000 });
    const slider = screen.getByLabelText("Timeline zoom");
    fireEvent.change(slider, { target: { value: "0.5" } });
    const clip = view.container.querySelector<HTMLElement>("[data-element-id=image]")!;
    expect(parseFloat(clip.style.width) / 100).toBeCloseTo(71.55417528);
    fireEvent.change(slider, { target: { value: timelineZoomPosition(1) } });
    viewport.scrollLeft = 400;
    const events = [0, 1].map(() => new WheelEvent("wheel", { bubbles: true, cancelable: true,
      ctrlKey: true, deltaY: -100, clientX: 500 }));
    act(() => { events.forEach(event => viewport.dispatchEvent(event)); });
    const scale = parseFloat(clip.style.width) / 100;
    expect(scale).toBeCloseTo(80 * Math.exp(.4));
    expect((viewport.scrollLeft + 500 - 136) / scale).toBeCloseTo((400 + 500 - 136) / 80);
    expect(events.every(event => event.defaultPrevented)).toBe(true);
    const pan = new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaX: 100 });
    viewport.dispatchEvent(pan);
    expect(pan.defaultPrevented).toBe(false);
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("consumer cursor updates reuse authored clip bodies while callbacks and language stay current", async () => {
  const { f, props } = await setup();
  try {
    const body = vi.fn(() => null); props.renderAudioControls = body;
    const view = render(<TimelinePanel {...props} />), rendered = body.mock.calls.length;
    const nextSeek = vi.fn();
    view.rerender(<TimelinePanel {...props} playhead={{ ticks: "2", timebase: 3 }} onSeek={nextSeek} />);
    expect(body).toHaveBeenCalledTimes(rendered);
    fireEvent.click(screen.getByRole("button", { name: "Whole image · Image" }));
    expect(nextSeek).toHaveBeenCalledWith(props.sequence.tracks[0].elements[0].timeline_range.start);
    act(() => preferences({ language: "zh" }));
    expect(body.mock.calls.length).toBeGreaterThan(rendered);
  } finally { act(() => preferences({ language: "en" })); f.controller.dispose(); }
});

it("file drops use the lane coordinate at the current zoom, including scroll, and reject locked or busy tracks", async () => {
  const { f, props } = await setup();
  try {
    props.onFiles = vi.fn(async () => {});
    const view = render(<TimelinePanel {...props} />);
    const lane = view.container.querySelector<HTMLElement>("[data-file-track=track]")!;
    const file = new File(["png"], "image.png", { type: "image/png" });
    const transfer = { files: [file], types: ["Files"], dropEffect: "none" };
    vi.spyOn(lane, "getBoundingClientRect").mockReturnValue({ left: -28, top: 100 } as DOMRect);
    fireEvent.change(screen.getByLabelText("Timeline zoom"), { target: { value: timelineZoomPosition(2) } });
    fireEvent.drop(lane, { dataTransfer: transfer, clientX: 132 });
    expect(props.onFiles).toHaveBeenCalledExactlyOnceWith([file], { scope: props.scope, trackId: "track", at: { ticks: "1", timebase: 1 } });
    props.sequence.tracks[0].locked = true;
    view.rerender(<TimelinePanel {...props} />);
    fireEvent.drop(lane, { dataTransfer: transfer, clientX: 132 });
    expect(props.onFiles).toHaveBeenCalledOnce();
    expect(screen.getByRole("alert").textContent).toContain("unlocked");
    props.sequence.tracks[0].locked = false; props.disabled = true;
    view.rerender(<TimelinePanel {...props} />);
    fireEvent.drop(lane, { dataTransfer: transfer, clientX: 132 });
    expect(props.onFiles).toHaveBeenCalledOnce();
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("real pointer cancellation writes nothing; pointerup plans once and consumes the following clip click", async () => {
  const { f, props } = await setup();
  try {
    const view = render(<TimelinePanel {...props} />);
    const clip = screen.getByRole("button", { name: "Whole image · Image" });
    const panel = view.container.querySelector(".editor-timeline-panel")!;
    const viewport = view.container.querySelector(".editor-timeline-viewport")!;
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
    await waitFor(() => expect(f.applyCalls()).toHaveLength(1)); expect(props.onSeek).not.toHaveBeenCalled();
    expect(props.onReview).not.toHaveBeenCalled();
  } finally { f.controller.dispose(); }
});

it("gain owns its focus; modifiers only select; keyboard split uses the original playhead", async () => {
  const { f, props } = await setup();
  try {
    const view = render(<TimelinePanel {...props} />, { wrapper: Shortcuts });
    const slider = screen.getByRole("slider", { name: "Volume image" });
    fireEvent.pointerDown(slider, { button: 0, pointerId: 1, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(slider, { pointerId: 1, clientX: 240, clientY: 130 });
    fireEvent.pointerUp(slider, { pointerId: 1, clientX: 240, clientY: 130 });
    fireEvent.keyDown(slider, { key: "s" });
    fireEvent.keyUp(slider, { key: "s" });
    expect(f.planner).not.toHaveBeenCalled();
    const clip = screen.getByRole("button", { name: "Whole image · Image" });
    clip.focus();
    fireEvent.click(clip, { detail: 0 });
    expect(document.activeElement).toBe(view.container.querySelector(".editor-timeline-panel"));
    fireEvent.pointerDown(clip, { button: 0, ctrlKey: true, pointerId: 2, clientX: 200, clientY: 130 });
    fireEvent.pointerUp(clip, { pointerId: 2, clientX: 240, clientY: 130 });
    expect(props.onSelect).toHaveBeenCalledWith(props.selection[0], "toggle");
    expect(f.planner).not.toHaveBeenCalled();
    fireEvent.keyDown(view.container.querySelector(".editor-timeline-panel")!, { key: "s" });
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
    const view = render(<TimelinePanel {...props} />);
    const clip = screen.getByRole("button", { name: "Whole image · Image" });
    const viewport = view.container.querySelector(".editor-timeline-viewport")!;
    fireEvent.pointerDown(clip, { button: 0, pointerId: 1, clientX: 200, clientY: 130 });
    fireEvent.pointerMove(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
    f.changeApi(); view.rerender(<TimelinePanel {...props} />);
    fireEvent.pointerUp(viewport, { pointerId: 1, clientX: 240, clientY: 130 });
    expect(f.planner).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Bookmark at 00:01.00: Note" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "Main Track · Video" }).getAttribute("aria-pressed")).toBe("false");
  } finally { f.controller.dispose(); }
});

it("rubberband uses scrolled content rectangles and Escape restores the captured selection without planning", async () => {
  const { f, props } = await setup();
  try {
    const first = props.sequence.tracks[0];
    first.elements[0].timeline_range = { start: { ticks: "1", timebase: 1 }, duration: { ticks: "1", timebase: 1 } };
    const second = structuredClone(first); second.id = "second-track"; second.elements[0].id = "second-image";
    props.sequence.tracks.push(second);
    const view = render(<TimelinePanel {...props} />, { wrapper: Shortcuts });
    const viewport = view.container.querySelector(".editor-timeline-viewport")!;
    const lane = view.container.querySelector(".editor-track-lane")!;
    fireEvent.pointerDown(lane, { button: 0, pointerId: 1, clientX: 173, clientY: 110 });
    fireEvent.pointerMove(viewport, { pointerId: 1, clientX: 400, clientY: 230 });
    expect(props.onSelectMany).toHaveBeenLastCalledWith([
      { track_id: "track", element_id: "image" }, { track_id: "second-track", element_id: "second-image" },
    ], "second-image");
    fireEvent.keyDown(view.container.querySelector(".editor-timeline-panel")!, { key: "Escape" });
    expect(props.onSelectMany).toHaveBeenLastCalledWith(props.selection);
    fireEvent.pointerUp(viewport, { pointerId: 1, clientX: 400, clientY: 230 });
    expect(f.planner).not.toHaveBeenCalled(); expect(f.applyCalls()).toHaveLength(0);
  } finally { f.controller.dispose(); }
});

it("actual text clip selects for timeline editing, exposes both trim handles and never requests media art", async () => {
  const { textTimelineSequence } = await import("./text-fixture");
  const {f,props} = await setup();
  try {
    const sequence = textTimelineSequence(), target = {track_id:"titles",element_id:"title"};
    const head = structuredClone(f.state().head); head.project.sequences[0] = sequence;
    f.set({head}); props.sequence = sequence; props.selection = [target]; props.selectedId = "title";
    const view = render(<TimelinePanel {...props} />, { wrapper: Shortcuts });
    const clip = screen.getByRole("button",{name:"Independent title · Text"});
    const body = clip.closest(".editor-timeline-element")!;
    expect(body.hasAttribute("data-timeline-media")).toBe(false);
    expect(body.getAttribute("data-timeline-editable")).toBe("true");
    expect(body.querySelectorAll("[data-trim-edge]")).toHaveLength(2);
    fireEvent.click(clip,{detail:0});
    expect(props.onSelect).toHaveBeenCalledWith(target,"replace");
    fireEvent.keyDown(view.container.querySelector(".editor-timeline-panel")!,{key:"s"});
    await waitFor(()=>expect(f.planner).toHaveBeenCalledTimes(1));
    expect(f.planner.mock.calls[0][1].command.kind).toBe("split");
    await waitFor(() => expect(f.applyCalls()).toHaveLength(1));
  } finally {f.controller.dispose();}
});
