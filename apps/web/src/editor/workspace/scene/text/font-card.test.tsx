// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ProjectFontCard } from "./font-card";
import { textFonts } from "./fixture";
import { preferences } from "../../../shared/locale";
const f = vi.hoisted(() => ({
  load: vi.fn(),
  apply: vi.fn(),
  release: vi.fn(),
  scene: {
    head: { revision: "fixed", project: { id: "project" } },
    sequenceId: "scene",
  },
  api: { connectionIdentity: "principal" },
  reason: null as string | null,
}));
vi.mock("./canvas-font", () => ({
  loadProjectFont: (...args: unknown[]) => f.load(...args),
}));
vi.mock("../../context", () => ({
  useEditingWorkspace: () => ({ api: f.api, scene: f.scene }),
}));
vi.mock("./workspace", () => ({
  useFontApplication: () => ({
    targetName: "Opening title",
    disabledReason: f.reason,
    apply: f.apply,
  }),
}));
vi.mock("../workbench/view", () => ({
  useWorkbenchView: () => ({ showProperties() {} }),
}));
beforeEach(() => {
  preferences({ language: "en" });
  f.reason = null;
  vi.stubGlobal("FontFace", class {});
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
  f.load.mockResolvedValue({ family: "VerifiedFont", release: f.release });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});
it("keeps preview and Escape read-only and makes Shift+Enter an explicit apply action", async () => {
  render(<ProjectFontCard font={textFonts()[0]} active />);
  const card = screen.getByRole("button", {
    name: "Preview font Locked Example · Regular",
  }) as HTMLButtonElement;
  expect(card.disabled).toBe(true);
  expect(screen.queryByText("Aa")).toBeNull();
  await waitFor(() => expect(card.disabled).toBe(false));
  fireEvent.click(card);
  const dialog = screen.getByRole("dialog");
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent(dialog, new Event("cancel", { cancelable: true }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent.keyDown(card, { key: "Enter", shiftKey: true });
  expect(f.apply).toHaveBeenCalledOnce();
});
it("shows a failed load without a fallback specimen, then retries the same scoped resource", async () => {
  f.load.mockRejectedValueOnce(new Error("Font bytes unavailable"));
  const { unmount } = render(<ProjectFontCard font={textFonts()[0]} active />);
  await screen.findByRole("alert");
  expect(screen.queryByText("Aa")).toBeNull();
  expect(screen.queryByRole("button", { name: /Apply.*to/ })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Retry font loading" }));
  await screen.findByText("Aa");
  expect(f.load).toHaveBeenCalledTimes(2);
  expect(f.load.mock.calls[0].slice(0, 3)).toEqual(
    f.load.mock.calls[1].slice(0, 3),
  );
  expect(f.apply).not.toHaveBeenCalled();
  unmount();
  expect(f.release).toHaveBeenCalledOnce();
});
it("explains an incompatible target while still allowing read-only font preview", async () => {
  f.reason = "Select one editable text or subtitle clip.";
  render(<ProjectFontCard font={textFonts()[0]} active />);
  const card = screen.getByRole("button", {
    name: "Preview font Locked Example · Regular",
  }) as HTMLButtonElement;
  await waitFor(() => expect(card.disabled).toBe(false));
  fireEvent.keyDown(card, { key: "Enter", shiftKey: true });
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent.click(card);
  expect(screen.getByRole("status").textContent).toBe(f.reason);
  expect(
    (
      screen.getAllByRole("button", {
        name: /Apply.*to/,
      }) as HTMLButtonElement[]
    ).every((button) => button.disabled),
  ).toBe(true);
});
it("keeps the same loaded face when polling returns equivalent font metadata", async () => {
  const { rerender } = render(<ProjectFontCard font={textFonts()[0]} active />);
  await screen.findByText("Aa");
  rerender(<ProjectFontCard font={textFonts()[0]} active />);
  expect(f.load).toHaveBeenCalledOnce();
  expect(f.release).not.toHaveBeenCalled();
});
