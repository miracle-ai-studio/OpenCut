import { afterEach, expect, it, vi } from "vitest";
import { frameAvailability } from "./frame";
import { setupFrame, png } from "./frame-fixture";
import { readPng, verifiedResult } from "./frame/png";
import type { V4FrameRequest } from "./types";
afterEach(() => vi.restoreAllMocks());

it.each(["missing", "unavailable", "version", "tool"])(
  "refuses %s blend capability without falling back to a normal renderer",
  async (problem) => {
    const f = await setupFrame();
    try {
      const state = f.controller.getSnapshot().v4!;
      state.head.project.sequences[0].tracks[0].elements[0].visual!.blend_mode =
        "multiply";
      state.textFrameCapability = {
        schema_version: 2,
        available: true,
        submit_tool: "opencut_job_v4_text_frame_png",
        unavailable_reason: null,
      };
      if (problem !== "missing")
        state.blendFrameCapability = {
          schema_version: problem === "version" ? 2 : 1,
          available: problem !== "unavailable",
          submit_tool:
            problem === "tool"
              ? "opencut_job_v4_graphic_frame_png"
              : "opencut_job_v4_blend_frame_png",
          unavailable_reason: null,
        };
      expect(frameAvailability(state)).toContain("blend modes");
      await expect(f.controller.v4.frame.start()).rejects.toThrow(
        "blend modes",
      );
      expect(f.requests).toHaveLength(0);
    } finally {
      f.controller.dispose();
    }
  },
);

it("captures the blend protocol and original request across an uncertain reply, scene edits and capability loss", async () => {
  const f = await setupFrame();
  try {
    f.current().project.sequences[0].tracks[0].elements[0].visual!.blend_mode =
      "screen";
    await f.controller.v4.refresh();
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    const requests: V4FrameRequest[] = [];
    let available = true;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_v4_blend_frame_capability")
          return {
            schema_version: 1,
            available,
            submit_tool: "opencut_job_v4_blend_frame_png",
            unavailable_reason: null,
          } as never;
        if (name === "opencut_job_v4_blend_frame_png") {
          requests.push(structuredClone(input as V4FrameRequest));
          if (requests.length === 1) throw Error("Blend receipt lost");
          const job = f.jobFor(input as V4FrameRequest);
          job.kind = "render.project_blend_frame.v4";
          f.rows.push(job);
          return job as never;
        }
        return previous(name, input, options);
      },
    );
    await f.controller.v4.frame.refreshCapability();
    await expect(f.controller.v4.frame.start()).rejects.toThrow(
      "Blend receipt lost",
    );
    expect(f.controller.getSnapshot().v4!.frame!.protocol).toBe("blend-v1");
    available = false;
    f.controller.getSnapshot().v4!.head.project.sequences[0].tracks[0].elements[0].visual!.blend_mode =
      "normal";
    await f.controller.v4.frame.refreshCapability();
    await f.controller.v4.frame.retry();
    expect(requests).toHaveLength(2);
    expect(requests[1]).toEqual(requests[0]);
    expect(f.requests).toHaveLength(0);
    expect(f.controller.getSnapshot().v4!.frame!.phase).toBe("accepted");
  } finally {
    f.controller.dispose();
  }
});

it("verifies the additive blend result identity without accepting a normal graphic receipt", async () => {
  const f = await setupFrame();
  try {
    const request = {
      project_id: "p",
      revision: "r0",
      sequence_id: "scene",
      time: { ticks: "1001", timebase: 30000 },
      idempotency_key: "blend-fixed",
    };
    const job = f.jobFor(request),
      legacy = f.complete(job, request);
    job.kind = "render.project_blend_frame.v4";
    const result = {
      ...legacy,
      identity: {
        ...legacy.identity,
        profile: "classic-v31-gpu-blend-frame-v1",
      },
    };
    job.extended_result = { schema_version: 1, kind: job.kind, data: result };
    const verified = verifiedResult(job, request, 1, 1, "blend-v1");
    expect(verified.result).toEqual(result);
    const blob = await readPng(
      f.api,
      verified.artifact,
      verified.result,
      new AbortController().signal,
    );
    expect(new Uint8Array(await blob.arrayBuffer())).toEqual(png);
    expect(() => verifiedResult(job, request, 1, 1, "graphic-v1")).toThrow(
      "task identity",
    );
    result.identity.profile = "classic-v31-gpu-normal-graphic-frame-v1";
    expect(() => verifiedResult(job, request, 1, 1, "blend-v1")).toThrow(
      "render identity",
    );
  } finally {
    f.controller.dispose();
  }
});
