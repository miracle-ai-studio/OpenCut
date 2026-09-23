import { describe, expect, it } from "vitest";
import type { Asset } from "../lib/editor-client";
import {
  assetReplaceOperation,
  assetVersionView,
  trackKindForAsset,
} from "./media-assets";

const asset = (id: string, kind: Asset["kind"]): Asset => ({
  id,
  name: id,
  kind,
  uri: `file:///${id}`,
  duration: null,
  metadata: {},
});

describe("asset replacement requests", () => {
  it("keeps the bound id and removes the upload's provisional version", () => {
    const uploaded = {
      ...asset("upload-id", "video"),
      uri: "asset://new-content",
      version: { id: "av_upload", content_hash: "a".repeat(64), size: 42 },
    };
    const operation = assetReplaceOperation(
      asset("bound-id", "video"),
      uploaded,
    );
    expect(operation.type).toBe("asset.replace");
    expect(operation.asset.id).toBe("bound-id");
    expect(operation.asset.uri).toBe("asset://new-content");
    expect(operation.asset.version).toBeUndefined();
  });

  it("rejects cross-kind replacement before submitting", () => {
    expect(() =>
      assetReplaceOperation(asset("bound-id", "audio"), asset("new", "video")),
    ).toThrow("Cannot replace audio media with video media");
  });
});

describe("asset version display", () => {
  it("reads a revision-bound version and treats legacy assets as unresolved", () => {
    expect(assetVersionView(asset("legacy", "image"))).toBe(null);
    expect(
      assetVersionView({
        ...asset("fixed", "image"),
        version: {
          id: `av_${"b".repeat(64)}`,
          content_hash: "b".repeat(64),
          size: 512,
        },
      } as Asset),
    ).toEqual({
      id: `av_${"b".repeat(64)}`,
      contentHash: "b".repeat(64),
      size: 512,
    });
  });
});

describe("import track selection", () => {
  it("routes images to graphic tracks and preserves audio/video kinds", () => {
    expect(trackKindForAsset("image")).toBe("graphic");
    expect(trackKindForAsset("audio")).toBe("audio");
    expect(trackKindForAsset("video")).toBe("video");
  });
});
