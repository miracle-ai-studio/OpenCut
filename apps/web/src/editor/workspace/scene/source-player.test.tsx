// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { setupMedia } from "./media-fixture";
import { SourcePlayer } from "./source-player";
import { placeMediaDirect } from "./media-dialog";
import { preferences } from "../../shared/locale";

beforeEach(() => {
  preferences({ language: "en" });
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
async function fixture() {
  const f = await setupMedia(), asset = f.current().project.assets[0];
  asset.kind = "video"; asset.duration = { ticks: "15", timebase: 1 }; asset.metadata.audio = { status: "absent" };
  await f.controller.scene.refresh();
  f.controller.scene.seek({ ticks: "1", timebase: 3 });
  return { ...f, asset };
}
it("keeps the source cursor separate and sends2–10 as an exact8-second source range through the original media port", async () => {
  const f = await fixture();
  try {
    const view = render(<WorkspaceProvider controller={f.controller}><SourcePlayer asset={f.asset} projectId={f.current().project.id} revision={f.current().revision} url="/source.mp4" onPlaced={() => {}} /></WorkspaceProvider>);
    const video = view.container.querySelector("video")!;
    video.currentTime = 2;
    fireEvent.keyDown(view.container.querySelector(".source-player")!, {key:"i"});
    video.currentTime = 10;
    fireEvent.keyDown(view.container.querySelector(".source-player")!, {key:"o"});
    expect((screen.getByLabelText("In point (seconds)") as HTMLInputElement).value).toBe("2.000000");
    expect((screen.getByLabelText("Out point (seconds)") as HTMLInputElement).value).toBe("10.000000");
    expect(f.controller.scene.currentPlayhead()).toEqual({ticks:"1",timebase:3});
    expect(f.requests).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("Target track"), {target:{value:"new"}});
    fireEvent.click(screen.getByRole("button", {name:"Add selected range"}));
    await waitFor(() => expect(f.requests).toHaveLength(1));
    expect(f.requests[0].placements[0]).toMatchObject({start:{ticks:"1",timebase:3},content:{kind:"video",source_audio_enabled:false,timing:{kind:"source_range",range:{start:{ticks:"2",timebase:1},duration:{ticks:"8",timebase:1}},rate:{numerator:1,denominator:1}}}});
    await waitFor(() => expect(f.controller.getSnapshot().scene?.media.attempt).toBeNull());
  } finally { f.controller.dispose(); }
});
it("blocks unconfirmed fields, rejects an out point beyond the source, and Escape restores only that field", async () => {
  const f = await fixture();
  try {
    render(<WorkspaceProvider controller={f.controller}><SourcePlayer asset={f.asset} projectId={f.current().project.id} revision={f.current().revision} url="/source.mp4" onPlaced={() => {}} /></WorkspaceProvider>);
    const input = screen.getByLabelText("Out point (seconds)");
    fireEvent.change(input,{target:{value:"16"}});
    expect((screen.getByRole("button",{name:"Add selected range"}) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.keyDown(input,{key:"Enter"});
    expect(screen.getByRole("alert").textContent).toContain("source duration");
    fireEvent.keyDown(input,{key:"Escape"});
    expect((input as HTMLInputElement).value).toBe("15.000000");
    expect(f.requests).toHaveLength(0);
    expect(f.controller.scene.currentPlayhead()).toEqual({ticks:"1",timebase:3});
  } finally { f.controller.dispose(); }
});
it("uses selected duration for collision checks and rejects out-of-source ranges before planning", async () => {
  const f = await fixture();
  try {
    const track=f.current().project.sequences[0].tracks[0];
    track.elements[0].timeline_range.duration={ticks:"2",timebase:1};
    const second=structuredClone(track.elements[0]);
    second.id="later"; second.timeline_range.start={ticks:"12",timebase:1}; track.elements.push(second);
    await f.controller.scene.refresh();
    const position={projectId:f.current().project.id,revision:f.current().revision,sequenceId:"scene",at:{ticks:"3",timebase:1},trackId:track.id};
    await expect(placeMediaDirect(f.controller,f.asset,position,{start:{ticks:"2",timebase:1},duration:{ticks:"20",timebase:1}})).rejects.toThrow("observed media duration");
    expect(f.requests).toHaveLength(0);
    await placeMediaDirect(f.controller,f.asset,position,{start:{ticks:"2",timebase:1},duration:{ticks:"8",timebase:1}});
    expect(f.requests).toHaveLength(1);
    expect(f.requests[0].placements[0].start).toEqual(position.at);
  } finally { f.controller.dispose(); }
});
it("does not claim a default range for a video whose observed duration is unavailable",async()=>{
  const f=await fixture();
  try {
    f.asset.duration=null;
    render(<WorkspaceProvider controller={f.controller}><SourcePlayer asset={f.asset} projectId={f.current().project.id} revision={f.current().revision} url="/source.mp4" onPlaced={()=>{}}/></WorkspaceProvider>);
    expect(screen.queryByLabelText("In point (seconds)")).toBeNull();
    expect(screen.getByText("Source duration is not confirmed. Range placement is unavailable.")).toBeTruthy();
    expect((screen.getByRole("button",{name:"Add to timeline"}) as HTMLButtonElement).disabled).toBe(true);
    expect(f.requests).toHaveLength(0);
  } finally {f.controller.dispose();}
});
