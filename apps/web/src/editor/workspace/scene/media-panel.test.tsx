// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { MediaPanel } from "./media-panel";
import { ResourcePreview } from "./resource-preview";
import { MediaPlanDialog } from "./media-dialog";
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
it("imports with one server plan and applies it directly without a review dialog", async () => {
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
        <MediaPanel />
      </WorkspaceProvider>,
    );
    fireEvent.change(screen.getByLabelText("Import media files"), {
      target: { files: [new File(["tiny"], "image.png")] },
    });
    await screen.findByRole("button", { name: "Place Imported image" });
    expect(f.requests).toHaveLength(1);
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
    ).toHaveLength(1);
    expect(
      f.call.mock.calls.find(([name]) => name === "opencut_edit_apply")![1],
    ).toEqual(f.plans[0].transaction);
    expect(f.controller.getSnapshot().scene!.media.attempt).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
it("places at the exact playhead directly and preserves untouched rational duration", async () => {
  const f = await setupMedia();
  try {
    const asset = f.current().project.assets[0];
    asset.duration = { ticks: "2", timebase: 3 };
    asset.kind = "video";
    asset.metadata.audio = { status: "absent" };
    await f.controller.scene.refresh();
    f.controller.scene.seek({ ticks: "1", timebase: 3 });
    render(
      <WorkspaceProvider controller={f.controller}>
        <MediaPanel />
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Place Image" }));
    await waitFor(() =>
      expect(
        f.call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
      ).toHaveLength(1),
    );
    expect(f.requests[0].placements[0]).toMatchObject({
      start: { ticks: "1", timebase: 3 },
      content: {
        kind: "video",
        source_audio_enabled: false,
        timing: {
          range: { duration: { ticks: "2", timebase: 3 } },
          rate: { numerator: 1, denominator: 1 },
        },
      },
    });
    await waitFor(() =>
      expect(f.controller.getSnapshot().scene!.media.attempt).toBeNull(),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
it("optional placement settings save directly, preserving speed and a new track", async () => {
  const f = await setupMedia();
  try {
    const asset = f.current().project.assets[0];
    asset.duration = { ticks: "2", timebase: 3 };
    asset.kind = "video";
    asset.metadata.audio = { status: "absent" };
    await f.controller.scene.refresh();
    render(
      <WorkspaceProvider controller={f.controller}>
        <MediaPanel />
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "More actions for Image" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Placement settings" }));
    const form = screen.getByRole("dialog", { name: "Place media" });
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
      within(form).getByRole("button", { name: "Add to timeline" }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(f.requests[0].new_tracks?.[0]).toMatchObject({
      name: "Second video",
      id: f.requests[0].placements[0].track_id,
    });
    expect(f.requests[0].placements[0].content).toMatchObject({
      timing: { rate: { numerator: 3, denominator: 2 } },
    });
  } finally {
    f.controller.dispose();
  }
});
it("a refused placement keeps exact requested timing visible without inventing a resolved plan", async () => {
  const f = await setupMedia();
  try {
    vi.spyOn(f.api.scene, "mediaPlan").mockRejectedValueOnce(
      new Error("Track collision"),
    );
    await expect(
      f.controller.scene.media.place({
        asset_id: "asset",
        name: "Fractional image",
        track_id: "track",
        start: { ticks: "63", timebase: 50 },
        content: { kind: "image", duration: { ticks: "101", timebase: 100 } },
      }),
    ).rejects.toThrow("Track collision");
    render(
      <WorkspaceProvider controller={f.controller}>
        <MediaPlanDialog onClose={() => {}} />
      </WorkspaceProvider>,
    );
    const placementReview = screen.getByRole("region", {
      name: "Placement of Fractional image",
    });
    expect(
      within(placementReview).getByText("Requested position: 00:01.26"),
    ).toBeTruthy();
    expect(within(placementReview).queryByText(/Resolved position/)).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Apply media changes" }),
    ).toBeNull();
    expect(screen.getByRole("alert").textContent).toContain("Track collision");
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
      .spyOn(f.api.scene, "resource")
      .mockResolvedValue(resource);
    const url = vi
      .spyOn(f.api.sdk, "projectResourceUrl")
      .mockReturnValue(`${location.origin}/verified-resource`);
    const rendered = render(
      <WorkspaceProvider controller={f.controller}>
        <ResourcePreview
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
        <ResourcePreview
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

it("keeps an uncertain direct save and retries only its original transaction", async () => {
  const f = await setupMedia();
  try {
    const base = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let lose = true;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_edit_apply") {
          const receipt = f.commit(
            input as (typeof f.plans)[number]["transaction"],
          );
          if (lose) {
            lose = false;
            throw Error("Connection lost after commit");
          }
          return receipt as never;
        }
        return base(name, input, options);
      },
    );
    render(
      <WorkspaceProvider controller={f.controller}>
        <MediaPanel />
      </WorkspaceProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Place Image" }));
    const recovery = await screen.findByRole("dialog", {
      name: "Finish saving media",
    });
    expect(f.controller.getSnapshot().scene?.media.attempt?.phase).toBe(
      "unconfirmed",
    );
    expect(
      within(recovery).queryByRole("button", { name: "Cancel this addition" }),
    ).toBeNull();
    fireEvent.click(
      within(recovery).getByRole("button", { name: "Retry original save" }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(f.requests).toHaveLength(1);
    const apply = f.call.mock.calls.filter(
      ([name]) => name === "opencut_edit_apply",
    );
    expect(apply).toHaveLength(2);
    expect(apply[0][1]).toEqual(apply[1][1]);
    expect(f.controller.getSnapshot().scene?.media.attempt).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
