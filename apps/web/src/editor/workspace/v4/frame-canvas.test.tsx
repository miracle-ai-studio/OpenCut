// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { V4Product } from "./product";
import { setupFrame } from "./frame-fixture";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("renders the saved scene frame in the existing canvas, keeps notes as a separate HUD and removes stale pixels immediately on seek", async () => {
  const f = await setupFrame();
  const create = vi.fn(() => "blob:scene-frame"),
    revoke = vi.fn();
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true,
    value: create,
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true,
    value: revoke,
  });
  try {
    f.current().project.sequences[0].bookmarks = [
      {
        id: "note",
        sequence_id: "scene",
        time: { ticks: "0", timebase: 1 },
        duration: { ticks: "2", timebase: 1 },
        note: "Editor-only note",
      },
    ];
    await f.controller.v4.refresh();
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Render scene frame" }));
    await waitFor(() => expect(f.requests).toHaveLength(1));
    await act(async () => {
      f.complete();
      await f.controller.sync();
    });
    const image = await screen.findByAltText("Rendered scene frame");
    expect(image.getAttribute("src")).toBe("blob:scene-frame");
    expect(
      screen.getByRole("region", { name: "Canvas notes" }).textContent,
    ).toContain("Editor-only note");
    expect(f.requests[0]).not.toHaveProperty("bookmarks");
    await act(async () => f.controller.v4.seek({ ticks: "1", timebase: 3 }));
    expect(screen.queryByAltText("Rendered scene frame")).toBeNull();
    expect(
      screen.getByRole("region", { name: "Canvas notes" }).textContent,
    ).toContain("Editor-only note");
    await waitFor(() =>
      expect(revoke).toHaveBeenCalledWith("blob:scene-frame"),
    );
    expect(screen.queryByRole("button", { name: "Play preview" })).toBeNull();
  } finally {
    cleanup();
    f.controller.dispose();
  }
});

it("disables unavailable services with a clear capability reason while the rest of the editor stays usable", async () => {
  const f = await setupFrame();
  try {
    f.setCapability({
      schema_version: 1,
      available: false,
      submit_tool: "opencut_job_v4_frame_png",
      unavailable_reason: "GPU worker unavailable.",
    });
    await f.controller.v4.frame.refreshCapability();
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4Product />
      </WorkspaceProvider>,
    );
    expect(
      screen
        .getByRole("button", { name: "Render scene frame" })
        .hasAttribute("disabled"),
    ).toBe(true);
    expect(screen.getByText("GPU worker unavailable.")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: "Add bookmark" })
        .hasAttribute("disabled"),
    ).toBe(false);
    expect(f.requests).toHaveLength(0);
  } finally {
    cleanup();
    f.controller.dispose();
  }
});
