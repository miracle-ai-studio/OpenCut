// @vitest-environment jsdom
import { useState } from "react";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import {
  OpenCutApiError,
  OpenCutTransportError,
} from "../../../lib/editor-sdk";
import { WorkspaceProvider } from "../context";
import { AudioWavDialog } from "./audio-wav-dialog";
import { setupAudioWav } from "./audio-wav-fixture";
import type { V4AudioWavRequest } from "./types";
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("offers only the host sample rates and displays an actual unavailable reason", async () => {
  const { controller, setCapability } = await setupAudioWav();
  try {
    setCapability({
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_audio_wav",
      sample_rates: [44100],
      unavailable_reason: null,
    });
    await controller.v4.audioWav.refreshCapability();
    render(
      <WorkspaceProvider controller={controller}>
        <AudioWavDialog onClose={vi.fn()} showTasks={vi.fn()} />
      </WorkspaceProvider>,
    );
    expect(
      Array.from(
        screen.getByLabelText("Sample rate").querySelectorAll("option"),
      ).map((o) => o.textContent),
    ).toEqual(["44.1 kHz"]);
    expect(
      (
        screen.getByRole("button", {
          name: "Start audio export",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    await act(async () => {
      setCapability({
        schema_version: 1,
        available: false,
        submit_tool: "opencut_job_v4_audio_wav",
        sample_rates: [],
        unavailable_reason: "The audio worker is unavailable.",
      });
      await controller.v4.audioWav.refreshCapability();
    });
    expect(screen.getByText("The audio worker is unavailable.")).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Start audio export",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  } finally {
    controller.dispose();
  }
});
it("closing and reopening an unknown export retains its original submission instead of cancelling or creating another task", async () => {
  const { controller, api, rows, jobFor } = await setupAudioWav();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    const cancel = vi.spyOn(api.sdk, "cancelPublicJob"),
      submissions: V4AudioWavRequest[] = [];
    let first = true;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_job_v4_audio_wav") {
        const request = structuredClone(input as V4AudioWavRequest);
        submissions.push(request);
        if (first) {
          first = false;
          rows.push(jobFor(request));
          throw new OpenCutTransportError(new Error("Lost response"));
        }
        return structuredClone(rows[0]) as never;
      }
      return previous(name, input, options);
    });
    function Harness() {
      const [open, setOpen] = useState(true);
      return (
        <WorkspaceProvider controller={controller}>
          <button onClick={() => setOpen(true)}>Open export window</button>
          {open && (
            <AudioWavDialog
              onClose={() => setOpen(false)}
              showTasks={vi.fn()}
            />
          )}
        </WorkspaceProvider>
      );
    }
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Start audio export" }));
    await screen.findByText(
      "The audio export submission is not confirmed. Retry the original submission to find its task.",
    );
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(cancel).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Open export window" }));
    expect(
      (
        screen.getByRole("button", {
          name: "Start audio export",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (screen.getByLabelText("Sample rate") as HTMLSelectElement).disabled,
    ).toBe(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Retry original export" }),
    );
    await screen.findByText(
      "Audio WAV task accepted. Progress and downloads are in Tasks.",
    );
    expect(submissions[1]).toEqual(submissions[0]);
    expect(rows).toHaveLength(1);
    await waitFor(() => expect(controller.getSnapshot().jobs).toHaveLength(1));
    expect(controller.getSnapshot().jobs[0].state).toBe("queued");
    expect(screen.queryByText("Export complete")).toBeNull();
  } finally {
    controller.dispose();
  }
});

it("an accepted export keeps its captured scene and revision visible until the user explicitly starts a new export", async () => {
  const { controller, current, requests } = await setupAudioWav();
  try {
    render(
      <WorkspaceProvider controller={controller}>
        <AudioWavDialog onClose={vi.fn()} showTasks={vi.fn()} />
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Start audio export" }));
    await screen.findByText(
      "Audio WAV task accepted. Progress and downloads are in Tasks.",
    );
    await act(async () => {
      current().revision = "later-head";
      current().project.sequences[0].name = "Renamed current scene";
      await controller.v4.refresh();
    });
    expect(screen.getByText("Scene: Main Scene")).toBeTruthy();
    expect(screen.getByText("Saved revision: r0")).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Start audio export",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "New audio export" }));
    expect(screen.getByText("Scene: Renamed current scene")).toBeTruthy();
    expect(screen.getByText("Saved revision: later-head")).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Start audio export",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
    expect(requests).toHaveLength(1);
  } finally {
    controller.dispose();
  }
});
it("sample range fields preserve canonical strings and show local input errors before submitting to the same task feed", async () => {
  const { controller, rangeRequests, requests } = await setupAudioWav();
  try {
    render(
      <WorkspaceProvider controller={controller}>
        <AudioWavDialog onClose={vi.fn()} showTasks={vi.fn()} />
      </WorkspaceProvider>,
    );
    fireEvent.change(screen.getByLabelText("Export scope"), {
      target: { value: "range" },
    });
    fireEvent.change(screen.getByLabelText("Starting sample (0-based)"), {
      target: { value: "01" },
    });
    fireEvent.change(screen.getByLabelText("Frame count"), {
      target: { value: "64000" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Start audio export" }));
    await screen.findByText(
      "Enter a canonical starting sample and a positive frame count.",
    );
    expect(rangeRequests).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("Starting sample (0-based)"), {
      target: { value: "24000" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Start audio export" }));
    await screen.findByText(
      "Audio WAV task accepted. Progress and downloads are in Tasks.",
    );
    expect(rangeRequests[0].range).toEqual({
      start_sample: "24000",
      frame_count: "64000",
    });
    expect(requests).toHaveLength(0);
    expect(
      (screen.getByLabelText("Export scope") as HTMLSelectElement).value,
    ).toBe("range");
    expect(
      (screen.getByLabelText("Starting sample (0-based)") as HTMLInputElement)
        .disabled,
    ).toBe(true);
    await waitFor(() =>
      expect(controller.getSnapshot().jobs[0].kind).toBe(
        "render.project_audio_range.v4",
      ),
    );
  } finally {
    controller.dispose();
  }
});
it("a missing range capability disables only Sample range in the shared export window", async () => {
  const { controller, api } = await setupAudioWav();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_project_v4_audio_range_capability")
        throw new OpenCutApiError(
          "UNKNOWN_TOOL",
          "Unknown range capability tool",
          null,
          404,
          "request",
          [],
        );
      return previous(name, input, options);
    });
    render(
      <WorkspaceProvider controller={controller}>
        <AudioWavDialog onClose={vi.fn()} showTasks={vi.fn()} />
      </WorkspaceProvider>,
    );
    await waitFor(() =>
      expect(controller.getSnapshot().v4!.audioRangeWavCapability).toBeNull(),
    );
    const start = screen.getByRole("button", {
      name: "Start audio export",
    }) as HTMLButtonElement;
    expect(start.disabled).toBe(false);
    fireEvent.change(screen.getByLabelText("Export scope"), {
      target: { value: "range" },
    });
    expect(
      screen.getByText(
        "This service does not support sample range WAV export.",
      ),
    ).toBeTruthy();
    expect(start.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Export scope"), {
      target: { value: "whole" },
    });
    expect(start.disabled).toBe(false);
  } finally {
    controller.dispose();
  }
});
