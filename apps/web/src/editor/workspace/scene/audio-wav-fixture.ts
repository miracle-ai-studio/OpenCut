import { vi } from "vitest";
import type { Job, ToolInput } from "../../../lib/editor-sdk";
import type { AudioWavRequest, AudioRangeWavRequest, AudioWavCapability } from "./types";
import { setup, renderCapability } from "./fixture";

/** Client lifecycle only. The original candidate evidence owns actual PCM. */
export async function setupAudioWav() {
  const fixture = await setup();
  const { api, controller } = fixture;
  let capability: AudioWavCapability = renderCapability();
  const rangeRequests: AudioRangeWavRequest[] = [], requests: AudioWavRequest[] = [], rows: Job[] = [];
  const jobFor = (request: AudioWavRequest): Job => ({
    job_id: `wav-${request.idempotency_key}`, kind: "render.audio", state: "queued", cancel_requested: false,
    target: { project_id: request.project_id, revision: request.revision, sequence_id: request.sequence_id },
    resource_target: null, progress: null, failure: null, result: null, result_unavailable: null, artifacts: [],
  });
  const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
  vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_render_capability") return structuredClone(capability) as never;
    if (name === "opencut_render_submit") {
      const submission = input as ToolInput<"opencut_render_submit">;
      if (submission.kind === "audio") {
        const request = structuredClone(submission.request);
        if (request.range) rangeRequests.push(request as AudioRangeWavRequest); else requests.push(request);
        const job = rows.find(row => row.job_id === `wav-${request.idempotency_key}`) ?? jobFor(request);
        if (!rows.includes(job)) rows.push(job);
        return structuredClone(job) as never;
      }
    }
    return previous(name, input, options);
  });
  vi.mocked(api.tasks).mockImplementation(async projectId => ({
    jobs: structuredClone(rows.filter(row => row.target?.project_id === projectId)), next_cursor: null,
  }));
  await controller.scene.audioWav.refreshCapability();
  return { ...fixture, rows, requests, rangeRequests, jobFor,
    setCapability: (value: AudioWavCapability) => { capability = value; } };
}
