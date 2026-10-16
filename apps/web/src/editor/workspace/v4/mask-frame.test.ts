import { afterEach, expect, it, vi } from "vitest";
import { frameAvailability } from "./frame";
import { setupFrame, png } from "./frame-fixture";
import { readPng, verifiedResult } from "./frame/png";
import type { V4FrameRequest } from "./types";
import { boxMask } from "./mask/fixture";
afterEach(() => vi.restoreAllMocks());

it.each(["missing", "unavailable", "version", "tool"])(
  "refuses %s mask capability without falling back to a normal renderer",
  async (problem) => {
    const f = await setupFrame();
    try {
      const state = f.controller.getSnapshot().v4!;
      state.head.project.sequences[0].tracks[0].elements[0].visual!.masks = [
        boxMask(),
      ];
      state.textFrameCapability = {
        schema_version: 2,
        available: true,
        submit_tool: "opencut_job_v4_text_frame_png",
        unavailable_reason: null,
      };
      if (problem !== "missing")
        state.maskFrameCapability = {
          schema_version: problem === "version" ? 2 : 1,
          available: problem !== "unavailable",
          submit_tool:
            problem === "tool"
              ? "opencut_job_v4_graphic_frame_png"
              : "opencut_job_v4_mask_frame_png",
          unavailable_reason: null,
        };
      expect(frameAvailability(state)).toContain(
        "rectangular or elliptical masks",
      );
      await expect(f.controller.v4.frame.start()).rejects.toThrow(
        "rectangular or elliptical masks",
      );
      expect(f.requests).toHaveLength(0);
    } finally {
      f.controller.dispose();
    }
  },
);

it("captures the mask protocol and original request across an uncertain reply, scene edits and capability loss", async () => {
  const f = await setupFrame();
  try {
    f.current().project.sequences[0].tracks[0].elements[0].visual!.masks = [
      boxMask(),
    ];
    await f.controller.v4.refresh();
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    const requests: V4FrameRequest[] = [];
    let available = true;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_v4_mask_frame_capability")
          return {
            schema_version: 1,
            available,
            submit_tool: "opencut_job_v4_mask_frame_png",
            unavailable_reason: null,
          } as never;
        if (name === "opencut_job_v4_mask_frame_png") {
          requests.push(structuredClone(input as V4FrameRequest));
          if (requests.length === 1) throw Error("Mask receipt lost");
          const job = f.jobFor(input as V4FrameRequest);
          job.kind = "render.project_mask_frame.v4";
          f.rows.push(job);
          return job as never;
        }
        return previous(name, input, options);
      },
    );
    await f.controller.v4.frame.refreshCapability();
    await expect(f.controller.v4.frame.start()).rejects.toThrow(
      "Mask receipt lost",
    );
    expect(f.controller.getSnapshot().v4!.frame!.protocol).toBe("mask-v1");
    available = false;
    f.controller.getSnapshot().v4!.head.project.sequences[0].tracks[0].elements[0].visual!.masks =
      [];
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

it("verifies the additive mask result identity without accepting a normal graphic receipt", async () => {
  const f = await setupFrame();
  try {
    const request = {
      project_id: "p",
      revision: "r0",
      sequence_id: "scene",
      time: { ticks: "1001", timebase: 30000 },
      idempotency_key: "mask-fixed",
    };
    const job = f.jobFor(request),
      legacy = f.complete(job, request);
    job.kind = "render.project_mask_frame.v4";
    const result = {
      ...legacy,
      identity: {
        ...legacy.identity,
        profile: "classic-v31-gpu-mask-frame-v1",
      },
    };
    job.extended_result = { schema_version: 1, kind: job.kind, data: result };
    const verified = verifiedResult(job, request, 1, 1, "mask-v1");
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
    expect(() => verifiedResult(job, request, 1, 1, "mask-v1")).toThrow(
      "render identity",
    );
  } finally {
    f.controller.dispose();
  }
});

it("loads mask rendering capability on initial project open without a manual refresh", async () => {
  const f = await setupFrame();
  try {
    f.current().project.sequences[0].tracks[0].elements[0].visual!.masks = [boxMask()];
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    const capability = { schema_version: 1, available: true, submit_tool: "opencut_job_v4_mask_frame_png", unavailable_reason: null };
    vi.mocked(f.api.sdk.call).mockImplementation(async (name, input, options) => name === "opencut_project_v4_mask_frame_capability" ? capability as never : previous(name, input, options));
    await f.controller.open(f.ref());
    expect(f.controller.getSnapshot().v4!.maskFrameCapability).toEqual(capability);
    expect(frameAvailability(f.controller.getSnapshot().v4!)).toBeNull();
  } finally { f.controller.dispose(); }
});
