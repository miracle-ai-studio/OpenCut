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
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { V4MediaPanel } from "./media-panel";
import { V4ResourcePreview } from "./resource-preview";
import { setupMedia } from "./media-fixture";
import * as upload from "../upload";
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
it("uploads then reviews server decisions, keeps a closed plan, and explicitly commits unchanged changes", async () => {
  const f = await setupMedia();
  try {
    vi.spyOn(upload, "uploadMedia").mockResolvedValue({
      asset: f.observation.asset,
      media_id: f.observation.media_id,
      version: f.observation.asset.version!,
      media: f.observation,
      warning: null,
    });
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4MediaPanel />
      </WorkspaceProvider>,
    );
    fireEvent.change(screen.getByLabelText("Import media files"), {
      target: { files: [new File(["tiny"], "image.png")] },
    });
    const dialog = await screen.findByRole("dialog", {
      name: "Review media changes",
    });
    expect(within(dialog).getByText("Frame rate: 60/1 → 24/1")).toBeTruthy();
    expect(
      f.call.mock.calls.some(([name]) => name === "opencut_edit_v4_apply"),
    ).toBe(false);
    fireEvent.click(
      within(dialog).getAllByRole("button", { name: "Close" })[0],
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    const captured = structuredClone(
      f.controller.getSnapshot().v4!.media.attempt!.request,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Review media changes" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Apply media changes" }),
    );
    await screen.findAllByText("Media changes saved.");
    expect(f.requests).toEqual([captured]);
    expect(
      f.call.mock.calls.find(([name]) => name === "opencut_edit_v4_apply")![1],
    ).toEqual(f.plans[0].transaction);
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(f.controller.getSnapshot().v4!.media.attempt).toBeNull();
    expect(
      await screen.findByRole("button", { name: "Place Imported image" }),
    ).toBeTruthy();
  } finally {
    f.controller.dispose();
  }
});
it("placement preserves untouched rational playhead/duration, captures a new track, and displays server placement", async () => {
  const f = await setupMedia();
  try {
    f.current().project.assets[0].duration = { ticks: "2", timebase: 3 };
    f.current().project.assets[0].kind = "video";
    f.current().project.assets[0].metadata.audio = { status: "absent" };
    await f.controller.v4.refresh();
    f.controller.v4.seek({ ticks: "1", timebase: 3 });
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4MediaPanel />
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Place Image" }));
    const form = screen.getByRole("dialog", { name: "Place media" });
    expect(
      (
        within(form).getByRole("checkbox", {
          name: "Include source audio",
        }) as HTMLInputElement
      ).disabled,
    ).toBe(true);
    fireEvent.change(within(form).getByLabelText("Target track"), {
      target: { value: "new" },
    });
    fireEvent.change(within(form).getByLabelText("New track name"), {
      target: { value: "Second video" },
    });
    fireEvent.change(within(form).getByLabelText("Speed numerator"), {
      target: { value: "3" },
    });
    fireEvent.change(within(form).getByLabelText("Speed denominator"), {
      target: { value: "2" },
    });
    fireEvent.click(
      within(form).getByRole("button", { name: "Review placement" }),
    );
    const review = await screen.findByRole("dialog", {
      name: "Review media changes",
    });
    expect(
      within(review).getByText("Placement: 00:00.00 · Duration: 00:02.50"),
    ).toBeTruthy();
    const request = f.requests[0];
    expect(request.bindings).toEqual([]);
    expect(request.placements[0].start).toEqual({ ticks: "1", timebase: 3 });
    expect(request.placements[0].content).toMatchObject({
      kind: "video",
      source_audio_enabled: false,
      timing: {
        range: {
          start: { ticks: "0", timebase: 1 },
          duration: { ticks: "2", timebase: 3 },
        },
        rate: { numerator: 3, denominator: 2 },
      },
    });
    expect(request.new_tracks[0]).toMatchObject({
      kind: "video",
      name: "Second video",
      id: request.placements[0].track_id,
    });
    fireEvent.click(
      within(review).getAllByRole("button", { name: "Close" })[0],
    );
    await act(async () => {
      await f.controller.v4.inspectRevision("r0");
    });
    expect(
      (
        screen.getByRole("button", {
          name: "Import media",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  } finally {
    f.controller.dispose();
  }
});
it("source preview selects fixed revision bytes and rejects a different resource version", async () => {
  const f = await setupMedia();
  try {
    const asset = f.current().project.assets[0],
      resource = {
        project_id: f.current().project.id,
        revision: "r0",
        resource_id: asset.id,
        kind: "asset" as const,
        version: asset.version,
        mime_type: "image/png",
        content_path: "unused",
      };
    const inspected = vi
      .spyOn(f.api.v4, "resource")
      .mockResolvedValue(resource);
    const url = vi
      .spyOn(f.api.sdk, "projectV4ResourceUrl")
      .mockReturnValue(`${location.origin}/verified-resource`);
    const rendered = render(
      <WorkspaceProvider controller={f.controller}>
        <V4ResourcePreview
          asset={asset}
          projectId={f.current().project.id}
          revision="r0"
          onClose={() => {}}
        />
      </WorkspaceProvider>,
    );
    await screen.findByRole("img", { name: "Image" });
    expect(inspected).toHaveBeenCalledWith(
      f.current().project.id,
      "r0",
      asset.id,
    );
    expect(url).toHaveBeenCalledWith(resource);
    expect(screen.getByRole("img").getAttribute("src")).toBe(
      `${location.origin}/verified-resource`,
    );
    rendered.unmount();
    inspected.mockResolvedValue({
      ...resource,
      version: { ...asset.version, content_hash: "c".repeat(64) },
    });
    url.mockClear();
    render(
      <WorkspaceProvider controller={f.controller}>
        <V4ResourcePreview
          asset={asset}
          projectId={f.current().project.id}
          revision="r0"
          onClose={() => {}}
        />
      </WorkspaceProvider>,
    );
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "selected media has changed",
      ),
    );
    expect(url).not.toHaveBeenCalled();
    expect(screen.queryByRole("img")).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
