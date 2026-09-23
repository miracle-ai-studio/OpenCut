import { afterEach, describe, expect, it, vi } from "vitest";
import { EditorClient } from "../lib/editor-client";
import { isCurrentRenderRevision, mediaAssetLock } from "./render-preflight";

const identity = {
  projectId: "project-a",
  revision: "rev-8",
  sequenceId: "sequence-a",
  generation: 3,
};

describe("render preflight identity", () => {
  it("rejects stale project, revision, sequence, and request responses", () => {
    expect(isCurrentRenderRevision(identity, { ...identity })).toBe(true);
    for (const changed of [
      { projectId: "project-b" },
      { revision: "rev-9" },
      { sequenceId: "sequence-b" },
      { generation: 4 },
    ])
      expect(
        isCurrentRenderRevision(identity, { ...identity, ...changed }),
      ).toBe(false);
  });
});

describe("job media asset locks", () => {
  it("reports only a valid versioned asset lock", () => {
    expect(
      mediaAssetLock({
        snapshot: { revision: "rev-8" },
        sequence_id: "sequence-a",
        asset_lock: {
          version: 1,
          revision: "rev-8",
          sequence_id: "sequence-a",
          assets: [
            { asset_id: "a", content_hash: "a".repeat(64), size: 10 },
            { asset_id: "b", content_hash: "b".repeat(64), size: 20 },
          ],
        },
      }),
    ).toEqual({
      version: 1,
      revision: "rev-8",
      sequenceId: "sequence-a",
      assets: [
        { assetId: "a", contentHash: "a".repeat(64), size: 10 },
        { assetId: "b", contentHash: "b".repeat(64), size: 20 },
      ],
    });
    expect(
      mediaAssetLock({
        asset_lock: {
          version: 2,
          revision: "r",
          sequence_id: "s",
          assets: [],
        },
      }),
    ).toBe(null);
    expect(
      mediaAssetLock({
        asset_lock: {
          version: 1,
          revision: "r",
          sequence_id: "s",
          assets: [{}],
        },
      }),
    ).toBe(null);
  });
  it("rejects malformed hashes, duplicate assets, and snapshot mismatches", () => {
    const input = {
      snapshot: { revision: "rev-8" },
      sequence_id: "sequence-a",
      asset_lock: {
        version: 1,
        revision: "rev-8",
        sequence_id: "sequence-a",
        assets: [{ asset_id: "a", content_hash: "a".repeat(64), size: 10 }],
      },
    };
    expect(
      mediaAssetLock({
        ...input,
        asset_lock: {
          ...input.asset_lock,
          assets: [{ ...input.asset_lock.assets[0], content_hash: "ABC" }],
        },
      }),
    ).toBe(null);
    expect(
      mediaAssetLock({
        ...input,
        asset_lock: {
          ...input.asset_lock,
          assets: [input.asset_lock.assets[0], input.asset_lock.assets[0]],
        },
      }),
    ).toBe(null);
    expect(
      mediaAssetLock({
        ...input,
        snapshot: { revision: "rev-9" },
      }),
    ).toBe(null);
    expect(
      mediaAssetLock({
        ...input,
        sequence_id: "sequence-b",
      }),
    ).toBe(null);
  });
});

describe("render preflight requests", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("cancels a stale HTTP request and allows a fixed-revision retry", async () => {
    const report = {
        revision: "rev-8",
        sequence_id: "sequence-a",
        ready: true,
        issues: [],
      },
      fetchMock = vi
        .fn()
        .mockImplementationOnce(
          (_: string, init?: RequestInit) =>
            new Promise((_, reject) =>
              init?.signal?.addEventListener("abort", () =>
                reject(new DOMException("Aborted", "AbortError")),
              ),
            ),
        )
        .mockResolvedValueOnce(
          Response.json({
            api_version: "v1",
            request_id: "request-2",
            data: report,
            error: null,
            warnings: [],
          }),
        );
    vi.stubGlobal("fetch", fetchMock);
    const client = new EditorClient("http://localhost:8788", "secret"),
      controller = new AbortController(),
      input = {
        project_id: "project-a",
        revision: "rev-8",
        sequence_id: "sequence-a",
        container: "mp4" as const,
        video_codec: "h264" as const,
      },
      stale = client.renderPreflight(input, controller.signal);

    controller.abort();
    await expect(stale).rejects.toMatchObject({ name: "AbortError" });
    await expect(client.renderPreflight(input)).resolves.toEqual(report);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
