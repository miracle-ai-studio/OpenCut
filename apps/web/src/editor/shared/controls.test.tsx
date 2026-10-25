// @vitest-environment jsdom
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Select, MediaPlayer } from "./controls";
import { decimalRate } from "./rate-input";
import { secondsRange } from "../workspace/scene/export-status";
afterEach(cleanup);
it("selects by keyboard, skips disabled options and closes without changing on Escape", () => {
  function Form() { const [value,setValue]=useState("a"); return <label>Format<Select value={value} onChange={e=>setValue(e.target.value)}><option value="a">First</option><option disabled value="b">Unavailable</option><option value="c">Last</option></Select></label>; }
  render(<Form/>); const select=screen.getByLabelText("Format") as HTMLSelectElement;
  fireEvent.keyDown(select,{key:"Enter"}); fireEvent.keyDown(select,{key:"ArrowDown"}); fireEvent.keyDown(select,{key:"Enter"});
  expect(select.value).toBe("c"); expect(screen.queryByRole("listbox")).toBeNull();
  fireEvent.keyDown(select,{key:"Enter"}); fireEvent.keyDown(select,{key:"Home"}); fireEvent.keyDown(select,{key:"Escape"}); expect(select.value).toBe("c");
});
it("disables custom choices when the parent fieldset is disabled", () => {
  render(<fieldset disabled><label>Format<Select><option>A</option></Select></label></fieldset>);
  fireEvent.keyDown(screen.getByLabelText("Format"),{key:"Enter"}); expect(screen.queryByRole("listbox")).toBeNull();
});
it("converts decimal rates and time ranges exactly at the authoring boundary", () => {
  expect(decimalRate("1.25")).toEqual(["5","4"]);
  expect(secondsRange("1.000001","2.25",48000)).toEqual({start_sample:"48000",frame_count:"60000"});
  expect(secondsRange("0.0005","0.0015",1000)).toEqual({start_sample:"1",frame_count:"1"});
  expect(()=>secondsRange("2","1",48000)).toThrow(); expect(()=>decimalRate("0")).toThrow();
});

it("keeps source playback controls available after an interrupted play request, while reporting real playback failures", async () => {
  const pause = vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockRejectedValueOnce(new DOMException("Playback was interrupted", "AbortError")).mockRejectedValueOnce(new DOMException("Unsupported media", "NotSupportedError"));
  try {
    render(<MediaPlayer src="/source.mp4" />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Play" })));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("button", { name: "Play" })).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Play" })));
    expect(screen.getByRole("alert")).toBeTruthy();
  } finally { cleanup(); play.mockRestore(); pause.mockRestore(); }
});
