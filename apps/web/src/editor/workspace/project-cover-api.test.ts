import { expect, it, vi } from "vitest";
import { EditorApi } from "./api";
import { WorkspaceController } from "./controller";
import { JobCompletionError, OpenCutApiError } from "../../lib/editor-sdk";
import { projectRef, type Snapshot } from "../model/types";
vi.mock("./project-covers", async (original) => ({
  ...(await original<typeof import("./project-covers")>()),
  thumbnail: vi.fn(async () => new Blob(["thumbnail"], { type: "image/png" })),
}));
const snapshot = (format: "base" | "visual" = "visual"): Snapshot => ({
  format,
  revision: "fixed",
  visual: [],
  curves: [],
  project: {
    id: "p",
    name: "P",
    schema_version: 2,
    assets: [],
    default_sequence_id: "main",
    sequences: [
      {
        id: "other",
        name: "Other",
        width: 160,
        height: 90,
        fps: { numerator: 24, denominator: 1 },
        tracks: [],
        caption_cues: [],
      },
      {
        id: "main",
        name: "Main",
        width: 160,
        height: 90,
        fps: { numerator: 24, denominator: 1 },
        tracks: [
          {
            id: "v",
            name: "V",
            kind: "video",
            enabled: true,
            muted: false,
            locked: false,
            params: {},
            clips: [
              {
                id: "c",
                asset_id: "a",
                timeline_range: {
                  start: { ticks: "0", timebase: 1 },
                  duration: { ticks: "1", timebase: 1 },
                },
                source_range: {
                  start: { ticks: "0", timebase: 1 },
                  duration: { ticks: "1", timebase: 1 },
                },
                params: {},
                channels: [],
              },
            ],
          },
        ],
        caption_cues: [],
      },
    ],
  },
});
function fixture(format: "base" | "visual" = "visual") {
  const api = new EditorApi("http://localhost:8788", "test"),
    s = snapshot(format);
  const inspect = vi.spyOn(api, "inspect").mockResolvedValue(s);
  const submit = vi
    .spyOn(api.sdk, "callWithAdmissionRetry")
    .mockResolvedValue({ job_id: "j" } as never);
  vi.spyOn(api.sdk, "waitForPublicJob").mockResolvedValue({
    artifacts: [{ artifact_id: "artifact", mime_type: "image/png" }],
  } as never);
  vi.spyOn(api.sdk, "downloadArtifact").mockImplementation(
    async () =>
      new Response("full frame", { headers: { "Content-Type": "image/png" } }),
  );
  const cancel = vi
    .spyOn(api.sdk, "cancelPublicJob")
    .mockResolvedValue({} as never);
  return { api, s, inspect, submit, cancel };
}
it("requests the exact native main-scene first frame for both formats and never writes content", async () => {
  for (const format of ["base", "visual"] as const) {
    const { api, s, inspect, submit } = fixture(format);
    const signal = new AbortController().signal;
    expect((await api.covers.request(projectRef(s), signal))!.type).toBe(
      "image/png",
    );
    expect(inspect).toHaveBeenCalledWith(
      expect.objectContaining(projectRef(s)),
      "fixed",
      expect.any(AbortSignal),
    );
    const [name, value] = submit.mock.calls[0];
    expect(name).toBe(
      format === "visual" ? "opencut_job_v3_frame" : "opencut_job_submit",
    );
    expect(
      format === "visual" ? value : (value as { input: unknown }).input,
    ).toMatchObject({
      project_id: "p",
      revision: "fixed",
      sequence_id: "main",
      requested_time: { ticks: "0", timebase: 1 },
      alignment: "nearest",
    });
    expect(submit).toHaveBeenCalledTimes(1);
  }
});
it("empty project uses a placeholder, mismatched fixed revision is refused before acceptance", async () => {
  const { api, s, submit } = fixture();
  s.project.sequences[1].tracks = [];
  expect(
    await api.covers.request(projectRef(s), new AbortController().signal),
  ).toBeNull();
  expect(submit).not.toHaveBeenCalled();
  const mismatch = fixture();
  mismatch.inspect.mockResolvedValue({ ...mismatch.s, revision: "wrong" });
  await expect(
    mismatch.api.covers.request(
      projectRef(mismatch.s),
      new AbortController().signal,
    ),
  ).rejects.toThrow("revision changed");
  expect(mismatch.submit).not.toHaveBeenCalled();
});
it("uses the captured last-active scene and falls back to main after that scene is deleted", async () => {
  const { api, s, submit } = fixture();
  s.project.sequences[0].tracks = structuredClone(
    s.project.sequences[1].tracks,
  );
  await api.covers.request(
    projectRef(s),
    new AbortController().signal,
    "other",
  );
  expect(submit.mock.calls[0][1]).toMatchObject({ sequence_id: "other" });
  await api.covers.request(
    projectRef(s),
    new AbortController().signal,
    "deleted",
  );
  expect(submit.mock.calls[1][1]).toMatchObject({ sequence_id: "main" });
});
it("navigation during acceptance cancels the known native job and never downloads its result", async () => {
  const { api, s, submit, cancel } = fixture();
  let resolve!: (value: never) => void;
  submit.mockImplementation(() => new Promise((r) => (resolve = r)));
  const abort = new AbortController(),
    result = api.covers.request(projectRef(s), abort.signal).catch((e) => e);
  await vi.waitFor(() => expect(submit).toHaveBeenCalledTimes(1));
  abort.abort("navigation");
  expect(await result).toBe("navigation");
  resolve({ job_id: "accepted-after-navigation" } as never);
  await vi.waitFor(() =>
    expect(cancel).toHaveBeenCalledWith("accepted-after-navigation"),
  );
  expect(api.sdk.downloadArtifact).not.toHaveBeenCalled();
});
it("quiescing a cover waits for native cancellation terminal state rather than only its request", async () => {
  const { api, s, cancel } = fixture();
  cancel.mockResolvedValue({ state: "running" } as never);
  let rejectTerminal!: (error: unknown) => void;
  vi.mocked(api.sdk.waitForPublicJob)
    .mockImplementationOnce(
      (_id, options) =>
        new Promise((_resolve, reject) =>
          options!.signal!.addEventListener(
            "abort",
            () => reject(options!.signal!.reason),
            { once: true },
          ),
        ),
    )
    .mockImplementationOnce(
      () =>
        new Promise((_resolve, reject) => {
          rejectTerminal = reject;
        }),
    );
  const result = api.covers
    .request(projectRef(s), new AbortController().signal)
    .catch((e) => e);
  await vi.waitFor(() =>
    expect(api.sdk.waitForPublicJob).toHaveBeenCalledTimes(1),
  );
  let done = false;
  const quiet = api.covers.quiesce(["p"]).then(() => {
    done = true;
  });
  await vi.waitFor(() =>
    expect(api.sdk.waitForPublicJob).toHaveBeenCalledTimes(2),
  );
  expect(done).toBe(false);
  rejectTerminal(
    new JobCompletionError({ state: "cancelled", job_id: "j" } as never),
  );
  await quiet;
  await result;
  expect(api.sdk.downloadArtifact).not.toHaveBeenCalled();
});
it("project deletion waits for owned-cover quiescence and does not cancel unrelated jobs", async () => {
  const { api, s, cancel } = fixture();
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(s)]);
  const controller = new WorkspaceController(() => api);
  await controller.connect("http://localhost:8788", "test");
  const call = vi.spyOn(api.sdk, "call").mockResolvedValue({
    receipt: { projects: [] },
    cleanup_pending: [],
  } as never);
  let resolve!: () => void;
  vi.spyOn(api.covers, "quiesce").mockImplementation(
    () => new Promise((r) => (resolve = r)),
  );
  const pending = controller.deleteProjects(
    [projectRef(s)],
    "fixed-deletion-key",
  );
  expect(call).not.toHaveBeenCalled();
  resolve();
  await pending;
  expect(call).toHaveBeenCalledWith("opencut_project_delete", {
    idempotency_key: "fixed-deletion-key",
    projects: [{ project_id: "p", expected_revision: "fixed" }],
  });
  expect(cancel).not.toHaveBeenCalled();
});

