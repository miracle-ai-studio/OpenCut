// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../../context";
import { setup } from "../fixture";
import { preferences } from "../../../shared/locale";
import { PlayerPreviewProvider, WorkbenchPlayer } from "./player-preview";
import { usePlayerContext } from "./player-context";

beforeEach(() => { preferences({language:"en"}); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
let context: NonNullable<ReturnType<typeof usePlayerContext>>;
function Probe() { context=usePlayerContext()!; return <button onClick={() => void context.open({kind:"effect",name:"Soft",intensity:5})}>Preview effect</button>; }
async function mount() {
  const f=await setup();
  render(<WorkspaceProvider controller={f.controller}><PlayerPreviewProvider><Probe/><WorkbenchPlayer><div data-testid="timeline-canvas">Retained canvas</div></WorkbenchPlayer></PlayerPreviewProvider></WorkspaceProvider>);
  return f;
}
it("keeps the timeline mounted and restores resource focus without changing selection/time on return", async () => {
  const f=await mount();
  try {
    const before=structuredClone({selected:f.controller.getSnapshot().scene!.selectedId,at:f.controller.scene.currentPlayhead()}), canvas=screen.getByTestId("timeline-canvas"), trigger=screen.getByRole("button",{name:"Preview effect"});
    trigger.focus(); fireEvent.click(trigger);
    await screen.findByLabelText("Effect example");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(canvas.closest("[inert]")).toBeTruthy();
    fireEvent.keyDown(screen.getByLabelText("Effect example"),{key:"Escape"});
    await waitFor(() => expect(screen.queryByLabelText("Effect example")).toBeNull());
    expect(screen.getByTestId("timeline-canvas")).toBe(canvas);
    expect(canvas.closest("[inert]")).toBeNull();
    expect(document.activeElement).toBe(trigger);
    expect({selected:f.controller.getSnapshot().scene!.selectedId,at:f.controller.scene.currentPlayhead()}).toEqual(before);
  } finally {f.controller.dispose();}
});
it("awaits timeline pause and admits only the latest audition owner, without an old release stopping the new one", async () => {
  const f=await mount();
  try {
    f.controller.getSnapshot().scene!.audioPreview.phase="playing";
    let finish!:()=>void;
    const paused=new Promise<void>(resolve=>{finish=()=>{f.controller.getSnapshot().scene!.audioPreview.phase="paused";resolve();};});
    vi.spyOn(f.controller.scene.audioPreview,"pause").mockReturnValue(paused);
    const first=Symbol(),second=Symbol(),stopFirst=vi.fn(),stopSecond=vi.fn();
    const one=context.begin(first,stopFirst),two=context.begin(second,stopSecond);
    expect(stopFirst).toHaveBeenCalledOnce();
    let accepted=false; void two.then(value=>{accepted=value;});
    expect(accepted).toBe(false);
    await act(async()=>{finish();await Promise.all([one,two]);});
    expect(await one).toBe(false);expect(await two).toBe(true);
    context.release(first); expect(stopSecond).not.toHaveBeenCalled();
    context.stopExternal(); expect(stopSecond).toHaveBeenCalledOnce();
  } finally {f.controller.dispose();}
});
it("does not reopen a source/example after cancellation while timeline pause is pending", async () => {
  const f=await mount();
  try {
    f.controller.getSnapshot().scene!.audioPreview.phase="playing";
    let finish!:()=>void;
    vi.spyOn(f.controller.scene.audioPreview,"pause").mockImplementation(()=>new Promise<void>(resolve=>{finish=()=>{f.controller.getSnapshot().scene!.audioPreview.phase="paused";resolve();};}));
    let opened!:Promise<void>;
    act(()=>{opened=context.open({kind:"effect",name:"Soft",intensity:5});});
    expect(screen.getByLabelText("Effect example")).toBeTruthy();
    act(()=>context.close());
    await act(async()=>{finish();await opened;});
    expect(screen.queryByLabelText("Effect example")).toBeNull();
  } finally {f.controller.dispose();}
});
it("does not admit source playback when timeline pause returns an unconfirmed result", async () => {
  const f=await mount();
  try {
    f.controller.getSnapshot().scene!.audioPreview.phase="playing";
    vi.spyOn(f.controller.scene.audioPreview,"pause").mockImplementation(async()=>{f.controller.getSnapshot().scene!.audioPreview.phase="unconfirmed";});
    const stop=vi.fn();
    await act(async()=>{expect(await context.begin(Symbol(),stop)).toBe(false);});
    expect(stop).toHaveBeenCalledOnce();
    expect(f.controller.getSnapshot().error).toContain("Could not confirm paused timeline playback");
  } finally {f.controller.dispose();}
});
