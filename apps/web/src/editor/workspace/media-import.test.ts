import { beforeEach, expect, it, vi } from "vitest";
import { WorkspaceController } from "./controller";
import { EditorApi } from "./api";
import { uploadMedia } from "./upload";
import { projectRef, type Snapshot, type Asset } from "../model/types";
import { fromSeconds } from "../model/time";
vi.mock("./upload", () => ({ uploadMedia: vi.fn() }));
const head = (): Snapshot => ({
  format: "visual",
  revision: "fixed",
  visual: [],
  curves: [],
  project: {
    id: "p",
    name: "p",
    schema_version: 2,
    assets: [],
    default_sequence_id: "s",
    sequences: [
      {
        id: "s",
        name: "s",
        width: 640,
        height: 360,
        fps: { numerator: 24, denominator: 1 },
        caption_cues: [],
        tracks: [
          {
            id: "target",
            kind: "video",
            name: "V",
            enabled: true,
            muted: false,
            locked: false,
            params: {},
            clips: [],
          },
        ],
      },
    ],
  },
});
const asset = (id: string, kind: Asset["kind"] = "video"): Asset => ({
  id,
  name: id,
  uri: `opencut-media:${id}`,
  kind,
  duration: fromSeconds(2),
  metadata: {},
});
// Preserve the legacy behavior fixture while supplying the additive upload
// observation now consumed by the V4 workspace.
const uploaded = (value: Asset): Awaited<ReturnType<typeof uploadMedia>> => ({
  asset: value,
  media_id: value.id,
  version: { id: "av_fixture", content_hash: "a".repeat(64), size: 1 },
  media: {
    project_id: "p",
    media_id: value.id,
    job_id: "import",
    upload_id: "upload",
    asset: value,
  },
  warning: null,
});
beforeEach(() => vi.resetAllMocks());
async function fixture() {
  let current = head();
  const api = new EditorApi("http://localhost:8788", "");
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(current)]);
  vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
  vi.spyOn(api, "inspect").mockImplementation(async () =>
    structuredClone(current),
  );
  vi.spyOn(api, "history").mockResolvedValue({
    project_id: "p",
    head_revision: "fixed",
    revisions: [],
    next_cursor: null,
    navigation: null,
  });
  const apply = vi
    .spyOn(api, "apply")
    .mockResolvedValue({ transaction_id: "t", revision: "next" } as never);
  const controller = new WorkspaceController(() => api);
  await controller.connect("x", "");
  await controller.open(projectRef(current));
  vi.mocked(uploadMedia).mockImplementation(async (_sdk, _project, file) =>
    uploaded(asset(file.name)),
  );
  return {
    api,
    controller,
    apply,
    remote: async () => {
      current.revision = "remote";
      await controller.sync();
    },
  };
}
const files = [new File(["a"], "a.mp4"), new File(["b"], "b.mp4")];
it("imports all bytes then binds/inserts one ordered fixed-revision transaction at the captured track/time", async () => {
  const f = await fixture();
  await f.controller.importMedia(files, vi.fn(), new AbortController().signal, {
    sequenceId: "s",
    trackId: "target",
    at: fromSeconds(5),
  });
  expect(uploadMedia).toHaveBeenCalledTimes(2);
  expect(f.apply).toHaveBeenCalledTimes(1);
  const [snapshot, label, ops] = f.apply.mock.calls[0];
  expect(snapshot.revision).toBe("fixed");
  expect(label).toBe("Import and insert media");
  expect(ops.map((op) => op.type)).toEqual([
    "asset.bind",
    "asset.bind",
    "clip.insert",
    "clip.insert",
  ]);
  expect(
    ops
      .filter((op) => op.type === "clip.insert")
      .map((op) => [op.track_id, op.clip.timeline_range.start]),
  ).toEqual([
    ["target", fromSeconds(5)],
    ["target", fromSeconds(7)],
  ]);
});
it("rejects incompatible files as an entire batch before any content commit", async () => {
  const f = await fixture();
  vi.mocked(uploadMedia)
    .mockResolvedValueOnce(uploaded(asset("a")))
    .mockResolvedValueOnce(uploaded(asset("b", "audio")));
  await expect(
    f.controller.importMedia(files, vi.fn(), new AbortController().signal, {
      sequenceId: "s",
      trackId: "target",
    }),
  ).rejects.toThrow("compatible");
  expect(f.apply).not.toHaveBeenCalled();
});
it("rejects stale revisions, navigation and cancellation without partial content writes", async () => {
  for (const mode of ["revision", "navigation", "cancel"]) {
    const f = await fixture(),
      request = new AbortController();
    vi.mocked(uploadMedia).mockImplementationOnce(async () => {
      if (mode === "revision") await f.remote();
      if (mode === "navigation") f.controller.chooseSequence("s");
      if (mode === "cancel") request.abort();
      return uploaded(asset("a"));
    });
    await expect(
      f.controller.importMedia(files, vi.fn(), request.signal, {
        sequenceId: "s",
        trackId: "target",
      }),
    ).rejects.toThrow();
    expect(f.apply).not.toHaveBeenCalled();
  }
});
it("keeps an ongoing replacement's captured target and refuses concurrent paste/drop imports", async () => {
  const f = await fixture();
  let release!: (value: Awaited<ReturnType<typeof uploadMedia>>) => void;
  vi.mocked(uploadMedia).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  const replacement = f.controller.importMedia(
    files.slice(0, 1),
    vi.fn(),
    new AbortController().signal,
    undefined,
    { id: "old", kind: "video" },
  );
  await expect(
    f.controller.importMedia(files, vi.fn(), new AbortController().signal),
  ).rejects.toThrow("current import");
  release(uploaded(asset("new")));
  await replacement;
  expect(f.apply.mock.calls[0][2]).toEqual([
    { type: "asset.replace", asset: { ...asset("new"), id: "old" } },
  ]);
});
it("sends native resource identities and the captured scene to the shared import planner", async () => {
  const f = await fixture();
  await f.controller.importMedia(
    files.slice(0, 1),
    vi.fn(),
    new AbortController().signal,
  );
  const intent = f.apply.mock.calls[0][6]!;
  expect(intent.sequenceId).toBe("s");
  expect(intent.media).toEqual([{ media_id: "a.mp4", replace_asset_id: null }]);
  expect(() => intent.beforeApply()).not.toThrow();
  f.controller.chooseSequence("s");
  expect(() => intent.beforeApply()).toThrow("target changed");
});
it("cancellation after planning prevents applying the import transaction", async () => {
  const f = await fixture(),
    signal = new AbortController();
  f.apply.mockImplementationOnce(async (...args) => {
    signal.abort();
    args[6]!.beforeApply();
    return { transaction_id: "t", revision: "next" } as never;
  });
  await expect(
    f.controller.importMedia(files.slice(0, 1), vi.fn(), signal.signal),
  ).rejects.toThrow();
});
it("the API lowers import bindings through the versioned native plan and applies its exact transaction", async () => {
  const api = new EditorApi("http://localhost:8788", ""),
    captured = head();
  const transaction = {
    project_id: "p",
    expected_revision: "fixed",
    label: "Import",
    idempotency_key: "key",
    operations: [],
  };
  const call = vi.spyOn(api.sdk, "call").mockImplementation(async (name) => {
    if (name === "opencut_media_import_plan_v1")
      return {
        transaction,
        previous_fps: { numerator: 24, denominator: 1 },
        raised_fps: { numerator: 60, denominator: 1 },
      } as never;
    return { revision: "next", transaction_id: "t" } as never;
  });
  const beforeApply = vi.fn();
  await api.apply(
    captured,
    "Import",
    [
      { type: "asset.bind", asset: asset("v") },
      {
        type: "track.create",
        sequence_id: "s",
        track: captured.project.sequences[0].tracks[0],
      },
    ],
    [],
    "key",
    "s",
    {
      sequenceId: "s",
      media: [{ media_id: "v", replace_asset_id: null }],
      beforeApply,
      ripple: true,
      signal: new AbortController().signal,
    },
  );
  expect(call.mock.calls[0][0]).toBe("opencut_media_import_plan_v1");
  expect(call.mock.calls[0][1]).toMatchObject({
    sequence_id: "s",
    ripple: true,
    media: [{ media_id: "v", replace_asset_id: null }],
    operations: [{ type: "base.apply", operation: { type: "track.create" } }],
  });
  expect(call.mock.calls[1].slice(0, 2)).toEqual([
    "opencut_edit_v3_apply",
    transaction,
  ]);
  expect(beforeApply).toHaveBeenCalledTimes(1);
});
