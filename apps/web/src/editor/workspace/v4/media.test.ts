import { afterEach, expect, it, vi } from "vitest";
import { setupMedia } from "./media-fixture";
import { OpenCutApiError } from "../../../lib/editor-sdk";
import * as upload from "../upload";
afterEach(() => vi.restoreAllMocks());
const placement = {
  asset_id: "asset",
  name: "Image again",
  track_id: "track",
  start: { ticks: "1", timebase: 3 },
  content: { kind: "image" as const, duration: { ticks: "2", timebase: 3 } },
};
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
    const mocked = vi
      .spyOn(upload, "uploadMedia")
      .mockResolvedValue({
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
