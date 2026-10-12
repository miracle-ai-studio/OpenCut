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
import { WorkspaceProvider } from "../context";
import { V4Product } from "./product";
import { setupAudioGain } from "./audio-gain-fixture";
import { gainLinePercent } from "./audio-gain-display";
beforeEach(() => {
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
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    () => ({
      top: 100,
      left: 0,
      right: 200,
      bottom: 132,
      width: 200,
      height: 32,
      x: 0,
      y: 100,
      toJSON() {},
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
const y = (db: number) => 100 + (gainLinePercent(db) / 100) * 32;
it("pointer cancellation, Escape, capture loss and blur discard drafts; unchanged up never plans", async () => {
  const f = await setupAudioGain();
  try {
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    const slider = screen.getByRole("slider", { name: "Volume Source video" });
    for (const how of ["escape", "cancel", "lost", "blur", "window"]) {
      fireEvent.pointerDown(slider, {
        pointerId: 1,
        button: 0,
        clientY: y(-12),
      });
      expect(f.controller.getSnapshot().v4!.gainDraft!.tenths).toBe(-120);
      if (how === "escape") fireEvent.keyDown(slider, { key: "Escape" });
      else if (how === "cancel")
        fireEvent.pointerCancel(slider, { pointerId: 1 });
      else if (how === "lost")
        fireEvent.lostPointerCapture(slider, { pointerId: 1 });
      else if (how === "window") fireEvent.blur(window);
      else fireEvent.blur(slider);
      fireEvent.pointerUp(slider, { pointerId: 1 });
      expect(f.controller.getSnapshot().v4!.gainDraft).toBeNull();
    }
    fireEvent.pointerDown(slider, { pointerId: 2, button: 0, clientY: y(-6) });
    fireEvent.pointerUp(slider, { pointerId: 2 });
    expect(f.gainRequests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
it("pointerup saves exactly once, keeps focus/playhead and mute toggles independently", async () => {
  const f = await setupAudioGain();
  try {
    act(() => f.controller.v4.seek({ ticks: "5", timebase: 1 }));
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    const slider = screen.getByRole("slider", { name: "Volume Source video" });
    fireEvent.pointerDown(slider, { pointerId: 1, button: 0, clientY: y(-12) });
    expect(document.activeElement).toBe(slider);
    expect(f.controller.getSnapshot().v4!.playhead.ticks).toBe("5");
    expect(f.gainRequests).toHaveLength(0);
    fireEvent.pointerUp(slider, { pointerId: 1 });
    fireEvent.lostPointerCapture(slider, { pointerId: 1 });
    await waitFor(() =>
      expect(f.controller.getSnapshot().v4!.audioGain?.phase).toBe("saved"),
    );
    expect(f.gainRequests).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Mute Source video" }));
    await screen.findByRole("button", { name: "Unmute Source video" });
    expect(slider.getAttribute("aria-valuetext")).toBe("-12.0 dB");
    fireEvent.click(
      screen.getByRole("button", { name: "Unmute Source video" }),
    );
    await screen.findByRole("button", { name: "Mute Source video" });
    expect(f.gainRequests.map((r) => r.command)).toEqual([
      { kind: "set_decibels", decibels_tenths: -120 },
      { kind: "set_muted", muted: true },
      { kind: "set_muted", muted: false },
    ]);
  } finally {
    f.controller.dispose();
  }
});
it("keyboard Home/End and arrows submit exact tenths on keyup; Escape before release cancels", async () => {
  const f = await setupAudioGain();
  try {
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    const slider = screen.getByRole("slider", { name: "Volume Source video" });
    slider.focus();
    for (const [key, value] of [
      ["Home", -600],
      ["End", 200],
      ["ArrowDown", 199],
    ] as const) {
      fireEvent.keyDown(slider, { key });
      expect(f.controller.getSnapshot().v4!.gainDraft!.tenths).toBe(value);
      const count = f.gainRequests.length;
      fireEvent.keyUp(slider, { key });
      await waitFor(() => expect(f.gainRequests).toHaveLength(count + 1));
      await waitFor(() =>
        expect(f.controller.getSnapshot().v4!.audioGain?.phase).toBe("saved"),
      );
    }
    fireEvent.keyDown(slider, { key: "ArrowDown" });
    fireEvent.keyDown(slider, { key: "Escape" });
    fireEvent.keyUp(slider, { key: "ArrowDown" });
    expect(f.gainRequests).toHaveLength(3);
    expect(document.activeElement).toBe(slider);
  } finally {
    f.controller.dispose();
  }
});
it("historical or unknown-audio views expose disabled controls and never dispatch edits", async () => {
  const f = await setupAudioGain();
  try {
    await f.controller.v4.inspectRevision("r0");
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    let slider = screen.getByRole("slider", { name: "Volume Source video" });
    expect(slider.getAttribute("aria-disabled")).toBe("true");
    fireEvent.keyDown(slider, { key: "Home" });
    fireEvent.keyUp(slider, { key: "Home" });
    expect(
      (
        screen.getByRole("button", {
          name: "Mute Source video",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    f.current().project.assets[0].metadata.audio = { status: "unknown" };
    await act(async () => {
      f.controller.v4.returnToHead();
      await f.controller.v4.refresh();
    });
    slider = screen.getByRole("slider", { name: "Volume Source video" });
    expect(slider.getAttribute("aria-disabled")).toBe("true");
    expect(f.gainRequests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
