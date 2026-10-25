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
import { SourceAudioControls } from "./source-audio-controls";
import { setupSourceAudio } from "./source-audio-fixture";
import { OpenCutApiError } from "../../../lib/editor-sdk";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("offers source audio actions only for videos with available audio, explaining locked, unknown, absent and historical states", async () => {
  const { controller, current } = await setupSourceAudio();
  try {
    render(
      <WorkspaceProvider controller={controller}>
        <SourceAudioControls />
      </WorkspaceProvider>,
    );
    const separate = screen.getByRole("button", {
      name: "Separate source audio",
    }) as HTMLButtonElement;
    const restore = screen.getByRole("button", {
      name: "Restore source audio",
    }) as HTMLButtonElement;
    expect(separate.disabled).toBe(false);
    expect(restore.disabled).toBe(true);
    for (const status of ["unknown", "absent"] as const) {
      await act(async () => {
        current().project.assets[0].metadata.audio = { status };
        await controller.scene.refresh();
      });
      expect(separate.disabled).toBe(true);
      expect(restore.disabled).toBe(true);
      expect(
        screen.getByText(
          status === "unknown"
            ? "Audio presence must be checked before separating or restoring source audio."
            : "This video has no source audio.",
        ),
      ).toBeTruthy();
    }
    await act(async () => {
      current().project.assets[0].metadata.audio = { status: "present" };
      current().project.sequences[0].tracks[0].locked = true;
      await controller.scene.refresh();
    });
    expect(screen.getByText("The source video track is locked.")).toBeTruthy();
    expect(separate.disabled).toBe(true);
    await act(async () => {
      current().project.sequences[0].tracks[0].locked = false;
      await controller.scene.refresh();
      await controller.scene.inspectRevision("r0");
      controller.scene.select("video");
    });
    expect(
      screen.getByText("Return to the current revision before editing."),
    ).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Separate source audio",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    act(() => {
      controller.scene.returnToHead();
      controller.scene.select("track");
    });
    expect(
      screen.queryByRole("button", { name: "Separate source audio" }),
    ).toBeNull();
  } finally {
    controller.dispose();
  }
});
it("uses real product controls for Separate and Restore while explaining retained detached audio", async () => {
  const { controller, current } = await setupSourceAudio();
  try {
    render(
      <WorkspaceProvider controller={controller}>
        <SourceAudioControls />
      </WorkspaceProvider>,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Separate source audio" }),
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Restore source audio",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    expect(
      screen.getByText(
        "Restoring source audio keeps separated audio in the scene. Both may play together.",
      ),
    ).toBeTruthy();
    expect(current().project.sequences[0].tracks[1].elements).toHaveLength(1);
    fireEvent.click(
      screen.getByRole("button", { name: "Restore source audio" }),
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Separate source audio",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    expect(current().project.sequences[0].tracks[1].elements).toHaveLength(1);
  } finally {
    controller.dispose();
  }
});
it("shows revoked edit permission and does not hide it behind a success or playable-audio claim", async () => {
  const { controller, api } = await setupSourceAudio();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_apply")
        throw new OpenCutApiError(
          "PERMISSION_DENIED",
          "Editing permission was revoked.",
          null,
          403,
          "req",
          [],
        );
      return previous(name, input, options);
    });
    render(
      <WorkspaceProvider controller={controller}>
        <SourceAudioControls />
      </WorkspaceProvider>,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Separate source audio" }),
    );
    await screen.findByText("Editing permission was revoked.");
    expect(screen.getByRole("alert").textContent).toContain("not saved");
    expect(screen.queryByText("Source audio changes saved.")).toBeNull();
    expect(
      (
        screen.getByRole("button", {
          name: "Separate source audio",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      screen.getByRole("button", { name: "Retry operation" }),
    ).toBeTruthy();
  } finally {
    controller.dispose();
  }
});
