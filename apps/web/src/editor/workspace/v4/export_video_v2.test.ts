import { expect, it, vi } from "vitest";
import type { PublicJob, ToolOutput } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";
import type { V4SessionHost } from "./session";
import { setupV4 } from "./fixture";
import { ExportVideoV2, type ExportVideoOptionsV2, type ExportVideoRequestV2 } from "./export_video_v2";

const options: ExportVideoOptionsV2 = { format: "webm", quality: "very_high", include_audio: false };
const capability = (): ToolOutput<"opencut_export_video_v2_capability"> => ({
  schema_version: 2, available: true, audio_available: true,
  submit_tool: "opencut_job_export_video_v2", formats: ["mp4", "webm"],
  qualities: ["low", "medium", "high", "very_high"], unavailable_reason: null,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
async function setup() {
  const f = await setupV4(), requests: ExportVideoRequestV2[] = [], data = new Map<string, string>();
  const storage = { getItem: (key: string) => data.get(key) ?? null,
    setItem: vi.fn((key: string, value: string) => { data.set(key, value); }), removeItem: (key: string) => { data.delete(key); } };
  let api: EditorApi | null = f.api, state = f.controller.getSnapshot().v4!, generation = 0;
  const release = vi.fn(), host: V4SessionHost = {
    api: () => api, state: () => state, generation: () => generation, viewGeneration: () => generation,
    changeView: () => ++generation, publish: next => { state = next; }, acquire: () => release, refreshTasks: vi.fn(async () => {}),
  };
  const jobFor = (r: ExportVideoRequestV2): PublicJob => ({ job_id: `export-${r.idempotency_key}`, kind: "render.export_video.v2",
    state: "queued", cancel_requested: false, target: { project_id: r.project_id, revision: r.revision, sequence_id: r.sequence_id },
    resource_target: null, progress: null, failure: null, result: null, extended_result: null, result_unavailable: null, artifacts: [] });
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, callOptions) => {
    if (name === "opencut_export_video_v2_capability") return capability() as never;
    if (name === "opencut_job_export_video_v2") {
      const request = structuredClone(input as ExportVideoRequestV2); requests.push(request); return jobFor(request) as never;
    }
    return previous(name, input, callOptions);
  });
  const exporter = new ExportVideoV2(host, storage); await exporter.refreshCapability();
  return { ...f, host, storage, data, requests, exporter, jobFor, release, state,
    replaceApi: (next: EditorApi | null) => { api = next; generation++; } };
}

it("captures all options and restores an uncertain original request after revision changes and controller recreation", async () => {
  const f = await setup(), prior = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  let original: ExportVideoRequestV2 | null = null;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, callOptions) => {
    if (name === "opencut_job_export_video_v2" && !original) {
      original = structuredClone(input as ExportVideoRequestV2); throw new TypeError("Connection lost after acceptance");
    }
    return prior(name, input, callOptions);
  });
  const chosen = structuredClone(options);
  await expect(f.exporter.start(chosen)).rejects.toThrow("Connection lost");
  chosen.format = "mp4"; chosen.include_audio = true;
  expect(f.exporter.getSnapshot().attempt?.phase).toBe("unconfirmed");
  expect(() => f.exporter.dismiss()).toThrow("Confirm the original");
  await expect(f.exporter.start(options)).rejects.toThrow("Resolve or reset");
  f.state.head.revision = "new-head"; f.host.changeView();
  const reopened = new ExportVideoV2(f.host, f.storage); await reopened.refreshCapability();
  expect(reopened.getSnapshot().attempt?.request).toEqual(original);
  await reopened.retry();
  expect(f.requests).toEqual([original]);
  expect(reopened.getSnapshot().attempt?.phase).toBe("accepted");
  expect(reopened.getSnapshot().attempt?.request.options).toEqual(options);
});

it("can export without audio when only the visual runtime is available", async () => {
  const f = await setup(), prior = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, callOptions) => {
    if (name === "opencut_export_video_v2_capability") return { ...capability(), audio_available: false } as never;
    return prior(name, input, callOptions);
  });
  await f.exporter.refreshCapability();
  expect(f.exporter.unavailable({ ...options, include_audio: true })).toContain("audio renderer");
  expect(f.exporter.unavailable(options)).toBeNull();
  await f.exporter.start(options);
  expect(f.requests[0].options).toEqual(options);
});

it("does not dispatch when durable recovery storage fails", async () => {
  const f = await setup(); f.storage.setItem.mockImplementation(() => { throw Error("Storage denied"); });
  await expect(f.exporter.start(options)).rejects.toThrow("Storage denied");
  expect(f.requests).toHaveLength(0);
});

it("blocks concurrent replay and retains the accepted old task without publishing it into a new service generation", async () => {
  const f = await setup(), result = deferred<PublicJob>(), dispatched = deferred<ExportVideoRequestV2>();
  const prior = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, callOptions) => {
    if (name === "opencut_job_export_video_v2") {
      dispatched.resolve(structuredClone(input as ExportVideoRequestV2)); return await result.promise as never;
    }
    return prior(name, input, callOptions);
  });
  const running = f.exporter.start(options), request = await dispatched.promise;
  await expect(f.exporter.retry()).rejects.toThrow("Wait for the original");
  const rendered = f.exporter.getSnapshot().attempt!;
  rendered.request.options.quality = "low"; // Published view cannot mutate the durable request.
  f.replaceApi(null); result.resolve(f.jobFor(request)); await running;
  expect(f.release).toHaveBeenCalledTimes(1);
  expect(f.host.refreshTasks).not.toHaveBeenCalled();
  expect(f.exporter.getSnapshot().attempt?.phase).toBe("submitting");
  const saved = JSON.parse([...f.data.values()][0]).entries[0].attempt;
  expect(saved.phase).toBe("accepted"); expect(saved.request).toEqual(request);
});

it("ignores a late capability from an earlier host generation", async () => {
  const f = await setup(), waiting = deferred<ToolOutput<"opencut_export_video_v2_capability">>(), entered = deferred<void>();
  const prior = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, callOptions) => {
    if (name === "opencut_export_video_v2_capability") { entered.resolve(); return await waiting.promise as never; }
    return prior(name, input, callOptions);
  });
  const refresh = f.exporter.refreshCapability(); await entered.promise;
  f.replaceApi(null); waiting.resolve(capability()); await refresh;
  expect(f.exporter.getSnapshot().capability).toBeNull();
});
