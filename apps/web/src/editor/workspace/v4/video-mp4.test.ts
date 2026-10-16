import { expect, it, vi } from "vitest";
import { OpenCutApiError, type PublicJob } from "../../../lib/editor-sdk";
import type { V4VideoMp4Request } from "./types";
import { setupV4 } from "./fixture";
import { boxMask } from "./mask/fixture";

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

it("caption export preserves its versioned tool through uncertain retry after scene changes", async () => {
  const f = await setup();
  const state = f.controller.getSnapshot().v4!;
  // Only presence is used by submission routing; no caption layout is mocked.
  state.head.project.sequences[0].captions.push({} as never);
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  let original: V4VideoMp4Request | null = null;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_caption_video_mp4_capability")
      return {
        schema_version: 2,
        available: true,
        submit_tool: "opencut_job_v4_caption_video_mp4",
        unavailable_reason: null,
      } as never;
    if (name === "opencut_job_v4_caption_video_mp4") {
      if (!original) {
        original = structuredClone(input as V4VideoMp4Request);
        throw new TypeError("Lost caption submission");
      }
      expect(input).toEqual(original);
      return {
        ...f.jobFor(input as V4VideoMp4Request),
        kind: "render.project_caption_video.v4",
      } as never;
    }
    if (name === "opencut_job_v4_video_mp4")
      throw new Error("Caption retry switched operation");
    return previous(name, input, options);
  });
  await f.controller.v4.videoMp4.refreshCapability();
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "Lost caption submission",
  );
  expect(f.controller.getSnapshot().v4!.videoMp4?.tool).toBe(
    "opencut_job_v4_caption_video_mp4",
  );
  state.head.project.sequences[0].captions.length = 0;
  await f.controller.v4.videoMp4.retry();
  expect(f.controller.getSnapshot().v4!.videoMp4?.phase).toBe("accepted");
});

it("graphic export requires its own capability and replays its original tool after the scene changes", async () => {
  const f = await setup(),
    state = f.controller.getSnapshot().v4!;
  state.head.project.sequences[0].captions.push({} as never);
  state.head.project.sequences[0].tracks[0].elements.push({
    enabled: true,
    content: { kind: "graphic" },
  } as never);
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "does not support",
  );
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  let original: V4VideoMp4Request | null = null;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_graphic_video_mp4_capability")
      return {
        schema_version: 1,
        available: true,
        submit_tool: "opencut_job_v4_graphic_video_mp4",
        unavailable_reason: null,
      } as never;
    if (name === "opencut_job_v4_graphic_video_mp4") {
      if (!original) {
        original = structuredClone(input as V4VideoMp4Request);
        throw new TypeError("Lost graphic submission");
      }
      expect(input).toEqual(original);
      return {
        ...f.jobFor(input as V4VideoMp4Request),
        kind: "render.project_graphic_video.v4",
      } as never;
    }
    if (
      name === "opencut_job_v4_video_mp4" ||
      name === "opencut_job_v4_caption_video_mp4"
    )
      throw new Error("Graphic retry switched operation");
    return previous(name, input, options);
  });
  await f.controller.v4.videoMp4.refreshCapability();
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "Lost graphic submission",
  );
  expect(f.controller.getSnapshot().v4!.videoMp4?.tool).toBe(
    "opencut_job_v4_graphic_video_mp4",
  );
  state.head.project.sequences[0].tracks[0].elements.pop();
  state.head.project.sequences[0].captions.length = 0;
  await f.controller.v4.videoMp4.retry();
  expect(f.controller.getSnapshot().v4!.videoMp4?.phase).toBe("accepted");
  expect(f.requests).toHaveLength(0);
});

it("an unavailable graphic backend keeps MP4 disabled without hiding the project", async () => {
  const f = await setup(),
    state = f.controller.getSnapshot().v4!;
  state.head.project.sequences[0].tracks[0].elements.push({
    enabled: true,
    content: { kind: "graphic" },
  } as never);
  const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
  vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => {
    if (name === "opencut_project_v4_graphic_video_mp4_capability")
      return {
        schema_version: 1,
        available: false,
        submit_tool: "opencut_job_v4_graphic_video_mp4",
        unavailable_reason: "Graphic renderer unavailable",
      } as never;
    return previous(name, input, options);
  });
  await f.controller.v4.videoMp4.refreshCapability();
  await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
    "Graphic renderer unavailable",
  );
  expect(f.controller.getSnapshot().v4!.head.project.id).toBe(
    state.head.project.id,
  );
  expect(f.requests).toHaveLength(0);
});

it.each(["element", "track"])(
  "disabled %s graphics still select the authored graphic export profile",
  async (disabled) => {
    const f = await setup(),
      state = f.controller.getSnapshot().v4!,
      track = state.head.project.sequences[0].tracks[0];
    track.elements.push({
      enabled: disabled !== "element",
      content: { kind: "graphic" },
    } as never);
    track.enabled = disabled !== "track";
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_v4_graphic_video_mp4_capability")
          return {
            schema_version: 1,
            available: true,
            submit_tool: "opencut_job_v4_graphic_video_mp4",
            unavailable_reason: null,
          } as never;
        if (name === "opencut_job_v4_graphic_video_mp4")
          return {
            ...f.jobFor(input as V4VideoMp4Request),
            kind: "render.project_graphic_video.v4",
          } as never;
        return previous(name, input, options);
      },
    );
    await f.controller.v4.videoMp4.refreshCapability();
    await f.controller.v4.videoMp4.start();
    expect(f.controller.getSnapshot().v4!.videoMp4?.tool).toBe(
      "opencut_job_v4_graphic_video_mp4",
    );
    expect(f.requests).toHaveLength(0);
  },
);

