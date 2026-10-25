// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { Product } from "./product";
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
it("preserves a volume draft at its original revision until an external head is explicitly refreshed", async () => {
  const f = await setupAudioGain();
  try {
    const draft = f.controller.scene.audioGain.begin("video");
    f.controller.scene.audioGain.update(draft, -120);
    f.current().revision = "external-volume-head";
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene).toMatchObject({
      head: { revision: "r0" }, externalRevision: "external-volume-head",
      gainDraft: { id: draft, revision: "r0", tenths: -120 },
    });
    expect(f.gainRequests).toHaveLength(0);
    f.controller.scene.audioGain.cancel(draft);
    await f.controller.scene.refresh();
    expect(f.controller.getSnapshot().scene).toMatchObject({ head: { revision: "external-volume-head" }, externalRevision: null, gainDraft: null });
  } finally { f.controller.dispose(); }
});
it("pointer cancellation, Escape, capture loss and blur discard drafts; unchanged up never plans", async () => {
  const f = await setupAudioGain();
  try {
    render(
      <WorkspaceProvider controller={f.controller}>
        <Product />
      </WorkspaceProvider>,
    );
    const slider = screen.getByRole("slider", { name: "Volume Source video" });
    for (const how of ["escape", "cancel", "lost", "blur", "window"]) {
      fireEvent.pointerDown(slider, {
        pointerId: 1,
        button: 0,
        clientY: y(-12),
      });
      expect(f.controller.getSnapshot().scene!.gainDraft!.tenths).toBe(-120);
      if (how === "escape") fireEvent.keyDown(slider, { key: "Escape" });
      else if (how === "cancel")
        fireEvent.pointerCancel(slider, { pointerId: 1 });
      else if (how === "lost")
        fireEvent.lostPointerCapture(slider, { pointerId: 1 });
      else if (how === "window") fireEvent.blur(window);
      else fireEvent.blur(slider);
      fireEvent.pointerUp(slider, { pointerId: 1 });
      expect(f.controller.getSnapshot().scene!.gainDraft).toBeNull();
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
    act(() => f.controller.scene.seek({ ticks: "5", timebase: 1 }));
    render(
      <WorkspaceProvider controller={f.controller}>
        <Product />
      </WorkspaceProvider>,
    );
    const slider = screen.getByRole("slider", { name: "Volume Source video" });
    const controls = within(slider.closest<HTMLElement>(".editor-audio-controls")!);
    fireEvent.pointerDown(slider, { pointerId: 1, button: 0, clientY: y(-12) });
    expect(document.activeElement).toBe(slider);
    expect(f.controller.getSnapshot().scene!.playhead.ticks).toBe("5");
    expect(f.gainRequests).toHaveLength(0);
    fireEvent.pointerUp(slider, { pointerId: 1 });
    fireEvent.lostPointerCapture(slider, { pointerId: 1 });
    await waitFor(() =>
      expect(f.controller.getSnapshot().scene!.audioGain?.phase).toBe("saved"),
    );
    expect(f.gainRequests).toHaveLength(1);
    fireEvent.click(controls.getByRole("button", { name: "Mute Source video" }));
    await controls.findByRole("button", { name: "Unmute Source video" });
    expect(slider.getAttribute("aria-valuetext")).toBe("-12.0 dB");
    fireEvent.click(
      controls.getByRole("button", { name: "Unmute Source video" }),
    );
    await controls.findByRole("button", { name: "Mute Source video" });
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
        <Product />
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
      expect(f.controller.getSnapshot().scene!.gainDraft!.tenths).toBe(value);
      const count = f.gainRequests.length;
      fireEvent.keyUp(slider, { key });
      await waitFor(() => expect(f.gainRequests).toHaveLength(count + 1));
      await waitFor(() =>
        expect(f.controller.getSnapshot().scene!.audioGain?.phase).toBe("saved"),
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
    await f.controller.scene.inspectRevision("r0");
    render(
      <WorkspaceProvider controller={f.controller}>
        <Product />
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
    f.current().revision = "unknown-audio-head";
    await act(async () => {
      f.controller.scene.returnToHead();
      await f.controller.scene.refresh();
    });
    slider = screen.getByRole("slider", { name: "Volume Source video" });
    expect(slider.getAttribute("aria-disabled")).toBe("true");
    expect(f.gainRequests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
