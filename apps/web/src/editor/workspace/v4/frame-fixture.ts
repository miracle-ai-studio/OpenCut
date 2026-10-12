import { vi } from "vitest";
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import type { PublicJob } from "../../../lib/editor-sdk";
import type { V4FrameCapability, V4FrameRequest, V4FrameResult } from "./types";
import { setupV4 } from "./fixture";

// A tiny PNG for download/lifecycle checks only; the root candidate owns
// independent source-to-render pixel acceptance.
export const png = Uint8Array.from(
  atob(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=",
  ),
  (char) => char.charCodeAt(0),
);
export async function setupFrame() {
  const fixture = await setupV4();
  const { api, controller } = fixture;
  const settings = fixture.current().project.settings;
  if (settings.profile === "classic_global")
    settings.settings.canvas_size = { width: 1, height: 1 };
  fixture.snapshots.set("r0", structuredClone(fixture.current()));
  await controller.v4.refresh();
  let capability: V4FrameCapability = {
    schema_version: 1,
    available: true,
    submit_tool: "opencut_job_v4_frame_png",
    unavailable_reason: null,
  };
  const rows: PublicJob[] = [],
    requests: V4FrameRequest[] = [];
  const jobFor = (request: V4FrameRequest): PublicJob => ({
    job_id: `job_${request.idempotency_key.replaceAll("-", "")}`,
    kind: "render.project_frame.v4",
    state: "queued",
    cancel_requested: false,
    target: {
      project_id: request.project_id,
      revision: request.revision,
      sequence_id: request.sequence_id,
    },
    artifacts: [],
    result: null,
    extended_result: null,
    result_unavailable: null,
    failure: null,
  });
  const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
  vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_frame_capability")
      return structuredClone(capability) as never;
    if (name === "opencut_job_v4_frame_png") {
      const request = structuredClone(input as V4FrameRequest);
      requests.push(request);
      const job =
        rows.find((row) => row.job_id === jobFor(request).job_id) ??
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
  const cancel = vi
    .spyOn(api.sdk, "cancelPublicJob")
    .mockImplementation(async (id) => {
      const job = rows.find((row) => row.job_id === id)!;
      job.cancel_requested = true;
      return structuredClone(job);
    });
  const download = vi
    .spyOn(api.sdk, "downloadArtifact")
    .mockImplementation(
      async () =>
        new Response(new Uint8Array(png), {
          headers: { "content-type": "image/png" },
        }),
    );
  const complete = (job = rows[0], request = requests[0]) => {
    const hash = bytesToHex(sha256(png));
    const result: V4FrameResult = {
      schema_version: 1,
      identity: {
        project_id: request.project_id,
        revision: request.revision,
        sequence_id: request.sequence_id,
        time: structuredClone(request.time),
        width: 1,
        height: 1,
        profile: "classic-v31-gpu-normal-frame",
      },
      asset_lock: {
        version: 1,
        revision: request.revision,
        sequence_id: request.sequence_id,
        assets: [],
      },
      render_input_sha256: "a".repeat(64),
      execution: {
        host_sha256: "a".repeat(64),
        browser_version: "fixture",
        browser_sha256: "b".repeat(64),
        module_sha256: "c".repeat(64),
        shader_sha256: "d".repeat(64),
        playwright_version: "fixture",
        mediabunny_version: "fixture",
        gpu_backend: "fixture",
        gpu_device: "fixture",
        texture_format: "rgba8unorm",
        decoder_profile: "fixture",
      },
      sources: [],
      artifact: {
        mime_type: "image/png",
        size_bytes: String(png.length),
        content_hash: hash,
      },
    };
    job.state = "succeeded";
    job.extended_result = {
      schema_version: 1,
      kind: "render.project_frame.v4",
      data: result,
    };
    job.artifacts = [
      {
        job_id: job.job_id,
        artifact_id: `artifact:${job.job_id}:${hash}`,
        content_hash: hash,
        size_bytes: String(png.length),
        mime_type: "image/png",
        role: "primary",
        availability: "unchecked",
      },
    ];
    return result;
  };
  await controller.v4.frame.refreshCapability();
  return {
    ...fixture,
    rows,
    requests,
    jobFor,
    complete,
    cancel,
    download,
    setCapability: (value: V4FrameCapability) => {
      capability = value;
    },
  };
}