it.each(["enabled", "element", "track"])(
  "uses the blend MP4 protocol for %s non-normal elements and keeps it through an uncertain retry",
  async (disabled) => {
    const f = await setup();
    try {
      const state = f.controller.getSnapshot().v4!,
        track = state.head.project.sequences[0].tracks[0],
        element = track.elements[0];
      element.visual!.blend_mode = "overlay";
      element.enabled = disabled !== "element";
      track.enabled = disabled !== "track";
      // Blend also includes the complete graphic/caption scene, not a separate output path.
      track.elements.push({
        enabled: true,
        content: { kind: "graphic" },
      } as never);
      state.head.project.sequences[0].captions.push({} as never);
      const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
      const requests: V4VideoMp4Request[] = [];
      vi.mocked(f.api.sdk.call).mockImplementation(
        async (name, input, options) => {
          if (name === "opencut_project_v4_blend_video_mp4_capability")
            return {
              schema_version: 1,
              available: true,
              submit_tool: "opencut_job_v4_blend_video_mp4",
              unavailable_reason: null,
            } as never;
          if (name === "opencut_job_v4_blend_video_mp4") {
            requests.push(structuredClone(input as V4VideoMp4Request));
            if (requests.length === 1) throw Error("Blend export receipt lost");
            return {
              ...f.jobFor(input as V4VideoMp4Request),
              kind: "render.project_blend_video.v4",
            } as never;
          }
          return previous(name, input, options);
        },
      );
      await f.controller.v4.videoMp4.refreshCapability();
      await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
        "Blend export receipt lost",
      );
      element.visual!.blend_mode = "normal";
      state.videoMp4Capability = null;
      await f.controller.v4.videoMp4.retry();
      expect(requests).toHaveLength(2);
      expect(requests[1]).toEqual(requests[0]);
      expect(f.controller.getSnapshot().v4!.videoMp4?.tool).toBe(
        "opencut_job_v4_blend_video_mp4",
      );
      expect(f.requests).toHaveLength(0);
    } finally {
      f.controller.dispose();
    }
  },
);

it("does not submit blend video when only the normal graphic capability is available", async () => {
  const f = await setup();
  try {
    const state = f.controller.getSnapshot().v4!;
    state.head.project.sequences[0].tracks[0].elements[0].visual!.blend_mode =
      "hue";
    state.videoMp4Capability = {
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_graphic_video_mp4",
      unavailable_reason: null,
    };
    await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
      "does not support",
    );
    expect(f.requests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});

it.each(["enabled", "element", "track"])(
  "uses the mask MP4 protocol for %s masked elements and keeps it through an uncertain retry",
  async (disabled) => {
    const f = await setup();
    try {
      const state = f.controller.getSnapshot().v4!,
        track = state.head.project.sequences[0].tracks[0],
        element = track.elements[0];
      element.visual!.masks = [boxMask()];
      element.enabled = disabled !== "element";
      track.enabled = disabled !== "track";
      // Mask also includes the complete graphic/caption scene, not a separate output path.
      track.elements.push({
        enabled: true,
        content: { kind: "graphic" },
      } as never);
      state.head.project.sequences[0].captions.push({} as never);
      const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
      const requests: V4VideoMp4Request[] = [];
      vi.mocked(f.api.sdk.call).mockImplementation(
        async (name, input, options) => {
          if (name === "opencut_project_v4_mask_video_mp4_capability")
            return {
              schema_version: 1,
              available: true,
              submit_tool: "opencut_job_v4_mask_video_mp4",
              unavailable_reason: null,
            } as never;
          if (name === "opencut_job_v4_mask_video_mp4") {
            requests.push(structuredClone(input as V4VideoMp4Request));
            if (requests.length === 1) throw Error("Mask export receipt lost");
            return {
              ...f.jobFor(input as V4VideoMp4Request),
              kind: "render.project_mask_video.v4",
            } as never;
          }
          return previous(name, input, options);
        },
      );
      await f.controller.v4.videoMp4.refreshCapability();
      await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
        "Mask export receipt lost",
      );
      element.visual!.masks = [];
      state.videoMp4Capability = null;
      await f.controller.v4.videoMp4.retry();
      expect(requests).toHaveLength(2);
      expect(requests[1]).toEqual(requests[0]);
      expect(f.controller.getSnapshot().v4!.videoMp4?.tool).toBe(
        "opencut_job_v4_mask_video_mp4",
      );
      expect(f.requests).toHaveLength(0);
    } finally {
      f.controller.dispose();
    }
  },
);

it("does not submit mask video when only the normal graphic capability is available", async () => {
  const f = await setup();
  try {
    const state = f.controller.getSnapshot().v4!;
    state.head.project.sequences[0].tracks[0].elements[0].visual!.masks = [
      boxMask(),
    ];
    state.videoMp4Capability = {
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_graphic_video_mp4",
      unavailable_reason: null,
    };
    await expect(f.controller.v4.videoMp4.start()).rejects.toThrow(
      "does not support",
    );
    expect(f.requests).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
