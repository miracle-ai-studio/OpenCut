import { expect, it, vi } from "vitest";
import { OpenCutApiError, type PublicJob } from "../../../lib/editor-sdk";
import type { V4VideoMp4Request } from "./types";
import { setupV4 } from "./fixture";

async function setup() {
  const f = await setupV4(),
    requests: V4VideoMp4Request[] = [];
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  const jobFor = (request: V4VideoMp4Request): PublicJob => ({
    job_id: `mp4-${request.idempotency_key}`,
    kind: "render.project_video.v4",
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
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_video_capability")
      return {
        schema_version: 1,
        available: true,
        submit_tool: "opencut_job_v4_video_mp4",
        unavailable_reason: null,
      } as never;
    if (name === "opencut_job_v4_video_mp4") {
      const request = structuredClone(input as V4VideoMp4Request);
      requests.push(request);
      return jobFor(request) as never;
    }
    return previous(name, input, options);
  });
  await f.controller.v4.videoMp4.refreshCapability();
  return { ...f, requests, jobFor };
}

it("captures the saved revision and hands MP4 progress to the existing task feed", async () => {
  const f = await setup();
  const revision = f.controller.getSnapshot().v4!.head.revision;
  await f.controller.v4.videoMp4.start();
  expect(f.requests).toHaveLength(1);
  expect(f.requests[0].revision).toBe(revision);
  expect(f.controller.getSnapshot().v4!.videoMp4?.phase).toBe("accepted");
  expect(f.controller.getSnapshot().v4!.videoMp4?.jobId).toBe(
    `mp4-${f.requests[0].idempotency_key}`,
  );
});

it("replays an uncertain submission with the original identity and blocks replacement", async () => {
  const f = await setup();
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  let original: V4VideoMp4Request | null = null;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_job_v4_video_mp4" && !original) {
      original = structuredClone(input as V4VideoMp4Request);
      throw new TypeError("Connection lost after submit");
    }
    return previous(name, input, options);
  });
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "Connection lost",
  );
  expect(f.controller.getSnapshot().v4!.videoMp4?.phase).toBe("unconfirmed");
  expect(() => f.controller.v4.videoMp4.dismiss()).toThrow(
    "Confirm the original",
  );
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "Resolve the previous",
  );
  await f.controller.v4.videoMp4.retry();
  expect(f.requests[0]).toEqual(original);
  expect(f.controller.getSnapshot().v4!.videoMp4?.phase).toBe("accepted");
});

it("keeps service capability failures isolated to MP4 export", async () => {
  const f = await setup();
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_video_capability")
      throw new OpenCutApiError(
        "FORBIDDEN",
        "Video export denied.",
        null,
        403,
        "req",
        [],
      );
    return previous(name, input, options);
  });
  await f.controller.v4.videoMp4.refreshCapability();
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "Video export denied.",
  );
  expect(f.controller.getSnapshot().v4!.head.project.id).toBeTruthy();
  expect(f.requests).toHaveLength(0);
});
