import { afterEach, expect, it, vi } from "vitest";
import {
  OpenCutApiError,
  OpenCutTransportError,
} from "../../../lib/editor-sdk";
import { png, setupFrame } from "./frame-fixture";
import { frameAvailability } from "./frame";
import { textInitial } from "./text/fixture";
import type { V4FrameRequest } from "./types";
afterEach(() => vi.restoreAllMocks());

it("keeps a successful fixed frame when an undo plan opens during its download without changing frame identity", async () => {
  const f = await setupFrame();
  try {
    await f.controller.v4.bookmark({
      kind: "put",
      bookmark: {
        id: "undo-note",
        sequence_id: "scene",
        time: { ticks: "0", timebase: 1 },
        note: "Undoable note",
      },
    });
    await f.controller.v4.frame.start();
    f.complete();
    await f.controller.sync();
    let release!: (value: Response) => void;
    f.download.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const reading = f.controller.v4.frame.observe(
      f.controller.getSnapshot().jobs,
    );
    await f.controller.v4.planUndo("tx1");
    expect(f.controller.getSnapshot().v4!.historyPlan!.direction).toBe("undo");
    release(
      new Response(new Uint8Array(png), {
        headers: { "content-type": "image/png" },
      }),
    );
    const value = await reading;
    expect(new Uint8Array(await value!.blob.arrayBuffer())).toEqual(png);
    expect(f.controller.getSnapshot().v4!.frame!.phase).toBe("ready");
    expect(f.requests).toHaveLength(1);
    expect(f.cancel).not.toHaveBeenCalled();
  } finally {
    f.controller.dispose();
  }
});

it("submits a fixed historical revision and exact rational time and reads only its published PNG from the existing task feed", async () => {
  const f = await setupFrame();
  try {
    f.current().revision = "r-later";
    await f.controller.v4.refresh();
    await f.controller.v4.inspectRevision("r0");
    f.controller.v4.seek({ ticks: "1", timebase: 3 });
    await f.controller.v4.frame.start();
    expect(f.requests[0]).toMatchObject({
      revision: "r0",
      sequence_id: "scene",
      time: { ticks: "1", timebase: 3 },
    });
    expect(f.requests[0]).not.toHaveProperty("alignment");
    const result = f.complete();
    await f.controller.sync();
    const value = await f.controller.v4.frame.observe(
      f.controller.getSnapshot().jobs,
    );
    expect(new Uint8Array(await value!.blob.arrayBuffer())).toEqual(png);
    expect(value!.result).toEqual(result);
    expect(f.controller.getSnapshot().v4!.frame!.phase).toBe("ready");
    expect(f.controller.getSnapshot().snapshot).toBeNull();
    expect(f.controller.getSnapshot().jobs).toHaveLength(1);
    expect(
      f.call.mock.calls.some(
        ([name]) =>
          name === "opencut_job_submit" || name === "opencut_job_v3_frame",
      ),
    ).toBe(false);
  } finally {
    f.controller.dispose();
  }
});

it("does not present a completed frame after a seek or head revision change while its download is in flight", async () => {
  const f = await setupFrame();
  try {
    await f.controller.v4.frame.start();
    f.complete();
    await f.controller.sync();
    let release!: (value: Response) => void;
    f.download.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const reading = f.controller.v4.frame.observe(
      f.controller.getSnapshot().jobs,
    );
    f.controller.v4.seek({ ticks: "1", timebase: 1 });
    f.controller.v4.frame.invalidate();
    release(
      new Response(new Uint8Array(png), {
        headers: { "content-type": "image/png" },
      }),
    );
    expect(await reading).toBeNull();
    expect(f.controller.getSnapshot().v4!.frame).toBeNull();
    await f.controller.v4.frame.start();
    f.complete(f.rows[1], f.requests[1]);
    await f.controller.sync();
    f.current().revision = "r2";
    await f.controller.v4.refresh();
    expect(
      await f.controller.v4.frame.observe(f.controller.getSnapshot().jobs),
    ).toBeNull();
  } finally {
    f.controller.dispose();
  }
});

