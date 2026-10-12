import { afterEach, expect, it, vi } from "vitest";
import { setupMedia } from "./media-fixture";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import * as upload from "../upload";
afterEach(() => vi.restoreAllMocks());
const fileTarget = {
  scope: { projectId: "v4-project", revision: "r0", sequenceId: "scene" },
  trackId: "track",
  at: { ticks: "1", timebase: 3 },
};

it("file placement captures exact track/time and binds the entire ordered batch in one reviewed transaction", async () => {
  const f = await setupMedia();
  try {
    const uploaded = vi
      .spyOn(upload, "uploadMedia")
      .mockImplementation(async (_sdk, _project, file) => ({
        asset: { ...f.observation.asset, name: file.name },
        media_id: file.name,
        version: f.observation.asset.version!,
        media: f.observation,
        warning: null,
      }));
    const target = structuredClone(fileTarget);
    const pending = f.controller.v4.media.importFiles(
      [new File(["a"], "a.png"), new File(["b"], "b.png")],
      undefined,
      target,
    );
    target.trackId = "changed";
    target.at.ticks = "90";
    await pending;
    expect(uploaded).toHaveBeenCalledTimes(2);
    expect(f.requests).toHaveLength(1);
    expect(f.requests[0].bindings).toHaveLength(2);
    expect(
      f.requests[0].placements.map((p) => [p.track_id, p.start, p.content]),
    ).toEqual([
      [
        "track",
        { ticks: "1", timebase: 3 },
        { kind: "image", duration: { ticks: "5", timebase: 1 } },
      ],
      [
        "track",
        { ticks: "16", timebase: 3 },
        { kind: "image", duration: { ticks: "5", timebase: 1 } },
      ],
    ]);
    expect(f.requests[0].placements.map((p) => p.asset_id)).toEqual(
      f.requests[0].bindings.map((b) => b.asset_id),
    );
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_v4_apply"),
    ).toHaveLength(0);
    await f.controller.v4.media.apply();
    expect(
      f.call.mock.calls
        .filter(([name]) => name === "opencut_edit_v4_apply")
        .map((call) => call[1]),
    ).toEqual([f.plans[0].transaction]);
  } finally {
    f.controller.dispose();
  }
});

it("incomplete or cancelled file batches retain no retryable partial content intent", async () => {
  for (const mode of ["failed", "cancelled"]) {
    const f = await setupMedia();
    try {
      const uploaded = vi
        .spyOn(upload, "uploadMedia")
        .mockImplementation(async (_sdk, _project, file) => {
          if (file.name === "b.png") {
            if (mode === "failed") throw new Error("Upload failed");
            f.controller.v4.media.cancelUpload();
          }
          return {
            asset: f.observation.asset,
            media_id: file.name,
            version: f.observation.asset.version!,
            media: f.observation,
            warning: null,
          };
        });
      await expect(
        f.controller.v4.media.importFiles(
          [new File(["a"], "a.png"), new File(["b"], "b.png")],
          undefined,
          fileTarget,
        ),
      ).rejects.toThrow();
      expect(uploaded).toHaveBeenCalledTimes(2);
      expect(f.requests).toHaveLength(0);
      expect(
        f.controller.getSnapshot().v4!.media.attempt!.request.bindings,
      ).toEqual([]);
      await expect(f.controller.v4.media.retryPlan()).rejects.toThrow(
        "ready to plan",
      );
      expect(
        f.call.mock.calls.some(([name]) => name === "opencut_edit_v4_apply"),
      ).toBe(false);
    } finally {
      f.controller.dispose();
      vi.restoreAllMocks();
    }
  }
});

