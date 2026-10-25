import { renderCapability } from "./fixture";
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
  OpenCutTransportError,
} from "../../../lib/editor-sdk";
import { WorkspaceProvider } from "../context";
import { AudioWavDialog } from "./audio-wav-dialog";
import { setupAudioWav } from "./audio-wav-fixture";
import type { AudioWavRequest } from "./types";
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
    setCapability(({...renderCapability(),audio: {available:true, unavailable_reason:null},sample_rates:[44100]}));
    await controller.scene.audioWav.refreshCapability();
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
      setCapability(({...renderCapability(),audio: {available:false, unavailable_reason:"The audio worker is unavailable."},sample_rates:[]}));
      await controller.scene.audioWav.refreshCapability();
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
    const cancel = vi.spyOn(api.sdk, "cancelJob"),
      submissions: AudioWavRequest[] = [];
    let first = true;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_render_submit") {
        const request = structuredClone((input as { request: AudioWavRequest }).request);
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
      "Your export continues even if you close this window.",
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
      "Your export continues even if you close this window.",
    );
    await act(async () => {
      current().revision = "later-head";
      current().project.sequences[0].name = "Renamed current scene";
      await controller.scene.refresh();
    });
    expect(screen.getByText("Scene: Main Scene")).toBeTruthy();
    expect(requests[0].revision).toBe("r0");
    expect(
      (
        screen.getByRole("button", {
          name: "Start audio export",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "New audio export" }));
    expect(screen.getByText("Scene: Renamed current scene")).toBeTruthy();
    expect(screen.queryByText(/Saved revision:/)).toBeNull();
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
it("time range fields validate before submission and convert seconds to sample boundaries", async () => {
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
    fireEvent.change(screen.getByLabelText("Start time (seconds)"), {
      target: { value: "bad" },
    });
    fireEvent.change(screen.getByLabelText("End time (seconds)"), {
      target: { value: "1.75" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Start audio export" }));
    await screen.findByText(
      "Enter start and end times in seconds.",
    );
    expect(rangeRequests).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("Start time (seconds)"), {
      target: { value: "0.5" },
    });
    expect(screen.getByText("1.25 seconds")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Start audio export" }));
    await screen.findByText(
      "Your export continues even if you close this window.",
    );
    expect(rangeRequests[0].range).toEqual({
      start_sample: "24000",
      frame_count: "60000",
    });
    expect(requests).toHaveLength(0);
    expect(
      (screen.getByLabelText("Export scope") as HTMLSelectElement).value,
    ).toBe("range");
    expect(
      (screen.getByLabelText("Start time (seconds)") as HTMLInputElement)
        .disabled,
    ).toBe(true);
    await waitFor(() =>
      expect(controller.getSnapshot().jobs[0].kind).toBe(
        "render.audio",
      ),
    );
  } finally {
    controller.dispose();
  }
});