const rejection = (code: string) =>
  new OpenCutApiError(code, code, null, 409, "request", []);
async function deletionFixture() {
  const { api, s, cancel } = fixture();
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({} as never);
  vi.spyOn(api, "authenticateConnection").mockResolvedValue(undefined);
  vi.spyOn(api, "projects").mockResolvedValue([projectRef(s)]);
  const controller = new WorkspaceController(() => api);
  await controller.connect("http://localhost:8788", "test");
  const call = vi.spyOn(api.sdk, "call");
  return { api, s, cancel, controller, call };
}
it("a definitive busy deletion retries the identical fixed request without cancelling user jobs", async () => {
  const { s, cancel, controller, call } = await deletionFixture();
  call.mockRejectedValueOnce(rejection("PROJECT_BUSY")).mockResolvedValue({
    receipt: { projects: [] },
    cleanup_pending: [],
  } as never);
  await controller.deleteProjects([projectRef(s)], "same-key");
  expect(call).toHaveBeenCalledTimes(2);
  expect(call.mock.calls[1]).toEqual(call.mock.calls[0]);
  expect(call.mock.calls[0][1]).toEqual({
    idempotency_key: "same-key",
    projects: [{ project_id: "p", expected_revision: "fixed" }],
  });
  expect(cancel).not.toHaveBeenCalled();
});
it("deletion does not retry revision conflicts or unknown transport outcomes", async () => {
  for (const error of [
    rejection("REVISION_CONFLICT"),
    new TypeError("network"),
  ]) {
    const { s, controller, call } = await deletionFixture();
    call.mockRejectedValue(error);
    await expect(
      controller.deleteProjects([projectRef(s)], "key"),
    ).rejects.toBe(error);
    expect(call).toHaveBeenCalledTimes(1);
  }
});
it("persistent project busy stops after ten seconds and releases the cover hold", async () => {
  const { api, s, controller, call } = await deletionFixture();
  const busy = rejection("PROJECT_BUSY");
  call.mockRejectedValue(busy);
  vi.useFakeTimers();
  try {
    const pending = controller
      .deleteProjects([projectRef(s)], "key")
      .catch((e) => e);
    await vi.advanceTimersByTimeAsync(10000);
    expect(await pending).toBe(busy);
    expect(call).toHaveBeenCalledTimes(51);
    expect(
      await api.covers.request(projectRef(s), new AbortController().signal),
    ).toBeInstanceOf(Blob);
  } finally {
    vi.useRealTimers();
  }
});
