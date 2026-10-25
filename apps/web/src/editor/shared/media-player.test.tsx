// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { MediaPlayer } from "./controls";
import { preferences } from "./locale";
beforeEach(()=>{
  preferences({language:"en"});
  vi.spyOn(HTMLMediaElement.prototype,"play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype,"pause").mockImplementation(()=>{});
  vi.spyOn(document,"hidden","get").mockReturnValue(false);
});
afterEach(()=>{cleanup();vi.restoreAllMocks();});
it("waits for playback ownership and does not double-start while it is pending",async()=>{
  let finish!:(allowed:boolean)=>void;
  const before=vi.fn(()=>new Promise<boolean>(resolve=>{finish=resolve;}));
  const view=render(<MediaPlayer src="/source.mp4" beforePlay={before}/>);
  fireEvent.click(screen.getByRole("button",{name:"Play"}));fireEvent.click(screen.getByRole("button",{name:"Play"}));
  expect(before).toHaveBeenCalledOnce();expect(view.container.querySelector("video")!.play).not.toHaveBeenCalled();
  await act(async()=>finish(true));
  expect(view.container.querySelector("video")!.play).toHaveBeenCalledOnce();
});
it("does not start after the view leaves while ownership is pending",async()=>{
  let finish!:(allowed:boolean)=>void;
  const view=render(<MediaPlayer src="/source.mp4" beforePlay={()=>new Promise<boolean>(resolve=>{finish=resolve;})}/>);
  const video=view.container.querySelector("video")!;
  fireEvent.click(screen.getByRole("button",{name:"Play"}));view.unmount();
  await act(async()=>finish(true));expect(video.play).not.toHaveBeenCalled();expect(video.pause).toHaveBeenCalled();
});
