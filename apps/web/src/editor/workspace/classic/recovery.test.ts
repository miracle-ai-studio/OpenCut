import { expect, it, vi } from "vitest";
import { OpenCutClient } from "../../../lib/editor-sdk";
import { cleanupRecovery, prepareRecovery } from "./recovery";
import { fileDigest, legacyId } from "./identity";

it.each(["metadata", "duration", "kind", "uri"])(
  "preserves a preparation library with user-edited %s instead of deleting it",
  async (field) => {
    const signal = new AbortController().signal;
    const file = new File(["synthetic-original"], "source.png"),
      sha256 = await fileDigest(file, signal);
    const prepared = await prepareRecovery(
      {
        origin: "http://localhost:3000",
        sourceId: "old",
        raw: {
          id: "old",
          name: "Old",
          version: 1,
          fps: 24,
          canvasSize: { width: 320, height: 180 },
          scenes: [
            {
              id: "main",
              name: "Main",
              isMain: true,
              tracks: [],
              bookmarks: [],
            },
          ],
        },
        context: { legacyTracksBySceneId: {}, mediaTypesById: {} },
        media: [{ id: "m", name: file.name, type: "image", size: file.size }],
      },
      [{ id: "m", file, sha256 }],
      signal,
      () => {},
    );
    const original = {
      id: "native",
      kind: "image",
      name: file.name,
      uri: `opencut-media://media_${"1".repeat(32)}`,
      metadata: {},
      version: { content_hash: sha256, size: file.size },
    };
    const asset = {
      ...original,
      id: legacyId("media", [prepared.targetId, "m"]),
    };
    Object.assign(asset, {
      [field]:
        field === "metadata"
          ? { user_note: "Keep this" }
          : field === "duration"
            ? { ticks: "1", timebase: 1 }
            : field === "kind"
              ? "video"
              : "other://binding",
    });
    const stage = {
      revision: "edited",
      project: { id: prepared.stageId, name: "preparing", assets: [asset] },
    };
    const before = structuredClone(stage),
      sdk = new OpenCutClient({ baseUrl: "http://localhost", token: "" });
    const call = vi.spyOn(sdk, "call").mockImplementation(async (name) => {
      if (name === "opencut_project_create")
        return {
          revision: "initial",
          project: { ...stage.project, assets: [] },
        } as never;
      if (name === "opencut_project_inspect") return stage as never;
      if (name === "opencut_project_media_inspect")
        return { asset: original } as never;
      throw new Error(`Unexpected mutation: ${name}`);
    });
    expect(await cleanupRecovery(sdk, prepared)).toContain("library changed");
    expect(call.mock.calls.map(([name]) => name)).not.toContain(
      "opencut_project_delete",
    );
    expect(stage).toEqual(before);
  },
);
