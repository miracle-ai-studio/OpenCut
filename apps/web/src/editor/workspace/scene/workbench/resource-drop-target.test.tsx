// @vitest-environment jsdom
import { useRef } from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { initialSnapshot } from "../fixture";
import { useResourceDropTarget } from "./resource-drop-target";
const mock = vi.hoisted(() => ({ hover: vi.fn(), cancel: vi.fn(), drop: vi.fn(),
  drag: { drag: { resource: { kind: "media" } }, hover: (...args: unknown[]) => mock.hover(...args),
    cancel: () => mock.cancel(), drop: (...args: unknown[]) => mock.drop(...args) } }));
vi.mock("./resource-drag", () => ({ RESOURCE_DRAG_TYPE: "application/x-opencut-workbench-resource", useResourceDrag: () => mock.drag }));
const sequence = initialSnapshot().project.sequences[0];
const frames = new Map<number, FrameRequestCallback>();
const originalHitTest = Object.getOwnPropertyDescriptor(document, "elementFromPoint");
let frameId = 0;
function Harness() {
  const ref = useRef<HTMLDivElement>(null);
  const target = useResourceDropTarget(ref, sequence, 80, true, false);
  return <div><div ref={ref} data-testid="viewport" onDragOver={target.over} onDragLeave={target.leave} onDrop={target.drop}>
    <div data-file-track="track" data-testid="lane" />
  </div><div data-resource-new-track="true" data-testid="new-track" onDragOver={target.over} onDrop={target.drop}/></div>;
}
function setup() {
  const view = render(<Harness />), viewport = screen.getByTestId("viewport"), lane = screen.getByTestId("lane");
  vi.spyOn(viewport, "getBoundingClientRect").mockReturnValue({ left: 0, top: 0, right: 700, bottom: 400 } as DOMRect);
  vi.spyOn(lane, "getBoundingClientRect").mockImplementation(() => ({ left: 136 - viewport.scrollLeft } as DOMRect));
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: () => lane });
  const over = (x: number) => fireEvent.dragOver(lane, { clientX: x, clientY: 150,
    dataTransfer: { types: ["application/x-opencut-workbench-resource"], dropEffect: "none" } });
  const tick = (at: number) => act(() => { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback => callback(at)); });
  return { ...view, viewport, lane, over, tick };
}
beforeEach(() => {
  mock.hover.mockReset().mockReturnValue({ reason: null }); mock.cancel.mockReset(); mock.drop.mockReset(); frames.clear();
  sequence.tracks[0].elements[0].timeline_range.start = { ticks: "1", timebase: 3 };
  Object.defineProperty(window, "DragEvent", { configurable: true, value: class extends MouseEvent {
    dataTransfer: DataTransfer | null;
    constructor(type: string, init: DragEventInit) { super(type, init); this.dataTransfer = init.dataTransfer ?? null; }
  } });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { frames.set(++frameId, callback); return frameId; });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
});
afterEach(() => {
  cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals();
  if (originalHitTest) Object.defineProperty(document, "elementFromPoint", originalHitTest);
  else Reflect.deleteProperty(document, "elementFromPoint");
});

it("returns the original exact rational edge when a resource snaps, including after scroll", () => {
  const { viewport, over } = setup(); viewport.scrollLeft = 15;
  over(136 - 15 + 80 / 3 + .5);
  expect(mock.hover.mock.lastCall?.[0].at).toBe(sequence.tracks[0].elements[0].timeline_range.start);
});
it("accepts the pinned new-track rail outside the scrolled viewport and preserves its horizontal time",()=>{
 const {viewport}=setup();viewport.scrollLeft=160;const lane=screen.getByTestId("new-track");
 const transfer={types:["application/x-opencut-workbench-resource"],dropEffect:"none",getData:()=>"drag-token"};
 fireEvent.dragOver(lane,{clientX:216,clientY:380,dataTransfer:transfer});expect(mock.hover.mock.lastCall?.[0]).toMatchObject({trackId:null,at:{ticks:"3",timebase:1}});
 fireEvent.drop(lane,{clientX:216,clientY:380,dataTransfer:transfer});expect(mock.drop).toHaveBeenLastCalledWith(expect.objectContaining({trackId:null,at:{ticks:"3",timebase:1}}),"drag-token");
});

it.each(["leave", "escape", "blur"])("scrolls continuously under a stationary drag, then stops immediately on %s", reason => {
  const { viewport, over, tick } = setup();
  over(696); tick(16); const first = viewport.scrollLeft; tick(32);
  expect(first).toBeGreaterThan(0); expect(viewport.scrollLeft).toBeGreaterThan(first);
  expect(mock.hover.mock.lastCall?.[0].at).not.toEqual({ ticks: "0", timebase: 1 });
  if (reason === "leave") fireEvent.dragLeave(viewport, { relatedTarget: document.body });
  else if (reason === "escape") fireEvent.keyDown(window, { key: "Escape" });
  else fireEvent.blur(window);
  const stopped = viewport.scrollLeft; tick(48); tick(64);
  expect(viewport.scrollLeft).toBe(stopped); expect(frames.size).toBe(0);
  expect(mock.drop).not.toHaveBeenCalled();
});
