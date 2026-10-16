import { expect, it, vi } from "vitest";
import { setupFrame } from "../frame-fixture";
import { frameAvailability, hasGraphics } from "../frame";
import { videoMp4Availability } from "../video-mp4";
import { graphicInitial } from "./fixture";
it("prioritizes graphics over caption/text capabilities and keeps disabled graphics out of admission", async () => {
  const f = await setupFrame();
  try {
    const state = f.controller.getSnapshot().v4!;
    const track = state.head.project.sequences[0].tracks[0];
    const element = track.elements[0];
    element.content = { kind: "graphic", graphic: graphicInitial().graphic };
    state.textFrameCapability = {
      schema_version: 2,
      available: true,
      submit_tool: "opencut_job_v4_text_frame_png",
      unavailable_reason: null,
    };
    expect(hasGraphics(state)).toBe(true);
    expect(frameAvailability(state)).toContain("containing graphics");
    state.graphicFrameCapability = {
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_graphic_frame_png",
      unavailable_reason: null,
    };
    expect(frameAvailability(state)).toBeNull();
    expect(videoMp4Availability(state).reason).not.toBeNull();
    state.videoMp4Capability = {
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_graphic_video_mp4",
      unavailable_reason: null,
    };
    expect(videoMp4Availability(state).reason).toBeNull();
    element.enabled = false;
    expect(hasGraphics(state)).toBe(false);
    expect(frameAvailability(state)).toBeNull();
  } finally {
    f.controller.dispose();
  }
});
it("captures graphic protocol for an unknown request and retries exactly its original key even after capability loss", async () => {
  const f = await setupFrame();
  try {
    f.current().project.sequences[0].tracks[0].elements[0].content = {
      kind: "graphic",
      graphic: graphicInitial().graphic,
    };
    await f.controller.v4.refresh();
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let available = true;
    let original: unknown;
    let submits = 0;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_v4_graphic_frame_capability")
          return {
            schema_version: 1,
            available,
            submit_tool: "opencut_job_v4_graphic_frame_png",
            unavailable_reason: null,
          } as never;
        if (name === "opencut_job_v4_graphic_frame_png") {
          submits++;
          if (submits === 1) {
            original = structuredClone(input);
            throw new Error("Receipt lost");
          }
          expect(input).toEqual(original);
          const job = f.jobFor(input as any);
          job.kind = "render.project_graphic_frame.v4";
          f.rows.push(job);
          return job as never;
        }
        return previous(name, input, options);
      },
    );
    await f.controller.v4.frame.refreshCapability();
    await expect(f.controller.v4.frame.start()).rejects.toThrow("Receipt lost");
    expect(f.controller.getSnapshot().v4!.frame!.protocol).toBe("graphic-v1");
    available = false;
    await f.controller.v4.frame.refreshCapability();
    await f.controller.v4.frame.retry();
    expect(submits).toBe(2);
    expect(f.requests).toHaveLength(0);
    expect(f.controller.getSnapshot().v4!.frame!.phase).toBe("accepted");
  } finally {
    f.controller.dispose();
  }
});