it("fences scene changes before acceptance and identical-target route reopen during a pending download", async () => {
  const f = await setupFrame();
  try {
    const scene = structuredClone(f.current().project.sequences[0]);
    scene.id = "other-scene";
    scene.name = "Other scene";
    scene.main_track_id = "other-track";
    scene.tracks = [{ ...scene.tracks[0], id: "other-track", elements: [] }];
    scene.bookmarks = [];
    f.current().project.sequences.push(scene);
    await f.controller.v4.refresh();
    const previous = f.call.getMockImplementation()!;
    let releaseSubmission!: () => void;
    f.call.mockImplementationOnce(async (name, input, options) => {
      if (name === "opencut_job_v4_frame_png") {
        await new Promise<void>((resolve) => {
          releaseSubmission = resolve;
        });
      }
      return previous(name, input, options);
    });
    const submission = f.controller.v4.frame.start();
    f.controller.v4.chooseSequence("other-scene");
    releaseSubmission();
    await submission;
    expect(f.cancel).toHaveBeenCalledWith(f.rows[0].job_id);
    expect(f.controller.getSnapshot().v4!.frame!.error).toContain(
      "previous view",
    );
    f.controller.v4.chooseSequence("scene");
    await f.controller.v4.frame.start();
    f.complete(f.rows[1], f.requests[1]);
    await f.controller.sync();
    let releaseDownload!: (value: Response) => void;
    f.download.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseDownload = resolve;
        }),
    );
    const reading = f.controller.v4.frame.observe(
      f.controller.getSnapshot().jobs,
    );
    await f.controller.open(f.ref());
    releaseDownload(
      new Response(new Uint8Array(png), {
        headers: { "content-type": "image/png" },
      }),
    );
    expect(await reading).toBeNull();
    expect(f.controller.getSnapshot().v4!.frame).toBeNull();
  } finally {
    f.controller.dispose();
  }
});

it("retains unknown acceptance across reopen and replays the exact original request before cancelling its obsolete task", async () => {
  const f = await setupFrame();
  try {
    const previous = f.call.getMockImplementation()!;
    let first = true;
    f.call.mockImplementation(async (name, input, options) => {
      if (name === "opencut_job_v4_frame_png" && first) {
        first = false;
        const request = structuredClone(input as V4FrameRequest);
        f.requests.push(request);
        f.rows.push(f.jobFor(request));
        throw new OpenCutTransportError(new Error("Lost accepted reply"));
      }
      return previous(name, input, options);
    });
    await expect(f.controller.v4.frame.start()).rejects.toThrow();
    const request = structuredClone(f.requests[0]);
    f.current().revision = "new-head";
    await f.controller.open(f.ref());
    expect(f.controller.getSnapshot().v4!.frame!.phase).toBe("unconfirmed");
    await expect(f.controller.v4.frame.start()).rejects.toThrow("Confirm");
    await f.controller.v4.frame.retry();
    expect(f.requests[1]).toEqual(request);
    expect(f.rows).toHaveLength(1);
    expect(f.cancel).toHaveBeenCalledWith(f.rows[0].job_id);
    expect(f.controller.getSnapshot().v4!.frame!.error).toContain(
      "previous view",
    );
  } finally {
    f.controller.dispose();
  }
});

