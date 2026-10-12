// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../workspace/context";
import { TasksPanel } from "./panel";
import { setupAudioWav } from "../workspace/v4/audio-wav-fixture";
import { png, setupFrame } from "../workspace/v4/frame-fixture";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("labels a V4 still frame distinctly and downloads its published PNG through the existing artifact port", async () => {
  const f = await setupFrame();
  try {
    await f.controller.v4.frame.start();
    f.complete();
    await f.controller.sync();
    const downloaded: number[][] = [];
    const picker = vi.fn().mockResolvedValue({
      createWritable: async () =>
        new WritableStream<Uint8Array>({
          write(chunk) {
            downloaded.push([...chunk]);
          },
        }),
    });
    Object.defineProperty(window, "showSaveFilePicker", {
      configurable: true,
      value: picker,
    });
    render(
      <WorkspaceProvider controller={f.controller}>
        <TasksPanel />
      </WorkspaceProvider>,
    );
    expect(screen.getByText("Scene frame")).toBeTruthy();
    expect(screen.queryByText("Export")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add to library" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    await waitFor(() => expect(downloaded).toEqual([[...png]]));
    expect(f.download).toHaveBeenCalledWith(f.rows[0].artifacts[0]);
    expect(picker.mock.calls[0][0].suggestedName).toMatch(/\.png$/);
  } finally {
    delete (window as Window & { showSaveFilePicker?: unknown })
      .showSaveFilePicker;
    f.controller.dispose();
  }
});
it("reuses durable cancellation and authenticated WAV downloads while hiding the legacy import recovery action in V4", async () => {
  const { controller, api, rows } = await setupAudioWav();
  try {
    await controller.v4.audioWav.start(48000);
    const wav = rows[0];
    rows.push({
      ...structuredClone(wav),
      job_id: "imported-media",
      kind: "media.import",
      state: "succeeded",
      resource_target: {
        project_id: "v4-project",
        resource_kind: "upload",
        resource_id: "upload",
      },
    });
    await controller.sync();
    const cancel = vi
      .spyOn(api.sdk, "cancelPublicJob")
      .mockImplementation(async () => {
        wav.cancel_requested = true;
        return structuredClone(wav);
      });
    render(
      <WorkspaceProvider controller={controller}>
        <TasksPanel />
      </WorkspaceProvider>,
    );
    expect(screen.getByText("Audio WAV export")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add to library" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(cancel).toHaveBeenCalledWith(wav.job_id));
    const cancelling = await screen.findByRole("button", {
      name: "Cancelling…",
    });
    expect((cancelling as HTMLButtonElement).disabled).toBe(true);
    expect(wav.state).toBe("queued");
    const artifact = {
      artifact_id: "artifact_" + "a".repeat(32),
      job_id: wav.job_id,
      content_hash: "b".repeat(64),
      size_bytes: "3",
      mime_type: "audio/wav",
      role: "primary" as const,
      availability: "unchecked" as const,
    };
    wav.state = "succeeded";
    wav.artifacts = [artifact];
    const downloaded: number[][] = [];
    const picker = vi.fn().mockResolvedValue({
      createWritable: async () =>
        new WritableStream<Uint8Array>({
          write(chunk) {
            downloaded.push([...chunk]);
          },
        }),
    });
    Object.defineProperty(window, "showSaveFilePicker", {
      configurable: true,
      value: picker,
    });
    const download = vi.spyOn(api.sdk, "downloadArtifact").mockResolvedValue(
      new Response(new Uint8Array([1, 2, 3]), {
        headers: { "content-type": "audio/wav" },
      }),
    );
    await controller.sync();
    fireEvent.click(await screen.findByRole("button", { name: "Download" }));
    await waitFor(() => expect(downloaded).toEqual([[1, 2, 3]]));
    expect(download).toHaveBeenCalledWith(artifact);
    expect(picker.mock.calls[0][0].suggestedName).toMatch(/\.wav$/);
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    delete (window as Window & { showSaveFilePicker?: unknown })
      .showSaveFilePicker;
  } finally {
    controller.dispose();
  }
});