it("stale or locked file targets refuse before upload and an ongoing replacement cannot be retargeted", async () => {
  const f = await setupMedia();
  try {
    const uploaded = vi.spyOn(upload, "uploadMedia");
    await expect(
      f.controller.v4.media.importFiles([new File(["a"], "a.png")], undefined, {
        ...fileTarget,
        scope: { ...fileTarget.scope, revision: "old" },
      }),
    ).rejects.toThrow("target changed");
    f.current().project.sequences[0].tracks[0].locked = true;
    await f.controller.v4.refresh();
    await expect(
      f.controller.v4.media.importFiles(
        [new File(["a"], "a.png")],
        undefined,
        fileTarget,
      ),
    ).rejects.toThrow("unlocked");
    expect(uploaded).not.toHaveBeenCalled();
    f.current().project.sequences[0].tracks[0].locked = false;
    await f.controller.v4.refresh();
    let release!: () => void;
    uploaded.mockImplementationOnce(
      () =>
        new Promise((done) => {
          release = () =>
            done({
              asset: f.observation.asset,
              media_id: f.observation.media_id,
              version: f.observation.asset.version!,
              media: f.observation,
              warning: null,
            });
        }),
    );
    const replacement = f.controller.v4.media.importFiles(
      [new File(["a"], "a.png")],
      { id: "asset", kind: "image" },
    );
    await expect(
      f.controller.v4.media.importFiles(
        [new File(["b"], "b.png")],
        undefined,
        fileTarget,
      ),
    ).rejects.toThrow("current media");
    release();
    await replacement;
    expect(f.requests[0].bindings[0]).toMatchObject({
      asset_id: "asset",
      mode: "replace",
    });
    expect(f.requests[0].placements).toEqual([]);
  } finally {
    f.controller.dispose();
  }
});
const placement = {
  asset_id: "asset",
  name: "Image again",
  track_id: "track",
  start: { ticks: "1", timebase: 3 },
  content: { kind: "image" as const, duration: { ticks: "2", timebase: 3 } },
};
it("a complete upload batch retries transient planning with the original times and IDs without uploading again", async () => {
  const f = await setupMedia();
  try {
    const uploaded = vi
      .spyOn(upload, "uploadMedia")
      .mockImplementation(async (_sdk, _project, file) => ({
        asset: {
          ...f.observation.asset,
          kind: "audio",
          duration: { ticks: "48480", timebase: 48000 },
          name: file.name,
        },
        media_id: file.name,
        version: f.observation.asset.version!,
        media: f.observation,
        warning: null,
      }));
    const failedRequests: unknown[] = [];
    vi.spyOn(f.api.v4, "mediaPlan").mockImplementationOnce(async (request) => {
      failedRequests.push(structuredClone(request));
      throw new Error("Temporary plan failure");
    });
    await expect(
      f.controller.v4.media.importFiles(
        [new File(["a"], "a.wav"), new File(["b"], "b.wav")],
        undefined,
        { ...fileTarget, at: { ticks: "63", timebase: 50 } },
      ),
    ).rejects.toThrow("Temporary plan failure");
    const captured = structuredClone(
      f.controller.getSnapshot().v4!.media.attempt!.request,
    );
    expect(captured.placements.map((item) => item.start)).toEqual([
      { ticks: "63", timebase: 50 },
      { ticks: "227", timebase: 100 },
    ]);
    await f.controller.v4.media.retryPlan();
    expect(failedRequests).toEqual([captured]);
    expect(f.requests).toEqual([captured]);
    expect(uploaded).toHaveBeenCalledTimes(2);
    await f.controller.v4.media.apply();
    expect(
      f.call.mock.calls
        .filter(([name]) => name === "opencut_edit_v4_apply")
        .map((call) => call[1]),
    ).toEqual([f.plans[0].transaction]);
  } finally {
    f.controller.dispose();
  }
});
it("a late available-media query cannot overwrite a newer completed-upload observation", async () => {
  const f = await setupMedia();
  try {
    let resolve!: (value: (typeof f.observation)[]) => void;
    vi.spyOn(f.api.v4, "availableMedia").mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const stale = f.controller.v4.media.refreshAvailable();
    await f.controller.v4.media.refreshAvailable();
    expect(f.controller.getSnapshot().v4!.media.available).toEqual([
      f.observation,
    ]);
    resolve([]);
    await stale;
    expect(f.controller.getSnapshot().v4!.media.available).toEqual([
      f.observation,
    ]);
    expect(f.controller.getSnapshot().v4!.media.loading).toBe(false);
  } finally {
    f.controller.dispose();
  }
});
it("recovers the observed media ID and version, requires confirmation and applies the server transaction unchanged", async () => {
  const f = await setupMedia();
  try {
    await f.controller.v4.media.refreshAvailable();
    await f.controller.v4.media.recover(f.observation);
    expect(f.requests[0].bindings[0]).toMatchObject({
      media_id: "observed-media",
      version: f.observation.asset.version,
    });
    expect(f.requests[0].bindings[0].asset_id).not.toBe(f.observation.asset.id);
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_edit_v4_apply"),
    ).toHaveLength(0);
    await f.controller.v4.media.apply();
    expect(
      f.call.mock.calls.find(([name]) => name === "opencut_edit_v4_apply")![1],
    ).toEqual(f.plans[0].transaction);
    expect(f.controller.getSnapshot().v4!.head.revision).toBe(
      f.current().revision,
    );
    expect(f.controller.getSnapshot().snapshot).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
it("places an existing asset with no new binding and captures new-track and element IDs once", async () => {
  const f = await setupMedia();
  try {
    const original = structuredClone(
      f.current().project.sequences[0].tracks[0].elements[0],
    );
    await f.controller.v4.media.place(placement, {
      kind: "graphic",
      name: "Picture layer",
    });
    const request = f.requests[0];
    expect(request.bindings).toEqual([]);
    expect(request.placements[0].start).toEqual({ ticks: "1", timebase: 3 });
    expect(request.placements[0].content).toEqual(placement.content);
    expect(request.new_tracks[0].id).toBe(request.placements[0].track_id);
    await f.controller.v4.media.apply();
    expect(f.current().project.sequences[0].tracks[0].elements[0]).toEqual(
      original,
    );
    expect(f.current().project.sequences[0].tracks[1].elements[0].id).toBe(
      request.placements[0].element_id,
    );
    expect(f.requests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("keeps unknown commit identity across reopen, permissions denial and identical receipt replay", async () => {
  const f = await setupMedia();
  try {
    await f.controller.v4.media.place(placement);
    const base = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let loss = true,
      denied = false;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_edit_v4_apply") {
          if (denied)
            throw new OpenCutApiError(
              "FORBIDDEN",
              "Revoked",
              null,
              403,
              "r",
              [],
            );
          const receipt = f.commit(
            input as (typeof f.plans)[number]["transaction"],
          );
          if (loss) {
            loss = false;
            throw new Error("Connection lost after commit");
          }
          return receipt as never;
        }
        return base(name, input, options);
      },
    );
    await expect(f.controller.v4.media.apply()).rejects.toThrow(
      "Connection lost",
    );
    expect(f.controller.getSnapshot().v4!.media.attempt!.phase).toBe(
      "unconfirmed",
    );
    expect(() => f.controller.v4.media.dismiss()).toThrow("original");
    await f.controller.open(f.ref());
    denied = true;
    await expect(f.controller.v4.media.apply()).rejects.toThrow("Revoked");
    expect(f.controller.getSnapshot().v4!.media.attempt!.phase).toBe(
      "unconfirmed",
    );
    denied = false;
    await f.controller.v4.media.apply();
    const calls = f.call.mock.calls.filter(
      ([name]) => name === "opencut_edit_v4_apply",
    );
    expect(calls).toHaveLength(3);
    for (const call of calls) expect(call[1]).toEqual(f.plans[0].transaction);
    expect(f.requests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("rejects a delayed plan after entering history and a stale placement form without submitting", async () => {
  const f = await setupMedia();
  try {
    let resolve!: (value: ReturnType<typeof f.planFor>) => void;
    vi.spyOn(f.api.v4, "mediaPlan").mockImplementationOnce(
      (request) =>
        new Promise((done) => {
          resolve = (value) => done(value);
          f.requests.push(request);
        }),
    );
    const pending = f.controller.v4.media.place(placement);
    await vi.waitFor(() => expect(resolve).toBeTruthy());
    await f.controller.v4.inspectRevision("r0");
    resolve(f.planFor(f.requests[0]));
    await expect(pending).rejects.toThrow("target changed");
    expect(
      f.call.mock.calls.some(([name]) => name === "opencut_edit_v4_apply"),
    ).toBe(false);
    await expect(
      f.controller.v4.media.place(placement, undefined, {
        projectId: f.current().project.id,
        revision: "stale",
        sequenceId: "scene",
      }),
    ).rejects.toThrow("target changed");
    expect(f.requests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("rejects known no-commit replay conflicts and releases only that unresolved command", async () => {
  const f = await setupMedia();
  try {
    await f.controller.v4.media.place(placement);
    const base = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let first = true;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_edit_v4_apply") {
          if (first) {
            first = false;
            throw new Error("Unknown acceptance");
          }
          throw new OpenCutApiError(
            "REVISION_CONFLICT",
            "Changed",
            null,
            403,
            "r",
            [],
          );
        }
        return base(name, input, options);
      },
    );
    await expect(f.controller.v4.media.apply()).rejects.toThrow("Unknown");
    await expect(f.controller.v4.media.apply()).rejects.toThrow("not saved");
    expect(f.controller.getSnapshot().v4!.media.attempt).toBeNull();
    expect(f.requests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});
it("uploads keep actual observed identity, do not auto-apply, and changed views leave the uploaded media recoverable", async () => {
  const f = await setupMedia();
  try {
    const mocked = vi.spyOn(upload, "uploadMedia").mockResolvedValue({
      asset: f.observation.asset,
      media_id: f.observation.media_id,
      version: f.observation.asset.version!,
      media: f.observation,
      warning: null,
    });
    await f.controller.v4.media.importFiles([new File(["small"], "image.png")]);
    expect(mocked).toHaveBeenCalledOnce();
    expect(f.requests[0].bindings[0].media_id).toBe(f.observation.media_id);
    expect(f.controller.getSnapshot().v4!.media.attempt!.phase).toBe("planned");
    f.controller.v4.media.dismiss();
    mocked.mockImplementationOnce(async () => {
      await f.controller.v4.inspectRevision("r0");
      return {
        asset: f.observation.asset,
        media_id: f.observation.media_id,
        version: f.observation.asset.version!,
        media: f.observation,
        warning: null,
      };
    });
    await expect(
      f.controller.v4.media.importFiles([new File(["small"], "other.png")]),
    ).rejects.toThrow("target changed");
    expect(f.requests).toHaveLength(1);
    expect(f.controller.getSnapshot().v4!.media.available[0].media_id).toBe(
      f.observation.media_id,
    );
    expect(
      f.call.mock.calls.some(([name]) => name === "opencut_edit_v4_apply"),
    ).toBe(false);
  } finally {
    f.controller.dispose();
  }
});