it("rejects a mismatched render identity and corrupt bytes, and retries authorized download without submitting another task", async () => {
  const f = await setupFrame();
  try {
    await f.controller.v4.frame.start();
    const result = f.complete();
    result.identity.time = { ticks: "1", timebase: 1 };
    await f.controller.sync();
    expect(
      await f.controller.v4.frame.observe(f.controller.getSnapshot().jobs),
    ).toBeNull();
    expect(f.download).not.toHaveBeenCalled();
    result.identity.time = f.requests[0].time;
    await f.controller.sync();
    const corrupt = new Uint8Array(png);
    corrupt[30] ^= 1;
    f.download.mockResolvedValueOnce(
      new Response(corrupt, { headers: { "content-type": "image/png" } }),
    );
    expect(
      await f.controller.v4.frame.retryDownload(
        f.controller.getSnapshot().jobs,
      ),
    ).toBeNull();
    expect(f.controller.getSnapshot().v4!.frame!.error).toContain("hash");
    const value = await f.controller.v4.frame.retryDownload(
      f.controller.getSnapshot().jobs,
    );
    expect(value!.blob.type).toBe("image/png");
    expect(f.requests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});

it("keeps permission failures visible, respects actual capability, and cancels only the known frame task", async () => {
  const f = await setupFrame();
  try {
    f.setCapability({
      schema_version: 1,
      available: false,
      submit_tool: "opencut_job_v4_frame_png",
      unavailable_reason: "GPU worker unavailable.",
    });
    await f.controller.v4.frame.refreshCapability();
    await expect(f.controller.v4.frame.start()).rejects.toThrow("GPU worker");
    expect(f.requests).toHaveLength(0);
    f.setCapability({
      schema_version: 1,
      available: true,
      submit_tool: "opencut_job_v4_frame_png",
      unavailable_reason: null,
    });
    await f.controller.v4.frame.refreshCapability();
    await f.controller.v4.frame.start();
    f.complete();
    await f.controller.sync();
    f.download.mockRejectedValueOnce(
      new OpenCutApiError(
        "PERMISSION_DENIED",
        "Artifact permission revoked.",
        null,
        403,
        "req",
        [],
      ),
    );
    expect(
      await f.controller.v4.frame.observe(f.controller.getSnapshot().jobs),
    ).toBeNull();
    expect(f.controller.getSnapshot().v4!.frame!.error).toContain("revoked");
    await f.controller.v4.frame.start();
    await f.controller.v4.frame.cancel();
    expect(f.cancel).toHaveBeenCalledTimes(1);
    expect(f.cancel).toHaveBeenCalledWith(f.rows[1].job_id);
    expect(
      f.controller
        .getSnapshot()
        .jobs.find((j) => j.job_id === f.rows[1].job_id)!.cancel_requested,
    ).toBe(true);
  } finally {
    f.controller.dispose();
  }
});

it("keeps the finite display budget on retries while leaving the published artifact available in Tasks", async () => {
  const f = await setupFrame();
  try {
    await f.controller.v4.frame.start();
    const result = f.complete();
    result.artifact.size_bytes = String(32 * 1024 * 1024 + 1);
    f.rows[0].artifacts[0].size_bytes = result.artifact.size_bytes;
    await f.controller.sync();
    expect(
      await f.controller.v4.frame.observe(f.controller.getSnapshot().jobs),
    ).toBeNull();
    expect(f.controller.getSnapshot().v4!.frame!.error).toContain(
      "Download the PNG from Tasks",
    );
    expect(
      await f.controller.v4.frame.retryDownload(
        f.controller.getSnapshot().jobs,
      ),
    ).toBeNull();
    expect(f.download).not.toHaveBeenCalled();
    expect(f.requests).toHaveLength(1);
    expect(f.controller.getSnapshot().jobs[0].artifacts).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});

it("discards a pending image read when the authoritative task feed reports revoked result access", async () => {
  const f = await setupFrame();
  try {
    await f.controller.v4.frame.start();
    f.complete();
    await f.controller.sync();
    let release!: (value: Response) => void;
    f.download.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const reading = f.controller.v4.frame.observe(
      f.controller.getSnapshot().jobs,
    );
    f.rows[0].result_unavailable = {
      code: "PERMISSION_DENIED",
      message: "Result access revoked.",
      diagnostic_job_id: f.rows[0].job_id,
    };
    f.controller.v4.frame.rejectTask(f.rows[0]);
    release(
      new Response(new Uint8Array(png), {
        headers: { "content-type": "image/png" },
      }),
    );
    expect(await reading).toBeNull();
    expect(f.controller.getSnapshot().v4!.frame!.error).toBe(
      "Result access revoked.",
    );
    expect(f.requests).toHaveLength(1);
  } finally {
    f.controller.dispose();
  }
});

it("prefers the declared text frame protocol and keeps an unknown retry on that protocol after capability loss", async () => {
  const f = await setupFrame();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    let available = true;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_v4_text_frame_capability")
          return {
            schema_version: 2,
            available,
            submit_tool: "opencut_job_v4_text_frame_png",
            unavailable_reason: null,
          } as never;
        return previous(name, input, options);
      },
    );
    const submit = vi
      .spyOn(f.api.v4, "textFramePng")
      .mockRejectedValue(new Error("Receipt lost"));
    await f.controller.v4.frame.refreshCapability();
    await expect(f.controller.v4.frame.start()).rejects.toThrow("Receipt lost");
    const original = structuredClone(submit.mock.calls[0][0]);
    expect(f.controller.getSnapshot().v4!.frame!.protocol).toBe(2);
    available = false;
    await f.controller.v4.frame.refreshCapability();
    submit.mockImplementation(async (request) => {
      const job = f.jobFor(request);
      job.kind = "render.project_text_frame.v4";
      f.rows.push(job);
      return job;
    });
    await f.controller.v4.frame.retry();
    expect(submit.mock.calls[1][0]).toEqual(original);
    expect(f.requests).toHaveLength(0);
    expect(f.controller.getSnapshot().v4!.frame!.phase).toBe("accepted");
  } finally {
    f.controller.dispose();
  }
});

