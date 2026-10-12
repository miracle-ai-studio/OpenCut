import { vi } from "vitest";
import type { PublicJob } from "../../../lib/editor-sdk";
import type {
  V4AudioWavRequest,
  V4AudioRangeWavRequest,
  V4AudioWavCapability,
  V4AudioRangeWavCapability,
} from "./types";
import { setupV4 } from "./fixture";

/** Protocol/client lifecycle only. The root candidate verifies actual queued WAV bytes. */
export async function setupAudioWav() {
  const fixture = await setupV4();
  const { api, controller } = fixture;
  let capability: V4AudioWavCapability = {
    schema_version: 1,
    available: true,
    submit_tool: "opencut_job_v4_audio_wav",
    sample_rates: [44100, 48000],
    unavailable_reason: null,
  };
  let rangeCapability: V4AudioRangeWavCapability = {
    schema_version: 1,
    available: true,
    submit_tool: "opencut_job_v4_audio_range_wav",
    sample_rates: [44100, 48000],
    unavailable_reason: null,
  };
  const rangeRequests: V4AudioRangeWavRequest[] = [];
  const rows: PublicJob[] = [],
    requests: V4AudioWavRequest[] = [];
  const jobFor = (
    request: V4AudioWavRequest | V4AudioRangeWavRequest,
  ): PublicJob => ({
    job_id: `wav-${request.idempotency_key}`,
    kind:
      "range" in request
        ? "render.project_audio_range.v4"
        : "render.project_audio.v4",
    state: "queued",
    cancel_requested: false,
    target: {
      project_id: request.project_id,
      revision: request.revision,
      sequence_id: request.sequence_id,
    },
    resource_target: null,
    progress: null,
    failure: null,
    result: null,
    extended_result: null,
    result_unavailable: null,
    artifacts: [],
  });
  const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
  vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_audio_capability")
      return structuredClone(capability) as never;
    if (name === "opencut_project_v4_audio_range_capability")
      return structuredClone(rangeCapability) as never;
    if (name === "opencut_job_v4_audio_range_wav") {
      const request = structuredClone(input as V4AudioRangeWavRequest);
      rangeRequests.push(request);
      const job =
        rows.find((row) => row.job_id === `wav-${request.idempotency_key}`) ??
        jobFor(request);
      if (!rows.includes(job)) rows.push(job);
      return structuredClone(job) as never;
    }
    if (name === "opencut_job_v4_audio_wav") {
      const request = structuredClone(input as V4AudioWavRequest);
      requests.push(request);
      const job =
        rows.find((row) => row.job_id === `wav-${request.idempotency_key}`) ??
        jobFor(request);
      if (!rows.includes(job)) rows.push(job);
      return structuredClone(job) as never;
    }
    return previous(name, input, options);
  });
  vi.mocked(api.tasks).mockImplementation(async (projectId) => ({
    jobs: structuredClone(
      rows.filter((row) => row.target?.project_id === projectId),
    ),
    next_cursor: null,
  }));
  await controller.v4.audioWav.refreshCapability();
  await controller.v4.audioWav.refreshCapability("range");
  return {
    ...fixture,
    rows,
    requests,
    rangeRequests,
    jobFor,
    setRangeCapability: (value: V4AudioRangeWavCapability) => {
      rangeCapability = value;
    },
    setCapability: (value: V4AudioWavCapability) => {
      capability = value;
    },
  };
}
