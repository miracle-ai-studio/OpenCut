// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { setupV4 } from "./fixture";
import { V4AudioPreviewControls } from "./audio-preview-controls";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("retires BFCache pagehide and removes the document listener on unmount", async () => {
  const { controller } = await setupV4();
  try {
    vi.spyOn(controller.v4.audioPreview, "capability").mockResolvedValue(
      undefined,
    );
    const retire = vi
      .spyOn(controller.v4.audioPreview, "retireDocument")
      .mockImplementation(() => {});
    const view = render(
      <WorkspaceProvider controller={controller}>
        <V4AudioPreviewControls />
      </WorkspaceProvider>,
    );
    window.dispatchEvent(
      new PageTransitionEvent("pagehide", { persisted: true }),
    );
    expect(retire).toHaveBeenCalledOnce();
    view.unmount();
    window.dispatchEvent(new PageTransitionEvent("pagehide"));
    expect(retire).toHaveBeenCalledOnce();
  } finally {
    controller.dispose();
  }
});