it("does not choose a text frame tool when its capability version or submit name is incompatible", async () => {
  const f = await setupFrame();
  try {
    const previous = vi.mocked(f.api.sdk.call).getMockImplementation()!;
    vi.mocked(f.api.sdk.call).mockImplementation(
      async (name, input, options) => {
        if (name === "opencut_project_v4_text_frame_capability")
          return {
            schema_version: 2,
            available: true,
            submit_tool: "opencut_job_v4_frame_png",
            unavailable_reason: null,
          } as never;
        return previous(name, input, options);
      },
    );
    const submit = vi.spyOn(f.api.v4, "textFramePng");
    await f.controller.v4.frame.refreshCapability();
    await f.controller.v4.frame.start();
    expect(submit).not.toHaveBeenCalled();
    expect(f.requests).toHaveLength(1);
    expect(f.controller.getSnapshot().v4!.frame!.protocol).toBe(1);
  } finally {
    f.controller.dispose();
  }
});

it("keeps text explicit on a media-only host and ignores a disabled title without projecting it as a graphic", async () => {
  const f = await setupFrame();
  try {
    const state = structuredClone(f.controller.getSnapshot().v4!);
    const track = state.head.project.sequences[0].tracks[0];
    track.elements[0].content = { kind: "text", text: textInitial().text };
    state.textFrameCapability = null;
    expect(frameAvailability(state)).toContain("containing text");
    track.elements[0].enabled = false;
    expect(frameAvailability(state)).toBeNull();
    track.elements[0].enabled = true;
    track.enabled = false;
    expect(frameAvailability(state)).toBeNull();
    track.enabled = true;
    state.textFrameCapability = {
      schema_version: 2,
      available: true,
      submit_tool: "opencut_job_v4_text_frame_png",
      unavailable_reason: null,
    };
    expect(frameAvailability(state)).toBeNull();
  } finally {
    f.controller.dispose();
  }
});

it("consumes a schema-2 text PNG through the schema-1 extended public wrapper", async () => {
  const f = await setupFrame();
  try {
    const request = { project_id: "p", revision: "r0", sequence_id: "scene", time: { ticks: "0", timebase: 1 }, idempotency_key: "a".repeat(32) };
    const job = f.jobFor(request);
    const legacy = f.complete(job, request);
    job.kind = "render.project_text_frame.v4";
    const result = { ...legacy, schema_version: 2, identity: { ...legacy.identity, profile: "classic-v31-gpu-normal-text-frame-v2" }, font_lock: [], texts: [] };
    job.extended_result = { schema_version: 1, kind: job.kind, data: result };
    const { verifiedResult, readPng } = await import("./frame/png");
    const verified = verifiedResult(job, request, 1, 1, 2);
    expect(verified.result).toEqual(result);
    const blob = await readPng(f.api, verified.artifact, verified.result, new AbortController().signal);
    expect(new Uint8Array(await blob.arrayBuffer())).toEqual(png);
    expect(() => verifiedResult(job, request, 1, 1, 1)).toThrow("task identity");
    job.extended_result.schema_version = 2;
    expect(() => verifiedResult(job, request, 1, 1, 2)).toThrow("available successful");
    job.extended_result.schema_version = 1;
    result.schema_version = 1;
    expect(() => verifiedResult(job, request, 1, 1, 2)).toThrow("render identity");
    result.schema_version = 2;
    result.identity.profile = "classic-v31-gpu-normal-frame";
    expect(() => verifiedResult(job, request, 1, 1, 2)).toThrow("render identity");
  } finally { f.controller.dispose(); }
});
