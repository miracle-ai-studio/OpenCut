// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { setup } from "./fixture";
import { AudioPreviewControls } from "./audio-preview-controls";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("retires beforeunload and BFCache pagehide and removes both listeners on unmount", async () => {
  const { controller } = await setup();
  try {
    vi.spyOn(controller.scene.audioPreview, "capability").mockResolvedValue(
      undefined,
    );
    const retire = vi
      .spyOn(controller.scene.audioPreview, "retireDocument")
      .mockImplementation(() => {});
    const view = render(
      <WorkspaceProvider controller={controller}>
        <AudioPreviewControls />
      </WorkspaceProvider>,
    );
    window.dispatchEvent(new Event("beforeunload"));
    expect(retire).toHaveBeenCalledOnce();
    window.dispatchEvent(
      new PageTransitionEvent("pagehide", { persisted: true }),
    );
    expect(retire).toHaveBeenCalledTimes(2);
    view.unmount();
    window.dispatchEvent(new Event("beforeunload"));
    window.dispatchEvent(new PageTransitionEvent("pagehide"));
    expect(retire).toHaveBeenCalledTimes(2);
  } finally {
    controller.dispose();
  }
});
